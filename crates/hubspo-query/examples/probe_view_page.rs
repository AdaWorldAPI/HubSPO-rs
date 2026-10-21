//! P-VIEW-1: what one saved-view page costs without `ORDER BY`.
//!
//! For each sort key and offset, one 25-record page through
//! `hubspo_query::view::page`, against two references on the same data:
//!
//! - **full sort:** every kept record materialised, sorted by (value,
//!   ordinal), sliced: what a page costs if the edge sorts everything;
//! - **one scan:** a single threshold `Agg::Count` over the key lane: a floor
//!   for any in-engine `ORDER BY`, which has to read the key lane at least once.
//!
//! Reported per case: programs run, rows materialised, median wall time of
//! `RUNS` repetitions. Keys: `score` (i32, near-unique), `amount` (i32 in
//! -20..20, 20 % null: heavy ties), `stage` (code, 4 values: a page inside
//! one code materialises that code).
//!
//! `cargo run --release -p hubspo-query --example probe_view_page [rows]`

use std::time::{Duration, Instant};

use hubspo_query::view::{Direction, Runner, SavedView, Sort, page};
use hubspo_store::catalog::{Catalog, Codebook, FieldSpec};
use lance_graph_mask_risc::{
    Foreign, LaneRef, Out, Planes, Scratch, Value, execute_into, materialize_rows, words_for,
};
use lance_graph_quack::bind::{FieldKind, create_table};
use lance_graph_quack::{Agg, Col, Mask, Query, lower};

const LIMIT: usize = 25;
const RUNS: usize = 5;
const STAGES: &[&str] = &["lead", "opportunity", "customer", "evangelist"];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

struct Data {
    n: usize,
    live: Vec<u64>,
    amount_valid: Vec<u64>,
    stage: Vec<u32>,
    amount: Vec<i32>,
    score: Vec<i32>,
    programs: usize,
    materialised: usize,
}

impl Data {
    fn new(n: usize) -> Self {
        let mut r = Rng(0x0D3E_5EED);
        let mut live = vec![0u64; words_for(n)];
        let mut amount_valid = vec![0u64; words_for(n)];
        let (mut stage, mut amount, mut score) = (Vec::new(), Vec::new(), Vec::new());
        for i in 0..n {
            let x = r.next();
            if x % 100 >= 5 {
                live[i / 64] |= 1 << (i % 64);
            }
            if (x >> 8) % 100 >= 20 {
                amount_valid[i / 64] |= 1 << (i % 64);
            }
            stage.push(((x >> 16) % 4) as u32);
            amount.push(((x >> 24) % 41) as i32 - 20);
            score.push(r.next() as i32);
        }
        Self {
            n,
            live,
            amount_valid,
            stage,
            amount,
            score,
            programs: 0,
            materialised: 0,
        }
    }

    fn run(&mut self, q: &Query, out: Out<'_>) -> Value {
        self.programs += 1;
        let masks: [&[u64]; 2] = [&self.live, &self.amount_valid];
        let lanes = [
            LaneRef::U32(&self.stage),
            LaneRef::I32(&self.amount),
            LaneRef::I32(&self.score),
        ];
        let planes = Planes {
            n_rows: self.n,
            masks: &masks,
            lanes: &lanes,
        };
        let program = lower(q).expect("lower");
        let mut scratch = Scratch::for_program(&program, self.n).expect("scratch");
        execute_into(&program, &planes, &Foreign::NONE, &mut scratch, out).expect("execute")
    }
}

impl Runner for Data {
    type Error = ();
    fn count(&mut self, q: &Query) -> Result<u64, ()> {
        match self.run(q, Out::None) {
            Value::Count(n) => Ok(n as u64),
            v => panic!("{v:?}"),
        }
    }
    fn rows(&mut self, q: &Query) -> Result<Vec<usize>, ()> {
        let mut kept = vec![0u64; words_for(self.n)];
        self.run(q, Out::Mask(&mut kept));
        let rows = materialize_rows(&kept, self.n);
        self.materialised += rows.len();
        Ok(rows)
    }
    fn groups(&mut self, q: &Query, groups: u32) -> Result<Vec<i64>, ()> {
        let mut sink = vec![0i64; groups as usize];
        self.run(q, Out::I64(&mut sink));
        Ok(sink)
    }
    fn code_domain(&self, _: Col) -> u32 {
        STAGES.len() as u32
    }
    fn i32_at(&self, col: Col, row: usize) -> i32 {
        match col {
            Col(1) => self.amount[row],
            _ => self.score[row],
        }
    }
}

fn catalog() -> Catalog {
    let mut c = Catalog::new();
    let t = create_table("contacts")
        .width(8)
        .register(&mut c)
        .expect("register")
        .id;
    let fields = [
        ("lifecyclestage", Col(0), FieldKind::Code, None),
        ("amount", Col(1), FieldKind::I32, Some(Mask(1))),
        ("score", Col(2), FieldKind::I32, None),
    ];
    for (name, col, kind, validity) in fields {
        c.declare(
            t,
            FieldSpec {
                name: name.into(),
                col,
                kind,
                validity,
                codebook: (kind == FieldKind::Code)
                    .then(|| Codebook::new(STAGES.iter().copied()).expect("codebook")),
            },
        )
        .expect("declare");
    }
    c
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort_unstable();
    v[v.len() / 2]
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

/// Every kept record, sorted at the edge: the reference a full sort costs.
fn full_sort(d: &mut Data, key: Col, offset: usize) -> (Vec<usize>, usize) {
    let live = Query {
        filter: lance_graph_quack::Filter::plane(Mask(0)),
        agg: Agg::Rows,
    };
    let rows = d.rows(&live).expect("rows");
    let n = rows.len();
    let mut keyed: Vec<(i32, usize)> = rows
        .into_iter()
        .map(|r| {
            let v = match key {
                Col(0) => d.stage[r] as i32,
                Col(1) => d.amount[r],
                _ => d.score[r],
            };
            (v, r)
        })
        .collect();
    keyed.sort_unstable();
    (
        keyed
            .into_iter()
            .skip(offset)
            .take(LIMIT)
            .map(|(_, r)| r)
            .collect(),
        n,
    )
}

fn main() {
    let n: usize = std::env::args()
        .nth(1)
        .map(|a| a.parse().expect("rows"))
        .unwrap_or(1_000_000);
    let cat = catalog();
    let mut d = Data::new(n);
    let live_n = d
        .count(&Query {
            filter: lance_graph_quack::Filter::plane(Mask(0)),
            agg: Agg::Count,
        })
        .unwrap() as usize;

    // One scan of the key lane: a threshold Count, the floor any in-engine
    // ORDER BY pays (a bare plane Count is a popcount and reads no lane).
    let scan = median(
        (0..RUNS)
            .map(|_| {
                let t = Instant::now();
                d.count(&Query {
                    filter: lance_graph_quack::Filter::and([
                        lance_graph_quack::Filter::plane(Mask(0)),
                        lance_graph_quack::Filter::cmp(Col(2), lance_graph_quack::Cmp::LeI32(0)),
                    ]),
                    agg: Agg::Count,
                })
                .unwrap();
                t.elapsed()
            })
            .collect(),
    );
    println!("rows {n}, live {live_n}, page {LIMIT}, median of {RUNS}");
    println!(
        "one scan (threshold Count on the key lane): {:.2} ms",
        ms(scan)
    );
    println!(
        "{:<16} {:>9} {:>9} {:>12} {:>10} {:>12} {:>10}",
        "key", "offset", "programs", "materialised", "page ms", "full-sort ms", "page/scan"
    );

    let keys = [
        ("score", Col(2), Direction::Ascending),
        ("score", Col(2), Direction::Descending),
        ("amount", Col(1), Direction::Ascending),
        ("lifecyclestage", Col(0), Direction::Ascending),
    ];
    for (name, col, direction) in keys {
        let view = SavedView {
            object: "contacts".into(),
            sort: Some(Sort {
                property: name.into(),
                direction,
            }),
            ..SavedView::default()
        };
        // Offsets over the key's VALUED records, so the tail page still sorts;
        // a nullable key adds one page wholly inside its null tail, reported
        // on its own row (it runs no bisection).
        let valued_n = if name == "amount" {
            d.count(&Query {
                filter: lance_graph_quack::Filter::and([
                    lance_graph_quack::Filter::plane(Mask(0)),
                    lance_graph_quack::Filter::plane(Mask(1)),
                ]),
                agg: Agg::Count,
            })
            .unwrap() as usize
        } else {
            live_n
        };
        let mut offsets = vec![0usize, valued_n / 2, valued_n - LIMIT];
        if valued_n < live_n {
            offsets.push(live_n - LIMIT);
        }
        for offset in offsets {
            let mut times = Vec::new();
            let (mut programs, mut materialised) = (0, 0);
            let mut got = Vec::new();
            for _ in 0..RUNS {
                d.programs = 0;
                d.materialised = 0;
                let t = Instant::now();
                got = page(&view, &cat, offset, LIMIT, &mut d).expect("page");
                times.push(t.elapsed());
                programs = d.programs;
                materialised = d.materialised;
            }
            assert_eq!(got.len(), LIMIT, "{name} {offset}");
            // The full-sort reference, and a check against it where the
            // order agrees (ascending, no nulls).
            let full = median(
                (0..RUNS)
                    .map(|_| {
                        let t = Instant::now();
                        let (want, _) = full_sort(&mut d, col, offset);
                        if direction == Direction::Ascending && name != "amount" {
                            assert_eq!(got, want, "{name} {offset}");
                        }
                        t.elapsed()
                    })
                    .collect(),
            );
            let p = median(times);
            println!(
                "{:<16} {:>9} {:>9} {:>12} {:>10.2} {:>12.2} {:>10.1}{}",
                format!(
                    "{name} {}",
                    if direction == Direction::Ascending {
                        "asc"
                    } else {
                        "desc"
                    }
                ),
                offset,
                programs,
                materialised,
                ms(p),
                ms(full),
                p.as_secs_f64() / scan.as_secs_f64(),
                if offset >= valued_n {
                    "  (null tail)"
                } else {
                    ""
                }
            );
        }
    }
}

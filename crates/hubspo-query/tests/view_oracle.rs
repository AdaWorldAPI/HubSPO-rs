//! D-QRY-3 saved-view half against a plain sort: every page of every view
//! (no sort, an `I32` key and a code key, both directions, nullable and not,
//! filtered and not) equals the same records sorted in Rust by the order
//! `view` documents — value, then ordinal ascending, records without a value
//! last in storage order.

use std::cell::Cell;

use hubspo_query::filter::{FilterGroup, Operator, PropertyFilter};
use hubspo_query::view::{Direction, Runner, SavedView, Sort, page};
use hubspo_store::catalog::{Catalog, Codebook, FieldSpec};
use lance_graph_mask_risc::{
    Foreign, LaneRef, Out, Planes, Scratch, Value, execute_into, materialize_rows, words_for,
};
use lance_graph_quack::bind::{FieldKind, create_table};
use lance_graph_quack::{Col, Mask, Query, lower};

const N: usize = 3_000;
const STAGES: &[&str] = &["lead", "opportunity", "customer", "evangelist", "other"];

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Lanes: 0 stage (code), 1 amount (i32, nullable via plane 1), 2 score
/// (i32, never null, wide range), 3 owner (code, nullable via plane 2).
/// Planes: 0 live, 1 amount valid, 2 owner valid.
struct Data {
    live: Vec<u64>,
    amount_valid: Vec<u64>,
    owner_valid: Vec<u64>,
    stage: Vec<u32>,
    amount: Vec<i32>,
    score: Vec<i32>,
    owner: Vec<u32>,
    programs: Cell<usize>,
}

fn set(w: &mut [u64], i: usize) {
    w[i / 64] |= 1 << (i % 64);
}
fn get(w: &[u64], i: usize) -> bool {
    w[i / 64] >> (i % 64) & 1 == 1
}

fn data() -> Data {
    let mut r = Rng(0x5A7E_D71E);
    let mut d = Data {
        live: vec![0; words_for(N)],
        amount_valid: vec![0; words_for(N)],
        owner_valid: vec![0; words_for(N)],
        stage: Vec::new(),
        amount: Vec::new(),
        score: Vec::new(),
        owner: Vec::new(),
        programs: Cell::new(0),
    };
    for i in 0..N {
        if r.below(100) >= 5 {
            set(&mut d.live, i);
        }
        // Narrow range: many ties at every page edge.
        if r.below(100) >= 20 {
            set(&mut d.amount_valid, i);
        }
        if r.below(100) >= 10 {
            set(&mut d.owner_valid, i);
        }
        // Stage 4 ("other") is never used: an empty code must be skipped.
        d.stage.push(r.below(4) as u32);
        d.amount.push(r.below(41) as i32 - 20);
        d.score.push(r.next() as i32);
        d.owner.push(r.below(3) as u32);
    }
    // The extremes of the domain must sort correctly too.
    d.score[3] = i32::MIN;
    d.score[4] = i32::MAX;
    set(&mut d.live, 3);
    set(&mut d.live, 4);
    d
}

fn catalog() -> Catalog {
    let mut c = Catalog::new();
    let t = create_table("contacts")
        .width(8)
        .register(&mut c)
        .expect("register")
        .id;
    let fields = [
        (
            "lifecyclestage",
            Col(0),
            FieldKind::Code,
            None,
            Some(STAGES),
        ),
        ("amount", Col(1), FieldKind::I32, Some(Mask(1)), None),
        ("score", Col(2), FieldKind::I32, None, None),
        (
            "hubspot_owner_id",
            Col(3),
            FieldKind::Code,
            Some(Mask(2)),
            Some(&["ana", "ben", "cy"][..]),
        ),
    ];
    for (name, col, kind, validity, codes) in fields {
        c.declare(
            t,
            FieldSpec {
                name: name.into(),
                col,
                kind,
                validity,
                codebook: codes.map(|c| Codebook::new(c.iter().copied()).expect("codebook")),
            },
        )
        .expect("declare");
    }
    c
}

impl Data {
    fn run(&self, q: &Query, out: Out<'_>) -> Value {
        self.programs.set(self.programs.get() + 1);
        let masks: [&[u64]; 3] = [&self.live, &self.amount_valid, &self.owner_valid];
        let lanes = [
            LaneRef::U32(&self.stage),
            LaneRef::I32(&self.amount),
            LaneRef::I32(&self.score),
            LaneRef::U32(&self.owner),
        ];
        let planes = Planes {
            n_rows: N,
            masks: &masks,
            lanes: &lanes,
        };
        let program = lower(q).expect("lower");
        let mut scratch = Scratch::for_program(&program, N).expect("scratch");
        execute_into(&program, &planes, &Foreign::NONE, &mut scratch, out).expect("execute")
    }
}

impl Runner for Data {
    type Error = ();
    fn count(&mut self, q: &Query) -> Result<u64, ()> {
        match self.run(q, Out::None) {
            Value::Count(n) => Ok(n as u64),
            other => panic!("count gave {other:?}"),
        }
    }
    fn rows(&mut self, q: &Query) -> Result<Vec<usize>, ()> {
        let mut kept = vec![0u64; words_for(N)];
        self.run(q, Out::Mask(&mut kept));
        Ok(materialize_rows(&kept, N))
    }
    fn groups(&mut self, q: &Query, groups: u32) -> Result<Vec<i64>, ()> {
        let mut sink = vec![0i64; groups as usize];
        self.run(q, Out::I64(&mut sink));
        Ok(sink)
    }
    fn code_domain(&self, col: Col) -> u32 {
        match col {
            Col(0) => STAGES.len() as u32,
            Col(3) => 3,
            _ => panic!("not a code lane"),
        }
    }
    fn i32_at(&self, col: Col, row: usize) -> i32 {
        match col {
            Col(1) => self.amount[row],
            Col(2) => self.score[row],
            _ => panic!("not an i32 lane"),
        }
    }
}

/// The documented order, by a plain sort.
fn oracle(d: &Data, keep: &dyn Fn(usize) -> bool, sort: Option<&Sort>) -> Vec<usize> {
    let rows: Vec<usize> = (0..N).filter(|&i| get(&d.live, i) && keep(i)).collect();
    let Some(sort) = sort else { return rows };
    let key = |i: usize| -> Option<i64> {
        match sort.property.as_str() {
            "lifecyclestage" => Some(i64::from(d.stage[i])),
            "amount" => get(&d.amount_valid, i).then(|| i64::from(d.amount[i])),
            "score" => Some(i64::from(d.score[i])),
            "hubspot_owner_id" => get(&d.owner_valid, i).then(|| i64::from(d.owner[i])),
            p => panic!("{p}"),
        }
    };
    let (mut valued, nulls): (Vec<usize>, Vec<usize>) =
        rows.into_iter().partition(|&i| key(i).is_some());
    valued.sort_by(|&a, &b| {
        let (ka, kb) = (key(a).unwrap(), key(b).unwrap());
        match sort.direction {
            Direction::Ascending => ka.cmp(&kb),
            Direction::Descending => kb.cmp(&ka),
        }
        .then(a.cmp(&b))
    });
    valued.extend(nulls);
    valued
}

fn every_page(d: &mut Data, view: &SavedView, keep: &dyn Fn(usize) -> bool) {
    let cat = catalog();
    let want = oracle(d, keep, view.sort.as_ref());
    assert!(want.len() > 1_000, "the fixture must be non-trivial");
    for limit in [1usize, 7, 50, 333] {
        let mut offset = 0;
        while offset <= want.len() + limit {
            let got = page(view, &cat, offset, limit, d).expect("page");
            let end = (offset + limit).min(want.len());
            let expect = if offset < want.len() {
                &want[offset..end]
            } else {
                &[][..]
            };
            assert_eq!(got, expect, "{view:?} offset {offset} limit {limit}");
            offset += limit.max(97);
        }
    }
}

fn view(sort: Option<(&str, Direction)>, groups: Vec<FilterGroup>) -> SavedView {
    SavedView {
        object: "contacts".into(),
        groups,
        sort: sort.map(|(p, direction)| Sort {
            property: p.into(),
            direction,
        }),
        columns: vec!["email".into()],
    }
}

#[test]
fn storage_order_without_a_sort() {
    let mut d = data();
    every_page(&mut d, &view(None, vec![]), &|_| true);
}

#[test]
fn every_sort_key_both_directions_matches_a_plain_sort() {
    let mut d = data();
    for property in ["amount", "score", "lifecyclestage", "hubspot_owner_id"] {
        for direction in [Direction::Ascending, Direction::Descending] {
            every_page(&mut d, &view(Some((property, direction)), vec![]), &|_| {
                true
            });
        }
    }
}

#[test]
fn a_filtered_view_sorts_only_the_kept_records() {
    let mut d = data();
    let groups = vec![
        FilterGroup {
            filters: vec![PropertyFilter::new(
                "lifecyclestage",
                Operator::Eq,
                "customer",
            )],
        },
        FilterGroup {
            filters: vec![PropertyFilter::new("score", Operator::Gt, "0")],
        },
    ];
    let stage = d.stage.clone();
    let score = d.score.clone();
    let keep = move |i: usize| stage[i] == 2 || score[i] > 0;
    for direction in [Direction::Ascending, Direction::Descending] {
        every_page(
            &mut d,
            &view(Some(("amount", direction)), groups.clone()),
            &keep,
        );
    }
}

#[test]
fn a_page_costs_programs_independent_of_its_offset() {
    let mut d = data();
    let cat = catalog();
    let v = view(Some(("score", Direction::Ascending)), vec![]);
    let mut costs = Vec::new();
    for offset in [0usize, 1_000, 2_000] {
        d.programs.set(0);
        page(&v, &cat, offset, 25, &mut d).expect("page");
        costs.push(d.programs.get());
    }
    // count + two bisections (≤ 33 each) + before-count + band rows.
    assert!(costs.iter().all(|&c| c <= 1 + 2 * 33 + 2), "{costs:?}");
}

#[test]
fn an_unknown_sort_property_is_refused() {
    let mut d = data();
    let cat = catalog();
    let v = view(Some(("no_such", Direction::Ascending)), vec![]);
    assert_eq!(
        page(&v, &cat, 0, 10, &mut d),
        Err(hubspo_query::view::PageError::UnknownSortProperty(
            "no_such".into()
        ))
    );
}

//! D-QRY-2 against full recomputation: an active list maintained by
//! `list::refresh` over 40 rounds of updates, inserts and deletes must equal
//! the predicate evaluated from scratch after every round, and a static list
//! maintained by `add` / `remove` must equal a plain set.

use std::collections::BTreeSet;

use hubspo_query::filter::{FilterGroup, Operator, PropertyFilter, compile};
use hubspo_query::list;
use hubspo_store::catalog::{Catalog, Codebook, FieldSpec};
use lance_graph_mask_risc::{
    Foreign, LaneRef, Out, Planes, Scratch, execute_into, materialize_rows, words_for,
};
use lance_graph_quack::bind::{FieldKind, create_table};
use lance_graph_quack::{Agg, Col, Filter, Mask, Query, lower};

const CAP: usize = 4_000;
const STAGES: &[&str] = &["lead", "opportunity", "customer", "evangelist"];
// Planes: 0 live, 1 stored membership, 2 changed / named rows, 3 second list.
const LIVE: Mask = Mask(0);
const PREV: Mask = Mask(1);
const CHANGED: Mask = Mask(2);
const NEXT: Mask = Mask(3);

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

struct Table {
    live: Vec<bool>,
    stage: Vec<u32>,
    amount: Vec<i32>,
}

fn bits(rows: impl IntoIterator<Item = usize>) -> Vec<u64> {
    let mut w = vec![0u64; words_for(CAP)];
    for r in rows {
        w[r / 64] |= 1 << (r % 64);
    }
    w
}

fn catalog() -> Catalog {
    let mut c = Catalog::new();
    let t = create_table("contacts")
        .width(8)
        .register(&mut c)
        .expect("register")
        .id;
    c.declare(
        t,
        FieldSpec {
            name: "lifecyclestage".into(),
            col: Col(0),
            kind: FieldKind::Code,
            validity: None,
            codebook: Some(Codebook::new(STAGES.iter().copied()).expect("codebook")),
        },
    )
    .expect("stage");
    c.declare(
        t,
        FieldSpec {
            name: "amount".into(),
            col: Col(1),
            kind: FieldKind::I32,
            validity: None,
            codebook: None,
        },
    )
    .expect("amount");
    c
}

/// `stage ∈ {opportunity, customer} ∧ amount ≥ 2000`, or `stage = evangelist`.
fn predicate(cat: &Catalog) -> Filter {
    compile(
        "contacts",
        &[
            FilterGroup {
                filters: vec![
                    PropertyFilter::set(
                        "lifecyclestage",
                        Operator::In,
                        &["opportunity", "customer"],
                    ),
                    PropertyFilter::new("amount", Operator::Gte, "2000"),
                ],
            },
            FilterGroup {
                filters: vec![PropertyFilter::new(
                    "lifecyclestage",
                    Operator::Eq,
                    "evangelist",
                )],
            },
        ],
        cat,
    )
    .expect("compile")
}

fn oracle_member(t: &Table, row: usize) -> bool {
    t.live[row] && ((matches!(t.stage[row], 1 | 2) && t.amount[row] >= 2000) || t.stage[row] == 3)
}

/// Run `filter` over the table with planes `[live, p1, p2, p3]`; returns the
/// kept rows.
fn run(t: &Table, filter: Filter, p1: &[u64], p2: &[u64], p3: &[u64]) -> Vec<usize> {
    let live = bits((0..CAP).filter(|&r| t.live[r]));
    let masks: [&[u64]; 4] = [&live, p1, p2, p3];
    let lanes = [LaneRef::U32(&t.stage), LaneRef::I32(&t.amount)];
    let planes = Planes {
        n_rows: CAP,
        masks: &masks,
        lanes: &lanes,
    };
    let program = lower(&Query {
        filter,
        agg: Agg::Rows,
    })
    .expect("lower");
    let mut out = vec![0u64; words_for(CAP)];
    let mut scratch = Scratch::for_program(&program, CAP).expect("scratch");
    execute_into(
        &program,
        &planes,
        &Foreign::NONE,
        &mut scratch,
        Out::Mask(&mut out),
    )
    .expect("execute");
    materialize_rows(&out, CAP)
}

fn table(r: &mut Rng) -> Table {
    Table {
        live: (0..CAP).map(|i| i < 3_000).collect(),
        stage: (0..CAP).map(|_| r.below(STAGES.len()) as u32).collect(),
        amount: (0..CAP).map(|_| r.below(5_000) as i32).collect(),
    }
}

#[test]
fn an_active_list_refreshed_on_changes_equals_full_recomputation() {
    let mut r = Rng(0x0A_C71E);
    let cat = catalog();
    let pred = predicate(&cat);
    let mut t = table(&mut r);
    let none = bits([]);
    let mut stored = run(&t, pred.clone(), &none, &none, &none);
    let (mut joined, mut left) = (0usize, 0usize);
    for round in 0..40 {
        let mut changed = BTreeSet::new();
        for _ in 0..60 {
            let row = r.below(CAP);
            match r.below(10) {
                // update an existing record
                0..=6 if t.live[row] => {
                    t.stage[row] = r.below(STAGES.len()) as u32;
                    t.amount[row] = r.below(5_000) as i32;
                }
                // delete
                7 if t.live[row] => t.live[row] = false,
                // insert into a free slot
                8 | 9 if !t.live[row] => {
                    t.live[row] = true;
                    t.stage[row] = r.below(STAGES.len()) as u32;
                    t.amount[row] = r.below(5_000) as i32;
                }
                _ => continue,
            }
            changed.insert(row);
        }
        let prev = bits(stored.iter().copied());
        let ch = bits(changed.iter().copied());
        let next = run(&t, list::refresh(&pred, PREV, CHANGED), &prev, &ch, &none);
        let full = run(&t, pred.clone(), &none, &none, &none);
        let want: Vec<usize> = (0..CAP).filter(|&row| oracle_member(&t, row)).collect();
        assert_eq!(full, want, "round {round}: full evaluation vs plain loop");
        assert_eq!(next, full, "round {round}: incremental vs full");

        // History of this step.
        let next_bits = bits(next.iter().copied());
        let add = run(&t, list::added(PREV, NEXT), &prev, &none, &next_bits);
        let rem = run(&t, list::removed(PREV, NEXT), &prev, &none, &next_bits);
        let before: BTreeSet<usize> = stored.iter().copied().collect();
        let after: BTreeSet<usize> = next.iter().copied().collect();
        assert_eq!(add, after.difference(&before).copied().collect::<Vec<_>>());
        assert_eq!(rem, before.difference(&after).copied().collect::<Vec<_>>());
        joined += add.len();
        left += rem.len();
        stored = next;
    }
    // Anti-vacuity: membership really moved in both directions.
    assert!(joined > 100 && left > 100, "joined {joined}, left {left}");
}

#[test]
fn a_static_list_follows_adds_removes_and_deletes() {
    let mut r = Rng(0x57A7_1C);
    let mut t = table(&mut r);
    let none = bits([]);
    let mut want: BTreeSet<usize> = BTreeSet::new();
    let mut stored: Vec<usize> = Vec::new();
    for round in 0..30 {
        let named: BTreeSet<usize> = (0..40).map(|_| r.below(CAP)).collect();
        let prev = bits(stored.iter().copied());
        let rows = bits(named.iter().copied());
        stored = if round % 3 == 2 {
            want.retain(|row| !named.contains(row));
            run(&t, list::remove(PREV, CHANGED), &prev, &rows, &none)
        } else {
            want.extend(named.iter().copied().filter(|&row| t.live[row]));
            run(&t, list::add(PREV, CHANGED, LIVE), &prev, &rows, &none)
        };
        // Some members are deleted between requests.
        for _ in 0..5 {
            let row = r.below(CAP);
            t.live[row] = false;
        }
        let stored_bits = bits(stored.iter().copied());
        let members = run(&t, list::members(PREV, LIVE), &stored_bits, &none, &none);
        let expect: Vec<usize> = want.iter().copied().filter(|&row| t.live[row]).collect();
        assert_eq!(members, expect, "round {round}");
    }
    assert!(want.len() > 200, "the list must grow to mean something");
    assert!(
        want.iter().any(|&row| !t.live[row]),
        "some stored members must have been deleted"
    );
}

#[test]
fn refresh_reads_the_predicate_only_where_records_changed() {
    // Stored bits that disagree with the predicate on purpose: every even
    // row. Only rows 0..100 are reported changed, so only they may be
    // re-evaluated; a refresh that re-reads the whole table would rewrite
    // the rest too.
    let mut r = Rng(0x1C_4A6E);
    let t = table(&mut r);
    let pred = predicate(&catalog());
    let none = bits([]);
    let stored: Vec<usize> = (0..CAP).filter(|row| row % 2 == 0).collect();
    let prev = bits(stored.iter().copied());
    let ch = bits(0..100);
    let next = run(&t, list::refresh(&pred, PREV, CHANGED), &prev, &ch, &none);
    let want: Vec<usize> = (0..CAP)
        .filter(|&row| {
            if row < 100 {
                oracle_member(&t, row)
            } else {
                row % 2 == 0
            }
        })
        .collect();
    assert_eq!(next, want);
    // Anti-vacuity: outside the changed rows the stored bits really differ
    // from the predicate, or keeping them would prove nothing.
    let differ = (100..CAP)
        .filter(|&row| (row % 2 == 0) != oracle_member(&t, row))
        .count();
    assert!(differ > 1_000, "only {differ} rows differ");
}

//! End to end on a real `Table`: batches applied to the store feed the
//! filter compiler and the active-list refresh, and the refreshed list
//! equals a full recomputation after every batch.

use hubspo_query::filter::{FilterGroup, Operator, PropertyFilter, compile};
use hubspo_query::list;
use hubspo_store::catalog::{Catalog, Codebook, FieldSpec};
use hubspo_store::table::{Batch, LaneKind, LaneSpec, Table, Value};
use lance_graph_mask_risc::{
    Foreign, Out, Planes, Scratch, execute_into, materialize_rows, words_for,
};
use lance_graph_quack::bind::{FieldKind, create_table};
use lance_graph_quack::{Agg, Col, Filter, Mask, Query, lower};

const OWNER: u32 = 3;
const STAGES: &[&str] = &["lead", "opportunity", "customer"];

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

/// Run `filter` over the table's planes plus two extra planes (stored list,
/// changed rows) appended after the table's own.
fn run(t: &Table, filter: &Filter, extra: [&[u64]; 2]) -> Vec<usize> {
    let (mut masks, lanes) = t.planes();
    masks.extend(extra);
    let planes = Planes {
        n_rows: t.n_rows(),
        masks: &masks,
        lanes: &lanes,
    };
    let program = lower(&Query {
        filter: filter.clone(),
        agg: Agg::Rows,
    })
    .expect("lower");
    let mut out = vec![0u64; words_for(t.n_rows())];
    let mut scratch = Scratch::for_program(&program, t.n_rows()).expect("scratch");
    execute_into(
        &program,
        &planes,
        &Foreign::NONE,
        &mut scratch,
        Out::Mask(&mut out),
    )
    .expect("execute");
    materialize_rows(&out, t.n_rows())
}

fn bits(n: usize, rows: &[usize]) -> Vec<u64> {
    let mut w = vec![0u64; words_for(n)];
    for &r in rows {
        w[r / 64] |= 1 << (r % 64);
    }
    w
}

#[test]
fn a_list_on_the_store_follows_its_batches() {
    let mut t = Table::new(
        OWNER,
        &[
            LaneSpec {
                kind: LaneKind::U32,
                nullable: false,
            },
            LaneSpec {
                kind: LaneKind::I32,
                nullable: true,
            },
        ],
    )
    .expect("table");
    let mut cat = Catalog::new();
    let id = create_table("deals")
        .width(8)
        .register(&mut cat)
        .expect("register")
        .id;
    cat.declare(
        id,
        FieldSpec {
            name: "dealstage".into(),
            col: Col(0),
            kind: FieldKind::Code,
            validity: t.validity(Col(0)),
            codebook: Some(Codebook::new(STAGES.iter().copied()).expect("codebook")),
        },
    )
    .expect("stage");
    cat.declare(
        id,
        FieldSpec {
            name: "amount".into(),
            col: Col(1),
            kind: FieldKind::I32,
            validity: t.validity(Col(1)),
            codebook: None,
        },
    )
    .expect("amount");
    // The table owns planes 0 (live) and 1 (amount validity); the list and
    // the changed rows go after them.
    let (prev, changed) = (Mask(2), Mask(3));
    let pred = compile(
        "deals",
        &[FilterGroup {
            filters: vec![
                PropertyFilter::new("dealstage", Operator::Neq, "lead"),
                PropertyFilter::new("amount", Operator::Gt, "5000"),
            ],
        }],
        &cat,
    )
    .expect("compile");

    let mut r = Rng(0x0005_7011);
    let mut stored: Vec<usize> = Vec::new();
    let (mut joined, mut left) = (0usize, 0usize);
    for round in 0..50 {
        let mut b = Batch {
            owner: OWNER,
            ..Batch::default()
        };
        for _ in 0..r.below(30) {
            b.inserts.push(vec![
                Some(Value::U32(r.below(3) as u32)),
                (r.below(4) > 0).then(|| Value::I32(r.below(10_000) as i32)),
            ]);
        }
        let mut touched = std::collections::BTreeSet::new();
        for _ in 0..r.below(25) {
            if t.n_rows() == 0 {
                break;
            }
            let row = r.below(t.n_rows() as u64) as u32;
            if !t.is_live(row) || !touched.insert(row) {
                continue;
            }
            match r.below(4) {
                0 => b.deletes.push(row),
                1 => b
                    .updates
                    .push((row, Col(0), Some(Value::U32(r.below(3) as u32)))),
                _ => b.updates.push((
                    row,
                    Col(1),
                    (r.below(4) > 0).then(|| Value::I32(r.below(10_000) as i32)),
                )),
            }
        }
        let applied = t.apply(&b).expect("apply");
        let n = t.n_rows();
        let prev_bits = bits(n, &stored);
        let next = run(
            &t,
            &list::refresh(&pred, prev, changed),
            [&prev_bits, &applied.changed],
        );
        let full = run(&t, &pred, [&prev_bits, &applied.changed]);
        assert_eq!(next, full, "round {round}");
        let before: std::collections::BTreeSet<_> = stored.iter().copied().collect();
        let after: std::collections::BTreeSet<_> = next.iter().copied().collect();
        joined += after.difference(&before).count();
        left += before.difference(&after).count();
        stored = next;
    }
    assert!(joined > 50 && left > 20, "joined {joined}, left {left}");
}

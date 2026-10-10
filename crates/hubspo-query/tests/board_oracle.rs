//! D-QRY-3 board half against a plain loop: per (pipeline, stage), the
//! grouped programs' counts and amount sums equal a row-by-row tally over
//! the same filtered records, with "no amount" kept apart from "zero".

use std::collections::BTreeMap;

use hubspo_query::board::{Board, BoardError, Column, weighted};
use hubspo_query::filter::{FilterGroup, Operator, PropertyFilter, compile};
use hubspo_store::catalog::{Catalog, Codebook, FieldSpec};
use lance_graph_mask_risc::{Foreign, LaneRef, Out, Planes, Scratch, execute_into, words_for};
use lance_graph_quack::bind::{FieldKind, create_table};
use lance_graph_quack::{Col, Filter, Mask, Query, lower};

const N: usize = 6_000;
const PIPELINES: u32 = 3;
const STAGES: u32 = 7;
const OWNERS: &[&str] = &["ana", "ben", "cy", "dee"];

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

struct Deals {
    live: Vec<bool>,
    pipeline: Vec<u32>,
    stage: Vec<u32>,
    owner: Vec<u32>,
    amount: Vec<Option<i32>>,
    amount_lane: Vec<i32>,
}

fn deals() -> Deals {
    let mut r = Rng(0x0B0A_4D00);
    let mut d = Deals {
        live: Vec::new(),
        pipeline: Vec::new(),
        stage: Vec::new(),
        owner: Vec::new(),
        amount: Vec::new(),
        amount_lane: Vec::new(),
    };
    for _ in 0..N {
        let pipeline = r.below(u64::from(PIPELINES)) as u32;
        let stage = r.below(u64::from(STAGES)) as u32;
        d.live.push(r.below(100) >= 4);
        d.pipeline.push(pipeline);
        d.stage.push(stage);
        d.owner.push(r.below(OWNERS.len() as u64) as u32);
        // Pipeline 2, stage 6 never carries an amount: a stage with records
        // and no amount must read None, not 0. Elsewhere 25 % lack one.
        let has = !(pipeline == 2 && stage == 6) && r.below(100) >= 25;
        let raw = r.below(200_000) as i32 - 20_000;
        d.amount.push(has.then_some(raw));
        d.amount_lane.push(raw);
    }
    d
}

fn bits(f: impl Fn(usize) -> bool) -> Vec<u64> {
    let mut w = vec![0u64; words_for(N)];
    for i in 0..N {
        if f(i) {
            w[i / 64] |= 1 << (i % 64);
        }
    }
    w
}

fn catalog() -> Catalog {
    let mut c = Catalog::new();
    let t = create_table("deals")
        .width(16)
        .register(&mut c)
        .expect("register")
        .id;
    c.declare(
        t,
        FieldSpec {
            name: "hubspot_owner_id".into(),
            col: Col(2),
            kind: FieldKind::Code,
            validity: None,
            codebook: Some(Codebook::new(OWNERS.iter().copied()).expect("codebook")),
        },
    )
    .expect("owner");
    c.declare(
        t,
        FieldSpec {
            name: "amount".into(),
            col: Col(3),
            kind: FieldKind::I32,
            validity: Some(Mask(1)),
            codebook: None,
        },
    )
    .expect("amount");
    c
}

const BOARD: Board = Board {
    pipeline: Col(0),
    stage: Col(1),
    pipelines: PIPELINES,
    stages: STAGES,
    amount: Some((Col(3), Some(Mask(1)))),
};

fn exec(d: &Deals, q: &Query, sink: &mut [i64]) {
    let live = bits(|i| d.live[i]);
    let valid = bits(|i| d.amount[i].is_some());
    let masks: [&[u64]; 2] = [&live, &valid];
    let lanes = [
        LaneRef::U32(&d.pipeline),
        LaneRef::U32(&d.stage),
        LaneRef::U32(&d.owner),
        LaneRef::I32(&d.amount_lane),
    ];
    let planes = Planes {
        n_rows: N,
        masks: &masks,
        lanes: &lanes,
    };
    let program = lower(q).expect("lower");
    let mut scratch = Scratch::for_program(&program, N).expect("scratch");
    execute_into(
        &program,
        &planes,
        &Foreign::NONE,
        &mut scratch,
        Out::I64(sink),
    )
    .expect("execute");
}

fn board(d: &Deals, filter: &Filter) -> Vec<Column> {
    let groups = BOARD.groups().expect("groups") as usize;
    let mut counts = vec![0i64; groups];
    let mut amounts = vec![0i64; groups];
    exec(d, &BOARD.count_query(filter), &mut counts);
    exec(
        d,
        &BOARD.amount_query(filter).expect("amount"),
        &mut amounts,
    );
    BOARD.columns(&mut counts, Some(&mut amounts))
}

fn tally(d: &Deals, keep: impl Fn(usize) -> bool) -> Vec<Column> {
    let mut cells: BTreeMap<(u32, u32), (u64, Option<i64>)> = BTreeMap::new();
    for i in (0..N).filter(|&i| d.live[i] && keep(i)) {
        let cell = cells
            .entry((d.pipeline[i], d.stage[i]))
            .or_insert((0, None));
        cell.0 += 1;
        if let Some(a) = d.amount[i] {
            cell.1 = Some(cell.1.unwrap_or(0) + i64::from(a));
        }
    }
    cells
        .into_iter()
        .map(|((pipeline, stage), (count, amount))| Column {
            pipeline,
            stage,
            count,
            amount,
        })
        .collect()
}

#[test]
fn every_stage_matches_a_plain_tally() {
    let d = deals();
    let cat = catalog();
    let all = compile("deals", &[], &cat).expect("all");
    let got = board(&d, &all);
    let want = tally(&d, |_| true);
    assert_eq!(got, want);
    assert_eq!(
        got.len(),
        (PIPELINES * STAGES) as usize,
        "every stage is occupied"
    );
    let empty_sum = got
        .iter()
        .find(|c| c.pipeline == 2 && c.stage == 6)
        .expect("stage 2/6");
    assert!(
        empty_sum.count > 0 && empty_sum.amount.is_none(),
        "{empty_sum:?}"
    );
}

#[test]
fn a_filtered_board_matches_and_leaves_empty_stages_out() {
    let d = deals();
    let cat = catalog();
    let groups = [
        FilterGroup {
            filters: vec![
                PropertyFilter::new("hubspot_owner_id", Operator::Eq, "ben"),
                PropertyFilter::new("amount", Operator::Gte, "150000"),
            ],
        },
        FilterGroup {
            filters: vec![PropertyFilter::new("hubspot_owner_id", Operator::Eq, "dee")],
        },
    ];
    let filter = compile("deals", &groups, &cat).expect("compile");
    let got = board(&d, &filter);
    let want = tally(&d, |i| {
        (d.owner[i] == 1 && d.amount[i].is_some_and(|a| a >= 150_000)) || d.owner[i] == 3
    });
    assert_eq!(got, want);
    // Anti-vacuity: the filter must change both counts and sums.
    let all = tally(&d, |_| true);
    assert!(
        got.iter().map(|c| c.count).sum::<u64>() < all.iter().map(|c| c.count).sum::<u64>() / 2
    );
}

#[test]
fn a_board_needs_a_group_universe() {
    let mut b = BOARD;
    b.stages = 0;
    assert_eq!(b.groups(), Err(BoardError::GroupUniverse));
    b.stages = u32::MAX;
    assert_eq!(b.groups(), Err(BoardError::GroupUniverse));
}

#[test]
fn weighting_the_stage_total_equals_summing_weighted_records() {
    let d = deals();
    let cat = catalog();
    let all = compile("deals", &[], &cat).expect("all");
    let probability_bp = |stage: u32| -> u16 { (stage as u16 * 1_437) % 10_001 };
    for c in board(&d, &all) {
        let Some(total) = c.amount else { continue };
        let per_record: i128 = (0..N)
            .filter(|&i| d.live[i] && d.pipeline[i] == c.pipeline && d.stage[i] == c.stage)
            .filter_map(|i| d.amount[i])
            .map(|a| weighted(i64::from(a), probability_bp(c.stage)))
            .sum();
        assert_eq!(weighted(total, probability_bp(c.stage)), per_record);
    }
}

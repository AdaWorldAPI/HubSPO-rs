//! D-STO-1 resident half: batches land whole or not at all, one version
//! each, for the table's owner only; the lanes and planes always equal a
//! plain model of the records.

use hubspo_store::table::{Batch, LaneKind, LaneSpec, StoreError, Table, Value};
use lance_graph_mask_risc::LaneRef;
use lance_graph_quack::Col;

const OWNER: u32 = 7;
const SPECS: [LaneSpec; 3] = [
    LaneSpec {
        kind: LaneKind::U32,
        nullable: false,
    },
    LaneSpec {
        kind: LaneKind::I32,
        nullable: true,
    },
    LaneSpec {
        kind: LaneKind::U32,
        nullable: true,
    },
];

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

type Record = [Option<Value>; 3];

fn record(r: &mut Rng) -> Record {
    [
        Some(Value::U32(r.below(5) as u32)),
        (r.below(4) > 0).then(|| Value::I32(r.below(10_000) as i32 - 100)),
        (r.below(3) > 0).then(|| Value::U32(r.below(1_000) as u32)),
    ]
}

/// What the table must show for `model`: per row, live and per-lane value.
fn assert_matches(t: &Table, model: &[Option<Record>], last: &[Option<Record>]) {
    assert_eq!(t.n_rows(), model.len());
    let (masks, lanes) = t.planes();
    let bit = |plane: &[u64], row: usize| plane[row / 64] >> (row % 64) & 1 == 1;
    for (row, rec) in model.iter().enumerate() {
        assert_eq!(bit(masks[0], row), rec.is_some(), "live {row}");
        // A deleted row keeps its last values in place, inert.
        let shown = rec
            .as_ref()
            .or(last[row].as_ref())
            .expect("row was inserted");
        for (k, want) in shown.iter().enumerate() {
            let valid = match t.validity(Col(k as u16)) {
                Some(m) => bit(masks[usize::from(m.0)], row),
                None => true,
            };
            assert_eq!(valid, want.is_some(), "validity {row}/{k}");
            if let Some(v) = want {
                let got = match lanes[k] {
                    LaneRef::U32(l) => Value::U32(l[row]),
                    LaneRef::I32(l) => Value::I32(l[row]),
                    _ => unreachable!(),
                };
                assert_eq!(got, *v, "value {row}/{k}");
            }
        }
    }
}

#[test]
fn random_batches_match_a_plain_model() {
    let mut r = Rng(0x0000_57E0);
    let mut t = Table::new(OWNER, &SPECS).expect("table");
    let mut model: Vec<Option<Record>> = Vec::new();
    let mut last: Vec<Option<Record>> = Vec::new();
    let (mut updates, mut deletes) = (0, 0);
    for round in 1..=60u64 {
        let mut b = Batch {
            owner: OWNER,
            ..Batch::default()
        };
        let mut touched = std::collections::BTreeSet::new();
        for _ in 0..r.below(20) {
            b.inserts.push(record(&mut r).to_vec());
        }
        let live: Vec<usize> = (0..model.len()).filter(|&i| model[i].is_some()).collect();
        for _ in 0..r.below(15) {
            if live.is_empty() {
                break;
            }
            let row = live[r.below(live.len() as u64) as usize];
            if !touched.insert(row) {
                continue;
            }
            if r.below(5) == 0 {
                b.deletes.push(row as u32);
            } else {
                let k = r.below(3) as usize;
                // `record` never leaves lane 0 empty, so only the nullable
                // lanes are ever cleared.
                let v = record(&mut r)[k];
                b.updates.push((row as u32, Col(k as u16), v));
            }
        }
        let first = model.len();
        let applied = t.apply(&b).expect("apply");
        assert_eq!(applied.version, round);
        assert_eq!(applied.first_insert as usize, first);
        for rec in &b.inserts {
            let rec: Record = [rec[0], rec[1], rec[2]];
            model.push(Some(rec));
            last.push(Some(rec));
        }
        for &(row, col, v) in &b.updates {
            let rec = model[row as usize].as_mut().expect("live");
            rec[usize::from(col.0)] = v;
            last[row as usize] = Some(*rec);
            updates += 1;
        }
        for &row in &b.deletes {
            model[row as usize] = None;
            deletes += 1;
        }
        let want_changed: Vec<usize> = (first..model.len())
            .chain(b.updates.iter().map(|u| u.0 as usize))
            .chain(b.deletes.iter().map(|&d| d as usize))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        assert_eq!(
            lance_graph_mask_risc::materialize_rows(&applied.changed, model.len()),
            want_changed,
            "changed rows, round {round}"
        );
        assert_matches(&t, &model, &last);
    }
    assert!(
        updates > 200 && deletes > 30,
        "{updates} updates, {deletes} deletes"
    );
}

#[test]
fn refused_batches_change_nothing() {
    let mut t = Table::new(OWNER, &SPECS).expect("table");
    let mut r = Rng(0x00BA_D0B0);
    let seed = Batch {
        owner: OWNER,
        inserts: (0..10).map(|_| record(&mut r).to_vec()).collect(),
        ..Batch::default()
    };
    t.apply(&seed).expect("seed");
    t.apply(&Batch {
        owner: OWNER,
        deletes: vec![9],
        ..Batch::default()
    })
    .expect("delete 9");
    let model: Vec<Option<Record>> = seed
        .inserts
        .iter()
        .enumerate()
        .map(|(i, rec)| (i != 9).then(|| [rec[0], rec[1], rec[2]]))
        .collect();
    let last: Vec<Option<Record>> = seed
        .inserts
        .iter()
        .map(|rec| Some([rec[0], rec[1], rec[2]]))
        .collect();
    let good = record(&mut r).to_vec();
    let base = |b: Batch| Batch { owner: OWNER, ..b };
    let cases: Vec<(Batch, StoreError)> = vec![
        (
            Batch {
                owner: OWNER + 1,
                inserts: vec![good.clone()],
                ..Batch::default()
            },
            StoreError::OwnerMismatch {
                table: OWNER,
                batch: OWNER + 1,
            },
        ),
        (
            base(Batch {
                inserts: vec![good.clone(), good[..2].to_vec()],
                ..Batch::default()
            }),
            StoreError::WrongArity {
                expected: 3,
                got: 2,
            },
        ),
        (
            base(Batch {
                inserts: vec![vec![Some(Value::I32(1)), None, None]],
                ..Batch::default()
            }),
            StoreError::KindMismatch(Col(0)),
        ),
        (
            base(Batch {
                inserts: vec![vec![None, None, None]],
                ..Batch::default()
            }),
            StoreError::Required(Col(0)),
        ),
        (
            base(Batch {
                updates: vec![(0, Col(0), None)],
                ..Batch::default()
            }),
            StoreError::Required(Col(0)),
        ),
        (
            base(Batch {
                updates: vec![(0, Col(3), Some(Value::U32(1)))],
                ..Batch::default()
            }),
            StoreError::UnknownLane(Col(3)),
        ),
        (
            base(Batch {
                updates: vec![(9, Col(1), Some(Value::I32(1)))],
                ..Batch::default()
            }),
            StoreError::RowNotLive(9),
        ),
        (
            base(Batch {
                updates: vec![(40, Col(1), Some(Value::I32(1)))],
                ..Batch::default()
            }),
            StoreError::RowNotLive(40),
        ),
        (
            base(Batch {
                deletes: vec![9],
                ..Batch::default()
            }),
            StoreError::RowNotLive(9),
        ),
        (
            base(Batch {
                updates: vec![
                    (2, Col(1), Some(Value::I32(1))),
                    (2, Col(1), Some(Value::I32(2))),
                ],
                ..Batch::default()
            }),
            StoreError::Conflict(2),
        ),
        (
            base(Batch {
                updates: vec![(3, Col(1), Some(Value::I32(1)))],
                deletes: vec![3],
                ..Batch::default()
            }),
            StoreError::Conflict(3),
        ),
        (
            base(Batch {
                deletes: vec![4, 4],
                ..Batch::default()
            }),
            StoreError::Conflict(4),
        ),
    ];
    for (batch, err) in cases {
        // Every refused batch also carries a valid insert, so a partial
        // apply would show up as a new row.
        let mut batch = batch;
        if err
            != (StoreError::WrongArity {
                expected: 3,
                got: 2,
            })
        {
            batch.inserts.push(good.clone());
        }
        assert_eq!(t.apply(&batch), Err(err));
        assert_eq!(t.version(), 2, "{err:?} moved the version");
        assert_matches(&t, &model, &last);
    }
    // A batch that changes nothing is not a version.
    assert_eq!(
        t.apply(&base(Batch::default())),
        Err(StoreError::EmptyBatch)
    );
    assert_eq!(t.version(), 2);
    // Two updates of different lanes of one row are fine.
    assert!(
        t.apply(&base(Batch {
            updates: vec![(2, Col(1), Some(Value::I32(1))), (2, Col(2), None),],
            ..Batch::default()
        }))
        .is_ok()
    );
}

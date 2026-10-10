//! D-STO-1 persistence half: a table committed batch by batch to the Lance
//! log is rebuilt exactly by replay; the log is one Lance version per batch;
//! a second writer, a foreign owner and a refused batch are all caught.
#![cfg(feature = "lance")]

use hubspo_store::log::{Change, Entry, Log, LogError};
use hubspo_store::table::{Batch, LaneKind, LaneSpec, StoreError, Table, Value};
use lance_graph_mask_risc::LaneRef;
use lance_graph_quack::Col;

const OWNER: u32 = 11;
const SPECS: [LaneSpec; 2] = [
    LaneSpec {
        kind: LaneKind::U32,
        nullable: false,
    },
    LaneSpec {
        kind: LaneKind::I32,
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

/// Everything a reader can see of a table, for equality.
fn image(t: &Table) -> (u64, usize, Vec<Vec<u64>>, Vec<u32>, Vec<i32>) {
    let (masks, lanes) = t.planes();
    let u = match lanes[0] {
        LaneRef::U32(v) => v.to_vec(),
        _ => unreachable!(),
    };
    let i = match lanes[1] {
        LaneRef::I32(v) => v.to_vec(),
        _ => unreachable!(),
    };
    (
        t.version(),
        t.n_rows(),
        masks.iter().map(|m| m.to_vec()).collect(),
        u,
        i,
    )
}

fn random_batch(r: &mut Rng, t: &Table) -> Batch {
    let mut b = Batch {
        owner: OWNER,
        ..Batch::default()
    };
    for _ in 0..1 + r.below(10) {
        b.inserts.push(vec![
            Some(Value::U32(r.below(7) as u32)),
            (r.below(3) > 0).then(|| Value::I32(r.below(2_000) as i32 - 1_000)),
        ]);
    }
    let mut touched = std::collections::BTreeSet::new();
    for _ in 0..r.below(8) {
        if t.n_rows() == 0 {
            break;
        }
        let row = r.below(t.n_rows() as u64) as u32;
        if !t.is_live(row) || !touched.insert(row) {
            continue;
        }
        match r.below(3) {
            0 => b.deletes.push(row),
            1 => b
                .updates
                .push((row, Col(0), Some(Value::U32(r.below(7) as u32)))),
            // Negative values exercise the I32 bit pattern in the log.
            _ => b.updates.push((
                row,
                Col(1),
                (r.below(3) > 0).then(|| Value::I32(r.below(2_000) as i32 - 1_000)),
            )),
        }
    }
    b
}

#[tokio::test(flavor = "multi_thread")]
async fn replay_rebuilds_the_committed_table_exactly() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut log = Log::new(dir.path().join("deals.lance").to_str().expect("utf8"));
    let mut t = Table::new(OWNER, &SPECS).expect("table");
    let mut r = Rng(0x0000_10C1);
    for round in 1..=25u64 {
        let b = random_batch(&mut r, &t);
        let applied = log.commit(&mut t, &b).await.expect("commit");
        assert_eq!(applied.version, round);
    }
    let lance_version = lance::Dataset::open(dir.path().join("deals.lance").to_str().unwrap())
        .await
        .expect("open")
        .version()
        .version;
    assert_eq!(lance_version, 25, "one Lance version per batch");
    let back = log.replay(OWNER, &SPECS).await.expect("replay");
    assert_eq!(image(&back), image(&t));
    // Anti-vacuity: the run deleted rows and stored negative values.
    let (_, _, masks, _, amounts) = image(&t);
    assert!(masks[0].iter().map(|w| w.count_zeros()).sum::<u32>() > 0);
    assert!(amounts.iter().any(|a| *a < 0));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_empty_log_replays_to_an_empty_table() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut log = Log::new(dir.path().join("none.lance").to_str().expect("utf8"));
    let t = log.replay(OWNER, &SPECS).await.expect("replay");
    assert_eq!((t.version(), t.n_rows()), (0, 0));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_writer_is_caught_before_it_applies() {
    let dir = tempfile::tempdir().expect("tempdir");
    let uri = dir.path().join("deals.lance");
    let mut log = Log::new(uri.to_str().expect("utf8"));
    let mut r = Rng(0x0000_2A17);
    let mut a = Table::new(OWNER, &SPECS).expect("a");
    let first = random_batch(&mut r, &a);
    log.commit(&mut a, &first).await.expect("a commits");
    // `b` is a second in-memory table for the same log: it replays, then a
    // commits again, so b's view is one version behind.
    let mut b = log.replay(OWNER, &SPECS).await.expect("b replays");
    let second = random_batch(&mut r, &a);
    log.commit(&mut a, &second).await.expect("a commits again");
    let before = image(&b);
    let third = random_batch(&mut r, &b);
    match log.commit(&mut b, &third).await {
        Err(LogError::Diverged {
            expected: 1,
            found: 2,
        }) => {}
        other => panic!("expected Diverged {{1, 2}}, got {other:?}"),
    }
    assert_eq!(image(&b), before, "a diverged commit must not apply");
    let back = log.replay(OWNER, &SPECS).await.expect("replay");
    assert_eq!(image(&back), image(&a), "the log holds a's batches only");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_batch_writes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let uri = dir.path().join("deals.lance");
    let mut log = Log::new(uri.to_str().expect("utf8"));
    let mut t = Table::new(OWNER, &SPECS).expect("table");
    let mut r = Rng(0x0000_3E55);
    let ok = random_batch(&mut r, &t);
    log.commit(&mut t, &ok).await.expect("commit");
    let bad = Batch {
        owner: OWNER,
        inserts: vec![vec![None, None]],
        ..Batch::default()
    };
    match log.commit(&mut t, &bad).await {
        Err(LogError::Store(StoreError::Required(Col(0)))) => {}
        other => panic!("expected Required(0), got {other:?}"),
    }
    match log
        .commit(
            &mut t,
            &Batch {
                owner: OWNER,
                ..Batch::default()
            },
        )
        .await
    {
        Err(LogError::Store(StoreError::EmptyBatch)) => {}
        other => panic!("expected EmptyBatch, got {other:?}"),
    }
    let v = lance::Dataset::open(uri.to_str().unwrap())
        .await
        .expect("open")
        .version()
        .version;
    assert_eq!(v, 1, "refused batches must not reach the log");
}

#[tokio::test(flavor = "multi_thread")]
async fn replay_refuses_a_foreign_owner() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut log = Log::new(dir.path().join("deals.lance").to_str().expect("utf8"));
    let mut t = Table::new(OWNER, &SPECS).expect("table");
    let mut r = Rng(0x0000_4F0E);
    let b = random_batch(&mut r, &t);
    log.commit(&mut t, &b).await.expect("commit");
    match log.replay(OWNER + 1, &SPECS).await {
        Err(LogError::Owner { expected, found }) => {
            assert_eq!((expected, found), (OWNER + 1, OWNER));
        }
        other => panic!("expected Owner, got {:?}", other.map(|t| t.version())),
    }
}

/// D-STO-5: the history read back from the log equals the history written
/// down independently from the batches, for every record and both lanes.
#[tokio::test(flavor = "multi_thread")]
async fn history_is_every_write_of_one_property_in_order() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut log = Log::new(dir.path().join("deals.lance").to_str().expect("utf8"));
    let mut t = Table::new(OWNER, &SPECS).expect("table");
    let mut r = Rng(0x0000_5157);
    // Oracle: per (row, lane), the writes in order.
    let mut want: std::collections::BTreeMap<(u32, u16), Vec<Entry>> = Default::default();
    for _ in 0..30 {
        let b = random_batch(&mut r, &t);
        let first = t.n_rows() as u32;
        let applied = log.commit(&mut t, &b).await.expect("commit");
        let e = |change| Entry {
            version: applied.version,
            owner: OWNER,
            change,
        };
        for (row, record) in (first..).zip(&b.inserts) {
            for (lane, v) in (0u16..).zip(record) {
                want.entry((row, lane))
                    .or_default()
                    .push(e(Change::Set(*v)));
            }
        }
        for &(row, col, v) in &b.updates {
            want.entry((row, col.0))
                .or_default()
                .push(e(Change::Set(v)));
        }
        for &row in &b.deletes {
            for lane in 0..2 {
                want.entry((row, lane))
                    .or_default()
                    .push(e(Change::Deleted));
            }
        }
    }
    for row in 0..t.n_rows() as u32 {
        for lane in 0..2u16 {
            let got = log.history(&SPECS, row, Col(lane)).expect("history");
            let exp = want.get(&(row, lane)).cloned().unwrap_or_default();
            assert_eq!(got, exp, "row {row} lane {lane}");
        }
    }
    // Replayable: a second log replayed from disk holds the same records,
    // so every history read from it is identical.
    let mut again = Log::new(dir.path().join("deals.lance").to_str().expect("utf8"));
    let back = again.replay(OWNER, &SPECS).await.expect("replay");
    assert_eq!(again.len(), log.len());
    for row in 0..t.n_rows() as u32 {
        for lane in 0..2u16 {
            assert_eq!(
                again.history(&SPECS, row, Col(lane)).expect("again"),
                log.history(&SPECS, row, Col(lane)).expect("log"),
            );
        }
    }
    // A log whose records are not resident at the table's version refuses
    // to commit: its history would miss the batches before it.
    let mut cold = Log::new(dir.path().join("deals.lance").to_str().expect("utf8"));
    let mut t2 = back;
    let extra = random_batch(&mut r, &t2);
    match cold.commit(&mut t2, &extra).await {
        Err(LogError::Diverged {
            expected: 30,
            found: 0,
        }) => {}
        other => panic!("expected Diverged {{30, 0}}, got {other:?}"),
    }
    // A row that does not exist has no history.
    let none = log
        .history(&SPECS, t.n_rows() as u32 + 5, Col(0))
        .expect("history");
    assert!(none.is_empty());
    // Anti-vacuity: histories with updates, with a deletion, and with a
    // negative value were compared.
    let all: Vec<&Entry> = want.values().flatten().collect();
    assert!(want.values().any(|h| h.len() >= 3));
    assert!(all.iter().any(|e| e.change == Change::Deleted));
    assert!(
        all.iter()
            .any(|e| matches!(e.change, Change::Set(Some(Value::I32(x))) if x < 0))
    );
    assert!(all.iter().any(|e| e.change == Change::Set(None)));
}

#[tokio::test(flavor = "multi_thread")]
async fn history_refuses_an_unknown_lane_and_reads_an_empty_log() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut log = Log::new(dir.path().join("deals.lance").to_str().expect("utf8"));
    assert!(log.history(&SPECS, 0, Col(0)).expect("empty").is_empty());
    let mut t = Table::new(OWNER, &SPECS).expect("table");
    let mut r = Rng(0x0000_6A11);
    let b = random_batch(&mut r, &t);
    log.commit(&mut t, &b).await.expect("commit");
    match log.history(&SPECS, 0, Col(2)) {
        Err(LogError::Corrupt("lane")) => {}
        other => panic!("expected Corrupt(lane), got {other:?}"),
    }
}

/// Append one raw record to the log, bypassing `commit` (a corrupt writer).
async fn append_raw(uri: &str, version: u64, op: u32, row: u32, lane: u32, present: bool) {
    use arrow::array::{BooleanArray, RecordBatch, RecordBatchIterator, UInt32Array, UInt64Array};
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;
    let schema = Arc::new(Schema::new(vec![
        Field::new("version", DataType::UInt64, false),
        Field::new("owner", DataType::UInt32, false),
        Field::new("op", DataType::UInt32, false),
        Field::new("row", DataType::UInt32, false),
        Field::new("lane", DataType::UInt32, false),
        Field::new("present", DataType::Boolean, false),
        Field::new("value", DataType::UInt32, false),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(UInt64Array::from(vec![version])),
            Arc::new(UInt32Array::from(vec![OWNER])),
            Arc::new(UInt32Array::from(vec![op])),
            Arc::new(UInt32Array::from(vec![row])),
            Arc::new(UInt32Array::from(vec![lane])),
            Arc::new(BooleanArray::from(vec![present])),
            Arc::new(UInt32Array::from(vec![0u32])),
        ],
    )
    .expect("batch");
    let params = lance::dataset::WriteParams {
        mode: lance::dataset::WriteMode::Append,
        ..Default::default()
    };
    lance::Dataset::write(
        RecordBatchIterator::new(vec![Ok(batch)], schema),
        uri,
        Some(params),
    )
    .await
    .expect("append");
}

/// An absent value in a required lane is a state the table can never hold.
/// Replay refuses such a record, and a refused replay leaves the resident
/// records as they were, so history can never report it.
#[tokio::test(flavor = "multi_thread")]
async fn a_corrupt_record_never_becomes_resident() {
    let dir = tempfile::tempdir().expect("tempdir");
    let uri = dir.path().join("deals.lance");
    let uri = uri.to_str().expect("utf8");
    let mut log = Log::new(uri);
    let mut t = Table::new(OWNER, &SPECS).expect("table");
    let b = Batch {
        owner: OWNER,
        inserts: vec![vec![Some(Value::U32(1)), None]],
        ..Batch::default()
    };
    log.commit(&mut t, &b).await.expect("commit");
    let before = log.history(&SPECS, 0, Col(0)).expect("history");
    assert_eq!(
        before,
        vec![Entry {
            version: 1,
            owner: OWNER,
            change: Change::Set(Some(Value::U32(1))),
        }]
    );
    // The nullable lane's absent value is a valid history.
    assert_eq!(
        log.history(&SPECS, 0, Col(1)).expect("history"),
        vec![Entry {
            version: 1,
            owner: OWNER,
            change: Change::Set(None),
        }]
    );
    // SET row 0, lane 0 (required), absent — written around `commit`.
    append_raw(uri, 2, 1, 0, 0, false).await;
    assert!(
        log.replay(OWNER, &SPECS).await.is_err(),
        "replay refuses it"
    );
    assert_eq!(log.history(&SPECS, 0, Col(0)).expect("history"), before);
    assert_eq!(log.len(), 3, "the resident records are unchanged");
}

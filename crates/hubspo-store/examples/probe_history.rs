//! D-STO-5 timing: history of one property of one record, read from a log of
//! `VERSIONS` batches of `UPDATES` updates each over `ROWS` records, every
//! batch also writing the probed property. History folds over the resident
//! records; replay is the one Lance read, timed separately.
//!
//! `cargo run --release -p hubspo-store --features lance --example probe_history`

use std::time::Instant;

use hubspo_store::log::Log;
use hubspo_store::table::{Batch, LaneKind, LaneSpec, Table, Value};
use lance_graph_quack::Col;

const OWNER: u32 = 7;
const ROWS: u32 = 100_000;
const VERSIONS: u32 = 1_000;
const UPDATES: u32 = 1_000;
const PROBE_ROW: u32 = 4_242;
const SPECS: [LaneSpec; 1] = [LaneSpec {
    kind: LaneKind::U32,
    nullable: false,
}];

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let dir = tempfile::tempdir().expect("tempdir");
    let uri = dir.path().join("probe.lance");
    let mut log = Log::new(uri.to_str().expect("utf8"));
    let mut t = Table::new(OWNER, &SPECS).expect("table");
    let started = Instant::now();
    let seed = Batch {
        owner: OWNER,
        inserts: (0..ROWS).map(|i| vec![Some(Value::U32(i))]).collect(),
        ..Batch::default()
    };
    log.commit(&mut t, &seed).await.expect("seed");
    for v in 0..VERSIONS {
        let mut updates: Vec<_> = (0..UPDATES - 1)
            .map(|k| ((v * 7_919 + k * 97) % ROWS, Col(0), Some(Value::U32(v))))
            .filter(|(r, _, _)| *r != PROBE_ROW)
            .collect();
        updates.push((PROBE_ROW, Col(0), Some(Value::U32(v))));
        let b = Batch {
            owner: OWNER,
            updates,
            ..Batch::default()
        };
        log.commit(&mut t, &b).await.expect("commit");
    }
    println!(
        "wrote {} versions in {:.1} s",
        VERSIONS + 1,
        started.elapsed().as_secs_f64()
    );
    // One fragment per version: the log is never compacted.
    let uri = uri.to_str().expect("utf8");

    // Live clock: history folds over the resident records.
    let mut hist = Vec::new();
    let mut n = 0;
    for _ in 0..101 {
        let s = Instant::now();
        n = log
            .history(&SPECS, PROBE_ROW, Col(0))
            .expect("history")
            .len();
        hist.push(s.elapsed().as_secs_f64() * 1e3);
    }
    assert_eq!(n, VERSIONS as usize + 1, "one entry per version");
    let (h, hmin, hmax) = median(hist);
    println!(
        "history (resident, {} records): {n} entries, median {h:.3} ms ({hmin:.3}..{hmax:.3}), 101 reads",
        log.len()
    );

    // Recovery clock: replay reads the whole log once from Lance.
    let mut rep = Vec::new();
    for _ in 0..5 {
        let mut cold = Log::new(uri);
        let s = Instant::now();
        let back = cold.replay(OWNER, &SPECS).await.expect("replay");
        rep.push(s.elapsed().as_secs_f64() * 1e3);
        assert_eq!(back.version(), t.version());
        assert_eq!(cold.len(), log.len());
    }
    let (r, rmin, rmax) = median(rep);
    println!("replay (Lance, cold): median {r:.1} ms ({rmin:.1}..{rmax:.1}), 5 runs");
}

fn median(mut ms: Vec<f64>) -> (f64, f64, f64) {
    ms.sort_by(f64::total_cmp);
    (ms[ms.len() / 2], ms[0], ms[ms.len() - 1])
}

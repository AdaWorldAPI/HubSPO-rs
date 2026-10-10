//! D-STO-5 timing: history of one property of one record, read from a log of
//! `VERSIONS` batches of `UPDATES` updates each over `ROWS` records, every
//! batch also writing the probed property. No index on `row`.
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
    let log = Log::new(uri.to_str().expect("utf8"));
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
    measure(&log, uri.to_str().expect("utf8"), "fragmented").await;
}

fn median(mut ms: Vec<f64>) -> (f64, f64, f64) {
    ms.sort_by(f64::total_cmp);
    (ms[ms.len() / 2], ms[0], ms[ms.len() - 1])
}

/// Time `history`, and a raw scan of every column with no program, 7 times.
async fn measure(log: &Log, uri: &str, label: &str) {
    use futures::TryStreamExt;
    let mut hist = Vec::new();
    let mut scan = Vec::new();
    let mut n = 0;
    let mut records = 0;
    for _ in 0..7 {
        let s = Instant::now();
        n = log
            .history(&SPECS, PROBE_ROW, Col(0))
            .await
            .expect("history")
            .len();
        hist.push(s.elapsed().as_secs_f64() * 1e3);
        let s = Instant::now();
        let ds = lance::Dataset::open(uri).await.expect("open");
        let rows: usize = ds
            .scan()
            .try_into_stream()
            .await
            .expect("stream")
            .try_collect::<Vec<_>>()
            .await
            .expect("collect")
            .iter()
            .map(|b| b.num_rows())
            .sum();
        records = rows;
        scan.push(s.elapsed().as_secs_f64() * 1e3);
    }
    assert_eq!(n, VERSIONS as usize + 1, "one entry per version");
    let (h, hmin, hmax) = median(hist);
    let (r, rmin, rmax) = median(scan);
    println!(
        "{label}: {records} log records; history {n} entries, median {h:.1} ms ({hmin:.1}..{hmax:.1}); \
         raw scan alone median {r:.1} ms ({rmin:.1}..{rmax:.1}); 7 reads each"
    );
}

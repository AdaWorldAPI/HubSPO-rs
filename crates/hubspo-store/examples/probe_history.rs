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
        "wrote {} versions ({} records) in {:.1} s",
        VERSIONS + 1,
        ROWS + VERSIONS * UPDATES,
        started.elapsed().as_secs_f64()
    );
    let mut times = Vec::new();
    let mut n = 0;
    for _ in 0..7 {
        let s = Instant::now();
        n = log
            .history(&SPECS, PROBE_ROW, Col(0))
            .await
            .expect("history")
            .len();
        times.push(s.elapsed().as_secs_f64() * 1e3);
    }
    times.sort_by(f64::total_cmp);
    assert_eq!(n, VERSIONS as usize + 1, "one entry per version");
    println!(
        "history: {n} entries, median {:.1} ms (min {:.1}, max {:.1}) over 7 reads",
        times[3], times[0], times[6]
    );
}

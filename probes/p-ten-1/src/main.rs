//! Probe P-TEN-1 (plan §5.5, probe table): one Lance dataset per tenant ×
//! object type. Budget: open + first read of a tenant's dataset at 10k
//! tenants < 50 ms p99.
//!
//! Method: write T tenants, each one dataset of `ROWS` rows `(id, a)`. Then,
//! for `SAMPLE` tenants picked at random, time `open` + a first read (the
//! first 100 rows of `a`) — each tenant opened for the first time in the
//! process. Two arms: a fresh session per open (`Dataset::open`) and one
//! shared `Session` across tenants (`DatasetBuilder::with_session`). The OS
//! page cache is dropped before each arm when the process may do so, so the
//! reads come from disk.
//!
//! Run in release: `cargo run --release --manifest-path probes/p-ten-1/Cargo.toml`.
//! Optional arguments: comma-separated tenant counts (default `1000,10000`),
//! data directory (default: a temp dir).

use std::sync::Arc;
use std::time::Instant;

use arrow::array::{Int32Array, RecordBatch, RecordBatchIterator, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use futures::TryStreamExt;
use lance::Dataset;
use lance::dataset::builder::DatasetBuilder;
use lance::session::Session;

const ROWS: u64 = 1_000;
const SAMPLE: usize = 1_000;

fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::UInt64, false),
        Field::new("a", DataType::Int32, false),
    ]))
}

fn batch(tenant: u64) -> RecordBatch {
    RecordBatch::try_new(
        schema(),
        vec![
            Arc::new(UInt64Array::from_iter_values(0..ROWS)),
            Arc::new(Int32Array::from_iter_values(
                (0..ROWS).map(|i| ((i + tenant) % 1000) as i32),
            )),
        ],
    )
    .expect("batch")
}

fn drop_caches() -> bool {
    std::process::Command::new("sync").status().is_ok()
        && std::fs::write("/proc/sys/vm/drop_caches", "3").is_ok()
}

fn pct(sorted: &[f64], p: f64) -> f64 {
    sorted[((sorted.len() as f64 - 1.0) * p).round() as usize]
}

async fn first_read(ds: &Dataset, tenant: u64) {
    let mut scan = ds.scan();
    scan.project(&["a"])
        .expect("project")
        .limit(Some(100), None)
        .expect("limit");
    let batches: Vec<RecordBatch> = scan
        .try_into_stream()
        .await
        .expect("scan")
        .try_collect()
        .await
        .expect("collect");
    let rows: usize = batches.iter().map(RecordBatch::num_rows).sum();
    assert_eq!(rows, 100, "first read of tenant {tenant}");
    let a = batches[0]
        .column(0)
        .as_any()
        .downcast_ref::<Int32Array>()
        .expect("i32");
    assert_eq!(a.value(0), (tenant % 1000) as i32, "tenant {tenant} data");
}

fn sample(t: u64) -> Vec<u64> {
    let mut x = 0x5EED_0005u64 ^ t;
    let mut out = Vec::with_capacity(SAMPLE);
    let mut seen = std::collections::HashSet::new();
    while out.len() < SAMPLE.min(t as usize) {
        x = x
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let id = (x >> 11) % t;
        if seen.insert(id) {
            out.push(id);
        }
    }
    out
}

async fn arm(root: &str, picks: &[u64], shared: Option<Arc<Session>>) -> Vec<f64> {
    let mut times = Vec::with_capacity(picks.len());
    for &tenant in picks {
        let uri = format!("{root}/t{tenant}/contacts.lance");
        let t = Instant::now();
        let ds = match &shared {
            None => Dataset::open(&uri).await.expect("open"),
            Some(s) => DatasetBuilder::from_uri(&uri)
                .with_session(s.clone())
                .load()
                .await
                .expect("open shared"),
        };
        first_read(&ds, tenant).await;
        times.push(t.elapsed().as_secs_f64() * 1e3);
    }
    times.sort_by(f64::total_cmp);
    times
}

async fn probe(root: &str, tenants: u64) {
    let t = Instant::now();
    for tenant in 0..tenants {
        let uri = format!("{root}/t{tenant}/contacts.lance");
        if std::path::Path::new(&uri).exists() {
            continue;
        }
        let reader = RecordBatchIterator::new(vec![Ok(batch(tenant))], schema());
        Dataset::write(reader, &uri, None).await.expect("write");
    }
    let build_s = t.elapsed().as_secs_f64();
    let picks = sample(tenants);

    let cold = drop_caches();
    let fresh = arm(root, &picks, None).await;
    drop_caches();
    let shared = arm(root, &picks, Some(Arc::new(Session::default()))).await;

    println!(
        "{tenants:>7} | {build_s:>7.1} | {cold:>5} | {:>7.2} | {:>7.2} | {:>7.2} | {:>7.2} | {:>7.2} | {:>7.2}",
        pct(&fresh, 0.5),
        pct(&fresh, 0.99),
        fresh[fresh.len() - 1],
        pct(&shared, 0.5),
        pct(&shared, 0.99),
        shared[shared.len() - 1],
    );
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let counts: Vec<u64> = args
        .next()
        .unwrap_or_else(|| "1000,10000".to_string())
        .split(',')
        .map(|s| s.trim().parse().expect("count"))
        .collect();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = args
        .next()
        .unwrap_or_else(|| tmp.path().to_string_lossy().into_owned());
    println!("P-TEN-1: {ROWS} rows per tenant; open + first read of {SAMPLE} random tenants; ms");
    println!("tenants | build s | cold  | fresh p50 | p99 | max | shared p50 | p99 | max");
    for t in counts {
        probe(&root, t).await;
    }
}

//! Probe P-HIST-1 (plan §5.2, probe table): the history of one property on
//! one record over 1,000 Lance versions, read from the version log alone.
//! Budget: < 20 ms.
//!
//! Method, per dataset size N:
//!
//! 1. v0: write N rows `(id, a, b)` with stable row ids on.
//! 2. 1,000 versions, each updating 100 random rows `a = a + 1`. The target
//!    record is in every 10th batch, so it changes exactly 100 times.
//! 3. Time two readers of the target's `a` history:
//!    - time travel: check out every version and take the row (one read per
//!      version);
//!    - change chain: take the row with `_row_last_updated_at_version`, check
//!      out the version before that change, repeat until v0 (one read per
//!      change to this record, not per version).
//!
//!    - address index: the change chain's `(version, row address)` pairs,
//!      kept as a per-record index (plan §5.2 fallback: an index, not a
//!      second truth). A read is a physical-slice read of that address in
//!      the current dataset (fragment files are immutable and the slice
//!      ignores deletions); only when the fragment has left the manifest does
//!      it check out the version the copy was live in.
//!
//! All arms must return the same 101 values (initial + 100 changes).
//!
//! Run in release: `cargo run --release --manifest-path probes/p-hist-1/Cargo.toml`.
//! Optional argument: a comma-separated list of sizes (default `100000`).

use std::sync::Arc;
use std::time::Instant;

use arrow::array::{Array, Int32Array, RecordBatch, RecordBatchIterator, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use futures::{StreamExt, TryStreamExt};
use lance::Dataset;
use lance::dataset::{ProjectionRequest, UpdateBuilder, WriteParams};

const VERSIONS: u64 = 1_000;
const BATCH: usize = 100;
const TARGET_EVERY: u64 = 10;
const RUNS: usize = 3;
const CONCURRENCY: usize = 32;
const LAST_UPDATED: &str = "_row_last_updated_at_version";

fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::UInt64, false),
        Field::new("a", DataType::Int32, false),
        Field::new("b", DataType::Int32, false),
    ]))
}

fn reader(
    n: u64,
) -> RecordBatchIterator<std::vec::IntoIter<Result<RecordBatch, arrow::error::ArrowError>>> {
    let chunk = 65_536u64;
    let batches: Vec<_> = (0..n)
        .step_by(chunk as usize)
        .map(|s| {
            let ids = s..(s + chunk).min(n);
            let id = UInt64Array::from_iter_values(ids.clone());
            let a = Int32Array::from_iter_values(ids.clone().map(|i| (i % 1000) as i32));
            let b = Int32Array::from_iter_values(ids.map(|i| (i % 7) as i32));
            Ok(
                RecordBatch::try_new(schema(), vec![Arc::new(id), Arc::new(a), Arc::new(b)])
                    .expect("batch"),
            )
        })
        .collect();
    RecordBatchIterator::new(batches.into_iter(), schema())
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn col_i32(b: &RecordBatch, name: &str) -> i32 {
    b.column_by_name(name)
        .expect(name)
        .as_any()
        .downcast_ref::<Int32Array>()
        .expect("i32")
        .value(0)
}

fn col_u64(b: &RecordBatch, name: &str) -> u64 {
    b.column_by_name(name)
        .expect(name)
        .as_any()
        .downcast_ref::<UInt64Array>()
        .expect("u64")
        .value(0)
}

/// One read per version.
async fn time_travel(ds: &Dataset, v0: u64, last: u64, row: u64) -> Vec<i32> {
    let mut out: Vec<i32> = Vec::new();
    for v in v0..=last {
        let at = ds.checkout_version(v).await.expect("checkout");
        let b = at
            .take_rows(&[row], ProjectionRequest::from_columns(["a"], at.schema()))
            .await
            .expect("take");
        let a = col_i32(&b, "a");
        if out.last() != Some(&a) {
            out.push(a);
        }
    }
    out
}

/// One read per change to this record. Also returns the `(version, row
/// address)` of every copy it read, oldest first.
async fn change_chain(ds: &Dataset, v0: u64, row: u64) -> (Vec<i32>, Vec<(u64, u64)>) {
    let mut out = Vec::new();
    let mut addrs = Vec::new();
    let mut at = ds.clone();
    loop {
        let b = Arc::new(at.clone())
            .take_builder(
                &[row],
                ProjectionRequest::from_columns(["a", LAST_UPDATED], at.schema()),
            )
            .expect("builder")
            .with_row_address(true)
            .execute()
            .await
            .expect("take");
        out.push(col_i32(&b, "a"));
        addrs.push((at.version().version, col_u64(&b, "_rowaddr")));
        let changed_at = col_u64(&b, LAST_UPDATED);
        if changed_at <= v0 {
            break;
        }
        at = ds.checkout_version(changed_at - 1).await.expect("checkout");
    }
    out.reverse();
    addrs.reverse();
    (out, addrs)
}

/// One physical-slice read per indexed change. Returns the values and how
/// many reads had to check out an older version.
async fn address_index(ds: &Dataset, index: &[(u64, u64)]) -> (Vec<i32>, usize) {
    let mut out = Vec::with_capacity(index.len());
    let mut checkouts = 0;
    let projection = ds.schema().project(&["a"]).expect("project");
    for &(v, addr) in index {
        let (frag, offset) = ((addr >> 32) as usize, addr & 0xFFFF_FFFF);
        let fragment = match ds.get_fragment(frag) {
            Some(f) => f,
            None => {
                checkouts += 1;
                ds.checkout_version(v)
                    .await
                    .expect("checkout")
                    .get_fragment(frag)
                    .expect("fragment live at its version")
            }
        };
        let batches: Vec<RecordBatch> = fragment
            .read_physical_slice(offset..offset + 1, &projection, 1)
            .await
            .expect("slice")
            .buffered(1)
            .try_collect()
            .await
            .expect("read");
        out.push(col_i32(&batches[0], "a"));
    }
    (out, checkouts)
}

/// The same reads as [`address_index`], issued concurrently. Every entry is
/// independent, so the reads overlap their I/O.
async fn address_index_concurrent(ds: &Dataset, index: &[(u64, u64)], width: usize) -> Vec<i32> {
    let projection = ds.schema().project(&["a"]).expect("project");
    futures::stream::iter(index.iter().copied())
        .map(|(v, addr)| {
            let projection = projection.clone();
            async move {
                let (frag, offset) = ((addr >> 32) as usize, addr & 0xFFFF_FFFF);
                let fragment = match ds.get_fragment(frag) {
                    Some(f) => f,
                    None => ds
                        .checkout_version(v)
                        .await
                        .expect("checkout")
                        .get_fragment(frag)
                        .expect("fragment live at its version"),
                };
                let batches: Vec<RecordBatch> = fragment
                    .read_physical_slice(offset..offset + 1, &projection, 1)
                    .await
                    .expect("slice")
                    .buffered(1)
                    .try_collect()
                    .await
                    .expect("read");
                col_i32(&batches[0], "a")
            }
        })
        .buffered(width)
        .collect()
        .await
}

async fn probe(n: u64) {
    let dir = tempfile::tempdir().expect("tempdir");
    let uri = dir.path().join("p_hist_1.lance");
    let uri = uri.to_str().expect("utf8 path");
    let params = WriteParams {
        enable_stable_row_ids: true,
        ..WriteParams::default()
    };
    let mut ds = Dataset::write(reader(n), uri, Some(params))
        .await
        .expect("write v0");
    let v0 = ds.version().version;
    let target: u64 = n / 2;

    let t = Instant::now();
    let mut x = 0x5EED_0003u64;
    for k in 1..=VERSIONS {
        let mut ids = Vec::with_capacity(BATCH);
        if k % TARGET_EVERY == 0 {
            ids.push(target);
        }
        while ids.len() < BATCH {
            x = x
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let id = (x >> 11) % n;
            if id != target && !ids.contains(&id) {
                ids.push(id);
            }
        }
        let list: Vec<String> = ids.iter().map(u64::to_string).collect();
        let next = UpdateBuilder::new(Arc::new(ds))
            .update_where(&format!("id IN ({})", list.join(",")))
            .expect("where")
            .set("a", "a + 1")
            .expect("set")
            .build()
            .expect("build")
            .execute()
            .await
            .expect("update")
            .new_dataset;
        ds = (*next).clone();
    }
    let build_s = t.elapsed().as_secs_f64();
    let last = ds.version().version;
    let fragments = ds.get_fragments().len();

    // Resolve the target's stable row id once (setup, not timed).
    let mut scan = ds.scan();
    scan.filter(&format!("id = {target}"))
        .expect("filter")
        .with_row_id()
        .project(&["id"])
        .expect("project");
    let found = scan.try_into_batch().await.expect("find target");
    assert_eq!(found.num_rows(), 1, "target row");
    let row = col_u64(&found, "_rowid");

    let initial = (target % 1000) as i32;
    let expected: Vec<i32> = (0..=(VERSIONS / TARGET_EVERY) as i32)
        .map(|d| initial + d)
        .collect();

    let (mut t_tt, mut t_cc, mut t_ai, mut t_ac) = (vec![], vec![], vec![], vec![]);
    let mut index = Vec::new();
    let mut fallbacks = 0;
    for _ in 0..RUNS {
        let t = Instant::now();
        let h = change_chain(&ds, v0, row).await;
        t_cc.push(t.elapsed().as_secs_f64() * 1e3);
        assert_eq!(h.0, expected, "change chain history");
        index = h.1;

        let t = Instant::now();
        let h = address_index(&ds, &index).await;
        t_ai.push(t.elapsed().as_secs_f64() * 1e3);
        assert_eq!(h.0, expected, "address index history");
        fallbacks = h.1;

        let t = Instant::now();
        let h = address_index_concurrent(&ds, &index, CONCURRENCY).await;
        t_ac.push(t.elapsed().as_secs_f64() * 1e3);
        assert_eq!(h, expected, "concurrent address index history");

        let t = Instant::now();
        let h = time_travel(&ds, v0, last, row).await;
        t_tt.push(t.elapsed().as_secs_f64() * 1e3);
        assert_eq!(h, expected, "time travel history");
    }

    // Split one chain step into its parts, at an early and the latest version.
    for v in [v0 + 1, last / 2, last] {
        let t = Instant::now();
        let at = ds.checkout_version(v).await.expect("checkout");
        let c = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let _ = at
            .take_rows(
                &[row],
                ProjectionRequest::from_columns(["a", LAST_UPDATED], at.schema()),
            )
            .await
            .expect("take");
        let k = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let _ = at
            .take_rows(
                &[row],
                ProjectionRequest::from_columns(["a", LAST_UPDATED], at.schema()),
            )
            .await
            .expect("take");
        let k2 = t.elapsed().as_secs_f64() * 1e3;
        eprintln!(
            "split v{v}: {} frags, checkout {c:.2} ms, first take {k:.2} ms, second take {k2:.2} ms",
            at.get_fragments().len()
        );
    }

    println!(
        "{n:>9} | {:>8} | {fragments:>5} | {build_s:>7.1} | {:>11.2} | {:>12.2} | {:>13.2} | {:>10.2} | {:>5} | {fallbacks:>9}",
        last - v0,
        median(t_tt),
        median(t_cc),
        median(t_ai),
        median(t_ac),
        index.len(),
    );
}

#[tokio::main]
async fn main() {
    let sizes: Vec<u64> = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "100000".to_string())
        .split(',')
        .map(|s| s.trim().parse().expect("size"))
        .collect();
    println!(
        "P-HIST-1: {VERSIONS} versions x {BATCH} updates, target changes every {TARGET_EVERY}th; median of {RUNS} runs"
    );
    println!(
        "        N | versions | frags | build s | time travel | change chain | address index | concurrent | reads | checkouts"
    );
    for n in sizes {
        probe(n).await;
    }
}

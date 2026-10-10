//! Probe P-SPARSE-1: the persistence arm of the alpha split tunnel, applied
//! to the two probes that failed on Lance's own copy-on-write update path.
//!
//! Split tunnel (lance-graph `contract/src/alpha_tunnel.rs`): reads go to a
//! read-only base, writes go to a sparse overlay at the same addresses, and
//! the read merges them. `AlphaOverlay` itself carries no payload (it stamps
//! where attention went), so this probe builds the persistence half the
//! tunnel lacks (lance-graph board entry 2026-09-23, "third probe arm"):
//!
//! - **base:** one Lance dataset `(a)` written once, never updated; a row's
//!   address is its position.
//! - **delta:** one append-only Lance dataset of sparse records
//!   `(row, a, version, op)`, one append (= one Lance version) per writer
//!   version; `op` 0 = upsert, 1 = delete. No row of the base is rewritten,
//!   so there are no deletion vectors and no row-id maps.
//! - **read:** merge base and delta, newest version wins.
//!
//! Two modes, each asserting against an in-memory oracle:
//!
//! - `hist N V B` (P-HIST-1): V versions of B updates (`a = a + 1`); the
//!   target changes every 10th version. Read its history (base value + its
//!   delta records in version order) by a filtered scan of the delta, then
//!   again with a BTree index on `row`. Budget: 1,000 versions < 20 ms.
//! - `list N P B` (P-LIST-1): P prefill versions of B updates, then one step
//!   version of 500 updates, 250 inserts and 250 deletes. A stored mask
//!   (`a >= 900`) is updated from the delta records appended since its
//!   offset. Budget: < 10 ms, allocation independent of the population.
//!
//! Run in release:
//! `cargo run --release --manifest-path probes/p-sparse-1/Cargo.toml -- hist 1000000 1000 100`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use arrow::array::{Array, Int32Array, RecordBatch, RecordBatchIterator, UInt8Array, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use futures::TryStreamExt;
use lance::Dataset;
use lance::dataset::optimize::{CompactionOptions, compact_files};
use lance::dataset::{ProjectionRequest, WriteMode, WriteParams};
use lance::index::DatasetIndexExt;
use lance_index::IndexType;
use lance_index::scalar::ScalarIndexParams;

struct Counting;
static ALLOCATED: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATED.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATED.fetch_add(new_size as u64, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

const UPSERT: u8 = 0;
const DELETE: u8 = 1;
const THRESHOLD: i32 = 900;
const RUNS: usize = 5;

fn base_value(row: u64) -> i32 {
    (row % 1000) as i32
}

fn lcg(x: &mut u64) -> u64 {
    *x = x
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *x >> 11
}

fn base_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![Field::new("a", DataType::Int32, false)]))
}

fn delta_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("row", DataType::UInt64, false),
        Field::new("a", DataType::Int32, false),
        Field::new("version", DataType::UInt64, false),
        Field::new("op", DataType::UInt8, false),
    ]))
}

async fn write_base(uri: &str, n: u64) -> Dataset {
    let chunk = 65_536u64;
    let batches = (0..n).step_by(chunk as usize).map(move |s| {
        let rows = s..(s + chunk).min(n);
        Ok(RecordBatch::try_new(
            base_schema(),
            vec![Arc::new(Int32Array::from_iter_values(rows.map(base_value)))],
        )
        .expect("base batch"))
    });
    Dataset::write(RecordBatchIterator::new(batches, base_schema()), uri, None)
        .await
        .expect("write base")
}

/// One writer version: append its sparse records as one Lance version.
async fn append_delta(uri: &str, version: u64, records: &[(u64, i32, u8)], first: bool) {
    let batch = RecordBatch::try_new(
        delta_schema(),
        vec![
            Arc::new(UInt64Array::from_iter_values(records.iter().map(|r| r.0))),
            Arc::new(Int32Array::from_iter_values(records.iter().map(|r| r.1))),
            Arc::new(UInt64Array::from_iter_values(std::iter::repeat_n(
                version,
                records.len(),
            ))),
            Arc::new(UInt8Array::from_iter_values(records.iter().map(|r| r.2))),
        ],
    )
    .expect("delta batch");
    let params = WriteParams {
        mode: if first {
            WriteMode::Create
        } else {
            WriteMode::Append
        },
        ..WriteParams::default()
    };
    Dataset::write(
        RecordBatchIterator::new(vec![Ok(batch)], delta_schema()),
        uri,
        Some(params),
    )
    .await
    .expect("append delta");
}

fn col<'a, T: 'static>(b: &'a RecordBatch, name: &str) -> &'a T {
    b.column_by_name(name)
        .expect(name)
        .as_any()
        .downcast_ref::<T>()
        .expect(name)
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// History of `row`: its base value, then each delta value in version order.
async fn history(base: &Dataset, delta: &Dataset, row: u64) -> Vec<i32> {
    let b = base
        .take(
            &[row],
            ProjectionRequest::from_columns(["a"], base.schema()),
        )
        .await
        .expect("base take");
    let mut out = vec![col::<Int32Array>(&b, "a").value(0)];
    let mut scan = delta.scan();
    scan.filter(&format!("row = {row}"))
        .expect("filter")
        .project(&["a", "version"])
        .expect("project");
    let batches: Vec<RecordBatch> = scan
        .try_into_stream()
        .await
        .expect("scan")
        .try_collect()
        .await
        .expect("collect");
    let mut changes: Vec<(u64, i32)> = Vec::new();
    for b in &batches {
        let (a, v) = (col::<Int32Array>(b, "a"), col::<UInt64Array>(b, "version"));
        changes.extend((0..b.num_rows()).map(|i| (v.value(i), a.value(i))));
    }
    changes.sort_unstable();
    out.extend(changes.into_iter().map(|c| c.1));
    out
}

async fn hist(n: u64, versions: u64, batch: usize) {
    let dir = tempfile::tempdir().expect("tempdir");
    let base_uri = dir.path().join("base.lance");
    let base_uri = base_uri.to_str().expect("utf8").to_string();
    let delta_uri = dir.path().join("delta.lance");
    let delta_uri = delta_uri.to_str().expect("utf8").to_string();
    let base = write_base(&base_uri, n).await;

    let target = n / 2;
    let mut current: HashMap<u64, i32> = HashMap::new();
    let mut x = 0x5EED_0003u64;
    let t = Instant::now();
    for k in 1..=versions {
        let mut rows: Vec<u64> = Vec::with_capacity(batch);
        if k % 10 == 0 {
            rows.push(target);
        }
        let mut seen: HashSet<u64> = rows.iter().copied().collect();
        while rows.len() < batch {
            let r = lcg(&mut x) % n;
            if r != target && seen.insert(r) {
                rows.push(r);
            }
        }
        let records: Vec<(u64, i32, u8)> = rows
            .iter()
            .map(|&r| {
                let v = current.entry(r).or_insert_with(|| base_value(r));
                *v += 1;
                (r, *v, UPSERT)
            })
            .collect();
        append_delta(&delta_uri, k, &records, k == 1).await;
    }
    let build_s = t.elapsed().as_secs_f64();
    let mut delta = Dataset::open(&delta_uri).await.expect("open delta");
    let records = delta.count_rows(None).await.expect("count");
    let expected: Vec<i32> = (0..=(versions / 10) as i32)
        .map(|d| base_value(target) + d)
        .collect();

    let mut t_scan = vec![];
    for _ in 0..RUNS {
        let t = Instant::now();
        assert_eq!(
            history(&base, &delta, target).await,
            expected,
            "scan history"
        );
        t_scan.push(t.elapsed().as_secs_f64() * 1e3);
    }

    let t = Instant::now();
    delta
        .create_index(
            &["row"],
            IndexType::BTree,
            None,
            &ScalarIndexParams::default(),
            true,
        )
        .await
        .expect("btree index");
    let index_s = t.elapsed().as_secs_f64();
    let mut t_idx = vec![];
    for _ in 0..RUNS {
        let t = Instant::now();
        assert_eq!(
            history(&base, &delta, target).await,
            expected,
            "indexed history"
        );
        t_idx.push(t.elapsed().as_secs_f64() * 1e3);
    }

    let frags = delta.fragments().len();

    // Compaction: the delta is append-only with no deletions, so merging its
    // fragments loses nothing; re-index over the compacted files.
    let t = Instant::now();
    compact_files(&mut delta, CompactionOptions::default(), None)
        .await
        .expect("compact");
    delta
        .create_index(
            &["row"],
            IndexType::BTree,
            None,
            &ScalarIndexParams::default(),
            true,
        )
        .await
        .expect("btree index after compaction");
    let compact_s = t.elapsed().as_secs_f64();
    let (mut t_cscan, mut t_cidx) = (vec![], vec![]);
    for _ in 0..RUNS {
        let t = Instant::now();
        assert_eq!(
            history(&base, &delta, target).await,
            expected,
            "compacted indexed history"
        );
        t_cidx.push(t.elapsed().as_secs_f64() * 1e3);
    }
    delta.drop_index("row_idx").await.expect("drop index");
    for _ in 0..RUNS {
        let t = Instant::now();
        assert_eq!(
            history(&base, &delta, target).await,
            expected,
            "compacted scan history"
        );
        t_cscan.push(t.elapsed().as_secs_f64() * 1e3);
    }

    println!(
        "hist | N {n} | versions {versions} x {batch} | delta records {records} | write {build_s:.1} s | {frags} fragments: scan {:.2} ms, btree build {index_s:.2} s, indexed {:.2} ms | compact+index {compact_s:.2} s, {} fragments: scan {:.2} ms, indexed {:.2} ms | values {}",
        median(t_scan),
        median(t_idx),
        delta.fragments().len(),
        median(t_cscan),
        median(t_cidx),
        expected.len(),
    );
}

#[inline]
fn set(mask: &mut [u64], row: u64, on: bool) {
    let (w, b) = ((row >> 6) as usize, row & 63);
    if on {
        mask[w] |= 1 << b;
    } else {
        mask[w] &= !(1 << b);
    }
}

/// Oracle: base predicate overridden by the current overlay state.
fn oracle_mask(n: u64, words: usize, current: &HashMap<u64, Option<i32>>) -> Vec<u64> {
    let mut mask = vec![0u64; words];
    for r in 0..n {
        if base_value(r) >= THRESHOLD {
            set(&mut mask, r, true);
        }
    }
    for (&r, v) in current {
        set(&mut mask, r, matches!(v, Some(a) if *a >= THRESHOLD));
    }
    mask
}

async fn list(n: u64, prefill: u64, batch: usize) {
    let dir = tempfile::tempdir().expect("tempdir");
    let base_uri = dir.path().join("base.lance");
    let delta_uri = dir.path().join("delta.lance");
    let delta_uri = delta_uri.to_str().expect("utf8").to_string();
    write_base(base_uri.to_str().expect("utf8"), n).await;

    let mut current: HashMap<u64, Option<i32>> = HashMap::new();
    let mut x = 0x5EED_0004u64;
    let mut first = true;
    for k in 1..=prefill {
        let mut seen = HashSet::new();
        let mut records = Vec::with_capacity(batch);
        while records.len() < batch {
            let r = lcg(&mut x) % n;
            if seen.insert(r) {
                let a = (lcg(&mut x) % 1000) as i32;
                current.insert(r, Some(a));
                records.push((r, a, UPSERT));
            }
        }
        append_delta(&delta_uri, k, &records, first).await;
        first = false;
    }
    let words = (n as usize + 250).div_ceil(64) + 1;
    let before = oracle_mask(n, words, &current);
    let offset = if first {
        0
    } else {
        Dataset::open(&delta_uri)
            .await
            .expect("open")
            .version()
            .version
    };

    // The step version: 500 updates, 250 inserts, 250 deletes.
    let mut seen = HashSet::new();
    let mut records = Vec::new();
    while records.len() < 500 {
        let r = lcg(&mut x) % n;
        if seen.insert(r) {
            let old = current
                .get(&r)
                .copied()
                .flatten()
                .unwrap_or_else(|| base_value(r));
            let a = (old + 500) % 1000;
            current.insert(r, Some(a));
            records.push((r, a, UPSERT));
        }
    }
    for i in 0..250u64 {
        let r = n + i;
        let a = (r % 1000) as i32;
        current.insert(r, Some(a));
        records.push((r, a, UPSERT));
    }
    while records.len() < 1000 {
        let r = lcg(&mut x) % n;
        if seen.insert(r) {
            current.insert(r, None);
            records.push((r, 0, DELETE));
        }
    }
    append_delta(&delta_uri, prefill + 1, &records, first).await;
    let mut after = oracle_mask(n, words, &HashMap::new());
    for (&r, v) in &current {
        set(&mut after, r, matches!(v, Some(a) if *a >= THRESHOLD));
    }

    let delta = Dataset::open(&delta_uri).await.expect("open delta");
    let (mut times, mut bytes) = (vec![], vec![]);
    for _ in 0..RUNS {
        let mut mask = before.clone();
        let a0 = ALLOCATED.load(Ordering::Relaxed);
        let t = Instant::now();
        let old: HashSet<u64> = if offset == 0 {
            HashSet::new()
        } else {
            delta
                .checkout_version(offset)
                .await
                .expect("offset")
                .fragments()
                .iter()
                .map(|f| f.id)
                .collect()
        };
        let added: Vec<_> = delta
            .fragments()
            .iter()
            .filter(|f| !old.contains(&f.id))
            .cloned()
            .collect();
        let mut scan = delta.scan();
        scan.with_fragments(added)
            .project(&["row", "a", "op"])
            .expect("project");
        let batches: Vec<RecordBatch> = scan
            .try_into_stream()
            .await
            .expect("scan")
            .try_collect()
            .await
            .expect("collect");
        for b in &batches {
            let (row, a, op) = (
                col::<UInt64Array>(b, "row"),
                col::<Int32Array>(b, "a"),
                col::<UInt8Array>(b, "op"),
            );
            for i in 0..b.num_rows() {
                let keep = op.value(i) == UPSERT && a.value(i) >= THRESHOLD;
                set(&mut mask, row.value(i), keep);
            }
        }
        times.push(t.elapsed().as_secs_f64() * 1e3);
        bytes.push((ALLOCATED.load(Ordering::Relaxed) - a0) as f64);
        assert!(mask == after, "incremental mask != merge-on-read oracle");
    }
    println!(
        "list | N {n} | prefill {prefill} x {batch} | delta fragments {} | step {:.2} ms | alloc {:.0} KiB",
        delta.fragments().len(),
        median(times),
        median(bytes) / 1024.0,
    );
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let num = |i: usize, d: u64| args.get(i).map_or(d, |s| s.parse().expect("number"));
    match args.first().map(String::as_str) {
        Some("hist") => hist(num(1, 1_000_000), num(2, 1_000), num(3, 100) as usize).await,
        Some("list") => list(num(1, 10_000_000), num(2, 100), num(3, 1_000) as usize).await,
        _ => eprintln!("usage: p-sparse-1 hist N V B | list N P B"),
    }
}

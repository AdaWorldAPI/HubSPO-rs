//! Probe P-LIST-1 (plan H-ADR-5, probe table): re-evaluate an active list
//! incrementally from one Lance version to the next. Budget: 1k changed rows
//! on a 10M population in < 10 ms, with allocation independent of the
//! population.
//!
//! The list is a stored mask: one bit per stable row id, preallocated. The
//! predicate is `a >= 900` (10 % of rows).
//!
//! Per population N:
//!
//! 1. v0: N rows `(id, a)`, stable row ids on; build the mask by a full scan.
//! 2. v1..v3: 500 updates (`a = (a + 500) % 1000`, flipping membership),
//!    250 inserts, 250 deletes — 1,000 changes.
//! 3. Timed step: clear the bits of `delta().get_deleted_row_ids()`, then
//!    scan only the fragments added since v0 and set/clear each row's bit by
//!    the predicate. (An updated row keeps its stable id, so its old copy's
//!    deletion is cleared first and its new copy sets the bit again.)
//! 4. Check the result against a full recompute at v3, and count the bytes
//!    allocated during the timed step.
//!
//! The predicate here is a plain loop over the changed rows standing in for
//! the Quack program the engine will run; at 1k rows it is not the cost
//! being measured.
//!
//! Run in release: `cargo run --release --manifest-path probes/p-list-1/Cargo.toml`.
//! Optional argument: comma-separated sizes (default `1000000,10000000`).

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use arrow::array::{Array, Int32Array, RecordBatch, RecordBatchIterator, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use futures::TryStreamExt;
use lance::Dataset;
use lance::dataset::{UpdateBuilder, WriteMode, WriteParams};

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

const UPDATES: usize = 500;
const INSERTS: usize = 250;
const DELETES: usize = 250;
const RUNS: usize = 5;
const THRESHOLD: i32 = 900;

fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::UInt64, false),
        Field::new("a", DataType::Int32, false),
    ]))
}

fn reader(
    n: u64,
    from: u64,
) -> RecordBatchIterator<std::vec::IntoIter<Result<RecordBatch, arrow::error::ArrowError>>> {
    let chunk = 65_536u64;
    let batches: Vec<_> = (from..from + n)
        .step_by(chunk as usize)
        .map(|s| {
            let ids = s..(s + chunk).min(from + n);
            let id = UInt64Array::from_iter_values(ids.clone());
            let a = Int32Array::from_iter_values(ids.map(|i| (i % 1000) as i32));
            Ok(RecordBatch::try_new(schema(), vec![Arc::new(id), Arc::new(a)]).expect("batch"))
        })
        .collect();
    RecordBatchIterator::new(batches.into_iter(), schema())
}

fn sample(n: u64, count: usize, seed: u64, exclude: &[u64]) -> Vec<u64> {
    let mut x = seed;
    let mut out = Vec::with_capacity(count);
    while out.len() < count {
        x = x
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let id = (x >> 11) % n;
        if !out.contains(&id) && !exclude.contains(&id) {
            out.push(id);
        }
    }
    out
}

fn in_list(ids: &[u64]) -> String {
    let list: Vec<String> = ids.iter().map(u64::to_string).collect();
    format!("id IN ({})", list.join(","))
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
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

/// Apply `a >= THRESHOLD` to every row of `batches` (which carry `_rowid`).
fn apply(mask: &mut [u64], batches: &[RecordBatch]) -> usize {
    let mut rows = 0;
    for b in batches {
        let ids = b
            .column_by_name("_rowid")
            .expect("_rowid")
            .as_any()
            .downcast_ref::<UInt64Array>()
            .expect("u64");
        let a = b
            .column_by_name("a")
            .expect("a")
            .as_any()
            .downcast_ref::<Int32Array>()
            .expect("i32");
        for i in 0..b.num_rows() {
            set(mask, ids.value(i), a.value(i) >= THRESHOLD);
        }
        rows += b.num_rows();
    }
    rows
}

/// The oracle: the mask over every live row of `ds`.
async fn full_mask(ds: &Dataset, words: usize) -> Vec<u64> {
    let mut mask = vec![0u64; words];
    let mut scan = ds.scan();
    scan.with_row_id().project(&["a"]).expect("project");
    let batches: Vec<RecordBatch> = scan
        .try_into_stream()
        .await
        .expect("scan")
        .try_collect()
        .await
        .expect("collect");
    apply(&mut mask, &batches);
    mask
}

async fn probe(n: u64) {
    let dir = tempfile::tempdir().expect("tempdir");
    let uri = dir.path().join("p_list_1.lance");
    let uri = uri.to_str().expect("utf8 path");
    let params = WriteParams {
        enable_stable_row_ids: true,
        ..WriteParams::default()
    };
    let ds = Dataset::write(reader(n, 0), uri, Some(params.clone()))
        .await
        .expect("write v0");
    let v0 = ds.version().version;
    let words = (n as usize + INSERTS).div_ceil(64) + 1;

    let t = Instant::now();
    let base = full_mask(&ds, words).await;
    let full_ms = t.elapsed().as_secs_f64() * 1e3;

    let updated = sample(n, UPDATES, 0x5EED_0001, &[]);
    let deleted = sample(n, DELETES, 0x5EED_0002, &updated);
    let next = UpdateBuilder::new(Arc::new(ds))
        .update_where(&in_list(&updated))
        .expect("where")
        .set("a", "(a + 500) % 1000")
        .expect("set")
        .build()
        .expect("build")
        .execute()
        .await
        .expect("update")
        .new_dataset;
    let mut ds = (*next).clone();
    ds.append(
        reader(INSERTS as u64, n),
        Some(WriteParams {
            mode: WriteMode::Append,
            ..params.clone()
        }),
    )
    .await
    .expect("append");
    ds.delete(&in_list(&deleted)).await.expect("delete");
    let v3 = ds.version().version;

    let oracle = full_mask(&ds, words).await;
    let flipped: u64 = base
        .iter()
        .zip(&oracle)
        .map(|(x, y)| (x ^ y).count_ones() as u64)
        .sum();

    let before = ds.checkout_version(v0).await.expect("checkout v0");
    let old_ids: HashSet<u64> = before.fragments().iter().map(|f| f.id).collect();

    let (mut t_step, mut bytes) = (vec![], vec![]);
    let mut touched = 0;
    let mut split = (0, 0);
    for _ in 0..RUNS {
        let mut mask = base.clone();
        let a0 = ALLOCATED.load(Ordering::Relaxed);
        let t = Instant::now();

        let delta = ds
            .delta()
            .with_begin_version(v0)
            .with_end_version(v3)
            .build()
            .expect("delta");
        let gone: Vec<RecordBatch> = delta
            .get_deleted_row_ids()
            .await
            .expect("deleted")
            .try_collect()
            .await
            .expect("collect deleted");
        for b in &gone {
            let ids = b
                .column(0)
                .as_any()
                .downcast_ref::<UInt64Array>()
                .expect("u64 row ids");
            for &id in ids.values() {
                set(&mut mask, id, false);
            }
        }

        let a_mid = ALLOCATED.load(Ordering::Relaxed);
        let added: Vec<_> = ds
            .fragments()
            .iter()
            .filter(|f| !old_ids.contains(&f.id))
            .cloned()
            .collect();
        let mut scan = ds.scan();
        scan.with_fragments(added)
            .with_row_id()
            .project(&["a"])
            .expect("project");
        let batches: Vec<RecordBatch> = scan
            .try_into_stream()
            .await
            .expect("scan added")
            .try_collect()
            .await
            .expect("collect added");
        touched = apply(&mut mask, &batches);

        t_step.push(t.elapsed().as_secs_f64() * 1e3);
        let a_end = ALLOCATED.load(Ordering::Relaxed);
        bytes.push((a_end - a0) as f64);
        split = ((a_mid - a0) / 1024, (a_end - a_mid) / 1024);
        assert!(mask == oracle, "incremental mask != full recompute");
    }

    eprintln!(
        "  split N={n}: deletes {} KiB, added scan {} KiB, fragments {}",
        split.0,
        split.1,
        ds.fragments().len()
    );
    let members: u64 = oracle.iter().map(|w| w.count_ones() as u64).sum();
    println!(
        "{n:>9} | {members:>9} | {flipped:>7} | {touched:>7} | {full_ms:>9.1} | {:>8.2} | {:>9.0}",
        median(t_step),
        median(bytes) / 1024.0,
    );
}

#[tokio::main]
async fn main() {
    let sizes: Vec<u64> = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "1000000,10000000".to_string())
        .split(',')
        .map(|s| s.trim().parse().expect("size"))
        .collect();
    println!(
        "P-LIST-1: {UPDATES} updates, {INSERTS} inserts, {DELETES} deletes; predicate a >= {THRESHOLD}; median of {RUNS} runs"
    );
    println!("        N |   members | flipped | touched | full scan |  step ms | alloc KiB");
    for n in sizes {
        probe(n).await;
    }
}

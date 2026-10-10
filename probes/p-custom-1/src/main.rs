//! Probe P-CUSTOM-1 (plan H-ADR-3, probe table): which layout should a
//! tenant's custom properties use? Budget: a fold over one custom property
//! (500 custom properties × 1M rows) within 2× of the same fold over a
//! standard property.
//!
//! Two layouts, same data:
//!
//! - **lanes:** one nullable `i32` column per custom property (`c0..cK`);
//! - **packed:** two parallel list columns per row, `cp: List<u16>` (the
//!   property's position) and `cv: List<i32>` (its value).
//!
//! Every row fills `FILLED` of the K custom properties (a contiguous window
//! starting at a row-dependent position), plus a standard `amount` column.
//! For K in 50 / 500 / 2,000 the probe writes both layouts, then times the
//! sum of one custom property and of `amount`, asserting the sums against
//! the generator.
//!
//! Run in release: `cargo run --release --manifest-path probes/p-custom-1/Cargo.toml`.
//! Optional arguments: rows (default 1000000), comma-separated K list
//! (default `50,500,2000`).

use std::sync::Arc;
use std::time::Instant;

use arrow::array::{
    Array, ArrayRef, Int32Array, ListArray, RecordBatch, RecordBatchIterator, UInt16Array,
    UInt64Array,
};
use arrow::buffer::{NullBuffer, OffsetBuffer};
use arrow::compute::filter;
use arrow::compute::kernels::cmp::eq;
use arrow::datatypes::{DataType, Field, Schema};
use futures::TryStreamExt;
use lance::Dataset;

const FILLED: usize = 20;
const CHUNK: usize = 16_384;
const RUNS: usize = 5;
const PROBE_PROP: usize = 7;

fn start(row: u64, k: usize) -> usize {
    (row.wrapping_mul(2_654_435_761) % k as u64) as usize
}

fn value(row: u64, prop: usize) -> i32 {
    ((row + prop as u64) % 100) as i32
}

fn amount(row: u64) -> i32 {
    (row % 1000) as i32
}

fn expected(rows: u64, k: usize, prop: usize) -> i64 {
    let f = FILLED.min(k);
    (0..rows)
        .filter(|&r| (prop + k - start(r, k)) % k < f)
        .map(|r| value(r, prop) as i64)
        .sum()
}

fn lanes_schema(k: usize) -> Arc<Schema> {
    let mut fields = vec![
        Field::new("id", DataType::UInt64, false),
        Field::new("amount", DataType::Int32, false),
    ];
    fields.extend((0..k).map(|p| Field::new(format!("c{p}"), DataType::Int32, true)));
    Arc::new(Schema::new(fields))
}

fn packed_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::UInt64, false),
        Field::new("amount", DataType::Int32, false),
        Field::new(
            "cp",
            DataType::List(Arc::new(Field::new("item", DataType::UInt16, false))),
            false,
        ),
        Field::new(
            "cv",
            DataType::List(Arc::new(Field::new("item", DataType::Int32, false))),
            false,
        ),
    ]))
}

fn lanes_batch(from: u64, n: usize, k: usize, schema: &Arc<Schema>) -> RecordBatch {
    let f = FILLED.min(k);
    let mut vals = vec![0i32; k * n];
    let mut valid = vec![false; k * n];
    for i in 0..n {
        let r = from + i as u64;
        let s = start(r, k);
        for j in 0..f {
            let p = (s + j) % k;
            vals[p * n + i] = value(r, p);
            valid[p * n + i] = true;
        }
    }
    let mut cols: Vec<ArrayRef> = vec![
        Arc::new(UInt64Array::from_iter_values(from..from + n as u64)),
        Arc::new(Int32Array::from_iter_values(
            (from..from + n as u64).map(amount),
        )),
    ];
    for p in 0..k {
        let v = vals[p * n..(p + 1) * n].to_vec();
        let nulls = NullBuffer::from(valid[p * n..(p + 1) * n].to_vec());
        cols.push(Arc::new(Int32Array::new(v.into(), Some(nulls))));
    }
    RecordBatch::try_new(schema.clone(), cols).expect("lanes batch")
}

fn packed_batch(from: u64, n: usize, k: usize, schema: &Arc<Schema>) -> RecordBatch {
    let f = FILLED.min(k);
    let mut pos = Vec::with_capacity(n * f);
    let mut val = Vec::with_capacity(n * f);
    for i in 0..n {
        let r = from + i as u64;
        let s = start(r, k);
        for j in 0..f {
            let p = (s + j) % k;
            pos.push(p as u16);
            val.push(value(r, p));
        }
    }
    let offsets = OffsetBuffer::from_lengths(std::iter::repeat_n(f, n));
    let cp = ListArray::new(
        Arc::new(Field::new("item", DataType::UInt16, false)),
        offsets.clone(),
        Arc::new(UInt16Array::from(pos)),
        None,
    );
    let cv = ListArray::new(
        Arc::new(Field::new("item", DataType::Int32, false)),
        offsets,
        Arc::new(Int32Array::from(val)),
        None,
    );
    let cols: Vec<ArrayRef> = vec![
        Arc::new(UInt64Array::from_iter_values(from..from + n as u64)),
        Arc::new(Int32Array::from_iter_values(
            (from..from + n as u64).map(amount),
        )),
        Arc::new(cp),
        Arc::new(cv),
    ];
    RecordBatch::try_new(schema.clone(), cols).expect("packed batch")
}

async fn write(
    uri: &str,
    rows: u64,
    schema: Arc<Schema>,
    make: impl Fn(u64, usize) -> RecordBatch + Send + 'static,
) -> (Dataset, f64) {
    let t = Instant::now();
    let chunks = (0..rows).step_by(CHUNK).map(move |s| {
        let n = (rows - s).min(CHUNK as u64) as usize;
        Ok(make(s, n))
    });
    let ds = Dataset::write(RecordBatchIterator::new(chunks, schema), uri, None)
        .await
        .expect("write");
    (ds, t.elapsed().as_secs_f64())
}

async fn batches(ds: &Dataset, cols: &[&str]) -> Vec<RecordBatch> {
    let mut scan = ds.scan();
    scan.project(cols).expect("project");
    scan.try_into_stream()
        .await
        .expect("scan")
        .try_collect()
        .await
        .expect("collect")
}

fn sum_i32(a: &dyn Array) -> i64 {
    let a = a.as_any().downcast_ref::<Int32Array>().expect("i32");
    // Widen per chunk: the column sum fits i64, not i32.
    a.iter().flatten().map(i64::from).sum()
}

async fn fold_column(ds: &Dataset, col: &str) -> i64 {
    batches(ds, &[col])
        .await
        .iter()
        .map(|b| sum_i32(b.column(0).as_ref()))
        .sum()
}

async fn fold_packed(ds: &Dataset, prop: u16) -> i64 {
    let mut total = 0;
    for b in batches(ds, &["cp", "cv"]).await {
        let cp = b
            .column(0)
            .as_any()
            .downcast_ref::<ListArray>()
            .expect("cp");
        let cv = b
            .column(1)
            .as_any()
            .downcast_ref::<ListArray>()
            .expect("cv");
        let pos = cp.values();
        let hit = eq(pos, &UInt16Array::new_scalar(prop)).expect("eq");
        let vals = filter(cv.values().as_ref(), &hit).expect("filter");
        total += sum_i32(vals.as_ref());
    }
    total
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn dir_bytes(path: &std::path::Path) -> u64 {
    std::fs::read_dir(path)
        .map(|it| {
            it.flatten()
                .map(|e| {
                    let p = e.path();
                    if p.is_dir() {
                        dir_bytes(&p)
                    } else {
                        e.metadata().map_or(0, |m| m.len())
                    }
                })
                .sum()
        })
        .unwrap_or(0)
}

async fn probe(rows: u64, k: usize) {
    let dir = tempfile::tempdir().expect("tempdir");
    let lanes_uri = dir.path().join("lanes.lance");
    let packed_uri = dir.path().join("packed.lance");
    let prop = PROBE_PROP % k;
    let want = expected(rows, k, prop);
    let want_amount: i64 = (0..rows).map(|r| amount(r) as i64).sum();

    let ls = lanes_schema(k);
    let (lanes, w_lanes) = write(lanes_uri.to_str().expect("utf8"), rows, ls.clone(), {
        let ls = ls.clone();
        move |s, n| lanes_batch(s, n, k, &ls)
    })
    .await;
    let ps = packed_schema();
    let (packed, w_packed) = write(packed_uri.to_str().expect("utf8"), rows, ps.clone(), {
        let ps = ps.clone();
        move |s, n| packed_batch(s, n, k, &ps)
    })
    .await;

    let col = format!("c{prop}");
    let (mut t_std, mut t_lane, mut t_pack) = (vec![], vec![], vec![]);
    for _ in 0..RUNS {
        let t = Instant::now();
        assert_eq!(
            fold_column(&lanes, "amount").await,
            want_amount,
            "standard fold"
        );
        t_std.push(t.elapsed().as_secs_f64() * 1e3);

        let t = Instant::now();
        assert_eq!(fold_column(&lanes, &col).await, want, "lane fold");
        t_lane.push(t.elapsed().as_secs_f64() * 1e3);

        let t = Instant::now();
        assert_eq!(fold_packed(&packed, prop as u16).await, want, "packed fold");
        t_pack.push(t.elapsed().as_secs_f64() * 1e3);
    }
    let (s, l, p) = (median(t_std), median(t_lane), median(t_pack));
    println!(
        "{k:>5} | {s:>8.2} | {l:>8.2} | {:>5.2} | {p:>8.2} | {:>6.2} | {w_lanes:>7.1} | {w_packed:>7.1} | {:>7.1} | {:>7.1}",
        l / s,
        p / s,
        dir_bytes(&lanes_uri) as f64 / 1e6,
        dir_bytes(&packed_uri) as f64 / 1e6,
    );
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let rows: u64 = args.next().map_or(1_000_000, |s| s.parse().expect("rows"));
    let ks: Vec<usize> = args
        .next()
        .unwrap_or_else(|| "50,500,2000".to_string())
        .split(',')
        .map(|s| s.trim().parse().expect("k"))
        .collect();
    println!(
        "P-CUSTOM-1: {rows} rows, {FILLED} custom properties filled per row; fold = sum of one property; median of {RUNS} runs, ms"
    );
    println!(
        "    K | standard |    lanes |  ×std |   packed |   ×std | write L s | write P s | lanes MB | packed MB"
    );
    for k in ks {
        probe(rows, k).await;
    }
}

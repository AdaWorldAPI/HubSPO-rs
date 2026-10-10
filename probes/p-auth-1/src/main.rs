//! Probe P-AUTH-1 (plan H-ADR-9, probe table): what does RBAC cost on a
//! read? Budget: < 10 % over the same read without RBAC.
//!
//! The role decision itself is one lookup per request; what scales with the
//! data is the ownership scope (HubSpot's everything / team-owned /
//! owned-only), applied per row. The read is a fold (`sum(amount)`) over N
//! rows `(id, owner, amount)` with 1,000 owners. Three arms per scope:
//!
//! - **unscoped:** the fold with no RBAC (the baseline);
//! - **owner check:** scan `owner` too and keep rows whose owner is in the
//!   scope (an allowed-owner table, one lookup per row);
//! - **scope mask:** the scope kept as a stored mask over row positions
//!   (H-ADR-5 mask algebra), ANDed into the fold; the mask is maintained
//!   like an active list, so building it is not part of the read.
//!
//! Every arm's sum is asserted against the generator. Overhead =
//! arm / unscoped − 1.
//!
//! Run in release: `cargo run --release --manifest-path probes/p-auth-1/Cargo.toml`.
//! Optional argument: rows (default 10000000).

use std::sync::Arc;
use std::time::Instant;

use arrow::array::{Int32Array, RecordBatch, RecordBatchIterator, UInt32Array, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use futures::TryStreamExt;
use lance::Dataset;

const OWNERS: u32 = 1_000;
const RUNS: usize = 7;
const CHUNK: u64 = 65_536;

fn owner(row: u64) -> u32 {
    (row.wrapping_mul(2_654_435_761) % OWNERS as u64) as u32
}

fn amount(row: u64) -> i32 {
    (row % 997) as i32
}

fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::UInt64, false),
        Field::new("owner", DataType::UInt32, false),
        Field::new("amount", DataType::Int32, false),
    ]))
}

async fn scan(ds: &Dataset, cols: &[&str]) -> Vec<RecordBatch> {
    let mut s = ds.scan();
    s.project(cols).expect("project");
    s.try_into_stream()
        .await
        .expect("scan")
        .try_collect()
        .await
        .expect("collect")
}

fn i32s(b: &RecordBatch, i: usize) -> &[i32] {
    b.column(i)
        .as_any()
        .downcast_ref::<Int32Array>()
        .expect("i32")
        .values()
}

fn u32s(b: &RecordBatch, i: usize) -> &[u32] {
    b.column(i)
        .as_any()
        .downcast_ref::<UInt32Array>()
        .expect("u32")
        .values()
}

async fn unscoped(ds: &Dataset) -> i64 {
    scan(ds, &["amount"])
        .await
        .iter()
        .map(|b| i32s(b, 0).iter().map(|&v| v as i64).sum::<i64>())
        .sum()
}

async fn owner_check(ds: &Dataset, allowed: &[bool]) -> i64 {
    scan(ds, &["owner", "amount"])
        .await
        .iter()
        .map(|b| {
            u32s(b, 0)
                .iter()
                .zip(i32s(b, 1))
                .map(|(&o, &v)| if allowed[o as usize] { v as i64 } else { 0 })
                .sum::<i64>()
        })
        .sum()
}

async fn scope_mask(ds: &Dataset, mask: &[u64]) -> i64 {
    let mut base = 0usize;
    let mut total = 0i64;
    for b in scan(ds, &["amount"]).await {
        for (i, &v) in i32s(&b, 0).iter().enumerate() {
            let r = base + i;
            if mask[r >> 6] >> (r & 63) & 1 == 1 {
                total += v as i64;
            }
        }
        base += b.num_rows();
    }
    total
}

/// The same mask, applied branchlessly: each value is ANDed with the all-ones
/// or all-zeros word its bit selects, so the loop has no data-dependent
/// branch (the shape a mask-risc fold kernel has).
async fn scope_mask_branchless(ds: &Dataset, mask: &[u64]) -> i64 {
    let mut base = 0usize;
    let mut total = 0i64;
    for b in scan(ds, &["amount"]).await {
        for (i, &v) in i32s(&b, 0).iter().enumerate() {
            let r = base + i;
            let keep = -(((mask[r >> 6] >> (r & 63)) & 1) as i64);
            total += (v as i64) & keep;
        }
        base += b.num_rows();
    }
    total
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

#[tokio::main]
async fn main() {
    let rows: u64 = std::env::args()
        .nth(1)
        .map_or(10_000_000, |s| s.parse().expect("rows"));
    let dir = tempfile::tempdir().expect("tempdir");
    let uri = dir.path().join("p_auth_1.lance");
    let batches = (0..rows).step_by(CHUNK as usize).map(move |s| {
        let ids = s..(s + CHUNK).min(rows);
        Ok(RecordBatch::try_new(
            schema(),
            vec![
                Arc::new(UInt64Array::from_iter_values(ids.clone())),
                Arc::new(UInt32Array::from_iter_values(ids.clone().map(owner))),
                Arc::new(Int32Array::from_iter_values(ids.map(amount))),
            ],
        )
        .expect("batch"))
    });
    let ds = Dataset::write(
        RecordBatchIterator::new(batches, schema()),
        uri.to_str().expect("utf8"),
        None,
    )
    .await
    .expect("write");

    println!(
        "P-AUTH-1: {rows} rows, {OWNERS} owners, fold = sum(amount); median of {RUNS} runs, ms"
    );
    println!(
        "scope        | share  | unscoped | owner check (overhead) | scope mask (overhead) | branchless mask (overhead)"
    );
    let scopes: [(&str, Box<dyn Fn(u32) -> bool>); 3] = [
        ("everything", Box::new(|_| true)),
        ("team-owned", Box::new(|o| o < 200)),
        ("owned-only", Box::new(|o| o == 7)),
    ];
    for (name, admits) in scopes {
        let allowed: Vec<bool> = (0..OWNERS).map(&admits).collect();
        let mut mask = vec![0u64; (rows as usize).div_ceil(64)];
        let mut want = 0i64;
        let mut kept = 0u64;
        for r in 0..rows {
            if admits(owner(r)) {
                mask[(r >> 6) as usize] |= 1 << (r & 63);
                want += amount(r) as i64;
                kept += 1;
            }
        }
        let all: i64 = (0..rows).map(|r| amount(r) as i64).sum();

        let (mut t_u, mut t_o, mut t_m, mut t_b) = (vec![], vec![], vec![], vec![]);
        for _ in 0..RUNS {
            let t = Instant::now();
            assert_eq!(unscoped(&ds).await, all, "unscoped");
            t_u.push(t.elapsed().as_secs_f64() * 1e3);
            let t = Instant::now();
            assert_eq!(owner_check(&ds, &allowed).await, want, "owner check");
            t_o.push(t.elapsed().as_secs_f64() * 1e3);
            let t = Instant::now();
            assert_eq!(scope_mask(&ds, &mask).await, want, "scope mask");
            t_m.push(t.elapsed().as_secs_f64() * 1e3);
            let t = Instant::now();
            assert_eq!(
                scope_mask_branchless(&ds, &mask).await,
                want,
                "branchless mask"
            );
            t_b.push(t.elapsed().as_secs_f64() * 1e3);
        }
        let (u, o, m, bl) = (median(t_u), median(t_o), median(t_m), median(t_b));
        println!(
            "{name:<12} | {:>5.1}% | {u:>8.2} | {o:>8.2} ({:>+6.1}%) | {m:>8.2} ({:>+6.1}%) | {bl:>8.2} ({:>+6.1}%)",
            100.0 * kept as f64 / rows as f64,
            100.0 * (o / u - 1.0),
            100.0 * (m / u - 1.0),
            100.0 * (bl / u - 1.0),
        );
    }
}

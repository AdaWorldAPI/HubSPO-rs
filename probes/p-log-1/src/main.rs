//! Probe P-LOG-1 (plan §9 probe table): can the rows and fields that changed
//! between two Lance versions be read from the datasets alone, at a cost
//! proportional to the changed rows rather than to the dataset size?
//!
//! Method, per dataset size N:
//!
//! 1. v0: write N rows `(id, a, b, c)` with stable row ids on.
//! 2. v1: update 500 rows (random ids) `a = a + 1`.
//! 3. v2: append 250 rows.
//! 4. v3: delete 250 other random ids (disjoint from the updated ones).
//! 5. Time Lance's `delta()` readers for v0..v3 (inserted, updated, deleted
//!    row ids), and the field-level diff: take the updated rows at v0 by
//!    stable row id and compare column by column.
//! 6. Time the fragment diff (plan H-ADR-15 condition 4 fallback): scan only
//!    the fragments added since v0 (inserted + rewritten rows; a row id below
//!    v0's `next_row_id` is an update) and diff the deletion vectors of the
//!    fragments both versions share (deletes + the old copies of updates).
//!
//! The change set is the same at every N, so a reader whose time stays flat
//! as N grows costs per change; one that grows with N is a scan.
//!
//! Run in release: `cargo run --release --manifest-path probes/p-log-1/Cargo.toml`.
//! Optional argument: a comma-separated list of sizes (default
//! `100000,1000000,4000000`).

use std::sync::Arc;
use std::time::Instant;

use arrow::array::{Array, Int32Array, RecordBatch, RecordBatchIterator, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use futures::TryStreamExt;
use lance::Dataset;
use lance::dataset::{UpdateBuilder, WriteMode, WriteParams};

const UPDATES: usize = 500;
const INSERTS: usize = 250;
const DELETES: usize = 250;
const RUNS: usize = 5;

fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::UInt64, false),
        Field::new("a", DataType::Int32, false),
        Field::new("b", DataType::Int32, false),
        Field::new("c", DataType::Int32, false),
    ]))
}

fn batch(ids: std::ops::Range<u64>) -> RecordBatch {
    let id = UInt64Array::from_iter_values(ids.clone());
    let a = Int32Array::from_iter_values(ids.clone().map(|i| (i % 1000) as i32));
    let b = Int32Array::from_iter_values(ids.clone().map(|i| (i % 7) as i32));
    let c = Int32Array::from_iter_values(ids.map(|i| (i % 13) as i32));
    RecordBatch::try_new(
        schema(),
        vec![Arc::new(id), Arc::new(a), Arc::new(b), Arc::new(c)],
    )
    .expect("batch")
}

fn reader(
    n: u64,
    from: u64,
) -> RecordBatchIterator<std::vec::IntoIter<Result<RecordBatch, arrow::error::ArrowError>>> {
    let chunk = 65_536;
    let batches: Vec<_> = (from..from + n)
        .step_by(chunk)
        .map(|s| Ok(batch(s..(s + chunk as u64).min(from + n))))
        .collect();
    RecordBatchIterator::new(batches.into_iter(), schema())
}

/// Distinct pseudo-random ids in `0..n`, deterministic (LCG).
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

async fn count(stream: lance::dataset::scanner::DatasetRecordBatchStream) -> usize {
    stream
        .try_fold(0usize, |acc, b| async move { Ok(acc + b.num_rows()) })
        .await
        .expect("stream")
}

async fn probe(n: u64) {
    let dir = tempfile::tempdir().expect("tempdir");
    let uri = dir.path().join("p_log_1.lance");
    let uri = uri.to_str().expect("utf8 path");
    let params = WriteParams {
        enable_stable_row_ids: true,
        ..WriteParams::default()
    };

    let t = Instant::now();
    let ds = Dataset::write(reader(n, 0), uri, Some(params.clone()))
        .await
        .expect("write v0");
    let v0 = ds.version().version;
    let write_ms = t.elapsed().as_secs_f64() * 1e3;

    let updated = sample(n, UPDATES, 0x5EED_0001, &[]);
    let deleted = sample(n, DELETES, 0x5EED_0002, &updated);

    let ds = UpdateBuilder::new(Arc::new(ds))
        .update_where(&in_list(&updated))
        .expect("where")
        .set("a", "a + 1")
        .expect("set")
        .build()
        .expect("build")
        .execute()
        .await
        .expect("update")
        .new_dataset;
    let mut ds = (*ds).clone();
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
    let fragments = ds.get_fragments().len();

    let delta = ds
        .delta()
        .with_begin_version(v0)
        .with_end_version(v3)
        .build()
        .expect("delta");

    let (mut t_ins, mut t_upd, mut t_del, mut t_diff) = (vec![], vec![], vec![], vec![]);
    let mut t_frag = vec![];
    let (mut f_ins, mut f_upd, mut f_del) = (0, 0, 0);
    let (mut n_ins, mut n_upd, mut n_del, mut changed_cols) = (0, 0, 0, Vec::new());
    for _ in 0..RUNS {
        let t = Instant::now();
        n_ins = count(delta.get_inserted_rows().await.expect("inserted")).await;
        t_ins.push(t.elapsed().as_secs_f64() * 1e3);

        let t = Instant::now();
        n_del = count(delta.get_deleted_row_ids().await.expect("deleted")).await;
        t_del.push(t.elapsed().as_secs_f64() * 1e3);

        let t = Instant::now();
        let rows: Vec<RecordBatch> = delta
            .get_updated_rows()
            .await
            .expect("updated")
            .try_collect()
            .await
            .expect("collect");
        t_upd.push(t.elapsed().as_secs_f64() * 1e3);
        n_upd = rows.iter().map(RecordBatch::num_rows).sum();

        // Field-level diff: the same rows at v0, by stable row id.
        let t = Instant::now();
        let before = ds.checkout_version(v0).await.expect("checkout v0");
        let mut cols = vec![0usize; 3];
        for b in &rows {
            let ids = b
                .column_by_name("_rowid")
                .expect("_rowid")
                .as_any()
                .downcast_ref::<UInt64Array>()
                .expect("u64 row ids");
            let old = before
                .take_rows(ids.values(), before.schema().clone())
                .await
                .expect("take at v0");
            for (k, name) in ["a", "b", "c"].iter().enumerate() {
                let new_col = b.column_by_name(name).expect("col");
                let old_col = old.column_by_name(name).expect("col");
                let new_col = new_col.as_any().downcast_ref::<Int32Array>().expect("i32");
                let old_col = old_col.as_any().downcast_ref::<Int32Array>().expect("i32");
                cols[k] += (0..new_col.len())
                    .filter(|&i| new_col.value(i) != old_col.value(i))
                    .count();
            }
        }
        t_diff.push(t.elapsed().as_secs_f64() * 1e3);
        changed_cols = cols;

        // Fragment diff: cost should follow the touched fragments, not N.
        let t = Instant::now();
        let before = ds.checkout_version(v0).await.expect("checkout v0");
        let old_next_row_id = before.manifest().next_row_id;
        let old_ids: std::collections::HashSet<u64> =
            before.fragments().iter().map(|f| f.id).collect();
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
        let new_rows: Vec<RecordBatch> = scan
            .try_into_stream()
            .await
            .expect("scan added")
            .try_collect()
            .await
            .expect("collect added");
        let (mut ins, mut upd) = (0, 0);
        for b in &new_rows {
            let ids = b
                .column_by_name("_rowid")
                .expect("_rowid")
                .as_any()
                .downcast_ref::<UInt64Array>()
                .expect("u64 row ids");
            for &id in ids.values() {
                if id < old_next_row_id {
                    upd += 1;
                } else {
                    ins += 1;
                }
            }
        }
        let mut gone = 0;
        for f in ds.fragments().iter().filter(|f| old_ids.contains(&f.id)) {
            let old_del = before
                .fragments()
                .iter()
                .find(|o| o.id == f.id)
                .and_then(|o| o.deletion_file.clone());
            if f.deletion_file == old_del {
                continue;
            }
            let now = ds.get_fragment(f.id as usize).expect("fragment");
            let then = before.get_fragment(f.id as usize).expect("fragment at v0");
            let n_now = now
                .get_deletion_vector()
                .await
                .expect("dv")
                .map_or(0, |d| d.len());
            let n_then = then
                .get_deletion_vector()
                .await
                .expect("dv")
                .map_or(0, |d| d.len());
            gone += n_now - n_then;
        }
        t_frag.push(t.elapsed().as_secs_f64() * 1e3);
        (f_ins, f_upd, f_del) = (ins, upd, gone - upd);
    }

    assert_eq!(n_ins, INSERTS, "inserted rows");
    assert_eq!(n_upd, UPDATES, "updated rows");
    assert_eq!(n_del, DELETES, "deleted row ids");
    assert_eq!(changed_cols, vec![UPDATES, 0, 0], "only `a` changed");
    assert_eq!(
        (f_ins, f_upd, f_del),
        (INSERTS, UPDATES, DELETES),
        "fragment diff"
    );

    println!(
        "{n:>9} | {fragments:>5} | {write_ms:>8.0} | {:>8.2} | {:>8.2} | {:>8.2} | {:>9.2} | {:>8.2}",
        median(t_ins),
        median(t_upd),
        median(t_del),
        median(t_diff),
        median(t_frag),
    );
}

#[tokio::main]
async fn main() {
    let sizes: Vec<u64> = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "100000,1000000,4000000".to_string())
        .split(',')
        .map(|s| s.trim().parse().expect("size"))
        .collect();
    println!(
        "P-LOG-1: {UPDATES} updates, {INSERTS} inserts, {DELETES} deletes; median of {RUNS} runs, ms"
    );
    println!(
        "        N | frags | write v0 | inserted |  updated |  deleted | fielddiff | fragdiff"
    );
    for n in sizes {
        probe(n).await;
    }
}

//! Probe P-FLOW-1 (plan H-ADR-7, probe table): how long from a published
//! version to the workflows it triggers being enrolled? Budget: < 1 s p99
//! at 1,000 workflows.
//!
//! The path is the one H-ADR-7 fixes (no actors, no trigger loop holding
//! state): a writer commits a version; a consumer woken by it reads the
//! version range from its stored offset by fragment diff (P-LOG-1), evaluates
//! every workflow's trigger criteria over the changed rows, writes the
//! enrolments as one Lance version, and advances its offset.
//!
//! Per trial: commit `UPDATES` contact updates as one version, then time
//! commit → enrolment version written. 1,000 workflows, each triggering on
//! `stage == s AND score >= t AND region == r`. The enrolments of every
//! trial are asserted against the generator.
//!
//! Not in the measurement: starting graph-flow runs (an enrolment here is a
//! row), and the watcher's own wake-up latency (the consumer is called right
//! after the commit returns).
//!
//! Run in release: `cargo run --release --manifest-path probes/p-flow-1/Cargo.toml`.
//! Optional arguments: rows (default 1000000), trials (default 50).

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

use arrow::array::{Array, Int32Array, RecordBatch, RecordBatchIterator, UInt32Array, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema};
use futures::TryStreamExt;
use lance::Dataset;
use lance::dataset::{UpdateBuilder, WriteMode, WriteParams};

const WORKFLOWS: usize = 1_000;
const UPDATES: usize = 1_000;

#[derive(Clone, Copy)]
struct Criteria {
    stage: i32,
    min_score: i32,
    region: i32,
}

fn lcg(x: &mut u64) -> u64 {
    *x = x
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *x >> 11
}

fn contacts_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::UInt64, false),
        Field::new("stage", DataType::Int32, false),
        Field::new("score", DataType::Int32, false),
        Field::new("region", DataType::Int32, false),
    ]))
}

fn enrol_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("workflow", DataType::UInt32, false),
        Field::new("row", DataType::UInt64, false),
        Field::new("version", DataType::UInt64, false),
    ]))
}

fn initial(id: u64) -> (i32, i32, i32) {
    ((id % 10) as i32, (id % 100) as i32, (id % 20) as i32)
}

fn i32s(b: &RecordBatch, name: &str) -> Vec<i32> {
    b.column_by_name(name)
        .expect(name)
        .as_any()
        .downcast_ref::<Int32Array>()
        .expect("i32")
        .values()
        .to_vec()
}

fn u64s(b: &RecordBatch, name: &str) -> Vec<u64> {
    b.column_by_name(name)
        .expect(name)
        .as_any()
        .downcast_ref::<UInt64Array>()
        .expect("u64")
        .values()
        .to_vec()
}

/// The consumer step: catch up from `offset` to the latest version, enrol,
/// write one enrolment version. Returns the new offset and the enrolments.
async fn consume(
    contacts_uri: &str,
    offset: u64,
    workflows: &[Criteria],
    enrol_uri: &str,
) -> (u64, Vec<(u32, u64)>) {
    let latest = Dataset::open(contacts_uri).await.expect("open latest");
    let head = latest.version().version;
    let before = latest.checkout_version(offset).await.expect("offset");
    let old: HashSet<u64> = before.fragments().iter().map(|f| f.id).collect();
    let added: Vec<_> = latest
        .fragments()
        .iter()
        .filter(|f| !old.contains(&f.id))
        .cloned()
        .collect();
    let mut scan = latest.scan();
    scan.with_fragments(added)
        .with_row_id()
        .project(&["id", "stage", "score", "region"])
        .expect("project");
    let changed: Vec<RecordBatch> = scan
        .try_into_stream()
        .await
        .expect("scan")
        .try_collect()
        .await
        .expect("collect");

    let mut enrolled = Vec::new();
    for b in &changed {
        let (ids, stage, score, region) = (
            u64s(b, "id"),
            i32s(b, "stage"),
            i32s(b, "score"),
            i32s(b, "region"),
        );
        for (w, c) in workflows.iter().enumerate() {
            for i in 0..ids.len() {
                if stage[i] == c.stage && score[i] >= c.min_score && region[i] == c.region {
                    enrolled.push((w as u32, ids[i]));
                }
            }
        }
    }

    let n = enrolled.len();
    let batch = RecordBatch::try_new(
        enrol_schema(),
        vec![
            Arc::new(UInt32Array::from_iter_values(enrolled.iter().map(|e| e.0))),
            Arc::new(UInt64Array::from_iter_values(enrolled.iter().map(|e| e.1))),
            Arc::new(UInt64Array::from_iter_values(std::iter::repeat_n(head, n))),
        ],
    )
    .expect("enrol batch");
    let params = WriteParams {
        mode: WriteMode::Append,
        ..WriteParams::default()
    };
    Dataset::write(
        RecordBatchIterator::new(vec![Ok(batch)], enrol_schema()),
        enrol_uri,
        Some(params),
    )
    .await
    .expect("write enrolments");
    (head, enrolled)
}

fn pct(sorted: &[f64], p: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let rows: u64 = args.next().map_or(1_000_000, |s| s.parse().expect("rows"));
    let trials: usize = args.next().map_or(50, |s| s.parse().expect("trials"));
    let dir = tempfile::tempdir().expect("tempdir");
    let contacts_uri = dir.path().join("contacts.lance");
    let contacts_uri = contacts_uri.to_str().expect("utf8").to_string();
    let enrol_uri = dir.path().join("enrolments.lance");
    let enrol_uri = enrol_uri.to_str().expect("utf8").to_string();

    let chunk = 65_536u64;
    let batches = (0..rows).step_by(chunk as usize).map(move |s| {
        let ids: Vec<u64> = (s..(s + chunk).min(rows)).collect();
        let v: Vec<(i32, i32, i32)> = ids.iter().map(|&i| initial(i)).collect();
        Ok(RecordBatch::try_new(
            contacts_schema(),
            vec![
                Arc::new(UInt64Array::from(ids)),
                Arc::new(Int32Array::from_iter_values(v.iter().map(|x| x.0))),
                Arc::new(Int32Array::from_iter_values(v.iter().map(|x| x.1))),
                Arc::new(Int32Array::from_iter_values(v.iter().map(|x| x.2))),
            ],
        )
        .expect("batch"))
    });
    let params = WriteParams {
        enable_stable_row_ids: true,
        ..WriteParams::default()
    };
    let mut contacts = Dataset::write(
        RecordBatchIterator::new(batches, contacts_schema()),
        &contacts_uri,
        Some(params),
    )
    .await
    .expect("write contacts");
    let mut offset = contacts.version().version;

    let mut x = 0x5EED_0006u64;
    let workflows: Vec<Criteria> = (0..WORKFLOWS)
        .map(|_| Criteria {
            stage: (lcg(&mut x) % 10) as i32,
            min_score: (lcg(&mut x) % 100) as i32,
            region: (lcg(&mut x) % 20) as i32,
        })
        .collect();

    let mut times = Vec::with_capacity(trials);
    let mut total_enrolled = 0usize;
    for trial in 0..trials {
        // The writer: UPDATES distinct contacts get a new stage and score.
        let mut ids = HashSet::new();
        while ids.len() < UPDATES {
            ids.insert(lcg(&mut x) % rows);
        }
        let ids: Vec<u64> = ids.into_iter().collect();
        let new_stage = (trial % 10) as i32;
        let new_score = (lcg(&mut x) % 100) as i32;
        let list: Vec<String> = ids.iter().map(u64::to_string).collect();
        let next = UpdateBuilder::new(Arc::new(contacts))
            .update_where(&format!("id IN ({})", list.join(",")))
            .expect("where")
            .set("stage", &new_stage.to_string())
            .expect("set stage")
            .set("score", &new_score.to_string())
            .expect("set score")
            .build()
            .expect("build")
            .execute()
            .await
            .expect("update")
            .new_dataset;
        contacts = (*next).clone();

        // Commit returned: the consumer wakes.
        let t = Instant::now();
        let (head, enrolled) = consume(&contacts_uri, offset, &workflows, &enrol_uri).await;
        times.push(t.elapsed().as_secs_f64() * 1e3);
        offset = head;

        // Oracle: the same criteria over the generator's view of the update.
        let mut want: Vec<(u32, u64)> = Vec::new();
        for (w, c) in workflows.iter().enumerate() {
            for &id in &ids {
                let (_, _, region) = initial(id);
                if new_stage == c.stage && new_score >= c.min_score && region == c.region {
                    want.push((w as u32, id));
                }
            }
        }
        let (mut got, mut want) = (enrolled, want);
        got.sort_unstable();
        want.sort_unstable();
        assert_eq!(got, want, "trial {trial} enrolments");
        total_enrolled += got.len();
    }
    times.sort_by(f64::total_cmp);
    let enrol_rows = Dataset::open(&enrol_uri)
        .await
        .expect("open enrolments")
        .count_rows(None)
        .await
        .expect("count");
    assert_eq!(enrol_rows, total_enrolled, "enrolment rows written");
    println!(
        "P-FLOW-1: {rows} contacts, {WORKFLOWS} workflows, {UPDATES} updates per version, {trials} trials"
    );
    println!(
        "commit → enrolled ms: p50 {:.2} | p99 {:.2} | max {:.2}; enrolments {total_enrolled} (avg {:.1}/version); fragments {}",
        pct(&times, 0.5),
        pct(&times, 0.99),
        times[times.len() - 1],
        total_enrolled as f64 / trials as f64,
        contacts.fragments().len(),
    );
}

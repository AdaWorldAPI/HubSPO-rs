//! D-STO-1 (persistence half): every batch is one Lance version of an
//! append-only log, and a table is rebuilt by replaying it.
//!
//! The shape is P-SPARSE-1's delta: nothing is ever updated in place, so
//! there are no deletion vectors to read and the log is safe to compact.
//! Each record is one fact of one batch:
//!
//! | `op` | meaning | `row` | `lane` | `present` / `value` |
//! |---|---|---|---|---|
//! | 0 `BORN` | a record is inserted | its ordinal | — | — |
//! | 1 `SET` | a property is written | the record | the lane | the value, or absent |
//! | 2 `DELETE` | a record is deleted | the record | — | — |
//!
//! Every record carries the batch's `version` and `owner`. Values are the
//! lane's 32 bits (`I32` stored as its bit pattern).
//!
//! **One Lance version per batch, and equal numbers.** The first batch
//! creates the dataset (Lance version 1, table version 1) and every later
//! one appends exactly one Lance version. [`Log::commit`] checks the log's
//! latest version before and after the append; anything else means a second
//! writer, which breaks one-writer-per-mailbox and is reported as
//! [`LogError::Diverged`], never accepted. Lance appends do not conflict
//! with each other, so this check is what enforces the rule.
//!
//! **Durable first.** `commit` validates the batch, appends it, and only
//! then applies it to the table. If the append fails the table is
//! unchanged.
//!
//! Not yet: folding the log into a base dataset (D-STO-5, history), and
//! object-store URIs beyond what `lance` resolves by itself.

use std::sync::Arc;

use arrow::array::{
    Array, BooleanArray, RecordBatch, RecordBatchIterator, UInt8Array, UInt16Array, UInt32Array,
    UInt64Array,
};
use arrow::datatypes::{DataType, Field, Schema};
use futures::TryStreamExt;
use lance::Dataset;
use lance::dataset::{WriteMode, WriteParams};
use lance_graph_contract::collapse_gate::MailboxId;
use lance_graph_quack::Col;

use crate::table::{Applied, Batch, LaneKind, LaneSpec, StoreError, Table, Value};

const BORN: u8 = 0;
const SET: u8 = 1;
const DELETE: u8 = 2;

/// Why a log operation failed.
#[derive(Debug)]
pub enum LogError {
    /// The batch was refused by the table; nothing was written.
    Store(StoreError),
    /// Lance or Arrow failed.
    Lance(String),
    /// The log's version does not match the table's: another writer
    /// appended, or the log is not this table's. The table must be
    /// replayed.
    Diverged {
        /// Version the log should be at.
        expected: u64,
        /// Version it is at.
        found: u64,
    },
    /// A record names another owner than the one replaying.
    Owner {
        /// Owner asked for.
        expected: MailboxId,
        /// Owner found.
        found: MailboxId,
    },
    /// A record does not decode against the lane declarations.
    Corrupt(&'static str),
}

impl From<StoreError> for LogError {
    fn from(e: StoreError) -> Self {
        LogError::Store(e)
    }
}

fn lance_err(e: impl std::fmt::Display) -> LogError {
    LogError::Lance(e.to_string())
}

fn schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("version", DataType::UInt64, false),
        Field::new("owner", DataType::UInt32, false),
        Field::new("op", DataType::UInt8, false),
        Field::new("row", DataType::UInt32, false),
        Field::new("lane", DataType::UInt16, false),
        Field::new("present", DataType::Boolean, false),
        Field::new("value", DataType::UInt32, false),
    ]))
}

#[derive(Default)]
struct Columns {
    version: Vec<u64>,
    owner: Vec<u32>,
    op: Vec<u8>,
    row: Vec<u32>,
    lane: Vec<u16>,
    present: Vec<bool>,
    value: Vec<u32>,
}

impl Columns {
    fn push(&mut self, version: u64, owner: u32, op: u8, row: u32, lane: u16, v: Option<Value>) {
        self.version.push(version);
        self.owner.push(owner);
        self.op.push(op);
        self.row.push(row);
        self.lane.push(lane);
        self.present.push(v.is_some());
        self.value.push(match v {
            Some(Value::U32(x)) => x,
            Some(Value::I32(x)) => x.cast_unsigned(),
            None => 0,
        });
    }

    fn batch(self) -> Result<RecordBatch, LogError> {
        RecordBatch::try_new(
            schema(),
            vec![
                Arc::new(UInt64Array::from(self.version)),
                Arc::new(UInt32Array::from(self.owner)),
                Arc::new(UInt8Array::from(self.op)),
                Arc::new(UInt32Array::from(self.row)),
                Arc::new(UInt16Array::from(self.lane)),
                Arc::new(BooleanArray::from(self.present)),
                Arc::new(UInt32Array::from(self.value)),
            ],
        )
        .map_err(lance_err)
    }
}

/// The records of `batch` as version `version`, inserting from ordinal
/// `first`.
fn encode(batch: &Batch, version: u64, first: u32) -> Columns {
    let mut c = Columns::default();
    let o = batch.owner;
    for (i, record) in (first..).zip(&batch.inserts) {
        c.push(version, o, BORN, i, 0, None);
        for (lane, v) in (0..=u16::MAX).zip(record) {
            c.push(version, o, SET, i, lane, *v);
        }
    }
    for &(row, lane, v) in &batch.updates {
        c.push(version, o, SET, row, lane.0, v);
    }
    for &row in &batch.deletes {
        c.push(version, o, DELETE, row, 0, None);
    }
    c
}

/// An append-only Lance log at `uri`.
#[derive(Debug, Clone)]
pub struct Log {
    uri: String,
}

impl Log {
    /// The log at `uri` (created by the first commit).
    pub fn new(uri: impl Into<String>) -> Self {
        Self { uri: uri.into() }
    }

    async fn latest(&self) -> Result<u64, LogError> {
        match Dataset::open(&self.uri).await {
            Ok(ds) => Ok(ds.version().version),
            // No dataset yet is version 0, like an empty table.
            Err(lance::Error::DatasetNotFound { .. }) => Ok(0),
            Err(e) => Err(lance_err(e)),
        }
    }

    /// Validate `batch`, append it as one Lance version, then apply it.
    ///
    /// # Errors
    ///
    /// - [`LogError::Store`]: the batch was refused; nothing was written.
    /// - [`LogError::Diverged`]: the log was not at the table's version
    ///   before the append, or not one past it after; nothing was applied.
    /// - [`LogError::Lance`]: the append failed; nothing was applied.
    pub async fn commit(&self, table: &mut Table, batch: &Batch) -> Result<Applied, LogError> {
        table.check(batch)?;
        let before = table.version();
        let found = self.latest().await?;
        if found != before {
            return Err(LogError::Diverged {
                expected: before,
                found,
            });
        }
        let first = u32::try_from(table.n_rows()).map_err(|_| StoreError::Full)?;
        let records = encode(batch, before + 1, first).batch()?;
        let params = WriteParams {
            mode: if before == 0 {
                WriteMode::Create
            } else {
                WriteMode::Append
            },
            ..WriteParams::default()
        };
        let ds = Dataset::write(
            RecordBatchIterator::new(vec![Ok(records)], schema()),
            &self.uri,
            Some(params),
        )
        .await
        .map_err(lance_err)?;
        let after = ds.version().version;
        // Reached only when another writer appends between the check above
        // and this write. No test can schedule that race; a disable run
        // stays green (the sequential case is caught above). Kept because
        // Lance appends do not conflict, so nothing else would notice.
        if after != before + 1 {
            return Err(LogError::Diverged {
                expected: before + 1,
                found: after,
            });
        }
        Ok(table.apply(batch)?)
    }

    /// Rebuild the table of `owner` with `specs` from the log, batch by
    /// batch, through the same [`Table::apply`] a live writer uses.
    ///
    /// # Errors
    ///
    /// [`LogError::Owner`] for a record of another owner,
    /// [`LogError::Corrupt`] for a record that does not decode,
    /// [`LogError::Store`] if a replayed batch is refused, and
    /// [`LogError::Diverged`] if the log's Lance version is not the number
    /// of batches in it.
    pub async fn replay(&self, owner: MailboxId, specs: &[LaneSpec]) -> Result<Table, LogError> {
        let mut table = Table::new(owner, specs)?;
        let latest = self.latest().await?;
        if latest == 0 {
            return Ok(table);
        }
        let ds = Dataset::open(&self.uri).await.map_err(lance_err)?;
        let batches: Vec<RecordBatch> = ds
            .scan()
            .try_into_stream()
            .await
            .map_err(lance_err)?
            .try_collect()
            .await
            .map_err(lance_err)?;
        let mut facts: Vec<(u64, u32, u8, u32, u16, bool, u32)> = Vec::new();
        for b in &batches {
            let version = column::<UInt64Array>(b, 0, "version")?;
            let own = column::<UInt32Array>(b, 1, "owner")?;
            let op = column::<UInt8Array>(b, 2, "op")?;
            let row = column::<UInt32Array>(b, 3, "row")?;
            let lane = column::<UInt16Array>(b, 4, "lane")?;
            let present = column::<BooleanArray>(b, 5, "present")?;
            let value = column::<UInt32Array>(b, 6, "value")?;
            for i in 0..b.num_rows() {
                facts.push((
                    version.value(i),
                    own.value(i),
                    op.value(i),
                    row.value(i),
                    lane.value(i),
                    present.value(i),
                    value.value(i),
                ));
            }
        }
        // Stable sort by version keeps each batch's facts in written order.
        facts.sort_by_key(|f| f.0);
        let mut i = 0;
        while i < facts.len() {
            let version = facts[i].0;
            let end = facts[i..]
                .iter()
                .position(|f| f.0 != version)
                .map_or(facts.len(), |p| i + p);
            let batch = decode(&facts[i..end], owner, &table)?;
            let applied = table.apply(&batch)?;
            if applied.version != version {
                return Err(LogError::Diverged {
                    expected: applied.version,
                    found: version,
                });
            }
            i = end;
        }
        if table.version() != latest {
            return Err(LogError::Diverged {
                expected: latest,
                found: table.version(),
            });
        }
        Ok(table)
    }
}

/// Column `i` of a log batch as `T`, with no nulls.
fn column<'a, T: Array + 'static>(
    b: &'a RecordBatch,
    i: usize,
    name: &'static str,
) -> Result<&'a T, LogError> {
    let a = b.column(i);
    if a.null_count() > 0 {
        return Err(LogError::Corrupt(name));
    }
    a.as_any()
        .downcast_ref::<T>()
        .ok_or(LogError::Corrupt(name))
}

fn decode(
    facts: &[(u64, u32, u8, u32, u16, bool, u32)],
    owner: MailboxId,
    table: &Table,
) -> Result<Batch, LogError> {
    let specs = table.specs();
    let first = u32::try_from(table.n_rows()).map_err(|_| StoreError::Full)?;
    let mut batch = Batch {
        owner,
        ..Batch::default()
    };
    for &(_, own, op, row, lane, present, value) in facts {
        if own != owner {
            return Err(LogError::Owner {
                expected: owner,
                found: own,
            });
        }
        let v = |lane: u16| -> Result<Option<Value>, LogError> {
            let spec = specs
                .get(usize::from(lane))
                .ok_or(LogError::Corrupt("lane"))?;
            Ok(present.then_some(match spec.kind {
                LaneKind::U32 => Value::U32(value),
                LaneKind::I32 => Value::I32(value.cast_signed()),
            }))
        };
        match op {
            BORN => {
                // Ordinals are born in order: the next one, every time.
                let next =
                    first + u32::try_from(batch.inserts.len()).map_err(|_| StoreError::Full)?;
                if row != next {
                    return Err(LogError::Corrupt("born out of order"));
                }
                batch.inserts.push(vec![None; specs.len()]);
            }
            SET if row >= first => {
                let k = usize::try_from(row - first).map_err(|_| LogError::Corrupt("row"))?;
                let record = batch
                    .inserts
                    .get_mut(k)
                    .ok_or(LogError::Corrupt("set before born"))?;
                let slot = record
                    .get_mut(usize::from(lane))
                    .ok_or(LogError::Corrupt("lane"))?;
                *slot = v(lane)?;
            }
            SET => batch.updates.push((row, Col(lane), v(lane)?)),
            DELETE => batch.deletes.push(row),
            _ => return Err(LogError::Corrupt("op")),
        }
    }
    Ok(batch)
}

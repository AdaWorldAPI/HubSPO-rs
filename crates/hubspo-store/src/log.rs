//! D-STO-1 (persistence half): every batch is one Lance version of an
//! append-only log, and a table is rebuilt by replaying it.
//!
//! The shape is P-SPARSE-1's delta: nothing is ever updated in place, so
//! there are no deletion vectors to read. It is never compacted: the
//! append order is the version order, and readers rely on it.
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
//! [`LogError::Diverged`], never accepted. One writer comes from ownership
//! (one writer per mailbox), not from these checks: a second writer is a
//! broken deployment, and the checks report it.
//!
//! **Durable first.** `commit` validates the batch, appends it, and only
//! then applies it to the table. If the append fails the table is
//! unchanged.
//!
//! **Resident records.** Lance is the persistence clock: [`Log::replay`]
//! reads it once, [`Log::commit`] writes one version per batch. The records
//! either touches stay resident, so reads fold over memory.
//!
//! **History (D-STO-5)** is a fold over those records: [`Log::history`]
//! reads one property of one record back as the list of writes that touched
//! it, without opening the dataset.
//!
//! Not yet: an index on `row`, and object-store URIs beyond what `lance`
//! resolves by itself.

use std::sync::Arc;

use arrow::array::{
    Array, BooleanArray, RecordBatch, RecordBatchIterator, UInt32Array, UInt64Array,
};
use arrow::datatypes::{DataType, Field, Schema};
use futures::TryStreamExt;
use lance::Dataset;
use lance::dataset::{WriteMode, WriteParams};
use lance_graph_contract::collapse_gate::MailboxId;
use lance_graph_mask_risc::{
    Foreign, LaneRef, Out, Planes, Scratch, execute_into, materialize_rows, words_for,
};
use lance_graph_quack::{Agg, Cmp, Col, Filter, Query, lower};

use crate::table::{Applied, Batch, LaneKind, LaneSpec, StoreError, Table, Value};

const BORN: u32 = 0;
const SET: u32 = 1;
const DELETE: u32 = 2;

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
        Field::new("op", DataType::UInt32, false),
        Field::new("row", DataType::UInt32, false),
        Field::new("lane", DataType::UInt32, false),
        Field::new("present", DataType::Boolean, false),
        Field::new("value", DataType::UInt32, false),
    ]))
}

#[derive(Default, Clone)]
struct Columns {
    version: Vec<u64>,
    owner: Vec<u32>,
    op: Vec<u32>,
    row: Vec<u32>,
    lane: Vec<u32>,
    present: Vec<bool>,
    value: Vec<u32>,
}

impl Columns {
    fn push(&mut self, version: u64, owner: u32, op: u32, row: u32, lane: u16, v: Option<Value>) {
        self.version.push(version);
        self.owner.push(owner);
        self.op.push(op);
        self.row.push(row);
        self.lane.push(u32::from(lane));
        self.present.push(v.is_some());
        self.value.push(match v {
            Some(Value::U32(x)) => x,
            Some(Value::I32(x)) => x.cast_unsigned(),
            None => 0,
        });
    }

    fn len(&self) -> usize {
        self.version.len()
    }

    fn extend(&mut self, other: Columns) {
        self.version.extend(other.version);
        self.owner.extend(other.owner);
        self.op.extend(other.op);
        self.row.extend(other.row);
        self.lane.extend(other.lane);
        self.present.extend(other.present);
        self.value.extend(other.value);
    }

    /// The records as one Arrow batch. Copies them: the copy goes to Lance,
    /// the original stays resident.
    fn batch(&self) -> Result<RecordBatch, LogError> {
        RecordBatch::try_new(
            schema(),
            vec![
                Arc::new(UInt64Array::from(self.version.clone())),
                Arc::new(UInt32Array::from(self.owner.clone())),
                Arc::new(UInt32Array::from(self.op.clone())),
                Arc::new(UInt32Array::from(self.row.clone())),
                Arc::new(UInt32Array::from(self.lane.clone())),
                Arc::new(BooleanArray::from(self.present.clone())),
                Arc::new(UInt32Array::from(self.value.clone())),
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

/// What one write did to a property (D-STO-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// The property was set; `None` is absent (inserted without a value, or
    /// cleared).
    Set(Option<Value>),
    /// The record was deleted. Ordinals are never reused, so nothing follows.
    Deleted,
}

/// One entry of a property's history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The table version (= Lance version) the write landed in.
    pub version: u64,
    /// The mailbox the batch wrote on behalf of.
    pub owner: MailboxId,
    /// What the write did.
    pub change: Change,
}

/// An append-only Lance log at `uri`, with its records resident.
///
/// Lance is the persistence clock: it is read once, by [`Log::replay`], and
/// written once per batch, by [`Log::commit`]. Every record either one
/// touches stays resident, so reads ([`Log::history`]) fold over memory and
/// never open the dataset. A Lance scan is what recovery pays (P-HIST-1:
/// ~320 ms over 1,000 fragments); a fold over the same records costs
/// nanoseconds each (lance-graph, 2026-09-24: one write buys about a
/// million folds).
#[derive(Default)]
pub struct Log {
    uri: String,
    /// Every record of versions `1..=version`, in append order.
    records: Columns,
    /// The last version the resident records hold.
    version: u64,
}

impl std::fmt::Debug for Log {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Log")
            .field("uri", &self.uri)
            .field("version", &self.version)
            .field("records", &self.records.len())
            .finish()
    }
}

impl Log {
    /// The log at `uri` (created by the first commit). Nothing is resident
    /// until [`Log::replay`]; a log that already holds batches must be
    /// replayed before it can be committed to.
    pub fn new(uri: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            ..Self::default()
        }
    }

    /// How many records are resident.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether no record is resident.
    pub fn is_empty(&self) -> bool {
        self.records.len() == 0
    }

    async fn latest(&self) -> Result<u64, LogError> {
        match Dataset::open(&self.uri).await {
            Ok(ds) => Ok(ds.version().version),
            // No dataset yet is version 0, like an empty table.
            Err(lance::Error::DatasetNotFound { .. }) => Ok(0),
            Err(e) => Err(lance_err(e)),
        }
    }

    /// Validate `batch`, append it as one Lance version, then apply it to
    /// the table and to the resident records.
    ///
    /// # Errors
    ///
    /// - [`LogError::Store`]: the batch was refused; nothing was written.
    /// - [`LogError::Diverged`]: the resident records, or the log on disk,
    ///   were not at the table's version before the append, or the log was
    ///   not one past it after; nothing was applied.
    /// - [`LogError::Lance`]: the append failed; nothing was applied.
    pub async fn commit(&mut self, table: &mut Table, batch: &Batch) -> Result<Applied, LogError> {
        table.check(batch)?;
        let before = table.version();
        if self.version != before {
            return Err(LogError::Diverged {
                expected: before,
                found: self.version,
            });
        }
        let found = self.latest().await?;
        if found != before {
            return Err(LogError::Diverged {
                expected: before,
                found,
            });
        }
        let first = u32::try_from(table.n_rows()).map_err(|_| StoreError::Full)?;
        let records = encode(batch, before + 1, first);
        let params = WriteParams {
            mode: if before == 0 {
                WriteMode::Create
            } else {
                WriteMode::Append
            },
            ..WriteParams::default()
        };
        let ds = Dataset::write(
            RecordBatchIterator::new(vec![Ok(records.batch()?)], schema()),
            &self.uri,
            Some(params),
        )
        .await
        .map_err(lance_err)?;
        let after = ds.version().version;
        // Reached only when another writer appends between the check above
        // and this write, which ownership rules out. No test can schedule
        // that; a disable run stays green (the sequential case is caught
        // above). Kept because nothing else would notice a broken deployment.
        if after != before + 1 {
            return Err(LogError::Diverged {
                expected: before + 1,
                found: after,
            });
        }
        let applied = table.apply(batch)?;
        self.records.extend(records);
        self.version = applied.version;
        Ok(applied)
    }

    /// Every record of the log, in storage order.
    async fn scan(&self) -> Result<Vec<RecordBatch>, LogError> {
        let ds = Dataset::open(&self.uri).await.map_err(lance_err)?;
        ds.scan()
            .try_into_stream()
            .await
            .map_err(lance_err)?
            .try_collect()
            .await
            .map_err(lance_err)
    }

    /// D-STO-5: the history of property `col` on record `row`, oldest first.
    ///
    /// Every write of the property is one [`Entry`]: the value given when the
    /// record was inserted (absent included), every later update, and the
    /// record's deletion. Each entry carries the version and the owner the
    /// batch wrote on behalf of. Nothing is stored for history and nothing is
    /// read from Lance: one Quack program,
    /// `row = r AND ((lane = c AND op = SET) OR op = DELETE)`, runs on
    /// mask-risc over the resident records, and only matches are decoded.
    ///
    /// Only records that passed [`Table::check`] (through `commit`) or were
    /// applied by a successful [`Log::replay`] are resident, so every match
    /// decodes against `specs` the table was built with.
    ///
    /// # Errors
    ///
    /// [`LogError::Corrupt`] if `col` is not a lane of `specs`.
    pub fn history(&self, specs: &[LaneSpec], row: u32, col: Col) -> Result<Vec<Entry>, LogError> {
        let spec = *specs
            .get(usize::from(col.0))
            .ok_or(LogError::Corrupt("lane"))?;
        let r = &self.records;
        let n = r.len();
        if n == 0 {
            return Ok(Vec::new());
        }
        // Lanes of the program: 0 = row, 1 = lane, 2 = op.
        let eq = |c: u16, v: u32| Filter::cmp(Col(c), Cmp::EqU32(v));
        let program = lower(&Query {
            filter: Filter::and([
                eq(0, row),
                Filter::or([
                    Filter::and([eq(1, u32::from(col.0)), eq(2, SET)]),
                    eq(2, DELETE),
                ]),
            ]),
            agg: Agg::Rows,
        })
        .map_err(|_| LogError::Corrupt("history program"))?;
        let lanes = [
            LaneRef::U32(&r.row),
            LaneRef::U32(&r.lane),
            LaneRef::U32(&r.op),
        ];
        let planes = Planes {
            n_rows: n,
            masks: &[],
            lanes: &lanes,
        };
        let mut hits = vec![0u64; words_for(n)];
        let mut scratch =
            Scratch::for_program(&program, n).map_err(|_| LogError::Corrupt("history scratch"))?;
        execute_into(
            &program,
            &planes,
            &Foreign::NONE,
            &mut scratch,
            Out::Mask(&mut hits),
        )
        .map_err(|_| LogError::Corrupt("history program"))?;
        // Append order is version order (the log is never compacted), so the
        // matches are already oldest first.
        Ok(materialize_rows(&hits, n)
            .into_iter()
            .map(|i| Entry {
                version: r.version[i],
                owner: r.owner[i],
                change: if r.op[i] == DELETE {
                    Change::Deleted
                } else {
                    Change::Set(r.present[i].then_some(match spec.kind {
                        LaneKind::U32 => Value::U32(r.value[i]),
                        LaneKind::I32 => Value::I32(r.value[i].cast_signed()),
                    }))
                },
            })
            .collect())
    }

    /// Rebuild the table of `owner` with `specs` from the log, batch by
    /// batch, through the same [`Table::apply`] a live writer uses, and make
    /// the log's records resident.
    ///
    /// This is the one place Lance is read. On any error nothing becomes
    /// resident: the log keeps what it held before.
    ///
    /// # Errors
    ///
    /// [`LogError::Owner`] for a record of another owner,
    /// [`LogError::Corrupt`] for a record that does not decode,
    /// [`LogError::Store`] if a replayed batch is refused, and
    /// [`LogError::Diverged`] if the log's Lance version is not the number
    /// of batches in it.
    pub async fn replay(
        &mut self,
        owner: MailboxId,
        specs: &[LaneSpec],
    ) -> Result<Table, LogError> {
        let mut table = Table::new(owner, specs)?;
        let latest = self.latest().await?;
        if latest == 0 {
            self.records = Columns::default();
            self.version = 0;
            return Ok(table);
        }
        let mut records = Columns::default();
        for b in &self.scan().await? {
            let version = column::<UInt64Array>(b, 0, "version")?;
            let own = column::<UInt32Array>(b, 1, "owner")?;
            let op = column::<UInt32Array>(b, 2, "op")?;
            let row = column::<UInt32Array>(b, 3, "row")?;
            let lane = column::<UInt32Array>(b, 4, "lane")?;
            let present = column::<BooleanArray>(b, 5, "present")?;
            let value = column::<UInt32Array>(b, 6, "value")?;
            records.version.extend_from_slice(version.values());
            records.owner.extend_from_slice(own.values());
            records.op.extend_from_slice(op.values());
            records.row.extend_from_slice(row.values());
            records.lane.extend_from_slice(lane.values());
            records.present.extend(present.values().iter());
            records.value.extend_from_slice(value.values());
        }
        // No sort: records come back in append order, so each batch's records
        // are contiguous and in written order. A log out of order is caught
        // below, as a batch whose version is not the next one.
        let mut i = 0;
        let n = records.len();
        while i < n {
            let version = records.version[i];
            let end = records.version[i..]
                .iter()
                .position(|&v| v != version)
                .map_or(n, |p| i + p);
            let batch = decode(&records, i..end, owner, &table)?;
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
        self.records = records;
        self.version = table.version();
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
    records: &Columns,
    range: std::ops::Range<usize>,
    owner: MailboxId,
    table: &Table,
) -> Result<Batch, LogError> {
    let specs = table.specs();
    let first = u32::try_from(table.n_rows()).map_err(|_| StoreError::Full)?;
    let mut batch = Batch {
        owner,
        ..Batch::default()
    };
    for k in range {
        let (own, op, row, lane, present, value) = (
            records.owner[k],
            records.op[k],
            records.row[k],
            records.lane[k],
            records.present[k],
            records.value[k],
        );
        let lane = u16::try_from(lane).map_err(|_| LogError::Corrupt("lane"))?;
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

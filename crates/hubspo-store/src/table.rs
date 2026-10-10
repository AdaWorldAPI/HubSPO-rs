//! D-STO-1 (resident half): one dataset's records as resident lanes, changed
//! only by whole batches stamped with the dataset's owner.
//!
//! A [`Table`] is the in-memory image of one dataset (tenant × object type,
//! plan §5.5): a live plane, one validity plane per nullable lane, and one
//! value lane per property. Every read borrows it ([`Table::planes`]); no
//! read copies a lane.
//!
//! Writes arrive as a [`Batch`]: inserts, updates and deletes that land
//! together or not at all, and become exactly one new version. The batch
//! names the mailbox it writes for; a table accepts only batches for its own
//! owner (write on behalf, one writer per mailbox — the contract's
//! `SoaEnvelope::mailbox_owner`). Applying a batch returns a plane of the
//! rows it touched, which is what an active list's refresh needs
//! (`hubspo-query::list::refresh`).
//!
//! Ordinals are never reused: inserts append, deletes clear the live bit and
//! leave the row in place. Static lists rely on that.
//!
//! Persistence (one Lance version per published batch, P-SPARSE-1's
//! base + sparse delta) is the other half of D-STO-1.

use lance_graph_contract::collapse_gate::MailboxId;
use lance_graph_mask_risc::{LaneRef, words_for};
use lance_graph_quack::{Col, Mask};

/// What a lane holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneKind {
    /// Codes and foreign keys.
    U32,
    /// Signed integers (amounts in minor units, days, seconds).
    I32,
}

/// One lane's declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaneSpec {
    /// What the lane holds.
    pub kind: LaneKind,
    /// Whether a record may have no value.
    pub nullable: bool,
}

/// One property value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value {
    /// For a `U32` lane.
    U32(u32),
    /// For an `I32` lane.
    I32(i32),
}

/// A write that lands as one version or not at all.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Batch {
    /// The mailbox this batch writes on behalf of.
    pub owner: MailboxId,
    /// New records; one value (or none) per lane, in lane order.
    pub inserts: Vec<Vec<Option<Value>>>,
    /// `(row, lane, value)`; `None` clears a nullable property.
    pub updates: Vec<(u32, Col, Option<Value>)>,
    /// Rows to delete.
    pub deletes: Vec<u32>,
}

/// Why a batch was refused. Nothing is applied when a batch is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    /// The batch writes for another mailbox than the table's owner.
    OwnerMismatch {
        /// The table's owner.
        table: MailboxId,
        /// The batch's owner.
        batch: MailboxId,
    },
    /// An insert does not give one entry per lane.
    WrongArity {
        /// Lanes the table has.
        expected: usize,
        /// Entries given.
        got: usize,
    },
    /// No such lane.
    UnknownLane(Col),
    /// A value of the wrong kind for its lane.
    KindMismatch(Col),
    /// No value for a lane that is not nullable.
    Required(Col),
    /// The row does not exist or is deleted.
    RowNotLive(u32),
    /// The same row is written twice in one batch (two updates of one lane,
    /// or an update and a delete). Order inside a batch carries no meaning,
    /// so the batch would be ambiguous.
    Conflict(u32),
    /// The table would exceed `u32::MAX` rows.
    Full,
    /// More lanes than a `Col` can address.
    TooManyLanes,
    /// A batch with no inserts, updates or deletes: it would be a version
    /// that changes nothing.
    EmptyBatch,
}

/// What one applied batch did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// The version the batch became.
    pub version: u64,
    /// The first ordinal it inserted (equal to the old row count).
    pub first_insert: u32,
    /// One bit per row it inserted, updated or deleted, over the new row
    /// count.
    pub changed: Vec<u64>,
}

enum Lane {
    U32(Vec<u32>),
    I32(Vec<i32>),
}

/// One dataset's records, resident. See the module docs.
pub struct Table {
    owner: MailboxId,
    version: u64,
    n_rows: usize,
    specs: Vec<LaneSpec>,
    lanes: Vec<Lane>,
    /// Plane 0 is `live`; plane `1 + k` is the validity of the `k`-th
    /// nullable lane.
    planes: Vec<Vec<u64>>,
    validity: Vec<Option<usize>>,
}

/// The live plane of every table.
pub const LIVE: Mask = Mask(0);

fn set(words: &mut [u64], row: usize, on: bool) {
    let bit = 1u64 << (row % 64);
    if on {
        words[row / 64] |= bit;
    } else {
        words[row / 64] &= !bit;
    }
}

fn get(words: &[u64], row: usize) -> bool {
    words[row / 64] >> (row % 64) & 1 == 1
}

impl Table {
    /// An empty table owned by `owner`, at version 0.
    ///
    /// # Errors
    ///
    /// [`StoreError::TooManyLanes`] past `u16::MAX` lanes, or past
    /// `u16::MAX` planes.
    pub fn new(owner: MailboxId, specs: &[LaneSpec]) -> Result<Self, StoreError> {
        if specs.len() > usize::from(u16::MAX) {
            return Err(StoreError::TooManyLanes);
        }
        let mut validity = Vec::with_capacity(specs.len());
        let mut planes = vec![Vec::new()];
        for s in specs {
            if s.nullable {
                validity.push(Some(planes.len()));
                planes.push(Vec::new());
            } else {
                validity.push(None);
            }
        }
        Ok(Self {
            owner,
            version: 0,
            n_rows: 0,
            specs: specs.to_vec(),
            lanes: specs
                .iter()
                .map(|s| match s.kind {
                    LaneKind::U32 => Lane::U32(Vec::new()),
                    LaneKind::I32 => Lane::I32(Vec::new()),
                })
                .collect(),
            planes,
            validity,
        })
    }

    /// The mailbox every batch must write for.
    pub fn owner(&self) -> MailboxId {
        self.owner
    }

    /// The last applied version (0 before the first batch).
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Rows ever inserted, deleted ones included.
    pub fn n_rows(&self) -> usize {
        self.n_rows
    }

    /// The validity plane of a nullable lane, for the catalog
    /// (`FieldSpec::validity`).
    pub fn validity(&self, col: Col) -> Option<Mask> {
        let plane = (*self.validity.get(usize::from(col.0))?)?;
        u16::try_from(plane).ok().map(Mask)
    }

    /// The lane declarations, in lane order.
    pub fn specs(&self) -> &[LaneSpec] {
        &self.specs
    }

    /// Whether `row` is live.
    pub fn is_live(&self, row: u32) -> bool {
        (row as usize) < self.n_rows && get(&self.planes[0], row as usize)
    }

    /// The planes and lanes for mask-risc, borrowed.
    pub fn planes(&self) -> (Vec<&[u64]>, Vec<LaneRef<'_>>) {
        let masks = self.planes.iter().map(Vec::as_slice).collect();
        let lanes = self
            .lanes
            .iter()
            .map(|l| match l {
                Lane::U32(v) => LaneRef::U32(v),
                Lane::I32(v) => LaneRef::I32(v),
            })
            .collect();
        (masks, lanes)
    }

    fn check_value(&self, col: Col, v: Option<Value>) -> Result<(), StoreError> {
        let spec = self
            .specs
            .get(usize::from(col.0))
            .ok_or(StoreError::UnknownLane(col))?;
        match (v, spec.kind) {
            (None, _) if spec.nullable => Ok(()),
            (None, _) => Err(StoreError::Required(col)),
            (Some(Value::U32(_)), LaneKind::U32) | (Some(Value::I32(_)), LaneKind::I32) => Ok(()),
            _ => Err(StoreError::KindMismatch(col)),
        }
    }

    /// Whether [`Table::apply`] would accept `batch`, without changing
    /// anything. A writer that persists first checks, then persists, then
    /// applies (`log::Log::commit`).
    ///
    /// # Errors
    ///
    /// The [`StoreError`] `apply` would return.
    pub fn check(&self, batch: &Batch) -> Result<(), StoreError> {
        if batch.owner != self.owner {
            return Err(StoreError::OwnerMismatch {
                table: self.owner,
                batch: batch.owner,
            });
        }
        if batch.inserts.is_empty() && batch.updates.is_empty() && batch.deletes.is_empty() {
            return Err(StoreError::EmptyBatch);
        }
        if self
            .n_rows
            .checked_add(batch.inserts.len())
            .is_none_or(|n| u32::try_from(n).is_err())
        {
            return Err(StoreError::Full);
        }
        for record in &batch.inserts {
            if record.len() != self.specs.len() {
                return Err(StoreError::WrongArity {
                    expected: self.specs.len(),
                    got: record.len(),
                });
            }
            // `new` caps the lane count at `u16::MAX`, so every index fits.
            for (k, v) in (0..=u16::MAX).zip(record) {
                self.check_value(Col(k), *v)?;
            }
        }
        let mut written: Vec<(u32, u16)> = Vec::with_capacity(batch.updates.len());
        for &(row, col, v) in &batch.updates {
            if !self.is_live(row) {
                return Err(StoreError::RowNotLive(row));
            }
            self.check_value(col, v)?;
            written.push((row, col.0));
        }
        written.sort_unstable();
        if let Some(w) = written.windows(2).find(|w| w[0] == w[1]) {
            return Err(StoreError::Conflict(w[0].0));
        }
        let mut deleted = batch.deletes.clone();
        deleted.sort_unstable();
        if let Some(w) = deleted.windows(2).find(|w| w[0] == w[1]) {
            return Err(StoreError::Conflict(w[0]));
        }
        for &row in &deleted {
            if !self.is_live(row) {
                return Err(StoreError::RowNotLive(row));
            }
            if written.binary_search_by(|w| w.0.cmp(&row)).is_ok() {
                return Err(StoreError::Conflict(row));
            }
        }
        Ok(())
    }

    fn write(&mut self, row: usize, k: usize, v: Option<Value>) {
        if let Some(plane) = self.validity[k] {
            set(&mut self.planes[plane], row, v.is_some());
        }
        // A cleared value keeps its old payload: the validity plane, not the
        // lane, says it is absent.
        match (&mut self.lanes[k], v) {
            (Lane::U32(lane), Some(Value::U32(x))) => lane[row] = x,
            (Lane::I32(lane), Some(Value::I32(x))) => lane[row] = x,
            _ => {}
        }
    }

    /// Apply `batch` as one new version.
    ///
    /// # Errors
    ///
    /// The first [`StoreError`]; the table is unchanged when a batch is
    /// refused.
    pub fn apply(&mut self, batch: &Batch) -> Result<Applied, StoreError> {
        self.check(batch)?;
        let first = self.n_rows;
        let n = first + batch.inserts.len();
        let words = words_for(n);
        for plane in &mut self.planes {
            plane.resize(words, 0);
        }
        for lane in &mut self.lanes {
            match lane {
                Lane::U32(v) => v.resize(n, 0),
                Lane::I32(v) => v.resize(n, 0),
            }
        }
        self.n_rows = n;
        let mut changed = vec![0u64; words];
        for (i, record) in batch.inserts.iter().enumerate() {
            let row = first + i;
            set(&mut self.planes[0], row, true);
            for (k, v) in record.iter().enumerate() {
                self.write(row, k, *v);
            }
            set(&mut changed, row, true);
        }
        for &(row, col, v) in &batch.updates {
            self.write(row as usize, usize::from(col.0), v);
            set(&mut changed, row as usize, true);
        }
        for &row in &batch.deletes {
            set(&mut self.planes[0], row as usize, false);
            set(&mut changed, row as usize, true);
        }
        self.version += 1;
        Ok(Applied {
            version: self.version,
            first_insert: u32::try_from(first).unwrap_or(u32::MAX),
            changed,
        })
    }
}

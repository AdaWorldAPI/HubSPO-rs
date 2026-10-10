//! # hubspo-store
//!
//! Write lanes, the batch image, publish-one-Lance-version, the KV value store, association edge lanes and list masks. Writes are stamped with their mailbox owner.
//!
//! - Zone: zone 2 (no serde, no wire formats).
//! - Deliverables: D-STO-1..6 (`.claude/plans/hubspo-integration-v1.md`).
//! - [`catalog`]: the table and field catalog every Quack read binds
//!   through (H-ADR-6).
//! - [`table`] (D-STO-1, resident half): one dataset's resident lanes, changed
//!   only by whole owner-stamped batches, one version each.
//! - [`log`] (D-STO-1, persistence half, feature `lance`): every batch is one
//!   Lance version of an append-only log; a table is rebuilt by replay.

pub mod catalog;
#[cfg(feature = "lance")]
pub mod log;
pub mod table;

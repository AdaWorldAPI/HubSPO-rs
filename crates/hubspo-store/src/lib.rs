//! # hubspo-store
//!
//! Write lanes, the batch image, publish-one-Lance-version, the KV value store, association edge lanes and list masks. Writes are stamped with their mailbox owner.
//!
//! - Zone: zone 2 (no serde, no wire formats).
//! - Deliverables: D-STO-1..6 (`.claude/plans/hubspo-integration-v1.md`).
//! - First module: [`catalog`], the table and field catalog every Quack read
//!   binds through (H-ADR-6). Storage lands with D-STO-1.

pub mod catalog;

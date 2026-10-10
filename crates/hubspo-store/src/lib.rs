//! # hubspo-store
//!
//! Write lanes, the batch image, publish-one-Lance-version, the KV value store, association edge lanes and list masks. Writes are stamped with their mailbox owner.
//!
//! - Zone: zone 2 (no serde, no wire formats).
//! - Deliverables: D-STO-1..6 (`.claude/plans/hubspo-integration-v1.md`).
//! - First real code lands in wave W2. Until then this crate is empty on
//!   purpose; Wave 0 only fixes the workspace shape.

//! # hubspo-iam
//!
//! Roles, the grant source, per-role field masks and directory actors over ogar-rbac and dir-sim.
//!
//! - Zone: zone 2 (no serde, no wire formats).
//! - Deliverables: D-IAM-1..7 (`.claude/plans/hubspo-integration-v1.md`).
//! - [`scope`]: D-IAM-2, HubSpot's ownership scopes as a row filter over the
//!   owner lane.

pub mod scope;

//! # hubspo-iam
//!
//! Roles, the grant source, per-role field masks and directory actors over ogar-rbac and dir-sim.
//!
//! - Zone: zone 2 (no serde, no wire formats).
//! - Deliverables: D-IAM-1..7 (`.claude/plans/hubspo-integration-v1.md`).
//! - [`fields`]: the D-IAM-3 contract, per-property read authorization by
//!   stable ordinal, independent of the packed run.
//! - [`roles`]: D-IAM-1, HubSpot's preset roles as grants over the plugged
//!   concepts, the bind of `hubspo_port::RBAC_PLUG`, and the request's roles
//!   read from the directory.
//! - [`scope`]: D-IAM-2, HubSpot's ownership scopes as a row filter over the
//!   owner lane.

pub mod fields;
pub mod roles;
pub mod scope;

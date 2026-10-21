//! # hubspo-iam
//!
//! Roles, the grant source, per-role field masks and directory actors over ogar-rbac and dir-sim.
//!
//! - Zone: zone 2 (no serde, no wire formats).
//! - Deliverables: D-IAM-1..7 (`.claude/plans/hubspo-integration-v1.md`).
//! - [`access`]: D-IAM-3, one request's effective access to one concept,
//!   compiled once from the tenant's permission sets and stamped.
//! - [`fields`]: the D-IAM-3 contract, per-property read authorization by
//!   stable ordinal, independent of the packed run.
//! - [`roles`]: D-IAM-1, the six role names bound to OGAR with no grants,
//!   the preset verb table as a fixture, and the request's roles read from
//!   the directory.
//! - [`scope`]: D-IAM-2, HubSpot's ownership scopes as a row filter over the
//!   owner lane.
//! - [`tenant`]: the tenant's permission sets, versioned data with stable
//!   ids assigned to roles or principals, and the seat ceiling.

pub mod access;
pub mod fields;
pub mod roles;
pub mod scope;
pub mod tenant;

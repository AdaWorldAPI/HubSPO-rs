//! # hubspo-compat
//!
//! The HubSpot CRM v3-shaped REST surface, the importer from a real HubSpot portal, and the value-oracle client. Holds no state; lowers every call to an action or a Quack read.
//!
//! - Zone: zone 3 (wire formats allowed).
//! - Deliverables: D-API-1..5, D-STO-6 (`.claude/plans/hubspo-integration-v1.md`).
//! - First real code lands in wave W11. Until then this crate is empty on
//!   purpose; Wave 0 only fixes the workspace shape.

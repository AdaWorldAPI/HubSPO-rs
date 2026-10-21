//! # hubspo-compat
//!
//! The HubSpot CRM v3-shaped REST surface, the importer from a real HubSpot portal, and the value-oracle client. Holds no state; lowers every call to an action or a Quack read.
//!
//! - Zone: zone 3 (wire formats allowed).
//! - Deliverables: D-API-1..5, D-STO-6 (`.claude/plans/hubspo-integration-v1.md`).
//! - [`contacts`]: the first authorized output path (D-IAM-3): one contact
//!   by id, and the gate a search's filters and sorts pass before they run.
//!   The REST wire format around it lands with D-API-1.

pub mod contacts;

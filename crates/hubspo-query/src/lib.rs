//! # hubspo-query
//!
//! Filters, lists, views and reports as Quack programs and `lance-graph-report` plans. No DataFusion: it is in a grace period and nothing new goes to it.
//!
//! - Zone: zone 2 (no serde, no wire formats).
//! - Deliverables: D-QRY-1..7 (`.claude/plans/hubspo-integration-v1.md`).
//! - [`filter`] (D-QRY-1): HubSpot filter groups → one Quack [`Filter`].
//!
//! [`Filter`]: lance_graph_quack::Filter

pub mod filter;

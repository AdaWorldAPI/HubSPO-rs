//! # hubspo-generated
//!
//! Output of the checked-in generator. Every file carries an `@generated` header and is never hand-edited. Depends on `lance-graph-contract` only.
//!
//! - Zone: zone 2 (no serde, no wire formats).
//! - Deliverables: D-ONT-6 (`.claude/plans/hubspo-integration-v1.md`).
//!
//! | module | generated from | by |
//! |---|---|---|
//! | [`standard_properties`] | `data/hubspot/standard_properties.tsv` | `hubspo-harvest emit` (`scripts/fetch-hubspot-kb.sh` runs it) |
//!
//! `hubspo-harvest`'s test `the_committed_generated_file_matches_the_table`
//! fails when a module and its table disagree.

// Generated code is kept byte-for-byte as the generator writes it.
#[rustfmt::skip]
pub mod standard_properties;

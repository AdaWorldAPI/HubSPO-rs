//! # hubspo-sim
//!
//! crm-sim: every bulk change is simulated as a version, validated by invariant programs, diffed and turned into a reviewable plan before anything is written (H-ADR-10).
//!
//! - Zone: zone 2 (no serde, no wire formats).
//! - Deliverables: D-SIM-1..3 (`.claude/plans/hubspo-integration-v1.md`).
//! - First real code lands in wave W4. Until then this crate is empty on
//!   purpose; Wave 0 only fixes the workspace shape.

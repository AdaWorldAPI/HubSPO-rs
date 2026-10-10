//! # hubspo-comms
//!
//! Adapters to stalwart (JMAP, signed webhooks, MTA hooks) and the tracking endpoints, plus spear wiring. stalwart is never linked: it runs as its own process (AGPL boundary, H-ADR-8).
//!
//! - Zone: zone 3 (wire formats allowed).
//! - Deliverables: D-MAIL-1..10 (`.claude/plans/hubspo-integration-v1.md`).
//! - First real code lands in wave W6. Until then this crate is empty on
//!   purpose; Wave 0 only fixes the workspace shape.

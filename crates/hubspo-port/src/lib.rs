//! # hubspo-port
//!
//! The one doorway to OGAR's codebook: the `HotPlug` and `RbacPlug` declarations and `class_id(name)` lookups over `HubSpoPort`. Holds no codebook, no concept hex, no name→id map.
//!
//! - Zone: zone 2 (no serde, no wire formats).
//! - Deliverables: D-ONT-5 (`.claude/plans/hubspo-integration-v1.md`).
//! - First real code lands in wave W1. Until then this crate is empty on
//!   purpose; Wave 0 only fixes the workspace shape.

// The two types this crate is built from in Wave 1. Re-exported now so the
// workspace resolves the real git dependencies from day one.
pub use lance_graph_contract::hotplug::HotPlug;
pub use ogar_vocab::ports::PortSpec;

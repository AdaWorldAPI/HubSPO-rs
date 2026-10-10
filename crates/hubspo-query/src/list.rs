//! D-QRY-2: active lists, static lists and membership history as mask
//! programs (H-ADR-5).
//!
//! A list's membership is a resident mask plane over record ordinals; the
//! store owns its words. Everything here builds a Quack [`Filter`] over
//! planes the caller names, so every list operation runs on mask-risc like
//! any other query: no row ids are materialised and no Rust loop walks the
//! population.
//!
//! - **Active list.** Membership is the compiled predicate
//!   ([`crate::filter::compile`]). After a write, [`refresh`] re-evaluates the
//!   predicate only where records changed and keeps the stored bits
//!   elsewhere: `(prev ∧ ¬changed) ∨ (predicate ∧ changed)`. A deleted record
//!   is a changed record whose live bit is clear, so it leaves the list with
//!   no special case — the compiled predicate already conjoins `live`.
//!   P-LIST-1 / P-SPARSE-1 measured the step at 0.85–2.6 ms up to 10M rows.
//! - **Static list.** Membership is whatever was added. [`add`] and
//!   [`remove`] take a plane of the records named in the request. A static
//!   list's stored bits can outlive a deleted record, so readers go through
//!   [`members`], which conjoins `live`. This relies on ordinals never being
//!   reused (see [`add`]).
//! - **History.** Between two stored versions of one list, [`added`] and
//!   [`removed`] are the two differences.
//!
//! [`Filter`]: lance_graph_quack::Filter

use lance_graph_quack::{Filter, Mask};

/// The active list's membership after a write that touched `changed`.
///
/// `predicate` is the list's compiled filter (it includes `live`); `prev` is
/// the stored membership; `changed` marks every record inserted, updated or
/// deleted since `prev` was stored.
pub fn refresh(predicate: &Filter, prev: Mask, changed: Mask) -> Filter {
    Filter::or([
        Filter::and([Filter::plane(prev), Filter::negate(Filter::plane(changed))]),
        Filter::and([predicate.clone(), Filter::plane(changed)]),
    ])
}

/// A static list after adding the records in `rows`.
///
/// Not gated on `live`: a bit stored for a deleted record is inert, because
/// every read goes through [`members`]. That holds only while ordinals are
/// never reused, which the store guarantees (records are appended, the base
/// is never rewritten; P-SPARSE-1). A store that reused ordinals would have
/// to clear a deleted record's bit in every static list first.
pub fn add(prev: Mask, rows: Mask) -> Filter {
    Filter::or([Filter::plane(prev), Filter::plane(rows)])
}

/// A static list after removing the records in `rows`.
pub fn remove(prev: Mask, rows: Mask) -> Filter {
    Filter::and([Filter::plane(prev), Filter::negate(Filter::plane(rows))])
}

/// A list's current members: its stored bits restricted to live records.
pub fn members(list: Mask, live: Mask) -> Filter {
    Filter::and([Filter::plane(list), Filter::plane(live)])
}

/// Records in `next` that were not in `prev`.
pub fn added(prev: Mask, next: Mask) -> Filter {
    Filter::and([Filter::plane(next), Filter::negate(Filter::plane(prev))])
}

/// Records in `prev` that are not in `next`.
pub fn removed(prev: Mask, next: Mask) -> Filter {
    Filter::and([Filter::plane(prev), Filter::negate(Filter::plane(next))])
}

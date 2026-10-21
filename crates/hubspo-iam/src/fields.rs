//! D-IAM-3 contract: per-property read authorization, independent of where
//! a property is stored.
//!
//! Operator ruling 2026-10-21 (`ISS-PACKED-RUN-RBAC-GRAIN`, option b):
//! storage grain may be a packed run, authorization grain is the property.
//! Two masks decide a read, and both must admit it:
//!
//! - **Outer** — the role's `WideFieldMask` over ClassView field positions
//!   (lanes `1..=255`, the packed run at position 0). Physical: it grants a
//!   column, and for position 0 only the container.
//! - **Inner** — the role's [`PropertyGrants`]: a set of stable property
//!   ordinals (the table position, `StandardProperty::position`). Logical:
//!   it never mentions a lane or the run, so moving a property between a
//!   lane and the run changes no grant.
//!
//! ## Why not a `WideFieldMask` for the inner mask
//!
//! The existing contracts were checked first. `ClassRbac::field_mask` and
//! `GrantSource::field_mask_of` return a `WideFieldMask`, whose positions
//! are `u8` field positions: it cannot name contact's 272 property ordinals,
//! carries no schema version, and is keyed to the physical field list this
//! policy must not depend on. So the inner set is a local type here, not a
//! new upstream abstraction.
//!
//! ## Use, once per request
//!
//! 1. Resolve the actor's roles ([`crate::roles::request_actors`]) and
//!    apply tenant and ownership scope to the rows (D-IAM-2) first.
//! 2. [`Projection::compile`] once per concept: the union over the roles of
//!    outer ∧ inner, against one [`Schema`] version.
//! 3. Every output path (REST, UI, export, events, workflow payloads)
//!    passes values through [`Projection::project`]; every filter, sort and
//!    aggregate passes the properties it reads through
//!    [`Projection::require`]. A raw packed run never leaves: only
//!    `(ordinal, value)` pairs that survived the projection do.
//!
//! ## Fails closed
//!
//! A role with no inner grant on the concept admits nothing; an ordinal the
//! schema does not know is refused; a projection compiled against another
//! schema version refuses everything, so a property added later is never
//! exposed by a mask compiled before it existed. The same holds for the
//! concept (ordinal 8 of a deal is not ordinal 8 of a contact) and for the
//! grants: a projection carries a [`Stamp`] and every read presents the
//! current one, so a revoked grant stops authorizing at once.

use hubspo_generated::standard_properties::{PACKED_RUN_POSITION, Slot, StandardProperty};
use lance_graph_contract::class_view::WideFieldMask;
use lance_graph_contract::rbac::RoleId;

/// A tenant's property schema version for one concept. Bumped whenever a
/// property is added; a projection is only valid for the version it was
/// compiled against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SchemaVersion(pub u32);

/// One version of a concept's property table: ordinal `i` is `properties[i]`.
#[derive(Debug, Clone, Copy)]
pub struct Schema {
    /// The concept the table belongs to.
    pub concept: u16,
    /// The version this table is.
    pub version: SchemaVersion,
    /// The properties, by ordinal.
    pub properties: &'static [StandardProperty],
}

impl Schema {
    /// The ordinal of `name`, if the schema has it.
    #[must_use]
    pub fn ordinal(&self, name: &str) -> Option<u16> {
        self.properties
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.position)
    }

    /// The property at `ordinal`, if the schema has one.
    #[must_use]
    pub fn property(&self, ordinal: u16) -> Option<&StandardProperty> {
        self.properties
            .get(usize::from(ordinal))
            .filter(|p| p.position == ordinal)
    }
}

/// A set of property ordinals.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OrdinalSet {
    words: Vec<u64>,
}

impl OrdinalSet {
    /// The empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add `ordinal`.
    pub fn insert(&mut self, ordinal: u16) {
        let (w, b) = (usize::from(ordinal) / 64, ordinal % 64);
        if self.words.len() <= w {
            self.words.resize(w + 1, 0);
        }
        self.words[w] |= 1 << b;
    }

    /// Whether `ordinal` is in the set.
    #[must_use]
    pub fn contains(&self, ordinal: u16) -> bool {
        let (w, b) = (usize::from(ordinal) / 64, ordinal % 64);
        self.words.get(w).is_some_and(|word| word & (1 << b) != 0)
    }

    /// Add every member of `other`.
    pub fn union_with(&mut self, other: &OrdinalSet) {
        if self.words.len() < other.words.len() {
            self.words.resize(other.words.len(), 0);
        }
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a |= b;
        }
    }

    /// The members, ascending.
    pub fn iter(&self) -> impl Iterator<Item = u16> + '_ {
        (0..self.words.len() * 64)
            .filter_map(|i| u16::try_from(i).ok())
            .filter(|&o| self.contains(o))
    }
}

/// The inner grants: which property ordinals each role may read on each
/// concept. A `(role, concept)` with no entry grants nothing.
#[derive(Debug, Clone, Default)]
pub struct PropertyGrants {
    entries: Vec<(RoleId, u16, OrdinalSet)>,
    generation: u64,
}

/// Why a grant could not be recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrantError {
    /// The schema has no property of this name.
    UnknownProperty(String),
}

impl PropertyGrants {
    /// No grants.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Grant `role` read on the named properties of `schema`'s concept.
    ///
    /// # Errors
    ///
    /// [`GrantError::UnknownProperty`] for a name the schema does not have;
    /// nothing is recorded then.
    pub fn grant(
        &mut self,
        role: RoleId,
        schema: &Schema,
        names: &[&str],
    ) -> Result<(), GrantError> {
        let mut set = OrdinalSet::new();
        for name in names {
            let ordinal = schema
                .ordinal(name)
                .ok_or_else(|| GrantError::UnknownProperty((*name).to_string()))?;
            set.insert(ordinal);
        }
        self.entry(role, schema.concept).union_with(&set);
        self.generation += 1;
        Ok(())
    }

    /// Grant `role` read on every property `schema` has at its version.
    /// A property added in a later version is not included.
    pub fn grant_all(&mut self, role: RoleId, schema: &Schema) {
        let mut set = OrdinalSet::new();
        for p in schema.properties {
            set.insert(p.position);
        }
        self.entry(role, schema.concept).union_with(&set);
        self.generation += 1;
    }

    /// Withdraw every grant `role` holds on `concept`.
    pub fn revoke(&mut self, role: RoleId, concept: u16) {
        self.entries
            .retain(|(r, c, _)| !(*r == role && *c == concept));
        self.generation += 1;
    }

    /// Bumped by every change to the grants, so a projection compiled
    /// before a change is refused after it.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    fn entry(&mut self, role: RoleId, concept: u16) -> &mut OrdinalSet {
        let i = match self
            .entries
            .iter()
            .position(|(r, c, _)| *r == role && *c == concept)
        {
            Some(i) => i,
            None => {
                self.entries.push((role, concept, OrdinalSet::new()));
                self.entries.len() - 1
            }
        };
        &mut self.entries[i].2
    }

    /// The ordinals `role` may read on `concept`, if any grant exists.
    #[must_use]
    pub fn of(&self, role: RoleId, concept: u16) -> Option<&OrdinalSet> {
        self.entries
            .iter()
            .find(|(r, c, _)| *r == role && *c == concept)
            .map(|(_, _, s)| s)
    }
}

/// The version of the outer policy: the role assignments and field masks a
/// projection was compiled from. Its owner bumps it on every grant change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PolicyVersion(pub u64);

/// What a projection is valid for: one concept, one schema version, one
/// generation of the inner grants and one outer policy version. A read
/// presents the current stamp; any difference refuses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    /// The concept whose ordinals the projection names.
    pub concept: u16,
    /// The concept's schema version.
    pub schema: SchemaVersion,
    /// [`PropertyGrants::generation`].
    pub inner: u64,
    /// The outer policy version.
    pub outer: PolicyVersion,
    /// The version of the principal's seat (the ceiling on what any grant
    /// can unlock). [`SeatVersion::NONE`] where no seat model applies.
    pub seat: SeatVersion,
    /// The version of the principal's role resolution (the directory
    /// snapshot and the group-role policy it was read from).
    /// [`RoleVersion::NONE`] where roles are passed in directly.
    pub roles: RoleVersion,
}

/// The version of a request's role resolution. Changing a group's grants or
/// a principal's membership invalidates every projection compiled under the
/// old roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RoleVersion(pub u64);

impl RoleVersion {
    /// Roles passed in directly, not resolved from a directory.
    pub const NONE: Self = Self(0);
}

/// The version of a principal's seat assignment. Changing the seat
/// invalidates every projection compiled under the old one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SeatVersion(pub u32);

impl SeatVersion {
    /// No seat model: the role-keyed path of this module.
    pub const NONE: Self = Self(0);
}

impl Stamp {
    /// The current stamp for `schema` under `inner` and `outer`, with no
    /// seat model ([`SeatVersion::NONE`]).
    #[must_use]
    pub fn of(schema: &Schema, inner: &PropertyGrants, outer: PolicyVersion) -> Self {
        Self {
            concept: schema.concept,
            schema: schema.version,
            inner: inner.generation(),
            outer,
            seat: SeatVersion::NONE,
            roles: RoleVersion::NONE,
        }
    }

    /// The same stamp under `seat`.
    #[must_use]
    pub fn with_seat(self, seat: SeatVersion) -> Self {
        Self { seat, ..self }
    }

    /// The same stamp under role resolution `roles`.
    #[must_use]
    pub fn with_roles(self, roles: RoleVersion) -> Self {
        Self { roles, ..self }
    }
}

/// Why a read was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The projection was compiled for another concept, schema version or
    /// grant generation than the read presents.
    Stale {
        /// What it was compiled for.
        compiled: Stamp,
        /// What the read presents.
        current: Stamp,
    },
    /// The schema has no property at this ordinal.
    Unknown(u16),
    /// The property exists but the request may not read it.
    Hidden(u16),
}

/// One request's readable properties on one concept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Projection {
    stamp: Stamp,
    known: u16,
    readable: OrdinalSet,
}

impl Projection {
    /// Compile the projection for `roles` on `schema`'s concept: a property
    /// is readable when some role's outer mask admits its slot (its lane, or
    /// position 0 for the packed run) and the same role's inner grant
    /// admits its ordinal.
    ///
    /// `outer` gives a role's outer mask on the concept; `None` admits no
    /// field. `policy` is the outer policy version those masks come from.
    /// Call once per request.
    #[must_use]
    pub fn compile(
        schema: &Schema,
        roles: &[RoleId],
        outer: impl Fn(RoleId) -> Option<WideFieldMask>,
        policy: PolicyVersion,
        inner: &PropertyGrants,
    ) -> Self {
        let holders = roles
            .iter()
            .map(|&role| (outer(role), inner.of(role, schema.concept)));
        Self::compile_holders(schema, holders, Stamp::of(schema, inner, policy))
    }

    /// Compile from grant holders directly: each holder is one role or one
    /// permission set, given as its outer mask and its inner ordinals on the
    /// concept. A property is readable when one holder admits it on both
    /// masks; a holder missing either admits nothing. `stamp` is what the
    /// projection is valid for; its concept and schema must be `schema`'s.
    #[must_use]
    pub fn compile_holders<'a>(
        schema: &Schema,
        holders: impl IntoIterator<Item = (Option<WideFieldMask>, Option<&'a OrdinalSet>)>,
        stamp: Stamp,
    ) -> Self {
        let mut readable = OrdinalSet::new();
        let consistent = stamp.concept == schema.concept && stamp.schema == schema.version;
        for (mask, granted) in holders {
            let (Some(mask), Some(granted), true) = (mask, granted, consistent) else {
                continue;
            };
            for p in schema.properties {
                let position = match p.slot {
                    Slot::Lane(n) => n,
                    Slot::Packed => PACKED_RUN_POSITION,
                };
                if mask.has(position) && granted.contains(p.position) {
                    readable.insert(p.position);
                }
            }
        }
        Self {
            stamp,
            known: u16::try_from(schema.properties.len()).unwrap_or(u16::MAX),
            readable,
        }
    }

    /// The concept the projection is for.
    #[must_use]
    pub fn concept(&self) -> u16 {
        self.stamp.concept
    }

    /// What the projection is valid for.
    #[must_use]
    pub fn stamp(&self) -> Stamp {
        self.stamp
    }

    fn current(&self, current: &Stamp) -> Result<(), Refusal> {
        if *current == self.stamp {
            Ok(())
        } else {
            Err(Refusal::Stale {
                compiled: self.stamp,
                current: *current,
            })
        }
    }

    fn decide(&self, ordinal: u16) -> Result<(), Refusal> {
        if ordinal >= self.known {
            Err(Refusal::Unknown(ordinal))
        } else if self.readable.contains(ordinal) {
            Ok(())
        } else {
            Err(Refusal::Hidden(ordinal))
        }
    }

    /// The values of a row the request may see: every `(ordinal, value)`
    /// pair whose property is readable, in input order. Hidden and unknown
    /// properties are dropped, so they are absent from the output rather
    /// than masked in it.
    ///
    /// # Errors
    ///
    /// [`Refusal::Stale`] when `current` is not the compiled stamp (another
    /// concept, schema version or grant generation); then nothing is
    /// returned.
    pub fn project<V>(
        &self,
        current: &Stamp,
        values: impl IntoIterator<Item = (u16, V)>,
    ) -> Result<Vec<(u16, V)>, Refusal> {
        self.current(current)?;
        Ok(values
            .into_iter()
            .filter(|(o, _)| self.decide(*o).is_ok())
            .collect())
    }

    /// Check the properties a filter, sort or aggregate reads. A predicate
    /// over a hidden property would reveal it through which rows match, so
    /// it is refused, not evaluated.
    ///
    /// # Errors
    ///
    /// [`Refusal::Stale`], or the first [`Refusal::Unknown`] /
    /// [`Refusal::Hidden`] property in `reads`.
    pub fn require(&self, current: &Stamp, reads: &[u16]) -> Result<(), Refusal> {
        self.current(current)?;
        reads.iter().try_for_each(|&o| self.decide(o))
    }

    /// Whether the property named `name` in `schema` is readable under the
    /// `current` stamp; `false` when the stamp or `schema` is not the one
    /// the projection was compiled for.
    #[must_use]
    pub fn admits(&self, current: &Stamp, schema: &Schema, name: &str) -> bool {
        self.current(current).is_ok()
            && schema.concept == self.stamp.concept
            && schema.version == self.stamp.schema
            && schema
                .ordinal(name)
                .and_then(|o| schema.property(o))
                .is_some_and(|p| self.decide(p.position).is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hubspo_generated::standard_properties::CONTACT;
    use hubspo_port::{ROLE_SALES_USER, ROLE_VIEW_ONLY};
    use ogar_vocab::class_ids;

    const V1: SchemaVersion = SchemaVersion(1);
    const P0: PolicyVersion = PolicyVersion(1);

    fn contact() -> Schema {
        Schema {
            concept: class_ids::CRM_CONTACT,
            version: V1,
            properties: CONTACT,
        }
    }

    /// Every field of contact, outer-granted: lanes and the packed run.
    fn all_fields(_: RoleId) -> Option<WideFieldMask> {
        let lanes = CONTACT.iter().filter_map(|p| match p.slot {
            Slot::Lane(n) => Some(n),
            Slot::Packed => None,
        });
        Some(WideFieldMask::from_positions(
            &std::iter::once(PACKED_RUN_POSITION)
                .chain(lanes)
                .collect::<Vec<_>>(),
        ))
    }

    const PERSONAL: [&str; 4] = [
        "date_of_birth",
        "gender",
        "marital_status",
        "military_status",
    ];

    fn ord(name: &str) -> u16 {
        contact().ordinal(name).unwrap()
    }

    /// A row carrying every property, value = ordinal.
    fn full_row() -> Vec<(u16, u16)> {
        CONTACT.iter().map(|p| (p.position, p.position)).collect()
    }

    fn school_only() -> Projection {
        let mut inner = PropertyGrants::new();
        inner
            .grant(ROLE_SALES_USER, &contact(), &["school"])
            .unwrap();
        Projection::compile(&contact(), &[ROLE_SALES_USER], all_fields, P0, &inner)
    }

    #[test]
    fn the_fixture_packs_school_with_the_personal_attributes() {
        let s = contact();
        for name in std::iter::once("school").chain(PERSONAL) {
            let o = s.ordinal(name).unwrap();
            assert_eq!(s.property(o).unwrap().slot, Slot::Packed, "{name}");
        }
        assert!(CONTACT.len() > 256, "ordinals exceed a u8 field position");
    }

    #[test]
    fn reading_school_does_not_grant_the_personal_attributes() {
        let p = school_only();
        let out = p.project(&p.stamp(), full_row()).unwrap();
        assert_eq!(out, vec![(ord("school"), ord("school"))]);
        for name in PERSONAL {
            assert!(!p.admits(&p.stamp(), &contact(), name), "{name}");
            assert_eq!(
                p.require(&p.stamp(), &[ord(name)]),
                Err(Refusal::Hidden(ord(name)))
            );
        }
        assert!(p.admits(&p.stamp(), &contact(), "school"));
        assert_eq!(p.require(&p.stamp(), &[ord("school")]), Ok(()));
    }

    #[test]
    fn a_predicate_mixing_granted_and_hidden_properties_is_refused() {
        let p = school_only();
        assert_eq!(
            p.require(&p.stamp(), &[ord("school"), ord("date_of_birth")]),
            Err(Refusal::Hidden(ord("date_of_birth")))
        );
    }

    #[test]
    fn the_container_without_the_inner_grant_shows_nothing() {
        // Outer grants every field, the packed run included; no inner grant.
        let p = Projection::compile(
            &contact(),
            &[ROLE_SALES_USER],
            all_fields,
            P0,
            &PropertyGrants::new(),
        );
        assert!(p.project(&p.stamp(), full_row()).unwrap().is_empty());
    }

    #[test]
    fn the_inner_grant_without_the_container_shows_nothing_packed() {
        let mut inner = PropertyGrants::new();
        inner.grant_all(ROLE_SALES_USER, &contact());
        let lanes_only = |_: RoleId| {
            let lanes: Vec<u8> = CONTACT
                .iter()
                .filter_map(|p| match p.slot {
                    Slot::Lane(n) => Some(n),
                    Slot::Packed => None,
                })
                .collect();
            Some(WideFieldMask::from_positions(&lanes))
        };
        let p = Projection::compile(&contact(), &[ROLE_SALES_USER], lanes_only, P0, &inner);
        let out = p.project(&p.stamp(), full_row()).unwrap();
        assert_eq!(
            out.len(),
            CONTACT.iter().filter(|q| q.slot != Slot::Packed).count()
        );
        assert!(!p.admits(&p.stamp(), &contact(), "school"));
        assert!(p.admits(&p.stamp(), &contact(), "firstname"));
    }

    #[test]
    fn a_stale_projection_refuses_everything() {
        let p = school_only();
        let current = Stamp {
            schema: SchemaVersion(2),
            ..p.stamp()
        };
        let stale = Refusal::Stale {
            compiled: p.stamp(),
            current,
        };
        assert_eq!(p.project(&current, full_row()), Err(stale.clone()));
        assert_eq!(p.require(&current, &[ord("school")]), Err(stale));
        assert!(!p.admits(&current, &contact(), "school"));
    }

    #[test]
    fn a_projection_for_another_concept_is_refused() {
        // A deal projection at the same versions must not read a contact
        // row: ordinal 8 is a deal property there and date_of_birth here.
        let deal = Schema {
            concept: class_ids::CRM_DEAL,
            ..contact()
        };
        let mut inner = PropertyGrants::new();
        inner.grant_all(ROLE_SALES_USER, &deal);
        let p = Projection::compile(&deal, &[ROLE_SALES_USER], all_fields, P0, &inner);
        let contact_now = Stamp::of(&contact(), &inner, P0);
        assert_eq!(p.stamp().schema, contact_now.schema, "same version");
        assert!(matches!(
            p.project(&contact_now, full_row()),
            Err(Refusal::Stale { .. })
        ));
        assert!(p.require(&contact_now, &[ord("date_of_birth")]).is_err());
        assert!(!p.admits(&p.stamp(), &contact(), "date_of_birth"));
        // Silent twin: under its own stamp the deal projection reads.
        assert_eq!(p.require(&p.stamp(), &[8]), Ok(()));
    }

    #[test]
    fn a_grant_change_invalidates_the_projection() {
        let mut inner = PropertyGrants::new();
        inner
            .grant(ROLE_SALES_USER, &contact(), &["school"])
            .unwrap();
        let p = Projection::compile(&contact(), &[ROLE_SALES_USER], all_fields, P0, &inner);
        let before = Stamp::of(&contact(), &inner, P0);
        assert_eq!(p.require(&before, &[ord("school")]), Ok(()));
        // Inner revocation.
        inner.revoke(ROLE_SALES_USER, class_ids::CRM_CONTACT);
        let after = Stamp::of(&contact(), &inner, P0);
        assert!(matches!(
            p.project(&after, full_row()),
            Err(Refusal::Stale { .. })
        ));
        assert!(p.require(&after, &[ord("school")]).is_err());
        // Outer policy change (a role or field mask withdrawn elsewhere).
        let outer_changed = Stamp::of(&contact(), &inner, PolicyVersion(2));
        let recompiled =
            Projection::compile(&contact(), &[ROLE_SALES_USER], all_fields, P0, &inner);
        assert!(recompiled.require(&outer_changed, &[]).is_err());
        assert!(recompiled.project(&after, full_row()).unwrap().is_empty());
    }

    #[test]
    fn unknown_ordinals_are_dropped_and_refused() {
        let mut inner = PropertyGrants::new();
        inner.grant_all(ROLE_SALES_USER, &contact());
        let p = Projection::compile(&contact(), &[ROLE_SALES_USER], all_fields, P0, &inner);
        let past = u16::try_from(CONTACT.len()).unwrap();
        assert_eq!(p.project(&p.stamp(), [(past, 0u16)]).unwrap(), vec![]);
        assert_eq!(p.require(&p.stamp(), &[past]), Err(Refusal::Unknown(past)));
        assert_eq!(
            inner.grant(ROLE_SALES_USER, &contact(), &["no_such_property"]),
            Err(GrantError::UnknownProperty("no_such_property".into()))
        );
    }

    #[test]
    fn roles_union_but_each_needs_both_masks() {
        let mut inner = PropertyGrants::new();
        inner
            .grant(ROLE_SALES_USER, &contact(), &["school"])
            .unwrap();
        inner
            .grant(ROLE_VIEW_ONLY, &contact(), &["gender"])
            .unwrap();
        // view-only has no outer mask: its inner grant must not combine with
        // the sales role's outer mask.
        let outer = |r: RoleId| (r == ROLE_SALES_USER).then(|| all_fields(r)).flatten();
        let p = Projection::compile(
            &contact(),
            &[ROLE_SALES_USER, ROLE_VIEW_ONLY],
            outer,
            P0,
            &inner,
        );
        assert!(p.admits(&p.stamp(), &contact(), "school"));
        assert!(!p.admits(&p.stamp(), &contact(), "gender"));
        let both = |r: RoleId| all_fields(r);
        let p = Projection::compile(
            &contact(),
            &[ROLE_SALES_USER, ROLE_VIEW_ONLY],
            both,
            P0,
            &inner,
        );
        assert!(
            p.admits(&p.stamp(), &contact(), "school")
                && p.admits(&p.stamp(), &contact(), "gender")
        );
    }

    #[test]
    fn the_inner_policy_ignores_where_a_property_is_stored() {
        // The same grants against a table where school has its own lane:
        // the inner set names ordinals, so the decision does not move.
        let moved: &'static [StandardProperty] = Box::leak(
            CONTACT
                .iter()
                .map(|p| {
                    let mut q = *p;
                    if q.name == "school" {
                        q.slot = Slot::Lane(255);
                    }
                    q
                })
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        );
        let schema = Schema {
            properties: moved,
            ..contact()
        };
        let mut inner = PropertyGrants::new();
        inner.grant(ROLE_SALES_USER, &schema, &["school"]).unwrap();
        let outer = |_: RoleId| Some(WideFieldMask::from_positions(&[PACKED_RUN_POSITION, 255]));
        let p = Projection::compile(&schema, &[ROLE_SALES_USER], outer, P0, &inner);
        assert_eq!(
            p.project(&p.stamp(), full_row()).unwrap(),
            vec![(ord("school"), ord("school"))]
        );
        for name in PERSONAL {
            assert!(!p.admits(&p.stamp(), &schema, name), "{name}");
        }
    }

    #[test]
    fn grant_all_covers_only_its_version() {
        // Version 1 knows the first 10 properties; version 2 knows all.
        let v1 = Schema {
            properties: &CONTACT[..10],
            ..contact()
        };
        let v2 = Schema {
            version: SchemaVersion(2),
            ..contact()
        };
        let mut inner = PropertyGrants::new();
        inner.grant_all(ROLE_SALES_USER, &v1);
        let p = Projection::compile(&v2, &[ROLE_SALES_USER], all_fields, P0, &inner);
        let out = p.project(&p.stamp(), full_row()).unwrap();
        assert_eq!(
            out.len(),
            10,
            "properties added after the grant stay hidden"
        );
    }
}

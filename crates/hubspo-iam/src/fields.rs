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
//! exposed by a mask compiled before it existed.

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

    fn property(&self, ordinal: u16) -> Option<&StandardProperty> {
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

/// Why a read was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The projection was compiled against another schema version.
    Stale {
        /// The version it was compiled against.
        compiled: SchemaVersion,
        /// The version the request reads.
        current: SchemaVersion,
    },
    /// The schema has no property at this ordinal.
    Unknown(u16),
    /// The property exists but the request may not read it.
    Hidden(u16),
}

/// One request's readable properties on one concept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Projection {
    concept: u16,
    version: SchemaVersion,
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
    /// field. Call once per request.
    #[must_use]
    pub fn compile(
        schema: &Schema,
        roles: &[RoleId],
        outer: impl Fn(RoleId) -> Option<WideFieldMask>,
        inner: &PropertyGrants,
    ) -> Self {
        let mut readable = OrdinalSet::new();
        for &role in roles {
            let (Some(mask), Some(granted)) = (outer(role), inner.of(role, schema.concept)) else {
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
            concept: schema.concept,
            version: schema.version,
            known: u16::try_from(schema.properties.len()).unwrap_or(u16::MAX),
            readable,
        }
    }

    /// The concept the projection is for.
    #[must_use]
    pub fn concept(&self) -> u16 {
        self.concept
    }

    fn current(&self, current: SchemaVersion) -> Result<(), Refusal> {
        if current == self.version {
            Ok(())
        } else {
            Err(Refusal::Stale {
                compiled: self.version,
                current,
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
    /// [`Refusal::Stale`] when `current` is not the compiled version; then
    /// nothing is returned.
    pub fn project<V>(
        &self,
        current: SchemaVersion,
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
    pub fn require(&self, current: SchemaVersion, reads: &[u16]) -> Result<(), Refusal> {
        self.current(current)?;
        reads.iter().try_for_each(|&o| self.decide(o))
    }

    /// Whether the property named `name` in `schema` is readable.
    #[must_use]
    pub fn admits(&self, schema: &Schema, name: &str) -> bool {
        schema.version == self.version
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
        Projection::compile(&contact(), &[ROLE_SALES_USER], all_fields, &inner)
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
        let out = p.project(V1, full_row()).unwrap();
        assert_eq!(out, vec![(ord("school"), ord("school"))]);
        for name in PERSONAL {
            assert!(!p.admits(&contact(), name), "{name}");
            assert_eq!(p.require(V1, &[ord(name)]), Err(Refusal::Hidden(ord(name))));
        }
        assert!(p.admits(&contact(), "school"));
        assert_eq!(p.require(V1, &[ord("school")]), Ok(()));
    }

    #[test]
    fn a_predicate_mixing_granted_and_hidden_properties_is_refused() {
        let p = school_only();
        assert_eq!(
            p.require(V1, &[ord("school"), ord("date_of_birth")]),
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
            &PropertyGrants::new(),
        );
        assert!(p.project(V1, full_row()).unwrap().is_empty());
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
        let p = Projection::compile(&contact(), &[ROLE_SALES_USER], lanes_only, &inner);
        let out = p.project(V1, full_row()).unwrap();
        assert_eq!(
            out.len(),
            CONTACT.iter().filter(|q| q.slot != Slot::Packed).count()
        );
        assert!(!p.admits(&contact(), "school"));
        assert!(p.admits(&contact(), "firstname"));
    }

    #[test]
    fn a_stale_projection_refuses_everything() {
        let p = school_only();
        let v2 = SchemaVersion(2);
        let stale = Refusal::Stale {
            compiled: V1,
            current: v2,
        };
        assert_eq!(p.project(v2, full_row()), Err(stale.clone()));
        assert_eq!(p.require(v2, &[ord("school")]), Err(stale));
    }

    #[test]
    fn unknown_ordinals_are_dropped_and_refused() {
        let mut inner = PropertyGrants::new();
        inner.grant_all(ROLE_SALES_USER, &contact());
        let p = Projection::compile(&contact(), &[ROLE_SALES_USER], all_fields, &inner);
        let past = u16::try_from(CONTACT.len()).unwrap();
        assert_eq!(p.project(V1, [(past, 0u16)]).unwrap(), vec![]);
        assert_eq!(p.require(V1, &[past]), Err(Refusal::Unknown(past)));
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
            &inner,
        );
        assert!(p.admits(&contact(), "school"));
        assert!(!p.admits(&contact(), "gender"));
        let both = |r: RoleId| all_fields(r);
        let p = Projection::compile(&contact(), &[ROLE_SALES_USER, ROLE_VIEW_ONLY], both, &inner);
        assert!(p.admits(&contact(), "school") && p.admits(&contact(), "gender"));
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
        let p = Projection::compile(&schema, &[ROLE_SALES_USER], outer, &inner);
        assert_eq!(
            p.project(V1, full_row()).unwrap(),
            vec![(ord("school"), ord("school"))]
        );
        for name in PERSONAL {
            assert!(!p.admits(&schema, name), "{name}");
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
        let p = Projection::compile(&v2, &[ROLE_SALES_USER], all_fields, &inner);
        let out = p.project(SchemaVersion(2), full_row()).unwrap();
        assert_eq!(
            out.len(),
            10,
            "properties added after the grant stay hidden"
        );
    }
}

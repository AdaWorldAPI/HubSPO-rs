//! The first authorized output path (D-IAM-3): a contact by id.
//!
//! Everything a caller can learn passes through one compiled
//! [`RequestAccess`] before a value is shaped for output:
//!
//! 1. the stamp is checked first, so stale grants answer nothing;
//! 2. a record the request may not see is [`LookupError::NotFound`], the same
//!    answer as a record that does not exist: no read grant, another
//!    tenant's record, or an owner outside the request's rows;
//! 3. the values are projected by ordinal, so a hidden property is absent
//!    from the output, not blanked; the packed run never leaves as a run;
//! 4. only then are ordinals named.
//!
//! [`check_search`] is the same gate for a search: every property a filter
//! or a sort reads must be readable, or the search is refused before it is
//! compiled, since which rows match would reveal the hidden values.
//!
//! The caller supplies the record's values as `(ordinal, value)` pairs; the
//! store read that produces them lands with D-API-1.

use hubspo_iam::access::RequestAccess;
use hubspo_iam::fields::{Refusal, Schema, Stamp};
use hubspo_iam::tenant::TenantId;
use hubspo_query::filter::FilterGroup;
use lance_graph_contract::rbac::OpMask;

/// One stored record as the store hands it over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record<V> {
    /// The tenant the record belongs to.
    pub tenant: TenantId,
    /// The owner code on the owner lane; `None` when unassigned.
    pub owner: Option<u32>,
    /// Every stored property value, by ordinal.
    pub values: Vec<(u16, V)>,
}

/// Why a lookup returned no record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookupError {
    /// No such record, or one the request may not see.
    NotFound,
    /// The access was compiled for another stamp.
    Refused(Refusal),
    /// `schema` is not the one the access was compiled for, so its ordinals
    /// cannot be named.
    SchemaMismatch,
}

impl From<Refusal> for LookupError {
    fn from(r: Refusal) -> Self {
        LookupError::Refused(r)
    }
}

fn same_schema(access: &RequestAccess, schema: &Schema) -> bool {
    let stamp = access.stamp();
    stamp.concept == schema.concept && stamp.schema == schema.version
}

/// The readable properties of `record`, by internal name, in record order.
///
/// # Errors
///
/// See [`LookupError`].
pub fn lookup<V>(
    access: &RequestAccess,
    current: &Stamp,
    schema: &Schema,
    record: Option<Record<V>>,
) -> Result<Vec<(&'static str, V)>, LookupError> {
    access.check(current)?;
    if !same_schema(access, schema) {
        return Err(LookupError::SchemaMismatch);
    }
    if !access.may(current, OpMask::READ)? {
        return Err(LookupError::NotFound);
    }
    let record = record.ok_or(LookupError::NotFound)?;
    if record.tenant != access.tenant() || !access.rows(current)?.admits(record.owner) {
        return Err(LookupError::NotFound);
    }
    let visible = access
        .projection(current)?
        .project(current, record.values)?;
    Ok(visible
        .into_iter()
        .filter_map(|(o, v)| schema.property(o).map(|p| (p.name, v)))
        .collect())
}

/// Why a search was refused before it ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchRefusal {
    /// A filter or sort names no property of the schema.
    UnknownProperty(String),
    /// A filter or sort reads a property the request may not read, or the
    /// access is stale.
    Refused(Refusal),
    /// See [`LookupError::SchemaMismatch`].
    SchemaMismatch,
    /// The request holds no read on the concept.
    NoRead,
    /// The view's object is not the concept the access was compiled for, or
    /// its table resolves to no concept.
    ObjectMismatch,
}

/// Check a search's filter groups and sort properties against the request's
/// access before the search is compiled.
///
/// # Errors
///
/// See [`SearchRefusal`].
pub fn check_search(
    access: &RequestAccess,
    current: &Stamp,
    schema: &Schema,
    groups: &[FilterGroup],
    sorts: &[&str],
) -> Result<(), SearchRefusal> {
    access.check(current).map_err(SearchRefusal::Refused)?;
    if !same_schema(access, schema) {
        return Err(SearchRefusal::SchemaMismatch);
    }
    if !access
        .may(current, OpMask::READ)
        .map_err(SearchRefusal::Refused)?
    {
        return Err(SearchRefusal::NoRead);
    }
    let names = groups
        .iter()
        .flat_map(|g| g.filters.iter().map(|f| f.property.as_str()))
        .chain(sorts.iter().copied());
    let mut reads = Vec::new();
    for name in names {
        reads.push(
            schema
                .ordinal(name)
                .ok_or_else(|| SearchRefusal::UnknownProperty(name.to_string()))?,
        );
    }
    access
        .require(current, &reads)
        .map_err(SearchRefusal::Refused)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hubspo_generated::standard_properties::{CONTACT, PACKED_RUN_POSITION, Slot};
    use hubspo_iam::fields::{OrdinalSet, PolicyVersion, RoleVersion, SchemaVersion, SeatVersion};
    use hubspo_iam::scope::{OwnerScope, RowScope};
    use hubspo_iam::tenant::{Assignee, PermissionSet, PermissionSetId, Seat, TenantPolicy};
    use hubspo_port::{ROLE_SUPER_ADMIN, ROLE_VIEW_ONLY};
    use hubspo_query::filter::{Operator, PropertyFilter};
    use lance_graph_contract::class_view::WideFieldMask;
    use lance_graph_contract::rbac::ClassGrant;
    use ogar_dir_core::Guid128;
    use ogar_vocab::class_ids;

    const T: TenantId = TenantId(3);
    const C: u16 = class_ids::CRM_CONTACT;
    const ME: Guid128 = Guid128([9; 16]);
    const MINE: u32 = 42;

    fn contact() -> Schema {
        Schema {
            concept: C,
            version: SchemaVersion(1),
            properties: CONTACT,
        }
    }

    fn ord(name: &str) -> u16 {
        contact().ordinal(name).unwrap()
    }

    fn every_field() -> WideFieldMask {
        let lanes = CONTACT.iter().filter_map(|p| match p.slot {
            Slot::Lane(n) => Some(n),
            Slot::Packed => None,
        });
        WideFieldMask::from_positions(
            &std::iter::once(PACKED_RUN_POSITION)
                .chain(lanes)
                .collect::<Vec<_>>(),
        )
    }

    /// View-only reads email and firstname on contacts it owns.
    fn policy() -> TenantPolicy {
        let mut props = OrdinalSet::new();
        props.insert(ord("email"));
        props.insert(ord("firstname"));
        let mut p = TenantPolicy::new(T, PolicyVersion(1));
        p.add_set(
            PermissionSetId(10),
            PermissionSet {
                grants: vec![ClassGrant::new(C, OpMask::READ)],
                scopes: vec![(C, OwnerScope::Owned)],
                fields: vec![(C, every_field())],
                properties: vec![(C, props)],
                ..PermissionSet::default()
            },
        )
        .unwrap();
        p.assign(Assignee::Role(ROLE_VIEW_ONLY), PermissionSetId(10))
            .unwrap();
        p
    }

    fn seat() -> Seat {
        Seat {
            version: SeatVersion(1),
            concepts: vec![C],
            actions: Vec::new(),
        }
    }

    fn access(p: &TenantPolicy, role: &'static str) -> (RequestAccess, Stamp) {
        let a = RequestAccess::compile(
            p,
            T,
            &seat(),
            &ME,
            &[role],
            RoleVersion(1),
            &contact(),
            |_| RowScope::Rows {
                owners: vec![MINE],
                unassigned: false,
            },
        )
        .unwrap();
        let now = RequestAccess::current_stamp(p, &contact(), &seat(), RoleVersion(1));
        (a, now)
    }

    /// A record carrying every contact property, value = its name.
    fn record(tenant: TenantId, owner: Option<u32>) -> Option<Record<&'static str>> {
        Some(Record {
            tenant,
            owner,
            values: CONTACT.iter().map(|p| (p.position, p.name)).collect(),
        })
    }

    #[test]
    fn a_visible_contact_carries_only_its_granted_properties() {
        let p = policy();
        let (a, now) = access(&p, ROLE_VIEW_ONLY);
        let out = lookup(&a, &now, &contact(), record(T, Some(MINE))).unwrap();
        let mut names: Vec<&str> = out.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        assert_eq!(names, ["email", "firstname"]);
        assert!(
            out.iter().all(|(n, v)| n == v),
            "values travel with their names"
        );
        assert!(
            !names.contains(&"date_of_birth"),
            "no packed-run property leaks"
        );
    }

    #[test]
    fn an_invisible_record_is_indistinguishable_from_a_missing_one() {
        let p = policy();
        let (a, now) = access(&p, ROLE_VIEW_ONLY);
        for rec in [
            None,
            record(TenantId(4), Some(MINE)),
            record(T, Some(MINE + 1)),
            record(T, None),
        ] {
            assert_eq!(
                lookup(&a, &now, &contact(), rec),
                Err(LookupError::NotFound)
            );
        }
    }

    #[test]
    fn a_role_with_no_set_sees_nothing_even_super_admin() {
        let p = policy();
        let (a, now) = access(&p, ROLE_SUPER_ADMIN);
        assert_eq!(
            lookup(&a, &now, &contact(), record(T, Some(MINE))),
            Err(LookupError::NotFound)
        );
        assert_eq!(
            check_search(&a, &now, &contact(), &[], &["email"]),
            Err(SearchRefusal::NoRead)
        );
    }

    #[test]
    fn a_revoked_set_refuses_the_old_access() {
        let mut p = policy();
        let (a, _) = access(&p, ROLE_VIEW_ONLY);
        p.unassign(Assignee::Role(ROLE_VIEW_ONLY), PermissionSetId(10));
        let now = RequestAccess::current_stamp(&p, &contact(), &seat(), RoleVersion(1));
        assert!(matches!(
            lookup(&a, &now, &contact(), record(T, Some(MINE))),
            Err(LookupError::Refused(Refusal::Stale { .. }))
        ));
    }

    #[test]
    fn another_schema_or_concept_is_refused() {
        let p = policy();
        let (a, now) = access(&p, ROLE_VIEW_ONLY);
        for schema in [
            Schema {
                version: SchemaVersion(2),
                ..contact()
            },
            Schema {
                concept: class_ids::CRM_DEAL,
                ..contact()
            },
        ] {
            assert_eq!(
                lookup(&a, &now, &schema, record(T, Some(MINE))),
                Err(LookupError::SchemaMismatch)
            );
        }
    }

    #[test]
    fn a_filter_or_sort_on_a_hidden_property_is_refused() {
        let p = policy();
        let (a, now) = access(&p, ROLE_VIEW_ONLY);
        let group = |prop: &str| FilterGroup {
            filters: vec![PropertyFilter::new(prop, Operator::Eq, "x")],
        };
        assert_eq!(
            check_search(&a, &now, &contact(), &[group("email")], &["firstname"]),
            Ok(())
        );
        assert_eq!(
            check_search(&a, &now, &contact(), &[group("date_of_birth")], &[]),
            Err(SearchRefusal::Refused(Refusal::Hidden(ord(
                "date_of_birth"
            ))))
        );
        assert_eq!(
            check_search(&a, &now, &contact(), &[group("email")], &["city"]),
            Err(SearchRefusal::Refused(Refusal::Hidden(ord("city"))))
        );
        assert_eq!(
            check_search(&a, &now, &contact(), &[group("no_such")], &[]),
            Err(SearchRefusal::UnknownProperty("no_such".into()))
        );
    }
}

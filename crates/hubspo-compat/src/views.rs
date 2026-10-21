//! A saved view through the D-IAM-3 gate.
//!
//! A view is shared data: whoever opens it gets its filters, sort and
//! columns. The request's access decides what that may reveal:
//!
//! - a filter or the sort on a property the request may not read refuses
//!   the whole view, since which records match (or their order) would leak
//!   the hidden values;
//! - a column the request may not read is dropped from the output, the same
//!   way [`crate::contacts::lookup`] drops a hidden property: absent, not
//!   blanked.

use crate::contacts::{SearchRefusal, check_search};
use hubspo_iam::access::RequestAccess;
use hubspo_iam::fields::{Schema, Stamp};
use hubspo_query::view::SavedView;

/// Check `view` against the request's access; on success, the columns the
/// request may see, in the view's order.
///
/// # Errors
///
/// See [`SearchRefusal`].
pub fn open_view(
    access: &RequestAccess,
    current: &Stamp,
    schema: &Schema,
    view: &SavedView,
) -> Result<Vec<&'static str>, SearchRefusal> {
    let sorts: Vec<&str> = view.sort.iter().map(|s| s.property.as_str()).collect();
    check_search(access, current, schema, &view.groups, &sorts)?;
    let projection = access.projection(current).map_err(SearchRefusal::Refused)?;
    Ok(view
        .columns
        .iter()
        .filter_map(|name| {
            let ordinal = schema.ordinal(name)?;
            let p = schema.property(ordinal)?;
            projection.admits(current, schema, name).then_some(p.name)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hubspo_generated::standard_properties::{CONTACT, PACKED_RUN_POSITION, Slot};
    use hubspo_iam::fields::{
        OrdinalSet, PolicyVersion, Refusal, RoleVersion, SchemaVersion, SeatVersion,
    };
    use hubspo_iam::scope::{OwnerScope, RowScope};
    use hubspo_iam::tenant::{
        Assignee, PermissionSet, PermissionSetId, Seat, TenantId, TenantPolicy,
    };
    use hubspo_port::ROLE_VIEW_ONLY;
    use hubspo_query::filter::{FilterGroup, Operator, PropertyFilter};
    use hubspo_query::view::{Direction, Sort};
    use lance_graph_contract::class_view::WideFieldMask;
    use lance_graph_contract::rbac::{ClassGrant, OpMask};
    use ogar_dir_core::Guid128;
    use ogar_vocab::class_ids;

    const T: TenantId = TenantId(3);
    const C: u16 = class_ids::CRM_CONTACT;

    fn contact() -> Schema {
        Schema {
            concept: C,
            version: SchemaVersion(1),
            properties: CONTACT,
        }
    }

    fn seat() -> Seat {
        Seat {
            version: SeatVersion(1),
            concepts: vec![C],
            actions: Vec::new(),
        }
    }

    /// Reads email, firstname and city.
    fn access() -> (RequestAccess, Stamp) {
        let lanes = CONTACT.iter().filter_map(|p| match p.slot {
            Slot::Lane(n) => Some(n),
            Slot::Packed => None,
        });
        let every = WideFieldMask::from_positions(
            &std::iter::once(PACKED_RUN_POSITION)
                .chain(lanes)
                .collect::<Vec<_>>(),
        );
        let mut props = OrdinalSet::new();
        for n in ["email", "firstname", "city"] {
            props.insert(contact().ordinal(n).unwrap());
        }
        let mut p = TenantPolicy::new(T, PolicyVersion(1));
        p.add_set(
            PermissionSetId(1),
            PermissionSet {
                grants: vec![ClassGrant::new(C, OpMask::READ)],
                scopes: vec![(C, OwnerScope::Everything)],
                fields: vec![(C, every)],
                properties: vec![(C, props)],
                ..PermissionSet::default()
            },
        )
        .unwrap();
        p.assign(Assignee::Role(ROLE_VIEW_ONLY), PermissionSetId(1))
            .unwrap();
        let a = RequestAccess::compile(
            &p,
            T,
            &seat(),
            &Guid128([1; 16]),
            &[ROLE_VIEW_ONLY],
            RoleVersion(1),
            &contact(),
            |_| RowScope::All,
        )
        .unwrap();
        let now = RequestAccess::current_stamp(&p, &contact(), &seat(), RoleVersion(1));
        (a, now)
    }

    fn view(filter_on: &str, sort_on: &str, columns: &[&str]) -> SavedView {
        SavedView {
            object: "contacts".into(),
            groups: vec![FilterGroup {
                filters: vec![PropertyFilter::new(filter_on, Operator::Eq, "x")],
            }],
            sort: Some(Sort {
                property: sort_on.into(),
                direction: Direction::Ascending,
            }),
            columns: columns.iter().map(|c| (*c).to_string()).collect(),
        }
    }

    #[test]
    fn a_hidden_column_is_dropped_and_the_rest_keep_their_order() {
        let (a, now) = access();
        let v = view(
            "email",
            "city",
            &["city", "date_of_birth", "email", "no_such", "firstname"],
        );
        assert_eq!(
            open_view(&a, &now, &contact(), &v),
            Ok(vec!["city", "email", "firstname"])
        );
    }

    #[test]
    fn a_filter_or_sort_on_a_hidden_property_refuses_the_view() {
        let (a, now) = access();
        let dob = contact().ordinal("date_of_birth").unwrap();
        assert_eq!(
            open_view(
                &a,
                &now,
                &contact(),
                &view("date_of_birth", "city", &["email"])
            ),
            Err(SearchRefusal::Refused(Refusal::Hidden(dob)))
        );
        assert_eq!(
            open_view(
                &a,
                &now,
                &contact(),
                &view("email", "date_of_birth", &["email"])
            ),
            Err(SearchRefusal::Refused(Refusal::Hidden(dob)))
        );
    }
}

//! D-IAM-3: one request's effective access to one concept, compiled once.
//!
//! [`RequestAccess::compile`] reads the tenant's permission sets
//! ([`crate::tenant`]) for the principal and its resolved roles and folds
//! them into everything an output path needs before it touches a value:
//!
//! - the rows it may see ([`RowScope`], D-IAM-2);
//! - the properties it may read ([`Projection`], by stable ordinal);
//! - the plain verbs it holds (`READ`, `WRITE`, `CREATE`);
//! - the named actions it may perform (`ACT`, `DELETE`, `EXPORT`), each
//!   needing its verb **and** a named gate in the same set **and** the seat.
//!
//! The result is stamped with the concept, its schema version, the policy's
//! version and generation, the seat version and the role-resolution
//! version. Every check presents the current stamp; any change in between
//! (a grant edited, a set revoked, a schema bumped, a seat changed, a group
//! re-read) refuses the request with [`Refusal::Stale`] instead of
//! answering from the old grants.
//!
//! Nothing here keys on a role name: a super admin holds what its sets
//! give it, and a role with no set holds nothing.

use crate::fields::{Projection, Refusal, RoleVersion, Schema, Stamp};
use crate::scope::{OwnerScope, RowScope};
use crate::tenant::{Seat, TenantId, TenantPolicy};
use lance_graph_contract::rbac::{OpMask, RoleId};
use ogar_dir_core::Guid128;

/// Why access could not be compiled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccessError {
    /// The policy belongs to another tenant than the request.
    WrongTenant {
        /// The policy's tenant.
        policy: TenantId,
        /// The request's tenant.
        request: TenantId,
    },
}

/// The verbs [`RequestAccess::may`] answers. `ACT` and `DELETE` are only
/// ever answered by [`RequestAccess::allows`], with a named gate.
const PLAIN: OpMask = OpMask::READ.union(OpMask::WRITE).union(OpMask::CREATE);

/// One request's compiled access to one concept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestAccess {
    tenant: TenantId,
    stamp: Stamp,
    rows: RowScope,
    verbs: OpMask,
    actions: Vec<&'static str>,
    projection: Projection,
}

impl RequestAccess {
    /// The stamp a request presents now: compare it with the compiled one.
    #[must_use]
    pub fn current_stamp(
        policy: &TenantPolicy,
        schema: &Schema,
        seat: &Seat,
        roles: RoleVersion,
    ) -> Stamp {
        Stamp {
            concept: schema.concept,
            schema: schema.version,
            inner: policy.generation(),
            outer: policy.version(),
            seat: seat.version,
            roles,
        }
    }

    /// Compile `principal`'s access to `schema`'s concept.
    ///
    /// `roles` are the principal's roles as resolved for this request
    /// (`crate::roles::request_actors`), at resolution version
    /// `role_version`. `row_scope` turns the broadest ownership scope the
    /// granting sets give into rows (`crate::scope::visible_owners`); it is
    /// not called when no set grants a read.
    ///
    /// A concept the seat does not reach, or one no assigned set grants,
    /// compiles to no rows, no properties and no verbs.
    ///
    /// # Errors
    ///
    /// [`AccessError::WrongTenant`] when `policy` is another tenant's.
    #[expect(clippy::too_many_arguments, reason = "one argument per input the access is compiled from")]
    pub fn compile(
        policy: &TenantPolicy,
        tenant: TenantId,
        seat: &Seat,
        principal: &Guid128,
        roles: &[RoleId],
        role_version: RoleVersion,
        schema: &Schema,
        row_scope: impl FnOnce(OwnerScope) -> RowScope,
    ) -> Result<Self, AccessError> {
        if policy.tenant() != tenant {
            return Err(AccessError::WrongTenant {
                policy: policy.tenant(),
                request: tenant,
            });
        }
        let concept = schema.concept;
        let stamp = Self::current_stamp(policy, schema, seat, role_version);
        let sets = if seat.concepts.contains(&concept) {
            policy.sets_for(principal, roles)
        } else {
            Vec::new()
        };

        let verbs_of = |set: &crate::tenant::PermissionSet| {
            set.grants
                .iter()
                .filter(|g| g.target_classid == concept)
                .fold(OpMask::NONE, |acc, g| acc.union(g.op_mask))
        };
        let readers: Vec<_> = sets
            .iter()
            .copied()
            .filter(|s| verbs_of(s).contains(OpMask::READ))
            .collect();

        let verbs = sets
            .iter()
            .fold(OpMask::NONE, |acc, s| acc.union(verbs_of(s)));

        let broadest = readers
            .iter()
            .filter_map(|s| s.scope(concept))
            .max_by_key(|scope| match scope {
                OwnerScope::Owned => 0,
                OwnerScope::Team => 1,
                OwnerScope::Everything => 2,
            });
        let rows = broadest.map_or(RowScope::Nothing, row_scope);

        let projection = Projection::compile_holders(
            schema,
            readers.iter().map(|s| s.property_holder(concept)),
            stamp,
        );

        let mut actions: Vec<&'static str> = sets
            .iter()
            .flat_map(|s| {
                s.actions
                    .iter()
                    .filter(|a| a.concept == concept && s.allows_action(concept, a.action))
                    .map(|a| a.action)
            })
            .filter(|a| seat.actions.contains(a))
            .collect();
        actions.sort_unstable();
        actions.dedup();

        Ok(Self {
            tenant,
            stamp,
            rows,
            verbs,
            actions,
            projection,
        })
    }

    /// The tenant the access was compiled for.
    #[must_use]
    pub fn tenant(&self) -> TenantId {
        self.tenant
    }

    /// What the access is valid for.
    #[must_use]
    pub fn stamp(&self) -> Stamp {
        self.stamp
    }

    /// Refuse a request presenting another stamp.
    ///
    /// # Errors
    ///
    /// [`Refusal::Stale`] when `current` differs from the compiled stamp.
    pub fn check(&self, current: &Stamp) -> Result<(), Refusal> {
        if *current == self.stamp {
            Ok(())
        } else {
            Err(Refusal::Stale {
                compiled: self.stamp,
                current: *current,
            })
        }
    }

    /// The rows the request may see.
    ///
    /// # Errors
    ///
    /// [`Refusal::Stale`].
    pub fn rows(&self, current: &Stamp) -> Result<&RowScope, Refusal> {
        self.check(current)?;
        Ok(&self.rows)
    }

    /// The readable properties.
    ///
    /// # Errors
    ///
    /// [`Refusal::Stale`].
    pub fn projection(&self, current: &Stamp) -> Result<&Projection, Refusal> {
        self.check(current)?;
        Ok(&self.projection)
    }

    /// Whether the request holds the plain verbs `verb` (any of `READ`,
    /// `WRITE`, `CREATE`). Always `false` for a mask containing `ACT` or
    /// `DELETE`: those are named actions, answered by [`Self::allows`].
    ///
    /// # Errors
    ///
    /// [`Refusal::Stale`].
    pub fn may(&self, current: &Stamp, verb: OpMask) -> Result<bool, Refusal> {
        self.check(current)?;
        Ok(PLAIN.contains(verb) && self.verbs.contains(verb))
    }

    /// Whether the request may perform `action` (`email.send`,
    /// `crate::tenant::ACTION_DELETE`, `crate::tenant::ACTION_EXPORT`, ...):
    /// one set holds both its verb and its named gate, and the seat allows
    /// it.
    ///
    /// # Errors
    ///
    /// [`Refusal::Stale`].
    pub fn allows(&self, current: &Stamp, action: &str) -> Result<bool, Refusal> {
        self.check(current)?;
        Ok(self.actions.contains(&action))
    }

    /// Check the properties a filter, sort or aggregate reads; a hidden one
    /// is refused, never evaluated.
    ///
    /// # Errors
    ///
    /// [`Refusal::Stale`], or the first unknown or hidden ordinal.
    pub fn require(&self, current: &Stamp, reads: &[u16]) -> Result<(), Refusal> {
        self.check(current)?;
        self.projection.require(current, reads)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fields::{OrdinalSet, PolicyVersion, SchemaVersion, SeatVersion};
    use crate::tenant::{
        ACTION_DELETE, ACTION_EXPORT, ActionGrant, Assignee, PermissionSet, PermissionSetId,
    };
    use hubspo_generated::standard_properties::{CONTACT, PACKED_RUN_POSITION, Slot};
    use hubspo_port::{ROLE_SALES_USER, ROLE_SUPER_ADMIN, ROLE_VIEW_ONLY};
    use lance_graph_contract::class_view::WideFieldMask;
    use lance_graph_contract::rbac::ClassGrant;
    use ogar_vocab::class_ids;

    const T: TenantId = TenantId(7);
    const C: u16 = class_ids::CRM_CONTACT;
    const ME: Guid128 = Guid128([1; 16]);

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

    fn ordinals(names: &[&str]) -> OrdinalSet {
        let mut s = OrdinalSet::new();
        for n in names {
            s.insert(ord(n));
        }
        s
    }

    fn reader(op: OpMask, scope: OwnerScope, props: &[&str]) -> PermissionSet {
        PermissionSet {
            grants: vec![ClassGrant::new(C, op)],
            scopes: vec![(C, scope)],
            fields: vec![(C, every_field())],
            properties: vec![(C, ordinals(props))],
            ..PermissionSet::default()
        }
    }

    fn seat(actions: &[&'static str]) -> Seat {
        Seat {
            version: SeatVersion(1),
            concepts: vec![C],
            actions: actions.to_vec(),
        }
    }

    fn policy_with(set: PermissionSet, to: Assignee) -> TenantPolicy {
        let mut p = TenantPolicy::new(T, PolicyVersion(1));
        p.add_set(PermissionSetId(1), set).unwrap();
        p.assign(to, PermissionSetId(1)).unwrap();
        p
    }

    fn owned_rows(scope: OwnerScope) -> RowScope {
        match scope {
            OwnerScope::Everything => RowScope::All,
            _ => RowScope::Rows {
                owners: vec![42],
                unassigned: false,
            },
        }
    }

    fn compile(policy: &TenantPolicy, seat: &Seat, roles: &[RoleId]) -> RequestAccess {
        RequestAccess::compile(
            policy,
            T,
            seat,
            &ME,
            roles,
            RoleVersion(1),
            &contact(),
            owned_rows,
        )
        .unwrap()
    }

    fn now(policy: &TenantPolicy, seat: &Seat) -> Stamp {
        RequestAccess::current_stamp(policy, &contact(), seat, RoleVersion(1))
    }

    #[test]
    fn an_assigned_set_grants_its_rows_properties_and_verbs() {
        let p = policy_with(
            reader(
                OpMask::READ.union(OpMask::WRITE),
                OwnerScope::Owned,
                &["email", "city"],
            ),
            Assignee::Role(ROLE_SALES_USER),
        );
        let s = seat(&[]);
        let a = compile(&p, &s, &[ROLE_SALES_USER]);
        let cur = now(&p, &s);
        assert_eq!(
            a.rows(&cur).unwrap(),
            &RowScope::Rows {
                owners: vec![42],
                unassigned: false
            }
        );
        assert!(a.may(&cur, OpMask::READ).unwrap());
        assert!(a.may(&cur, OpMask::WRITE).unwrap());
        assert!(!a.may(&cur, OpMask::CREATE).unwrap());
        assert_eq!(a.require(&cur, &[ord("email"), ord("city")]), Ok(()));
        assert_eq!(
            a.require(&cur, &[ord("date_of_birth")]),
            Err(Refusal::Hidden(ord("date_of_birth")))
        );
    }

    #[test]
    fn a_missing_policy_grants_nothing() {
        let p = TenantPolicy::new(T, PolicyVersion(1));
        let s = seat(&[ACTION_EXPORT]);
        for role in [ROLE_SUPER_ADMIN, ROLE_SALES_USER, ROLE_VIEW_ONLY] {
            let a = compile(&p, &s, &[role]);
            let cur = now(&p, &s);
            assert_eq!(a.rows(&cur).unwrap(), &RowScope::Nothing, "{role}");
            assert!(!a.may(&cur, OpMask::READ).unwrap(), "{role}");
            assert!(!a.allows(&cur, ACTION_EXPORT).unwrap(), "{role}");
            assert_eq!(
                a.require(&cur, &[ord("email")]),
                Err(Refusal::Hidden(ord("email"))),
                "{role}"
            );
        }
    }

    #[test]
    fn super_admin_is_not_a_bypass() {
        // A set assigned to sales; the super admin holds no set.
        let p = policy_with(
            reader(OpMask::READ, OwnerScope::Everything, &["email"]),
            Assignee::Role(ROLE_SALES_USER),
        );
        let s = seat(&[]);
        let a = compile(&p, &s, &[ROLE_SUPER_ADMIN]);
        let cur = now(&p, &s);
        assert_eq!(a.rows(&cur).unwrap(), &RowScope::Nothing);
        assert!(!a.may(&cur, OpMask::READ).unwrap());
        // The same principal holding sales gets exactly sales' grants.
        let b = compile(&p, &s, &[ROLE_SUPER_ADMIN, ROLE_SALES_USER]);
        assert_eq!(b.rows(&cur).unwrap(), &RowScope::All);
    }

    #[test]
    fn view_only_reads_no_property_it_was_not_given() {
        // Outer mask admits every field; the inner grant leaves the personal
        // properties out. Nothing sensitive is implied by the role or by READ.
        let p = policy_with(
            reader(
                OpMask::READ,
                OwnerScope::Everything,
                &["email", "firstname"],
            ),
            Assignee::Role(ROLE_VIEW_ONLY),
        );
        let s = seat(&[]);
        let a = compile(&p, &s, &[ROLE_VIEW_ONLY]);
        let cur = now(&p, &s);
        let row: Vec<(u16, u16)> = CONTACT.iter().map(|q| (q.position, q.position)).collect();
        let seen = a.projection(&cur).unwrap().project(&cur, row).unwrap();
        let mut want = vec![ord("email"), ord("firstname")];
        want.sort_unstable();
        let mut got: Vec<u16> = seen.iter().map(|(o, _)| *o).collect();
        got.sort_unstable();
        assert_eq!(got, want);
        assert!(!a.may(&cur, OpMask::WRITE).unwrap());
    }

    #[test]
    fn act_delete_and_export_need_a_named_gate_and_the_seat() {
        let full = OpMask::READ
            .union(OpMask::WRITE)
            .union(OpMask::CREATE)
            .union(OpMask::ACT)
            .union(OpMask::DELETE);
        let mut set = reader(full, OwnerScope::Everything, &["email"]);
        let p_no_gate = policy_with(set.clone(), Assignee::Role(ROLE_SALES_USER));
        let s_all = seat(&["email.send", ACTION_DELETE, ACTION_EXPORT]);
        let a = compile(&p_no_gate, &s_all, &[ROLE_SALES_USER]);
        let cur = now(&p_no_gate, &s_all);
        for action in ["email.send", ACTION_DELETE, ACTION_EXPORT] {
            assert!(!a.allows(&cur, action).unwrap(), "{action}: verbs alone");
        }
        assert!(!a.may(&cur, OpMask::DELETE).unwrap(), "DELETE is not plain");
        assert!(!a.may(&cur, OpMask::ACT).unwrap(), "ACT is not plain");

        set.actions = ["email.send", ACTION_DELETE, ACTION_EXPORT]
            .into_iter()
            .map(|action| ActionGrant { concept: C, action })
            .collect();
        let p = policy_with(set, Assignee::Role(ROLE_SALES_USER));
        // Gated, but the seat allows only the email send.
        let s = seat(&["email.send"]);
        let a = compile(&p, &s, &[ROLE_SALES_USER]);
        let cur = now(&p, &s);
        assert!(a.allows(&cur, "email.send").unwrap());
        assert!(!a.allows(&cur, ACTION_DELETE).unwrap());
        assert!(!a.allows(&cur, ACTION_EXPORT).unwrap());
        assert!(!a.allows(&cur, "sequence.enroll").unwrap());
        // Seat and gate both: allowed.
        let a = compile(&p, &s_all, &[ROLE_SALES_USER]);
        let cur = now(&p, &s_all);
        assert!(a.allows(&cur, ACTION_DELETE).unwrap());
        assert!(a.allows(&cur, ACTION_EXPORT).unwrap());
    }

    #[test]
    fn write_does_not_imply_act_or_delete() {
        let mut set = reader(
            OpMask::READ.union(OpMask::WRITE).union(OpMask::CREATE),
            OwnerScope::Everything,
            &["email"],
        );
        set.actions = vec![
            ActionGrant {
                concept: C,
                action: "email.send",
            },
            ActionGrant {
                concept: C,
                action: ACTION_DELETE,
            },
        ];
        let p = policy_with(set, Assignee::Role(ROLE_SALES_USER));
        let s = seat(&["email.send", ACTION_DELETE]);
        let a = compile(&p, &s, &[ROLE_SALES_USER]);
        let cur = now(&p, &s);
        assert!(!a.allows(&cur, "email.send").unwrap());
        assert!(!a.allows(&cur, ACTION_DELETE).unwrap());
    }

    #[test]
    fn revocation_refuses_the_old_access() {
        let mut p = policy_with(
            reader(OpMask::READ, OwnerScope::Everything, &["email"]),
            Assignee::Principal(ME),
        );
        let s = seat(&[]);
        let a = compile(&p, &s, &[]);
        assert_eq!(a.require(&now(&p, &s), &[ord("email")]), Ok(()));
        p.unassign(Assignee::Principal(ME), PermissionSetId(1));
        let cur = now(&p, &s);
        assert!(matches!(
            a.require(&cur, &[ord("email")]),
            Err(Refusal::Stale { .. })
        ));
        assert!(matches!(a.rows(&cur), Err(Refusal::Stale { .. })));
        // Recompiled: nothing.
        let b = compile(&p, &s, &[]);
        assert_eq!(b.rows(&cur).unwrap(), &RowScope::Nothing);
    }

    #[test]
    fn a_seat_role_or_schema_change_refuses_the_old_access() {
        let p = policy_with(
            reader(OpMask::READ, OwnerScope::Everything, &["email"]),
            Assignee::Role(ROLE_SALES_USER),
        );
        let s = seat(&[]);
        let a = compile(&p, &s, &[ROLE_SALES_USER]);
        let base = now(&p, &s);
        assert_eq!(a.check(&base), Ok(()));
        let reseated = Seat {
            version: SeatVersion(2),
            ..s.clone()
        };
        let changes = [
            now(&p, &reseated),
            RequestAccess::current_stamp(&p, &contact(), &s, RoleVersion(2)),
            RequestAccess::current_stamp(
                &p,
                &Schema {
                    version: SchemaVersion(2),
                    ..contact()
                },
                &s,
                RoleVersion(1),
            ),
            RequestAccess::current_stamp(
                &p,
                &Schema {
                    concept: class_ids::CRM_DEAL,
                    ..contact()
                },
                &s,
                RoleVersion(1),
            ),
        ];
        for cur in changes {
            assert!(
                matches!(a.check(&cur), Err(Refusal::Stale { .. })),
                "{cur:?}"
            );
            assert!(matches!(
                a.may(&cur, OpMask::READ),
                Err(Refusal::Stale { .. })
            ));
        }
    }

    #[test]
    fn a_seat_that_does_not_reach_the_concept_grants_nothing() {
        let p = policy_with(
            reader(OpMask::READ, OwnerScope::Everything, &["email"]),
            Assignee::Role(ROLE_SALES_USER),
        );
        let s = Seat {
            concepts: vec![class_ids::CRM_DEAL],
            ..seat(&[])
        };
        let a = compile(&p, &s, &[ROLE_SALES_USER]);
        let cur = now(&p, &s);
        assert_eq!(a.rows(&cur).unwrap(), &RowScope::Nothing);
        assert!(!a.may(&cur, OpMask::READ).unwrap());
    }

    #[test]
    fn another_tenants_policy_is_refused() {
        let p = TenantPolicy::new(TenantId(8), PolicyVersion(1));
        assert_eq!(
            RequestAccess::compile(
                &p,
                T,
                &seat(&[]),
                &ME,
                &[],
                RoleVersion(1),
                &contact(),
                owned_rows
            ),
            Err(AccessError::WrongTenant {
                policy: TenantId(8),
                request: T
            })
        );
    }

    #[test]
    fn a_grant_on_another_concept_does_not_reach_this_one() {
        let mut set = reader(OpMask::READ, OwnerScope::Everything, &["email"]);
        set.grants = vec![ClassGrant::new(class_ids::CRM_DEAL, OpMask::READ)];
        let p = policy_with(set, Assignee::Role(ROLE_SALES_USER));
        let s = seat(&[]);
        let a = compile(&p, &s, &[ROLE_SALES_USER]);
        let cur = now(&p, &s);
        assert_eq!(a.rows(&cur).unwrap(), &RowScope::Nothing);
        assert_eq!(
            a.require(&cur, &[ord("email")]),
            Err(Refusal::Hidden(ord("email")))
        );
    }

    #[test]
    fn the_broadest_scope_of_the_reading_sets_wins() {
        let mut p = TenantPolicy::new(T, PolicyVersion(1));
        p.add_set(
            PermissionSetId(1),
            reader(OpMask::READ, OwnerScope::Owned, &["email"]),
        )
        .unwrap();
        // A broader scope on a set without READ does not widen the rows.
        p.add_set(
            PermissionSetId(2),
            reader(OpMask::WRITE, OwnerScope::Everything, &[]),
        )
        .unwrap();
        p.assign(Assignee::Principal(ME), PermissionSetId(1))
            .unwrap();
        p.assign(Assignee::Principal(ME), PermissionSetId(2))
            .unwrap();
        let s = seat(&[]);
        let a = compile(&p, &s, &[]);
        let cur = now(&p, &s);
        assert_eq!(
            a.rows(&cur).unwrap(),
            &RowScope::Rows {
                owners: vec![42],
                unassigned: false
            }
        );
        p.add_set(
            PermissionSetId(3),
            reader(OpMask::READ, OwnerScope::Everything, &[]),
        )
        .unwrap();
        p.assign(Assignee::Principal(ME), PermissionSetId(3))
            .unwrap();
        let a = compile(&p, &s, &[]);
        assert_eq!(a.rows(&now(&p, &s)).unwrap(), &RowScope::All);
    }
}

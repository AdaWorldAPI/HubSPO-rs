//! Tenant permission policy: the only runtime source of access.
//!
//! Operator ruling 2026-10-21 (`ISS-PRESET-VERBS-PROVISIONAL`,
//! `ISS-PERMISSION-SETS-NOT-ROLES`): a role name describes a user and grants
//! nothing. Access comes from **permission sets**, versioned tenant data
//! with stable numeric ids, assigned to a role or directly to a principal.
//! A set may grant more than any preset; it is not a narrowing of one.
//! The preset verb table in [`crate::roles`] is a fixture and is never read
//! here.
//!
//! Effective access to a concept is the conjunction of:
//!
//! 1. the principal's [`Seat`]: the ceiling of concepts and actions any
//!    grant can unlock;
//! 2. a permission set that grants the verb ([`ClassGrant`]);
//! 3. the set's ownership scope on the concept (D-IAM-2);
//! 4. for properties, the set's outer field mask and inner ordinals
//!    (D-IAM-3, [`crate::fields`]);
//! 5. for an action, an explicit named gate in the same set.
//!
//! `ACT`, `DELETE` and `EXPORT` are never inherited from `WRITE` or
//! `CREATE`: each needs both its verb bit and a named [`ActionGrant`] in
//! the same set, and the seat must allow the action. A missing, unknown or
//! unassigned set grants nothing.

use crate::fields::{OrdinalSet, PolicyVersion, SeatVersion};
use crate::scope::OwnerScope;
use lance_graph_contract::class_view::WideFieldMask;
use lance_graph_contract::rbac::{ClassGrant, OpMask, Operation, RoleId, grants_permit};
use ogar_dir_core::Guid128;

/// A permission set's stable id within a tenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PermissionSetId(pub u32);

/// A tenant (a HubSpot portal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TenantId(pub u32);

/// The action name for deleting records; needs [`OpMask::DELETE`].
pub const ACTION_DELETE: &str = "delete";
/// The action name for exporting records; needs [`OpMask::READ`].
pub const ACTION_EXPORT: &str = "export";

/// The verb an action name needs besides its named gate: `delete` needs
/// `DELETE`, `export` needs `READ`, every other action needs `ACT`.
#[must_use]
pub fn action_verb(action: &str) -> OpMask {
    match action {
        ACTION_DELETE => OpMask::DELETE,
        ACTION_EXPORT => OpMask::READ,
        _ => OpMask::ACT,
    }
}

/// One named action a set allows on a concept (`email.send`,
/// `sequence.enroll`, `invoice.create`, [`ACTION_EXPORT`],
/// [`ACTION_DELETE`], ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionGrant {
    /// The concept the action runs on.
    pub concept: u16,
    /// The action name.
    pub action: &'static str,
}

/// One permission set: everything it grants is listed; nothing is implied.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PermissionSet {
    /// Verbs per concept.
    pub grants: Vec<ClassGrant>,
    /// Named action gates.
    pub actions: Vec<ActionGrant>,
    /// Ownership scope per concept; a concept without one admits no row.
    pub scopes: Vec<(u16, OwnerScope)>,
    /// Outer field mask per concept (ClassView positions).
    pub fields: Vec<(u16, WideFieldMask)>,
    /// Readable property ordinals per concept.
    pub properties: Vec<(u16, OrdinalSet)>,
}

impl PermissionSet {
    /// The scope this set gives on `concept`.
    #[must_use]
    pub fn scope(&self, concept: u16) -> Option<OwnerScope> {
        self.scopes
            .iter()
            .find_map(|(c, s)| (*c == concept).then_some(*s))
    }

    /// This set's outer mask and inner ordinals on `concept`.
    #[must_use]
    pub fn property_holder(&self, concept: u16) -> (Option<WideFieldMask>, Option<&OrdinalSet>) {
        (
            self.fields
                .iter()
                .find_map(|(c, m)| (*c == concept).then(|| m.clone())),
            self.properties
                .iter()
                .find_map(|(c, o)| (*c == concept).then_some(o)),
        )
    }

    /// Whether this set allows `action` on `concept`: the verb the action
    /// needs and the named gate, both in this set.
    #[must_use]
    pub fn allows_action(&self, concept: u16, action: &str) -> bool {
        let verb = action_verb(action);
        self.grants
            .iter()
            .any(|g| g.target_classid == concept && g.op_mask.contains(verb))
            && self
                .actions
                .iter()
                .any(|a| a.concept == concept && a.action == action)
    }

    /// Whether this set permits `op` on `class` (verb gate only).
    #[must_use]
    pub fn permits(&self, class: u32, op: &Operation<'_>) -> bool {
        grants_permit(&self.grants, class, op)
    }
}

/// Who a set is assigned to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Assignee {
    /// Everyone holding the role.
    Role(RoleId),
    /// One directory principal.
    Principal(Guid128),
}

/// The principal's seat: the ceiling of what any grant can unlock.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Seat {
    /// Bumped whenever the seat assignment or its capabilities change.
    pub version: SeatVersion,
    /// Concepts the seat can reach at all.
    pub concepts: Vec<u16>,
    /// Action names the seat can perform at all.
    pub actions: Vec<&'static str>,
}

impl Default for SeatVersion {
    fn default() -> Self {
        Self::NONE
    }
}

/// Why the policy refused a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyError {
    /// A set with this id already exists.
    DuplicateSet(PermissionSetId),
    /// No set with this id.
    UnknownSet(PermissionSetId),
}

/// One tenant's permission policy at one version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantPolicy {
    tenant: TenantId,
    version: PolicyVersion,
    generation: u64,
    sets: Vec<(PermissionSetId, PermissionSet)>,
    assigned: Vec<(Assignee, PermissionSetId)>,
}

impl TenantPolicy {
    /// An empty policy: grants nothing.
    #[must_use]
    pub fn new(tenant: TenantId, version: PolicyVersion) -> Self {
        Self {
            tenant,
            version,
            generation: 0,
            sets: Vec::new(),
            assigned: Vec::new(),
        }
    }

    /// The tenant the policy belongs to.
    #[must_use]
    pub fn tenant(&self) -> TenantId {
        self.tenant
    }

    /// The declared policy version.
    #[must_use]
    pub fn version(&self) -> PolicyVersion {
        self.version
    }

    /// Bumped by every change, so a projection compiled before a change is
    /// refused after it even when the declared version was not bumped.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Add a set.
    ///
    /// # Errors
    ///
    /// [`PolicyError::DuplicateSet`] when `id` is taken.
    pub fn add_set(&mut self, id: PermissionSetId, set: PermissionSet) -> Result<(), PolicyError> {
        if self.sets.iter().any(|(i, _)| *i == id) {
            return Err(PolicyError::DuplicateSet(id));
        }
        self.sets.push((id, set));
        self.generation += 1;
        Ok(())
    }

    /// Replace a set's content (a tenant editing a permission set).
    ///
    /// # Errors
    ///
    /// [`PolicyError::UnknownSet`] when `id` does not exist.
    pub fn replace_set(
        &mut self,
        id: PermissionSetId,
        set: PermissionSet,
    ) -> Result<(), PolicyError> {
        let slot = self
            .sets
            .iter_mut()
            .find(|(i, _)| *i == id)
            .ok_or(PolicyError::UnknownSet(id))?;
        slot.1 = set;
        self.generation += 1;
        Ok(())
    }

    /// Assign a set.
    ///
    /// # Errors
    ///
    /// [`PolicyError::UnknownSet`] when `id` does not exist.
    pub fn assign(&mut self, to: Assignee, id: PermissionSetId) -> Result<(), PolicyError> {
        if !self.sets.iter().any(|(i, _)| *i == id) {
            return Err(PolicyError::UnknownSet(id));
        }
        self.assigned.push((to, id));
        self.generation += 1;
        Ok(())
    }

    /// Withdraw every assignment of `id` to `to`.
    pub fn unassign(&mut self, to: Assignee, id: PermissionSetId) {
        self.assigned.retain(|(a, i)| !(*a == to && *i == id));
        self.generation += 1;
    }

    /// The sets that apply to `principal` holding `roles`, each once.
    #[must_use]
    pub fn sets_for(&self, principal: &Guid128, roles: &[RoleId]) -> Vec<&PermissionSet> {
        let mut ids: Vec<PermissionSetId> = self
            .assigned
            .iter()
            .filter(|(a, _)| match a {
                Assignee::Role(r) => roles.contains(r),
                Assignee::Principal(p) => p == principal,
            })
            .map(|(_, id)| *id)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids.iter()
            .filter_map(|id| self.sets.iter().find(|(i, _)| i == id).map(|(_, s)| s))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ogar_vocab::class_ids;

    fn g(n: u8) -> Guid128 {
        Guid128([n; 16])
    }

    const SALES: RoleId = "crm_sales_user";

    fn set_with(op: OpMask, actions: &[&'static str]) -> PermissionSet {
        PermissionSet {
            grants: vec![ClassGrant::new(class_ids::CRM_CONTACT, op)],
            actions: actions
                .iter()
                .map(|&a| ActionGrant {
                    concept: class_ids::CRM_CONTACT,
                    action: a,
                })
                .collect(),
            ..PermissionSet::default()
        }
    }

    #[test]
    fn an_action_needs_its_verb_and_its_named_gate() {
        let all = OpMask::READ
            .union(OpMask::WRITE)
            .union(OpMask::CREATE)
            .union(OpMask::ACT)
            .union(OpMask::DELETE);
        let c = class_ids::CRM_CONTACT;
        // The verb alone does not allow it.
        assert!(!set_with(all, &[]).allows_action(c, "email.send"));
        assert!(!set_with(all, &[]).allows_action(c, ACTION_DELETE));
        assert!(!set_with(all, &[]).allows_action(c, ACTION_EXPORT));
        // The gate alone does not allow it either.
        let write = OpMask::READ.union(OpMask::WRITE).union(OpMask::CREATE);
        assert!(!set_with(write, &["email.send", ACTION_DELETE]).allows_action(c, "email.send"));
        assert!(!set_with(write, &[ACTION_DELETE]).allows_action(c, ACTION_DELETE));
        // Both: allowed, and only that action.
        let s = set_with(all, &["email.send"]);
        assert!(s.allows_action(c, "email.send"));
        assert!(!s.allows_action(c, "sequence.enroll"));
        assert!(!s.allows_action(class_ids::CRM_DEAL, "email.send"));
        // Export needs READ plus its gate.
        assert!(set_with(OpMask::READ, &[ACTION_EXPORT]).allows_action(c, ACTION_EXPORT));
    }

    #[test]
    fn sets_apply_by_role_or_principal_and_unknown_sets_are_refused() {
        let mut p = TenantPolicy::new(TenantId(1), PolicyVersion(1));
        assert_eq!(
            p.assign(Assignee::Role(SALES), PermissionSetId(9)),
            Err(PolicyError::UnknownSet(PermissionSetId(9)))
        );
        p.add_set(PermissionSetId(1), set_with(OpMask::READ, &[]))
            .unwrap();
        p.add_set(PermissionSetId(2), set_with(OpMask::WRITE, &[]))
            .unwrap();
        assert_eq!(
            p.add_set(PermissionSetId(1), PermissionSet::default()),
            Err(PolicyError::DuplicateSet(PermissionSetId(1)))
        );
        p.assign(Assignee::Role(SALES), PermissionSetId(1)).unwrap();
        p.assign(Assignee::Principal(g(7)), PermissionSetId(2))
            .unwrap();
        assert_eq!(p.sets_for(&g(7), &[SALES]).len(), 2);
        assert_eq!(p.sets_for(&g(8), &[SALES]).len(), 1);
        assert_eq!(p.sets_for(&g(7), &[]).len(), 1);
        assert!(p.sets_for(&g(8), &[]).is_empty(), "no assignment, no set");
    }

    #[test]
    fn every_change_bumps_the_generation() {
        let mut p = TenantPolicy::new(TenantId(1), PolicyVersion(1));
        let mut last = p.generation();
        let mut bumped = |p: &TenantPolicy| {
            assert!(p.generation() > last);
            last = p.generation();
        };
        p.add_set(PermissionSetId(1), PermissionSet::default())
            .unwrap();
        bumped(&p);
        p.assign(Assignee::Role(SALES), PermissionSetId(1)).unwrap();
        bumped(&p);
        p.replace_set(PermissionSetId(1), set_with(OpMask::READ, &[]))
            .unwrap();
        bumped(&p);
        p.unassign(Assignee::Role(SALES), PermissionSetId(1));
        bumped(&p);
        assert!(p.sets_for(&g(1), &[SALES]).is_empty());
    }
}

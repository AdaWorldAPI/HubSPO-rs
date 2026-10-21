//! D-IAM-1: HubSpot's preset roles as grants over the plugged concepts.
//!
//! [`Catalogue`] is the [`GrantSource`] behind `hubspo_port::RBAC_PLUG`:
//! one `(concept, verbs)` grant per plugged concept for each of the six
//! roles in `hubspo_port::RBAC_ROLES`. [`bind`] resolves the plug against
//! OGAR's authority and returns the [`RbacBinding`] every decision reads.
//!
//! ## What this is not
//!
//! - **Not actor assignment.** [`Catalogue::roles_of`] knows no actor: the
//!   roles a user holds come from the request's identity
//!   (`ogar_rbac::IdentityActors`, built once per request from the
//!   authenticated user). Resolve them once per request, never per record:
//!   a dir-sim `View` walk is a full membership scan (0.4–0.7 ms at
//!   35k–263k rows, lance-graph D-RMAP-DIR-1).
//! - **Not field masks.** [`Catalogue::field_mask_of`] is the trait default
//!   (`None`, no column restriction). Column grants and the packed-run inner
//!   mask are D-IAM-3.
//! - **Not tenant permission sets.** A HubSpot permission set is tenant
//!   data, created at runtime, so it cannot be a `&'static str` role; it is
//!   an open item on the D-IAM-1 row.
//!
//! ## The verb table
//!
//! The presets are a provisional policy, written from the seat names, not
//! from a HubSpot export. Every role reads every plugged concept; the table
//! below lists what it may do beyond reading. `edit` is
//! [`OpMask::WRITE`] | [`OpMask::CREATE`]; `act` adds [`OpMask::ACT`];
//! `delete` is [`OpMask::DELETE`].
//!
//! | role | edit + act | edit | act |
//! |---|---|---|---|
//! | super admin | all, plus delete | — | — |
//! | admin | all, plus delete, except users and teams (no delete) | — | — |
//! | sales user | contact, company, deal, lead, call, meeting, task, note, email, line item, quote/invoice | list | sequence |
//! | service user | contact, company, ticket, call, meeting, task, note, email | list | — |
//! | marketing user | contact, company, campaign, email | list, note, task | — |
//! | view only | — | — | — |
//!
//! Only the two admin roles delete.

use hubspo_port::{
    RBAC_CLASSIDS, RBAC_PLUG, ROLE_ADMIN, ROLE_MARKETING_USER, ROLE_SALES_USER, ROLE_SERVICE_USER,
    ROLE_SUPER_ADMIN, ROLE_VIEW_ONLY,
};
use lance_graph_contract::rbac::{ActorId, ClassGrant, OpMask, RoleId};
use lance_graph_contract::rbac_plug::{RbacAuthority, RbacBinding, RbacDrift};
use ogar_rbac::{GrantSource, OgarRbac};
use ogar_vocab::class_ids;

const READ: OpMask = OpMask::READ;
const EDIT: OpMask = READ.union(OpMask::WRITE).union(OpMask::CREATE);
const EDIT_ACT: OpMask = EDIT.union(OpMask::ACT);
const READ_ACT: OpMask = READ.union(OpMask::ACT);
const FULL: OpMask = EDIT_ACT.union(OpMask::DELETE);

/// How many concepts each role holds a grant on: every plugged one.
const N: usize = RBAC_CLASSIDS.len();

/// The preset a role is built from.
#[derive(Clone, Copy)]
enum Preset {
    SuperAdmin,
    Admin,
    Sales,
    Service,
    Marketing,
    ViewOnly,
}

/// The verbs `preset` holds on `concept`.
const fn verbs(preset: Preset, concept: u16) -> OpMask {
    match preset {
        Preset::SuperAdmin => FULL,
        Preset::Admin => match concept {
            class_ids::PROJECT_ACTOR => EDIT_ACT,
            _ => FULL,
        },
        Preset::Sales => match concept {
            class_ids::CRM_CONTACT
            | class_ids::CRM_COMPANY
            | class_ids::CRM_DEAL
            | class_ids::CRM_LEAD
            | class_ids::CRM_CALL
            | class_ids::CRM_MEETING
            | class_ids::CRM_TASK
            | class_ids::CRM_NOTE
            | class_ids::EMAIL
            | class_ids::COMMERCIAL_LINE_ITEM
            | class_ids::COMMERCIAL_DOCUMENT => EDIT_ACT,
            class_ids::CRM_LIST => EDIT,
            class_ids::CRM_SEQUENCE => READ_ACT,
            _ => READ,
        },
        Preset::Service => match concept {
            class_ids::CRM_CONTACT
            | class_ids::CRM_COMPANY
            | class_ids::CRM_TICKET
            | class_ids::CRM_CALL
            | class_ids::CRM_MEETING
            | class_ids::CRM_TASK
            | class_ids::CRM_NOTE
            | class_ids::EMAIL => EDIT_ACT,
            class_ids::CRM_LIST => EDIT,
            _ => READ,
        },
        Preset::Marketing => match concept {
            class_ids::CRM_CONTACT
            | class_ids::CRM_COMPANY
            | class_ids::CRM_CAMPAIGN
            | class_ids::EMAIL => EDIT_ACT,
            class_ids::CRM_LIST | class_ids::CRM_NOTE | class_ids::CRM_TASK => EDIT,
            _ => READ,
        },
        Preset::ViewOnly => READ,
    }
}

/// One grant per plugged concept, in [`RBAC_CLASSIDS`] order.
const fn grants(preset: Preset) -> [ClassGrant; N] {
    let mut out = [ClassGrant::new(0, OpMask::NONE); N];
    let mut i = 0;
    while i < N {
        let concept = RBAC_CLASSIDS[i];
        out[i] = ClassGrant::new(concept, verbs(preset, concept));
        i += 1;
    }
    out
}

const SUPER_ADMIN: [ClassGrant; N] = grants(Preset::SuperAdmin);
const ADMIN: [ClassGrant; N] = grants(Preset::Admin);
const SALES: [ClassGrant; N] = grants(Preset::Sales);
const SERVICE: [ClassGrant; N] = grants(Preset::Service);
const MARKETING: [ClassGrant; N] = grants(Preset::Marketing);
const VIEW_ONLY: [ClassGrant; N] = grants(Preset::ViewOnly);

/// The preset grant table: the [`GrantSource`] behind `RBAC_PLUG`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Catalogue;

impl GrantSource for Catalogue {
    /// Always empty: actors come from the request's identity, not from the
    /// catalogue (see the module docs).
    fn roles_of(&self, _actor: ActorId<'_>) -> &[RoleId] {
        &[]
    }

    fn grants_of(&self, role: RoleId) -> &[ClassGrant] {
        match role {
            ROLE_SUPER_ADMIN => &SUPER_ADMIN,
            ROLE_ADMIN => &ADMIN,
            ROLE_SALES_USER => &SALES,
            ROLE_SERVICE_USER => &SERVICE,
            ROLE_MARKETING_USER => &MARKETING,
            ROLE_VIEW_ONLY => &VIEW_ONLY,
            _ => &[],
        }
    }
}

/// Bind `RBAC_PLUG` against OGAR's authority with the preset grants.
///
/// # Errors
///
/// The drift OGAR reports when the plug names a concept it does not mint, a
/// role the catalogue does not define, or a concept the contract's wire
/// mirror disagrees on.
pub fn bind() -> Result<RbacBinding, RbacDrift> {
    OgarRbac::new(Catalogue).bind(&RBAC_PLUG)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hubspo_port::{CONSUMER, CRM_CLASSIDS, RBAC_ROLES};
    use lance_graph_contract::rbac::{ClassId, ClassRbac, Operation};
    use lance_graph_contract::rbac_plug::ActorSource;
    use std::collections::BTreeSet;

    fn class(concept: u16) -> ClassId {
        u32::from(concept) << 16
    }

    fn verbs_of(binding: &RbacBinding, role: RoleId, concept: u16) -> OpMask {
        let grants = binding.grants_for(role).expect("plugged role");
        let mut hits = grants.iter().filter(|g| g.target_classid == concept);
        let grant = hits.next().expect("a grant on every plugged concept");
        assert!(hits.next().is_none(), "one grant per concept");
        grant.op_mask
    }

    fn write() -> Operation<'static> {
        Operation::Write { predicate: "name" }
    }

    // ── the bind ──

    #[test]
    fn the_plug_binds_with_every_preset() {
        let binding = bind().expect("RBAC plug drifted from OGAR or the wire mirror");
        assert_eq!(binding.consumer(), CONSUMER);
        let bound: Vec<RoleId> = binding.declared_grants().iter().map(|(r, _)| *r).collect();
        assert_eq!(bound, RBAC_ROLES);
        for &(role, ref grants) in binding.declared_grants() {
            assert_eq!(grants.len(), RBAC_CLASSIDS.len(), "{role}: nothing dropped");
        }
        for &id in RBAC_CLASSIDS {
            assert_eq!(binding.plugged(class(id)), Ok(id));
        }
    }

    #[test]
    fn every_plug_role_is_defined_and_nothing_else_is() {
        for &role in RBAC_ROLES {
            assert!(Catalogue.defines_role(role), "{role}");
        }
        assert!(!Catalogue.defines_role("crm_owner"));
        assert!(
            !Catalogue.defines_role("CRM_SUPER_ADMIN"),
            "names are exact"
        );
    }

    #[test]
    fn a_concept_outside_the_plug_is_refused_not_granted() {
        let binding = bind().expect("binds");
        assert!(!RBAC_CLASSIDS.contains(&class_ids::PATIENT));
        assert_eq!(
            binding.permits(ROLE_SUPER_ADMIN, class(class_ids::PATIENT), &write()),
            Err(RbacDrift::NotPlugged(class_ids::PATIENT))
        );
    }

    // ── the table's shape ──

    #[test]
    fn every_role_reads_every_plugged_concept() {
        let binding = bind().expect("binds");
        let read = Operation::Read {
            depth: lance_graph_contract::property::PrefetchDepth::Full,
        };
        for &role in RBAC_ROLES {
            for &id in RBAC_CLASSIDS {
                assert_eq!(binding.permits(role, class(id), &read), Ok(true), "{role}");
            }
        }
    }

    #[test]
    fn view_only_reads_and_nothing_more() {
        let binding = bind().expect("binds");
        for &id in RBAC_CLASSIDS {
            assert_eq!(verbs_of(&binding, ROLE_VIEW_ONLY, id), OpMask::READ);
        }
    }

    #[test]
    fn only_the_admins_delete() {
        let binding = bind().expect("binds");
        let deleters: BTreeSet<RoleId> = RBAC_ROLES
            .iter()
            .copied()
            .filter(|&role| {
                RBAC_CLASSIDS
                    .iter()
                    .any(|&id| verbs_of(&binding, role, id).contains(OpMask::DELETE))
            })
            .collect();
        assert_eq!(deleters, BTreeSet::from([ROLE_SUPER_ADMIN, ROLE_ADMIN]));
    }

    #[test]
    fn super_admin_holds_every_verb_everywhere_and_admin_falls_short_on_users() {
        let binding = bind().expect("binds");
        for &id in RBAC_CLASSIDS {
            assert_eq!(verbs_of(&binding, ROLE_SUPER_ADMIN, id), FULL);
            let admin = verbs_of(&binding, ROLE_ADMIN, id);
            if id == class_ids::PROJECT_ACTOR {
                assert_eq!(admin, EDIT_ACT);
            } else {
                assert_eq!(admin, FULL, "{id:#06x}");
            }
        }
    }

    #[test]
    fn each_seat_edits_exactly_its_own_concepts() {
        let binding = bind().expect("binds");
        let editable = |role| -> BTreeSet<u16> {
            RBAC_CLASSIDS
                .iter()
                .copied()
                .filter(|&id| verbs_of(&binding, role, id).contains(OpMask::WRITE))
                .collect()
        };
        use class_ids::*;
        assert_eq!(
            editable(ROLE_SALES_USER),
            BTreeSet::from([
                CRM_CONTACT,
                CRM_COMPANY,
                CRM_DEAL,
                CRM_LEAD,
                CRM_CALL,
                CRM_MEETING,
                CRM_TASK,
                CRM_NOTE,
                EMAIL,
                COMMERCIAL_LINE_ITEM,
                COMMERCIAL_DOCUMENT,
                CRM_LIST,
            ])
        );
        assert_eq!(
            editable(ROLE_SERVICE_USER),
            BTreeSet::from([
                CRM_CONTACT,
                CRM_COMPANY,
                CRM_TICKET,
                CRM_CALL,
                CRM_MEETING,
                CRM_TASK,
                CRM_NOTE,
                EMAIL,
                CRM_LIST,
            ])
        );
        assert_eq!(
            editable(ROLE_MARKETING_USER),
            BTreeSet::from([
                CRM_CONTACT,
                CRM_COMPANY,
                CRM_CAMPAIGN,
                EMAIL,
                CRM_LIST,
                CRM_NOTE,
                CRM_TASK,
            ])
        );
        assert!(editable(ROLE_VIEW_ONLY).is_empty());
    }

    #[test]
    fn create_travels_with_write() {
        let binding = bind().expect("binds");
        for &role in RBAC_ROLES {
            for &id in RBAC_CLASSIDS {
                let v = verbs_of(&binding, role, id);
                assert_eq!(
                    v.contains(OpMask::WRITE),
                    v.contains(OpMask::CREATE),
                    "{role} on {id:#06x}"
                );
            }
        }
    }

    // ── decisions through an actor source ──

    /// One request's actors: a sales rep and a viewer.
    struct Request;

    impl ActorSource for Request {
        fn roles_of(&self, actor: ActorId<'_>) -> &[RoleId] {
            match actor {
                "rep" => &[ROLE_SALES_USER],
                "viewer" => &[ROLE_VIEW_ONLY],
                _ => &[],
            }
        }
    }

    #[test]
    fn a_sales_rep_writes_deals_but_not_tickets() {
        let binding = bind().expect("binds");
        let rbac = binding.with_actors(Request);
        let held = |actor, concept| {
            rbac.actor_roles(actor)
                .iter()
                .any(|&r| rbac.grant_permits(r, class(concept), &write()))
        };
        assert!(held("rep", class_ids::CRM_DEAL));
        assert!(!held("rep", class_ids::CRM_TICKET));
        assert!(!held("viewer", class_ids::CRM_DEAL));
        assert!(!held("stranger", class_ids::CRM_DEAL), "unknown actor");
    }

    #[test]
    fn sequences_are_enrolled_not_edited_by_sales() {
        let binding = bind().expect("binds");
        let act = Operation::Act { action: "enroll" };
        let seq = class(class_ids::CRM_SEQUENCE);
        assert_eq!(binding.permits(ROLE_SALES_USER, seq, &act), Ok(true));
        assert_eq!(binding.permits(ROLE_SALES_USER, seq, &write()), Ok(false));
        assert_eq!(binding.permits(ROLE_SERVICE_USER, seq, &act), Ok(false));
    }

    #[test]
    fn the_crm_concepts_are_inside_the_table() {
        let all: BTreeSet<u16> = SUPER_ADMIN.iter().map(|g| g.target_classid).collect();
        assert!(CRM_CLASSIDS.iter().all(|id| all.contains(id)));
        assert_eq!(all.len(), RBAC_CLASSIDS.len());
    }
}

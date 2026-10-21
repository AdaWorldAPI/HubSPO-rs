//! D-IAM-1: a request's roles come from the directory (security groups →
//! preset roles, as one version of the group-role policy states them) and
//! are decided against the preset grants through the plugged binding.

use hubspo_iam::roles::{RoleRefusal, bind, bind_preset_fixture, request_actors};
use hubspo_port::{ROLE_ADMIN, ROLE_SALES_USER, ROLE_VIEW_ONLY};
use lance_graph_contract::rbac::{ClassId, ClassRbac, Operation};
use lance_graph_contract::rbac_plug::ActorSource;
use lance_graph_dir_sim::actors::GroupRoles;
use lance_graph_dir_sim::*;
use ogar_dir_core::{DirectoryScope, Guid128};
use ogar_vocab::class_ids;

fn g(n: u8) -> Guid128 {
    Guid128([n; 16])
}
const DIR: DirectoryScope = DirectoryScope(Guid128([0x5C; 16]));

const REP: u8 = 0x11;
const VIEWER: u8 = 0x12;
const LEAD: u8 = 0x13;
const NEWCOMER: u8 = 0x14;
const LOOSE: u8 = 0x15;
/// Security group granted the sales preset.
const SALES: u8 = 0xA1;
/// Security group granted view-only.
const READERS: u8 = 0xA2;
/// Security group granted admin, nested in SALES.
const ADMINS: u8 = 0xA3;
/// Security group whose grants were never read.
const NEW: u8 = 0xA4;
/// Distribution group: no SID, passes on nothing.
const LIST: u8 = 0xD1;

fn security(security: bool) -> ObservedNode {
    let mut n = ObservedNode::group();
    n.security = Some(security);
    n
}

// SALES ⊃ {rep, ADMINS}; ADMINS ⊃ {lead}; READERS ⊃ {viewer};
// NEW ⊃ {newcomer}; LIST ⊃ {loose}, and LIST is granted sales.
fn observed() -> Observation {
    let user = |n: &str| ObservedNode::user(&format!("{n}@x.test"), &format!("{n}@x.test"));
    Observation {
        scope: DIR,
        nodes: vec![
            (g(REP), user("rep")),
            (g(VIEWER), user("viewer")),
            (g(LEAD), user("lead")),
            (g(NEWCOMER), user("newcomer")),
            (g(LOOSE), user("loose")),
            (g(SALES), security(true)),
            (g(READERS), security(true)),
            (g(ADMINS), security(true)),
            (g(NEW), security(true)),
            (g(LIST), security(false)),
        ],
        members: vec![
            (g(REP), g(SALES)),
            (g(ADMINS), g(SALES)),
            (g(LEAD), g(ADMINS)),
            (g(VIEWER), g(READERS)),
            (g(NEWCOMER), g(NEW)),
            (g(LOOSE), g(LIST)),
        ],
    }
}

fn policy() -> GroupRoles {
    let mut gr = GroupRoles::new();
    gr.read(g(SALES), &[ROLE_SALES_USER])
        .read(g(READERS), &[ROLE_VIEW_ONLY])
        .read(g(ADMINS), &[ROLE_ADMIN, "not_a_hubspo_role"])
        .read(g(LIST), &[ROLE_SALES_USER]);
    gr
}

fn class(concept: u16) -> ClassId {
    u32::from(concept) << 16
}

fn writes(rbac: &impl ClassRbac, actor: &str, concept: u16) -> bool {
    let op = Operation::Write { predicate: "name" };
    rbac.actor_roles(actor)
        .iter()
        .any(|&r| rbac.grant_permits(r, class(concept), &op))
}

#[test]
fn roles_come_from_security_groups_through_the_binding() {
    let mut st = VersionStore::new();
    let v = st.observe("crm", 1, observed()).unwrap();
    let view = st.view(v).unwrap();
    // The preset fixture, so the roles' verbs can be checked; the runtime
    // binding grants nothing (see `runtime_roles_resolve_but_grant_nothing`).
    let binding = bind_preset_fixture().unwrap();

    let rep = request_actors(&view, &g(REP), &policy(), &binding).unwrap();
    assert_eq!(rep.roles(), &[ROLE_SALES_USER]);
    let rbac = binding.with_actors(rep.clone());
    assert!(writes(&rbac, rep.subject(), class_ids::CRM_DEAL));
    assert!(!writes(&rbac, rep.subject(), class_ids::CRM_TICKET));
    assert!(!writes(&rbac, "someone-else", class_ids::CRM_DEAL));

    // Nested: lead carries ADMINS and, through it, SALES. The role OGAR's
    // policy names but the plug does not declare is dropped.
    let lead = request_actors(&view, &g(LEAD), &policy(), &binding).unwrap();
    let mut held = lead.roles().to_vec();
    held.sort_unstable();
    let mut want = vec![ROLE_ADMIN, ROLE_SALES_USER];
    want.sort_unstable();
    assert_eq!(held, want);
    assert!(writes(
        &binding.with_actors(lead.clone()),
        lead.subject(),
        class_ids::CRM_TICKET
    ));

    let viewer = request_actors(&view, &g(VIEWER), &policy(), &binding).unwrap();
    assert_eq!(viewer.roles(), &[ROLE_VIEW_ONLY]);
    assert!(!writes(
        &binding.with_actors(viewer.clone()),
        viewer.subject(),
        class_ids::CRM_CONTACT
    ));
}

#[test]
fn a_distribution_group_and_a_stranger_grant_nothing() {
    let mut st = VersionStore::new();
    let v = st.observe("crm", 1, observed()).unwrap();
    let view = st.view(v).unwrap();
    let binding = bind().unwrap();
    let loose = request_actors(&view, &g(LOOSE), &policy(), &binding).unwrap();
    assert!(loose.roles().is_empty(), "LIST has no SID");
    let ghost = request_actors(&view, &g(0xF0), &policy(), &binding).unwrap();
    assert!(ghost.roles().is_empty());
    assert!(ghost.roles_of(ghost.subject()).is_empty());
}

#[test]
fn an_unread_group_refuses_the_request() {
    let mut st = VersionStore::new();
    let v = st.observe("crm", 1, observed()).unwrap();
    let view = st.view(v).unwrap();
    let binding = bind().unwrap();
    assert_eq!(
        request_actors(&view, &g(NEWCOMER), &policy(), &binding),
        Err(RoleRefusal::UnreadGroups(vec![g(NEW)]))
    );
    // Read with no grants is not unread: the request is decided, with no role.
    let mut gr = policy();
    gr.read(g(NEW), &[]);
    let newcomer = request_actors(&view, &g(NEWCOMER), &gr, &binding).unwrap();
    assert!(newcomer.roles().is_empty());
}

#[test]
fn runtime_roles_resolve_but_grant_nothing() {
    let mut st = VersionStore::new();
    let v = st.observe("crm", 1, observed()).unwrap();
    let view = st.view(v).unwrap();
    let binding = bind().unwrap();
    let lead = request_actors(&view, &g(LEAD), &policy(), &binding).unwrap();
    let mut held = lead.roles().to_vec();
    held.sort_unstable();
    let mut want = vec![ROLE_ADMIN, ROLE_SALES_USER];
    want.sort_unstable();
    assert_eq!(held, want, "the names resolve");
    assert!(!writes(
        &binding.with_actors(lead.clone()),
        lead.subject(),
        class_ids::CRM_DEAL
    ));
}

//! # hubspo-port
//!
//! The one doorway to OGAR's codebook: the [`HOT_PLUG`] and [`RBAC_PLUG`]
//! declarations and [`class_id`] lookups over [`HubSpoPort`]. Holds no
//! codebook, no concept hex and no name→id map: every id is a named
//! `ogar_vocab::class_ids` constant, and every name resolves through
//! `HubSpoPort`'s alias table.
//!
//! - Zone: zone 2 (no serde, no wire formats).
//! - Deliverable: D-ONT-5 (`.claude/plans/hubspo-integration-v1.md`).
//!
//! ## What is plugged, and what is not yet
//!
//! | plug | classids | today |
//! |---|---|---|
//! | [`RBAC_PLUG`] | the 15 CRM concepts + the 7 core concepts HubSpot objects converge on | declares the six preset roles; their grants and the bind are `hubspo-iam::roles` (D-IAM-1) |
//! | [`HOT_PLUG`] | the 15 CRM concepts | refuses with `NoCapabilitiesFor` |
//!
//! The role names live here because they are part of the plug declaration;
//! the grants behind them live in `hubspo-iam`, which depends on this crate,
//! so the bind is done there.
//!
//! The capability gap is deliberate:
//!
//! - **No capabilities.** OGAR has no `crm_actions` table: capabilities are
//!   declared only once HubSPO-rs implements them (dossier v3 §7, the
//!   `geo_actions` rule). Until D-ACT-1, activating [`HOT_PLUG`] reports
//!   `NoCapabilitiesFor` for the first CRM id, and a test pins exactly that,
//!   so the day the table lands the test fails and forces the covered list
//!   to be written.

use lance_graph_contract::hotplug::HotPlug;
use lance_graph_contract::ogar_codebook::render_classid;
use lance_graph_contract::rbac::RoleId;
use lance_graph_contract::rbac_plug::RbacPlug;
use ogar_vocab::class_ids;
use ogar_vocab::ports::PortSpec;

pub use ogar_vocab::ports::HubSpoPort;

/// The consumer name both plugs carry.
pub const CONSUMER: &str = "hubspo-rs";

/// The 15 CRM concepts (`ConceptDomain::Crm`, minted by OGAR#330).
pub const CRM_CLASSIDS: &[u16] = &[
    class_ids::CRM_CONTACT,
    class_ids::CRM_COMPANY,
    class_ids::CRM_DEAL,
    class_ids::CRM_TICKET,
    class_ids::CRM_LEAD,
    class_ids::CRM_PIPELINE,
    class_ids::CRM_PIPELINE_STAGE,
    class_ids::CRM_CALL,
    class_ids::CRM_MEETING,
    class_ids::CRM_TASK,
    class_ids::CRM_NOTE,
    class_ids::CRM_LIST,
    class_ids::CRM_CAMPAIGN,
    class_ids::CRM_SEQUENCE,
    class_ids::CRM_CUSTOM_RECORD,
];

/// The core concepts HubSpot objects converge on instead of minting
/// (`HUBSPO_ALIASES`; dossier v3 §2).
pub const SHARED_CLASSIDS: &[u16] = &[
    class_ids::EMAIL,
    class_ids::PRODUCT,
    class_ids::COMMERCIAL_LINE_ITEM,
    class_ids::COMMERCIAL_DOCUMENT,
    class_ids::PAYMENT_RECORD,
    class_ids::TAX_POLICY,
    class_ids::PROJECT_ACTOR,
];

/// Every concept whose access HubSPO-rs decides: [`CRM_CLASSIDS`] followed
/// by [`SHARED_CLASSIDS`].
pub const RBAC_CLASSIDS: &[u16] = &[
    class_ids::CRM_CONTACT,
    class_ids::CRM_COMPANY,
    class_ids::CRM_DEAL,
    class_ids::CRM_TICKET,
    class_ids::CRM_LEAD,
    class_ids::CRM_PIPELINE,
    class_ids::CRM_PIPELINE_STAGE,
    class_ids::CRM_CALL,
    class_ids::CRM_MEETING,
    class_ids::CRM_TASK,
    class_ids::CRM_NOTE,
    class_ids::CRM_LIST,
    class_ids::CRM_CAMPAIGN,
    class_ids::CRM_SEQUENCE,
    class_ids::CRM_CUSTOM_RECORD,
    class_ids::EMAIL,
    class_ids::PRODUCT,
    class_ids::COMMERCIAL_LINE_ITEM,
    class_ids::COMMERCIAL_DOCUMENT,
    class_ids::PAYMENT_RECORD,
    class_ids::TAX_POLICY,
    class_ids::PROJECT_ACTOR,
];

/// HubSpot's Super Admin: every verb on every plugged concept.
pub const ROLE_SUPER_ADMIN: RoleId = "crm_super_admin";
/// An account admin: every verb, except deleting users and teams.
pub const ROLE_ADMIN: RoleId = "crm_admin";
/// The sales seat preset.
pub const ROLE_SALES_USER: RoleId = "crm_sales_user";
/// The service seat preset.
pub const ROLE_SERVICE_USER: RoleId = "crm_service_user";
/// The marketing seat preset.
pub const ROLE_MARKETING_USER: RoleId = "crm_marketing_user";
/// Read-only access to every plugged concept.
pub const ROLE_VIEW_ONLY: RoleId = "crm_view_only";

/// The roles HubSPO-rs declares: HubSpot's preset roles (D-IAM-1). Their
/// grants are `hubspo-iam::roles`; tenant permission sets are not roles.
pub const RBAC_ROLES: &[RoleId] = &[
    ROLE_SUPER_ADMIN,
    ROLE_ADMIN,
    ROLE_SALES_USER,
    ROLE_SERVICE_USER,
    ROLE_MARKETING_USER,
    ROLE_VIEW_ONLY,
];

/// HubSPO-rs's RBAC declaration.
pub const RBAC_PLUG: RbacPlug = RbacPlug {
    consumer: CONSUMER,
    classids: RBAC_CLASSIDS,
    roles: RBAC_ROLES,
};

/// The capability names the HubSPO-rs executor covers. Empty until D-ACT-1
/// implements the first CRM action; written by hand when it does, never
/// re-exported from OGAR's table (a re-export would make the coverage check
/// compare a list with itself).
pub const COVERED_CAPABILITIES: &[&str] = &[];

/// HubSPO-rs's hot-plug declaration: the CRM concepts it executes.
/// Aliased concepts are bound through [`RBAC_PLUG`] only, so other apps'
/// hot-plugs on them are untouched (dossier v3 §4.2).
pub const HOT_PLUG: HotPlug = HotPlug {
    consumer: CONSUMER,
    classids: CRM_CLASSIDS,
    covered: COVERED_CAPABILITIES,
};

/// The canonical concept of a HubSpot object name (`"Contact"`, `"Quote"`,
/// …), through `HubSpoPort`'s alias table.
#[must_use]
pub fn class_id(public_name: &str) -> Option<u16> {
    HubSpoPort::class_id(public_name)
}

/// The full render classid of a HubSpot object name: the canonical concept
/// in the high half, HubSpoPort's render prefix in the low half.
#[must_use]
pub fn render_class_id(public_name: &str) -> Option<u32> {
    class_id(public_name).map(|concept| render_classid(HubSpoPort::APP_PREFIX, concept))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lance_graph_contract::ogar_codebook::{AppPrefix, classid_app_prefix, classid_concept};
    use lance_graph_contract::rbac::{ActorId, ClassGrant};
    use lance_graph_contract::rbac_plug::{RbacAuthority, RbacDrift};
    use ogar_rbac::{GrantSource, OgarRbac};
    use ogar_vocab::capability_registry::{HotplugDrift, resolve_hotplug};
    use ogar_vocab::ports::HUBSPO_ALIASES;
    use ogar_vocab::{ConceptDomain, concepts_in_domain};
    use std::collections::BTreeSet;

    /// A source that defines no role, for the refusal arms.
    struct NoGrants;

    impl GrantSource for NoGrants {
        fn roles_of(&self, _actor: ActorId<'_>) -> &[RoleId] {
            &[]
        }

        fn grants_of(&self, _role: RoleId) -> &[ClassGrant] {
            &[]
        }
    }

    fn set(ids: &[u16]) -> BTreeSet<u16> {
        ids.iter().copied().collect()
    }

    // ── coverage: the lists are the authority's sets, not a guess ──

    #[test]
    fn crm_classids_are_exactly_the_crm_domain() {
        let domain: BTreeSet<u16> = concepts_in_domain(ConceptDomain::Crm)
            .map(|(_, id)| id)
            .collect();
        assert_eq!(domain.len(), 15);
        assert_eq!(set(CRM_CLASSIDS), domain);
        assert_eq!(CRM_CLASSIDS.len(), 15, "no duplicates");
    }

    #[test]
    fn rbac_classids_are_every_hubspo_alias_target() {
        let targets: BTreeSet<u16> = HUBSPO_ALIASES.iter().map(|&(_, id)| id).collect();
        assert_eq!(set(RBAC_CLASSIDS), targets);
        assert_eq!(
            RBAC_CLASSIDS.len(),
            CRM_CLASSIDS.len() + SHARED_CLASSIDS.len()
        );
        let union: BTreeSet<u16> = set(CRM_CLASSIDS)
            .union(&set(SHARED_CLASSIDS))
            .copied()
            .collect();
        assert_eq!(set(RBAC_CLASSIDS), union);
    }

    // ── RBAC: the plug's own refusal arms (the green bind is hubspo-iam's) ──

    #[test]
    fn role_names_are_distinct_and_plugged() {
        let names: BTreeSet<RoleId> = RBAC_ROLES.iter().copied().collect();
        assert_eq!(names.len(), 6);
        assert_eq!(RBAC_PLUG.roles, RBAC_ROLES);
    }

    #[test]
    fn an_unminted_classid_fails_the_bind() {
        let plug = RbacPlug {
            consumer: CONSUMER,
            classids: &[class_ids::CRM_CONTACT, 0x05FE],
            roles: &[],
        };
        assert_eq!(
            OgarRbac::new(NoGrants).bind(&plug),
            Err(RbacDrift::UnknownClassid(0x05FE))
        );
    }

    #[test]
    fn a_role_without_grants_fails_the_bind() {
        let plug = RbacPlug {
            consumer: CONSUMER,
            classids: CRM_CLASSIDS,
            roles: &[ROLE_SUPER_ADMIN],
        };
        assert_eq!(
            OgarRbac::new(NoGrants).bind(&plug),
            Err(RbacDrift::UnknownRole(ROLE_SUPER_ADMIN.into()))
        );
    }

    // ── hot-plug: refused until D-ACT-1, and each relevant arm can fire ──

    #[test]
    fn hot_plug_waits_for_crm_actions() {
        assert_eq!(
            resolve_hotplug(HOT_PLUG.consumer, HOT_PLUG.classids, HOT_PLUG.covered),
            Err(HotplugDrift::NoCapabilitiesFor(class_ids::CRM_CONTACT)),
            "OGAR now has CRM capabilities: write COVERED_CAPABILITIES (D-ACT-1)"
        );
    }

    #[test]
    fn an_unminted_hot_plug_id_is_refused_first() {
        assert_eq!(
            resolve_hotplug(CONSUMER, &[0x05FE], &[]),
            Err(HotplugDrift::UnknownClassid(0x05FE))
        );
    }

    #[test]
    fn hubspo_is_not_an_executor_of_another_apps_table() {
        // `patient` has capabilities, all expected from medcare-rs.
        assert_eq!(
            resolve_hotplug(CONSUMER, &[class_ids::PATIENT], &[]),
            Err(HotplugDrift::UnexpectedConsumer(CONSUMER.into()))
        );
    }

    // ── lookups ──

    #[test]
    fn hubspot_names_resolve_to_their_concepts() {
        assert_eq!(class_id("Contact"), Some(class_ids::CRM_CONTACT));
        assert_eq!(class_id("CustomObject"), Some(class_ids::CRM_CUSTOM_RECORD));
        assert_eq!(class_id("Quote"), Some(class_ids::COMMERCIAL_DOCUMENT));
        assert_eq!(class_id("Invoice"), Some(class_ids::COMMERCIAL_DOCUMENT));
        assert_eq!(class_id("Team"), Some(class_ids::PROJECT_ACTOR));
        assert_eq!(class_id("Cart"), None, "carts are parked, not aliased");
        assert_eq!(class_id("contact"), None, "names are case-sensitive");
    }

    #[test]
    fn render_class_id_puts_the_concept_high_and_the_prefix_low() {
        let rc = render_class_id("Deal").expect("Deal resolves");
        assert_eq!(classid_concept(rc), class_ids::CRM_DEAL);
        assert_eq!(classid_app_prefix(rc), HubSpoPort::APP_PREFIX);
        assert_eq!(render_class_id("Cart"), None);
    }

    #[test]
    fn ogar_and_the_contract_mirror_agree_on_the_prefix() {
        assert_eq!(AppPrefix::HubSpo.prefix(), HubSpoPort::APP_PREFIX);
        assert_eq!(
            AppPrefix::from_prefix(HubSpoPort::APP_PREFIX),
            Some(AppPrefix::HubSpo)
        );
    }
}

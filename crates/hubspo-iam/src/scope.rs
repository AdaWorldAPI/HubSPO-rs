//! D-IAM-2: HubSpot's ownership scopes (everything / team-owned /
//! owned-only, each restricted scope optionally with unassigned records) as
//! a row filter over a record type's owner lane.
//!
//! HubSpot decides visibility by who owns a record, not by where the record
//! sits. That is not a path prefix, so it is not a contract `ScopeSpec`
//! (address containment); a `ScopeSpec` keeps carrying the tenant. The
//! ownership scope resolves in two steps:
//!
//! 1. [`visible_owners`] reads the directory [`View`]: owned-only is the
//!    principal; team-owned adds everyone in the principal's teams and their
//!    sub-teams, walking only through groups that are teams
//!    ([`View::members_transitive_within`]); everything is unrestricted.
//! 2. [`RowScope::filter`] turns the owner set into one Quack filter over the
//!    owner lane. P-AUTH-1 measured the shape that fits the budget: evaluate
//!    it once and keep the result as a stored mask, ANDed into every read
//!    (≤ 4.3 % on a 10M-row fold), never as a per-row owner check on the
//!    read path (+43–49 %). Refresh it the way an active list is refreshed
//!    (`hubspo_query::list::refresh`), when the owner lane or the directory
//!    version changes.
//!
//! Fails closed: a principal that is not a user in the view sees nothing
//! under a restricted scope, unassigned records included; an owner with no
//! code on the owner lane is left out.

use lance_graph_dir_sim::View;
use lance_graph_quack::{Col, Filter, Mask};
use ogar_dir_core::Guid128;

/// Which records a role may see, by owner. HubSpot's three permission
/// levels for a CRM object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerScope {
    /// Every record.
    Everything,
    /// Records owned by the principal or by anyone in the principal's teams
    /// and their sub-teams.
    Team,
    /// Records owned by the principal.
    Owned,
}

/// Which rows a scope admits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowScope {
    /// No restriction.
    All,
    /// Rows whose owner code is one of `owners` (sorted, distinct), plus,
    /// when `unassigned`, rows with no owner. Never both empty and false.
    Rows {
        /// Owner codes admitted.
        owners: Vec<u32>,
        /// Whether rows with no owner are admitted.
        unassigned: bool,
    },
    /// No row.
    Nothing,
}

impl RowScope {
    /// Whether a single row with owner code `owner` (`None`: no owner) is
    /// admitted. For a one-record lookup; a scan uses [`RowScope::filter`].
    #[must_use]
    pub fn admits(&self, owner: Option<u32>) -> bool {
        match self {
            RowScope::All => true,
            RowScope::Nothing => false,
            RowScope::Rows { owners, unassigned } => match owner {
                Some(code) => owners.binary_search(&code).is_ok(),
                None => *unassigned,
            },
        }
    }

    /// The filter over the owner lane `owner`, whose validity plane is
    /// `owner_valid` (a row has an owner exactly when its bit is set).
    ///
    /// `None` for [`RowScope::All`] and [`RowScope::Nothing`], which the
    /// caller answers without a program (Quack refuses an empty `IN`, since
    /// it is an empty `OR`). An owner code only matches a row that has an
    /// owner: the lane value under a clear validity bit is not a code.
    #[must_use]
    pub fn filter(&self, owner: Col, owner_valid: Mask) -> Option<Filter> {
        let RowScope::Rows { owners, unassigned } = self else {
            return None;
        };
        let mut parts = Vec::new();
        if !owners.is_empty() {
            parts.push(Filter::and([
                Filter::is_not_null(owner_valid),
                Filter::in_u32(owner, owners.iter().copied()),
            ]));
        }
        if *unassigned {
            parts.push(Filter::is_null(owner_valid));
        }
        match parts.len() {
            1 => parts.pop(),
            _ => Some(Filter::or(parts)),
        }
    }
}

/// The owners whose records `principal` may see under `scope`.
///
/// - `teams`: the directory groups that are HubSpot teams. A team's
///   sub-teams are teams nested in it; a group nested in a team that is not
///   itself in `teams` is not walked, so its members are not reached. The
///   principal's teams are those it is a direct member of; membership
///   through nesting does not make it a member of the parent team, so it
///   never sees sibling teams.
/// - `unassigned`: HubSpot's "Unassigned" option, admitting records with no
///   owner under team-owned or owned-only. Ignored under everything.
/// - `owner_code`: the value the owner lane stores for a user, or `None`
///   when the user owns nothing (no code). Directory ordinals are local to a
///   snapshot and are not stored on records, so the caller supplies this.
pub fn visible_owners(
    view: &View<'_>,
    principal: &Guid128,
    scope: OwnerScope,
    teams: &[Guid128],
    unassigned: bool,
    owner_code: impl Fn(&Guid128) -> Option<u32>,
) -> RowScope {
    if scope == OwnerScope::Everything {
        return RowScope::All;
    }
    if view.user_ordinal(principal).is_none() {
        return RowScope::Nothing;
    }
    let mut owners = vec![*principal];
    if scope == OwnerScope::Team {
        for team in teams {
            if view.is_member(principal, team) {
                owners.extend(view.members_transitive_within(team, teams));
            }
        }
    }
    let mut codes: Vec<u32> = owners.iter().filter_map(owner_code).collect();
    codes.sort_unstable();
    codes.dedup();
    if codes.is_empty() && !unassigned {
        RowScope::Nothing
    } else {
        RowScope::Rows {
            owners: codes,
            unassigned,
        }
    }
}

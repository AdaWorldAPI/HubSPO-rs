//! D-IAM-2: ownership scopes resolve to an owner set through the directory
//! and filter the owner lane. The owner sets are checked against the team
//! layout by hand; the filter is checked against a row-by-row loop.

use hubspo_iam::scope::{OwnerScope, RowScope, visible_owners};
use lance_graph_dir_sim::*;
use lance_graph_mask_risc::{
    Foreign, LaneRef, Out, Planes, Scratch, execute_into, materialize_rows, words_for,
};
use lance_graph_quack::{Agg, Col, Mask, Query, lower};
use ogar_dir_core::{DirectoryScope, Guid128};

fn g(n: u8) -> Guid128 {
    Guid128([n; 16])
}
const DIR: DirectoryScope = DirectoryScope(Guid128([0x5C; 16]));

const U1: u8 = 0x11;
const U2: u8 = 0x12;
const U3: u8 = 0x13;
const U4: u8 = 0x14;
const U5: u8 = 0x15;
const U6: u8 = 0x16;
/// Team SALES with sub-teams EAST and WEST.
const SALES: u8 = 0xA1;
const EAST: u8 = 0xA2;
const WEST: u8 = 0xA3;
/// A directory group that is not a team.
const STAFF: u8 = 0xB1;
/// A security group nested in SALES that is not a team.
const ADMINS: u8 = 0xB2;
/// Not in the directory.
const GHOST: u8 = 0xF0;

// SALES ⊃ {u1, EAST, WEST, ADMINS}; EAST ⊃ {u2}; WEST ⊃ {u3};
// ADMINS ⊃ {u6}; STAFF ⊃ {u1, u5}; u4 is in no group.
fn observed() -> Observation {
    let user = |n: &str| ObservedNode::user(&format!("{n}@x.test"), &format!("{n}@x.test"));
    Observation {
        scope: DIR,
        nodes: vec![
            (g(U1), user("u1")),
            (g(U2), user("u2")),
            (g(U3), user("u3")),
            (g(U4), user("u4")),
            (g(U5), user("u5")),
            (g(U6), user("u6")),
            (g(SALES), ObservedNode::group()),
            (g(EAST), ObservedNode::group()),
            (g(WEST), ObservedNode::group()),
            (g(STAFF), ObservedNode::group()),
            (g(ADMINS), ObservedNode::group()),
        ],
        members: vec![
            (g(U1), g(SALES)),
            (g(EAST), g(SALES)),
            (g(WEST), g(SALES)),
            (g(ADMINS), g(SALES)),
            (g(U6), g(ADMINS)),
            (g(U2), g(EAST)),
            (g(U3), g(WEST)),
            (g(U1), g(STAFF)),
            (g(U5), g(STAFF)),
        ],
    }
}

const TEAMS: [u8; 3] = [SALES, EAST, WEST];

fn teams() -> Vec<Guid128> {
    TEAMS.iter().map(|&t| g(t)).collect()
}

/// The owner lane stores the user's last GUID byte; u5 owns nothing.
fn code(u: &Guid128) -> Option<u32> {
    (u.0[0] != U5).then_some(u32::from(u.0[0]))
}

fn owners(p: u8, scope: OwnerScope) -> RowScope {
    let mut st = VersionStore::new();
    let v = st.observe("crm", 1, observed()).unwrap();
    let view = st.view(v).unwrap();
    visible_owners(&view, &g(p), scope, &teams(), false, code)
}

fn codes(us: &[u8]) -> RowScope {
    RowScope::Rows {
        owners: us.iter().map(|&u| u32::from(u)).collect(),
        unassigned: false,
    }
}

#[test]
fn owned_only_is_the_principal() {
    assert_eq!(owners(U1, OwnerScope::Owned), codes(&[U1]));
    assert_eq!(owners(U4, OwnerScope::Owned), codes(&[U4]));
}

#[test]
fn everything_is_unrestricted() {
    assert_eq!(owners(U1, OwnerScope::Everything), RowScope::All);
    assert_eq!(owners(GHOST, OwnerScope::Everything), RowScope::All);
}

#[test]
fn team_reaches_sub_teams_but_never_siblings_or_parents() {
    // u1 is in SALES directly: SALES and both sub-teams.
    assert_eq!(owners(U1, OwnerScope::Team), codes(&[U1, U2, U3]));
    // u2 is in EAST directly and in SALES only through nesting: EAST alone,
    // not its sibling WEST and not SALES' own member u1.
    assert_eq!(owners(U2, OwnerScope::Team), codes(&[U2]));
    // u4 is in no team: its own records.
    assert_eq!(owners(U4, OwnerScope::Team), codes(&[U4]));
}

#[test]
fn a_group_that_is_not_a_team_grants_nothing() {
    // u1 and u5 share STAFF, which is not a team; u5 is not reached, and with
    // STAFF named a team u5 still has no owner code.
    assert_eq!(owners(U1, OwnerScope::Team), codes(&[U1, U2, U3]));
    let mut st = VersionStore::new();
    let v = st.observe("crm", 1, observed()).unwrap();
    let view = st.view(v).unwrap();
    let with_staff = [g(STAFF)];
    assert_eq!(
        visible_owners(&view, &g(U1), OwnerScope::Team, &with_staff, false, code),
        codes(&[U1])
    );
}

#[test]
fn fails_closed() {
    // Not a user in the directory: nothing, under every restricted scope.
    assert_eq!(owners(GHOST, OwnerScope::Owned), RowScope::Nothing);
    assert_eq!(owners(GHOST, OwnerScope::Team), RowScope::Nothing);
    // A user with no owner code owns nothing.
    assert_eq!(owners(U5, OwnerScope::Owned), RowScope::Nothing);
    // A team id that is not a group adds nothing.
    let mut st = VersionStore::new();
    let v = st.observe("crm", 1, observed()).unwrap();
    let view = st.view(v).unwrap();
    assert_eq!(
        visible_owners(&view, &g(U1), OwnerScope::Team, &[g(GHOST)], false, code),
        codes(&[U1])
    );
}

#[test]
fn a_non_team_group_nested_in_a_team_is_not_walked() {
    // ADMINS is nested in SALES but is not a team: u6 is not a team member.
    assert_eq!(owners(U1, OwnerScope::Team), codes(&[U1, U2, U3]));
    // Naming ADMINS a team brings u6 in.
    let mut st = VersionStore::new();
    let v = st.observe("crm", 1, observed()).unwrap();
    let view = st.view(v).unwrap();
    let mut with_admins = teams();
    with_admins.push(g(ADMINS));
    assert_eq!(
        visible_owners(&view, &g(U1), OwnerScope::Team, &with_admins, false, code),
        codes(&[U1, U2, U3, U6])
    );
}

#[test]
fn unassigned_is_carried_and_never_opens_a_closed_scope() {
    let mut st = VersionStore::new();
    let v = st.observe("crm", 1, observed()).unwrap();
    let view = st.view(v).unwrap();
    let t = teams();
    assert_eq!(
        visible_owners(&view, &g(U4), OwnerScope::Owned, &t, true, code),
        RowScope::Rows {
            owners: vec![u32::from(U4)],
            unassigned: true
        }
    );
    // A user with no code still sees unassigned records.
    assert_eq!(
        visible_owners(&view, &g(U5), OwnerScope::Owned, &t, true, code),
        RowScope::Rows {
            owners: vec![],
            unassigned: true
        }
    );
    // A principal outside the directory sees nothing, unassigned included.
    assert_eq!(
        visible_owners(&view, &g(GHOST), OwnerScope::Owned, &t, true, code),
        RowScope::Nothing
    );
    // Everything ignores the flag.
    assert_eq!(
        visible_owners(&view, &g(U1), OwnerScope::Everything, &t, true, code),
        RowScope::All
    );
}

#[test]
fn the_filter_admits_exactly_the_rows_of_visible_owners() {
    const N: usize = 1_000;
    let all: Vec<u32> = [U1, U2, U3, U4].iter().map(|&u| u32::from(u)).collect();
    // Every fifth row has no owner; its lane still holds a real code, so a
    // filter that matched codes without checking validity would admit it.
    let owner: Vec<u32> = (0..N).map(|r| all[(r * 7 + r / 3) % all.len()]).collect();
    let has_owner = |r: usize| !r.is_multiple_of(5);
    let mut valid = vec![0u64; words_for(N)];
    for r in (0..N).filter(|&r| has_owner(r)) {
        valid[r / 64] |= 1 << (r % 64);
    }
    const VALID: Mask = Mask(0);
    for p in [U1, U2, U4] {
        for scope in [OwnerScope::Team, OwnerScope::Owned] {
            for unassigned in [false, true] {
                let RowScope::Rows { owners: set, .. } = owners(p, scope) else {
                    panic!("{p:x} {scope:?}: expected rows");
                };
                let rs = RowScope::Rows {
                    owners: set.clone(),
                    unassigned,
                };
                let filter = rs.filter(Col(0), VALID).expect("rows filter");
                let masks: [&[u64]; 1] = [&valid];
                let lanes = [LaneRef::U32(&owner)];
                let planes = Planes {
                    n_rows: N,
                    masks: &masks,
                    lanes: &lanes,
                };
                let program = lower(&Query {
                    filter,
                    agg: Agg::Rows,
                })
                .expect("lower");
                let mut out = vec![0u64; words_for(N)];
                let mut scratch = Scratch::for_program(&program, N).expect("scratch");
                execute_into(
                    &program,
                    &planes,
                    &Foreign::NONE,
                    &mut scratch,
                    Out::Mask(&mut out),
                )
                .expect("execute");
                let got = materialize_rows(&out, N);
                let want: Vec<usize> = (0..N)
                    .filter(|&r| {
                        if has_owner(r) {
                            set.contains(&owner[r])
                        } else {
                            unassigned
                        }
                    })
                    .collect();
                assert_eq!(got, want, "{p:x} {scope:?} unassigned={unassigned}");
                // Not vacuous: the scope keeps some rows and drops others.
                assert!(!want.is_empty() && want.len() < N, "{p:x} {scope:?}");
            }
        }
    }
    // Unassigned only: exactly the rows with no owner.
    let rs = RowScope::Rows {
        owners: vec![],
        unassigned: true,
    };
    assert!(rs.filter(Col(0), VALID).is_some());
    assert!(RowScope::All.filter(Col(0), VALID).is_none());
    assert!(RowScope::Nothing.filter(Col(0), VALID).is_none());
}

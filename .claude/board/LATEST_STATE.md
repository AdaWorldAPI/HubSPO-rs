# LATEST_STATE — HubSPO-rs

> What exists right now. Update in the same commit as the change.

**As of 2026-10-10 (Wave 0 merged in #2; harvester + dossier in PR):**

## Workspace

- 18 crates, edition 2024, toolchain 1.98.1 (`rust-toolchain.toml`).
- 17 of them are empty on purpose: the crate layout from plan §4, with a
  module doc naming each crate's zone and deliverables.
- `hubspo-port` depends on `lance-graph-contract` (feature `guid-v3-tail`)
  and `ogar-vocab`, both on `branch = "main"`, and re-exports `HotPlug` and
  `PortSpec`. It holds no codebook and no prefix constant: `HubSpoPort`
  (`0x000B`) does not exist in OGAR yet (D-ONT-4).
- `hubspo-guards` enforces Gate G0 as tests (rules in its `lib.rs` header).
  9 unit tests (each rule can fire and can stay silent) + 1 workspace scan.
  All six disable runs measured red on 2026-10-10:

  | disable | failing test |
  |---|---|
  | `id: String` added to `hubspo-schema` | `the_workspace_breaks_no_guard` |
  | `serde` added to `hubspo-store` | `the_workspace_breaks_no_guard` |
  | `datafusion` added to root deps | `the_workspace_breaks_no_guard` |
  | `hubspo-ui` removed from `ZONES` | `the_workspace_breaks_no_guard` |
  | `has_string_id` forced to `false` | `string_ids_fire` |
  | stalwart-source check removed | `forbidden_dependency_fires_in_every_spelling` |

- CI: `.github/workflows/ci.yml` runs fmt, clippy (`--no-deps -D warnings`)
  and `cargo test --workspace` with debug info off.

## Harvest (D-ONT-1, public-catalog half)

- `scripts/fetch-hubspot-specs.sh` → `.cache/` (gitignored), then
  `cargo run -q -p hubspo-harvest -- report .cache/hubspot-specs`.
- Measured 2026-10-10: 123 APIs, 107 selected and read, 0 missing; 1,198
  operations, 472 on `/objects/` (0 unclassified), 726 outside; 35 object
  types; type ids `0-3` Deals, `0-162` Services, `0-410` Courses,
  `0-420` Listings.
- 13 unit tests. Five disable runs measured red (dated-release preference,
  preview exclusion, missing-spec recording, type-id recognition,
  dated version segment).
- Not covered yet: per-portal property, pipeline and label definitions
  (needs `ISS-ORACLE-PORTAL`).

## Mint dossier (D-ONT-3)

`.claude/plans/hubspo-concept-mint-dossier-v1.md`: 15 MINT, 10 ALIAS rows,
6 DATA, 6 PARK rows, 5 DEFER, six questions for the OGAR council. Every
harvested object-type name is accounted for (script-checked).

## Decisions

All open decisions OD-1..OD-11 in plan §10 were approved by the operator on
2026-10-10, as recommended. Cross-repo follow-ups they create are tracked in
`ISSUES.md`.

## Not built

Everything from Wave 1 on. No record can be stored, queried or rendered yet.

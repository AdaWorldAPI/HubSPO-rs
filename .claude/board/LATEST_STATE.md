# LATEST_STATE — HubSPO-rs

> What exists right now. Update in the same commit as the change.

**As of 2026-10-10 (Wave 0 PR):**

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

## Decisions

All open decisions OD-1..OD-11 in plan §10 were approved by the operator on
2026-10-10, as recommended. Cross-repo follow-ups they create are tracked in
`ISSUES.md`.

## Not built

Everything from Wave 1 on. No record can be stored, queried or rendered yet.

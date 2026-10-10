# CLAUDE.md — HubSPO-rs

Read first, every session. Methods live here; measurements live in
`.claude/board/` with their date and the command that produced them.

## What this is

HubSpot, reverse-engineered for its real shape (object graph, rule engine,
communication hub, reporting/AI) and rebuilt on our stack instead of
replicated: lance-graph + OGAR + ndarray as the substrate, spear as the
in-process mail and document hub, stalwart as the out-of-process mail
server, dir-sim for users and teams, graph-flow + ractor for automation,
a2ui-rs for screens. Philosophy unchanged since the seed: understand
first, reimagine second, touch grass.

## Session start — read in this order

1. This file.
2. `.claude/board/LATEST_STATE.md` and `.claude/board/STATUS_BOARD.md`
   (what exists, which D-ids are in flight).
3. `.claude/plans/hubspo-integration-v1.md` — decisions H-ADR-1..14,
   ground truth of every sibling repo, waves W0–W11 with gates.
4. `.claude/plans/hubspo-feature-matrix-v1.md` for the feature you touch.
5. Before any classid / port / concept work: OGAR
   `docs/OGAR-CONSUMER-BEST-PRACTICES.md`, `docs/HOTPLUG-MIGRATION-GUIDE.md`,
   lance-graph `.claude/knowledge/ogar-consumer-preflight.md`.

## Iron rules (approved 2026-10-10; enforced where marked)

1. **Identity is `NodeGuid` + OGAR classid.** Pull the concept through
   `HubSpoPort::class_id` (or `contract::ogar_codebook` in the server
   binary). Never a `String` id, never a local codebook or name→id map,
   never a hard-coded concept hex. New keys use `mint_for` (V3), never
   `NodeGuid::new`. *Enforced: hubspo-guards (`StringId`, `V1Mint`).*
2. **HubSPO-rs never mints concepts.** New CRM concepts are requested from
   OGAR (port `0x000B`, domain `0x10XX`) and pass the three-axis gate
   (METHOD ∧ STORAGE ∧ STRUCTURE) and the 5+3 codebook council. Converge
   onto an existing concept first.
3. **Custom objects and custom properties are tenant data**, never minted
   concepts (H-ADR-3).
4. **Behaviour is `ActionDef` + `KausalSpec`**, executed through
   `GatedOgarHandler` (def-match → RBAC → state guard → commit) by
   graph-flow under ractor. No DDL events, no SurrealQL, no behaviour in UI
   components. *Enforced: no surrealdb / SurrealQL adapter dependency.*
5. **Queries are Quack / mask-risc / `lance-graph-report`.** Lists and
   views are masks. No new DataFusion code — it is in a grace period
   (lance-graph, 2026-09-05). *Enforced: no `datafusion*` dependency.*
6. **stalwart runs as its own process.** It is AGPL; we reach it over JMAP,
   SMTP, CalDAV/CardDAV, signed webhooks and MTA hooks, and never link its
   crates. Mail is sent through its submission, not a second SMTP client.
   *Enforced: no stalwart source or crate, no `lettre`.*
7. **Zone discipline.** Zone-2 crates carry no wire formats: no serde, no
   `Serialize` derives. Wire formats live only in zone-3 crates
   (`hubspo-comms`, `-compat`, `-ui`, `-server`). *Enforced:
   `SerdeOutsideWireZone`, `SerializeDeriveOutsideWireZone`.*
8. **Every bulk change is simulated first** (crm-sim): version → validate
   → diff → reviewable plan → one Lance version. Revert is a version
   restore.
9. **RBAC happens before framing.** An unauthorised field is absent from
   the wire, not hidden in the client. Registries run sealed (fail closed).
10. **Spine-only crates stay out of the server binary.** `lance-graph-ogar`
    and the planner run in tests and the admin tool; the server reads
    classids through the contract (BBB barrier).
11. **One writer per mailbox.** Writes are stamped with their owner and go
    on behalf of it; no actor-per-message writes, no singleton sink.
12. **Two oracles.** Every surface is checked against a HubSpot sandbox
    export (values) and the HubSpot screen graph (structure, Klickwege).
13. **No model identifier** in any committed artifact.

## Build

```sh
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
cargo fmt --all --check
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo test --workspace
```

- Toolchain: `rust-toolchain.toml` pins 1.98.1; bare `cargo` resolves to
  it. Do not add a `+<version>` prefix.
- Never set `RUSTFLAGS` in the environment: it silently replaces every
  cargo-config rustflags entry (ndarray incident).
- `Cargo.lock` is not committed (fleet consumer convention for git-branch
  dependencies). ndarray's default branch is `master`; every other sibling
  is `main`.
- Worker agents never run cargo; the orchestrating session builds once.

## Test discipline

- Every guard has a can-fire and a can-stay-silent test on non-trivial
  input. Prefer `== N` over `>= N`.
- Disable runs prove a test is real: **commit first**, break the guard,
  see red, restore with `git checkout`. Restoring before committing deletes
  the work under test.
- Assert that a disable patch actually applied (`assert s.count(old) == 1`);
  a patch that silently no-ops looks exactly like a guard that is not
  load-bearing.
- Fixtures are generated and committed; nothing under `/tmp`. Contact data
  is PII: fixtures are synthetic, no real portal data is ever committed,
  logs never print property values.

## Adding a crate

1. Add it to the root `members` list.
2. Add it to `hubspo_guards::ZONES` with its zone. The workspace test fails
   until both lists agree.
3. Use `[lints] workspace = true` and the `*.workspace = true` package
   fields.

## Board hygiene

A change that adds a deliverable, finding or blocker updates the matching
`.claude/board/` file in the same commit. A fixed mistake gets one closeout
line, not a post-mortem.

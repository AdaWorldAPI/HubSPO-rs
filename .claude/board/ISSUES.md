# ISSUES — HubSPO-rs

> Open blockers and cross-repo follow-ups. Append new entries at the top;
> change only the `Status` line of an existing entry.

## ISS-LG-REPORT-OWNED-LANES — the report seam copies each lane once

- **Status:** OPEN (upstream ask; not built here — STOP rule)
- **Where:** lance-graph `crates/lance-graph-report/src/batch.rs:26` (`LaneData` holds `Arc<[i32]>` / `Arc<[u32]>` / `Arc<[u64]>`).
- **What:** publishing a Lance version into an `AbiBatch` copies every lane once, so graph-flow-lance and z8run-lance (both on this seam) are not zero-copy from Lance. mask-risc itself borrows slices and is. Ask: a borrowed or Arrow-buffer-backed `LaneData` variant. Plan H-ADR-15 condition 1.

## ISS-LG-REPORT-GENERATION-WIDTH — `AbiBatch` generation is u32, Lance versions are u64

- **Status:** OPEN (upstream ask)
- **Where:** lance-graph `crates/lance-graph-report/src/batch.rs:122`, `plan.rs:288`; rs-graph-llm `graph-flow-lance` `Envelope.generation: u32`.
- **What:** using the Lance version as the handle generation (the natural choice: offset = version = generation) needs a fail-closed mapping past `u32::MAX`, or a wider field upstream. Plan H-ADR-15 condition 2.

## ISS-OGAR-CRM-PORT — HubSpoPort and the CRM concept domain do not exist

- **Status:** OPEN (blocks D-ONT-5 and every classid in Wave 1)
- **Where:** OGAR `crates/ogar-vocab` (ports, `class_ids`, `ConceptDomain`,
  `capability_registry::domain_tables`), lance-graph `ogar_codebook` mirror.
- **What:** approved 2026-10-10 (OD-1, OD-2): port `0x000B`, domain
  `0x05XX` (OD-2 amended), a `crm_actions` domain table. Concepts mint only after the
  D-ONT-3 dossier passes the 5+3 codebook council.
- **Next:** the dossier (`hubspo-concept-mint-dossier-v1.md`) goes to the
  OGAR 5+3 council; then the OGAR PR (D-ONT-4).

## ISS-SPEAR-HUB-REHOME — spear's D-HUB-1/2/3/7 move to HubSPO-rs

- **Status:** RESOLVED 2026-10-10 by spear #13 (D-HUB-1/2/3/7 marked Abandoned there).
- **Where:** spear `.claude/plans/spear-outer-integration-v1.md` §3.1,
  `.claude/board/STATUS_BOARD.md`.
- **What:** approved 2026-10-10 (OD-5, H-ADR-12). spear keeps D-HUB-4/5/6/8
  (activity, send receipts, timeline query, round-trip test); the object
  model moves here. spear's plan and board need a supersession note.

## ISS-SPEAR-DATAFUSION-RULE — spear still mandates DataFusion `Expr`

- **Status:** RESOLVED 2026-10-10 by spear #13 (rule 1 now sends new query code to Quack).
- **Where:** spear `CLAUDE.md`, "Architecture Invariants" rule 1.
- **What:** approved 2026-10-10 (OD-7): new code uses Quack; DataFusion is
  in a grace period (lance-graph `CLAUDE.md`, 2026-09-05).

## ISS-LG-PREFLIGHT-STALE-PINS — consumer preflight whitelists old lance pins

- **Status:** RESOLVED 2026-10-10 by lance-graph #1436. Operator confirmed 2026-10-10: the whole fleet is on lance 12 / lancedb 0.39; this workspace floats `12.*` / `0.39.*`.
- **Where:** lance-graph `.claude/knowledge/ogar-consumer-preflight.md`.
- **What:** it whitelists `lance =9.0.0 / lancedb =0.33.0`; lance-graph
  `CLAUDE.md` floats `12.*` / `0.39.*`. Approved 2026-10-10 (OD-11): follow
  `CLAUDE.md`, fix the preflight upstream.

## ISS-STALWART-DIRSIM-WIRING — stalwart's dir-sim backend is unreachable

- **Status:** RESOLVED 2026-10-10 — operator chose option (c) (plan OD-12). D-MAIL-1 no longer depends on stalwart fork work; the stalwart dir-sim backend stays a library until option (b) is worth doing.
- **Where:** stalwart `crates/directory` (fork), `crates/main` features,
  config.
- **What:** the `dir-sim` recipient backend exists but nothing constructs
  it, `crates/main` does not forward the feature, and it cannot
  authenticate (needs OIDC/LDAP alongside). Fork-only work; stalwart's
  `AGENTS.md` forbids upstream contributions by agents.
- **Investigated 2026-10-10 — two blockers, not one wiring step:**
  1. `lance-graph-dir-sim` has no persistence: a `VersionStore` is built
     in memory from `ogar-ad` records (`observe::from_ad`); there is no
     save/load format a server could point a config entry at.
  2. stalwart's configuration types (`crates/registry/src/schema/*.rs`) are
     generated ("auto-generated. Do not edit directly"); the generator is
     not in the repository (only `resources/schema/schema.json.gz` + its
     hash). A `DirSim` directory config variant cannot be added without it.
- **Options:** (a) build `DirSimDirectory` in `Directories::build` from a
  setting read outside the registry — bypasses stalwart's config model;
  (b) add dir-sim persistence in lance-graph first and obtain the schema
  generator; (c) **recommended:** D-MAIL-1 does not need it. stalwart keeps
  its own directory (internal or LDAP/OIDC) for delivery and login, and the
  CRM's contact↔mailbox join key comes from spear's placement over the
  same dir-sim version (`spear/src/mail/directory.rs`), which already
  ships. The stalwart backend stays a library until (b) is worth doing.
- **Status note:** nothing changed in stalwart.

## ISS-ORACLE-PORTAL — no HubSpot sandbox portal yet

- **Status:** OPEN (blocks D-ONT-1 against a live portal, D-STO-6, every value-oracle gate)
- **What:** OD-10 — the operator names the sandbox portal and holds its
  token. Public OpenAPI specs can be harvested without it.

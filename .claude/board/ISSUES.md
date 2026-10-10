# ISSUES — HubSPO-rs

> Open blockers and cross-repo follow-ups. Append new entries at the top;
> change only the `Status` line of an existing entry.

## ISS-HIST-SCAN-COST — property history scans the whole log

- **Status:** OPEN (D-STO-5 budget; correctness shipped)
- **Where:** `hubspo-store::log::Log::history`.
- **What:** history reads every log record and runs the Quack program over them. Measured 2026-10-10 (`examples/probe_history.rs`, release, 100k records, 1,000 versions × 1,000 updates, 1.2M log records, one fragment per version): 336 ms, 320 ms of it the raw scan. P-HIST-1's budget is < 20 ms. The program is not the cost.
- **Not a lever:** compaction. DECISION 2026-10-10 (operator): the log is never compacted, because the append order gives version order for free. (A one-off measurement compacted to one fragment read in 57 ms, still over budget.)
- **Levers:**
  1. Read only `row`, `lane`, `op` for the program and fetch the other columns for the matches by position.
  2. An index on `row`. P-SPARSE-1 measured 16.9 ms with Lance's BTree on an uncompacted log of 1,000 fragments, inside the budget. Using it takes a Lance filter expression, which runs Lance's internal DataFusion; whether that sits within iron rule 5 (no `datafusion*` dependency, queries are Quack) is an operator decision. The plan's alternative (§5.2) is our own change index — positions per record, an index, not a second truth.

## ISS-LG-MASKRISC-PATH-NDARRAY — Quack could not be consumed by git

- **Status:** RESOLVED 2026-10-10 by lance-graph #1457 (`e724d83`): mask-risc names ndarray by git, and lance-graph's workspaces patch it back to the sibling checkout.
- **Where:** lance-graph `crates/lance-graph-mask-risc/Cargo.toml` (`ndarray = { path = "../../../ndarray" }`).
- **What:** the relative path leaves the lance-graph checkout, so any git consumer of `lance-graph-quack` (which depends on mask-risc) fails with "no matching package named `ndarray`". It also breaks lance-graph's own CARGO-COMPUTE-SUBSTRATE rule 5. Fix upstream: mask-risc names `ndarray = { git = "https://github.com/AdaWorldAPI/ndarray", branch = "master" }`, and lance-graph's root adds `[patch."https://github.com/AdaWorldAPI/ndarray"] ndarray = { path = "../ndarray" }`, so its own builds keep the sibling checkout and one ndarray identity. Verified locally: HubSPO-rs builds and tests against the patched checkout with one ndarray package.

## ISS-DIRSIM-HUBSPO-GAPS — dir-sim lacks what tickets, mail and documents need

- **Status:** OPEN (upstream asks; not built here — STOP rule)
- **Where:** lance-graph `crates/lance-graph-dir-sim`, OGAR `crates/ogar-dir-sim` (re-checked at lance-graph `a75bcc42`, OGAR `173de21`).
- **What:** found by the comms API read (`.claude/knowledge/comms-api-surface.md` §2):
  1. no persistence: `VersionStore` is in memory; `ad::project` (lance-graph #1452) writes LDIF for export and diffing only, not as a load format;
  2. no `ActorSource` over a `View` in either tree (only spear's `DirectoryActors`), so directory groups do not reach RBAC roles;
  3. no mailbox delegation (send-as, send-on-behalf, FullAccess);
  4. nested groups are held by identity and never expanded (spear's `mailbox_members` does the closure; HubSPO-rs reuses it rather than writing a second one);
  5. no queue or round-robin primitive (ticket routing, D-FLOW-5, stays here);
  6. no document ACL model.
- **Ask order:** 2 then 1 (they unblock D-IAM-2 and stalwart option (b)); 3 and 6 only when D-MAIL-5 / D-SVC-1 reach them.

## ISS-SPEAR-THREAD-AND-STALWART-DOC — spear thread id is null; a spear doc contradicts H-ADR-8

- **Status:** OPEN (spear follow-up)
- **Where:** spear `src/mail/ingest.rs:192` (`thread_id` written as `FixedSizeBinary::new_null`); spear `docs/STALWART_INTEGRATION.md` (spear as a storage backend linked into stalwart).
- **What:** D-MAIL-4 needs a thread key; the contract seam is `lance_graph_contract::mail::ThreadLinker` (no impl in tree), and stalwart's `message-ingest.ham` webhook carries no ThreadId, so the key must be computed on our side. The stalwart doc conflicts with H-ADR-8 (stalwart stays a separate AGPL process) and should be marked superseded in spear.

## ISS-LG-REPORT-OWNED-LANES — the report seam copies each lane once

- **Status:** OPEN (upstream ask; not built here — STOP rule)
- **Where:** lance-graph `crates/lance-graph-report/src/batch.rs:26` (`LaneData` holds `Arc<[i32]>` / `Arc<[u32]>` / `Arc<[u64]>`).
- **What:** publishing a Lance version into an `AbiBatch` copies every lane once, so graph-flow-lance and z8run-lance (both on this seam) are not zero-copy from Lance. mask-risc itself borrows slices and is. Ask: a borrowed or Arrow-buffer-backed `LaneData` variant. Plan H-ADR-15 condition 1.

## ISS-LG-REPORT-GENERATION-WIDTH — `AbiBatch` generation is u32, Lance versions are u64

- **Status:** OPEN (upstream ask)
- **Where:** lance-graph `crates/lance-graph-report/src/batch.rs:122`, `plan.rs:288`; rs-graph-llm `graph-flow-lance` `Envelope.generation: u32`.
- **What:** using the Lance version as the handle generation (the natural choice: offset = version = generation) needs a fail-closed mapping past `u32::MAX`, or a wider field upstream. Plan H-ADR-15 condition 2.

## ISS-OGAR-CRM-PORT — HubSpoPort and the CRM concept domain do not exist

- **Status:** RESOLVED 2026-10-10 by OGAR #330 (`c6a7797`: `ConceptDomain::Crm`, 15 concepts `0x0501`–`0x050F`, `HubSpoPort` `0x000B`) and lance-graph #1438 (mirror). `crm_actions` is deferred by design in OGAR (`docs/DISCOVERY-MAP.md` D-CRM-DOMAIN: no capability rows until HubSPO-rs implements them), so it is a HubSPO-rs follow-up, not an upstream ask.
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

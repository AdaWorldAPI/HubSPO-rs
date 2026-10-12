# ISSUES — HubSPO-rs

> Open blockers and cross-repo follow-ups. Append new entries at the top;
> change only the `Status` line of an existing entry.

## ISS-LG-DIRSIM-NOT-GIT-CONSUMABLE — dir-sim and cypher-quack cannot be consumed by git

- **Status:** RESOLVED 2026-10-11 for dir-sim by lance-graph `c471e8b5` (OGAR crates by git; the sibling paths stay only in the crate's own `[patch]`). Verified from outside: a scratch crate depending on `lance-graph-dir-sim` (feature `rbac`) and `lance-graph-dir-sim-lance` by git, importing `actors::{GroupRoles, ViewActors}`, passes `cargo check` on toolchain 1.99.0 at lance-graph `31652c5c`, OGAR `9803380`. `cypher-quack`'s paths are in-repo and resolve by git; `arm-discovery` still names ndarray by path, which HubSPO-rs does not need.
- **Where:** lance-graph `crates/lance-graph-dir-sim/Cargo.toml:27-30` (four non-optional `../../../OGAR/crates/*` paths); `lance-graph-cypher-quack` and `lance-graph-arm-discovery` through `ndarray-simd`; `probes/p-qdb-1` stale paths.
- **What:** #1457 fixed mask-risc only. Any HubSPO-rs crate that depends on dir-sim (hubspo-iam, hubspo-sim, option (d)) fails to resolve by git. Ask: OGAR by git, as `lance-graph-ogar` did, plus the root-only `[patch]`.

## ISS-CRM-KEY-CLASSVIEW — CRM keys would bake the fossil render prefix `0x000B`

- **Status:** OPEN (found 2026-10-11 by the audit; operator question + upstream ask)
- **Where:** plan key layout; lance-graph #1464 (`9f5d3dfe`) classid-layout ruling, plan `v3-mandatory-hotplug-reading-v1` rule 5.
- **What:** per-app render prefixes in the classview bits are a fossil; no new code may depend on `APP_PREFIX` being a classview value. HubSPO-rs has not minted keys yet. The CRM plug gives no row reading until `crm_actions` exists (D-ACT-1). Ask which classview CRM keys carry before D-STO-2 mints any.

## ISS-LOG-NON-BATCH-VERSION — a non-batch Lance version on the log dataset stops the log

- **Status:** RESOLVED 2026-10-12 by the fix below: the roadmap item is dropped from the `log.rs` module doc and `LATEST_STATE.md`, and the doc now states the rule that only `Log::commit` writes a log dataset. Earlier: OPEN (found 2026-10-10 by reading lance 13 and `log.rs`; not observed in a run)
- **Where:** `hubspo-store::log` — `commit`'s head check and `replay`'s version-count check; the `log.rs` module doc and `LATEST_STATE.md` list "an index on `row`" as not yet done.
- **What:** `replay` requires the Lance version to equal the batch count, and `commit` requires the head to be the table's version. A Lance version that is not a batch (CreateIndex, UpdateConfig, Restore, by any tool) therefore makes every later commit fail as `Diverged` and replay refuse the whole log. The roadmap item "an index on `row`" would add such a version. It also predates the resident fold (D-STO-5, 0.72 ms), which made that index unnecessary.
- **Fix:** drop the roadmap item, and keep the rule that nothing but `Log::commit` writes a log dataset. An index, if one is ever wanted, lives off the log dataset.

## ISS-LOG-RACING-WRITER — a racing second writer is detected only after its batch is durable

- **Status:** EXCLUDED 2026-10-10 — two writers on one mailbox break SoA ownership (iron rule 11), so the log does not handle that case. It was never observed; it came from reading Lance's commit code after a review of lance-graph #1466. The post-append check reports such a broken deployment, and replay refusing the log is the intended outcome. Nothing to build; the exploration map (lance-graph #1467) was closed unmerged.
- **Where:** `hubspo-store::log::Log::commit`.
- **What:** one writer per log comes from ownership (one writer per mailbox, iron rule 11), not from `commit`'s version checks. The check before the append refuses a stale writer. Two writers that both pass it both append, because Lance rebases an `Append` instead of failing it (lance-graph `spog-alpha-channel-v1.md` F6). The check after the append then reports `Diverged`, but the batch is already durable. Replay then refuses the log: two batches share a version and the Lance version no longer matches the batch count.
- **Fix:** in-band idempotency, `(cycle, batch_hash)` in the same commit and reconciled first (the F6 shape), or a writer lease. Not needed while a single owner writes each log, which is the deployment shape.

## ISS-HIST-SCAN-COST — property history scans the whole log

- **Status:** RESOLVED 2026-10-10 without either lever: history folds over the resident log records instead of scanning Lance (0.72 ms vs 336 ms); Lance is read once, by replay. No index, no projection, no Lance filter expression.
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

- **Status:** OPEN for gaps 3, 6 only (2026-10-11): gap 1 RESOLVED by lance-graph #1479/#1484 (`lance-graph-dir-sim-lance`: the history as a log of verified append-only batches, one Lance batch per append, replay without per-batch copies; persisted rules resolve by exact (name, version)); gap 2 RESOLVED by lance-graph `8a4743d0` (`actors::ViewActors`, roles through the security groups a principal carries) and #1486 (`GroupRoles::from_policy` reads group grants from OGAR `ogar_rbac::group_policy`, the authority's versioned policy — this answers OGAR W-2); Graph groups and members are observed by OGAR `ogar-az::group` (`groups_url`, `members_url`, `ingest_members_page`). Earlier audit, superseded for gaps 1-2: OPEN for gaps 1, 2, 3, 6; gap 4 RESOLVED, gap 5 EXCLUDED (audit 2026-10-11 at lance-graph `1116c120`, OGAR `b4f5a36`, spear `0c1b4a2`). Gap 1: still in memory; V16's LDIF/LDAP (lance-graph #1450/#1452/#1453) is export only; asked as lance-graph W-3 / OGAR W-3. Gap 2: inputs landed (lance-graph #1458 `members_transitive`, #1463 `security_identifiers`, OGAR #348 V19) but no `ActorSource` over a `View` anywhere; ask a thin `ViewActors` in lance-graph-dir-sim, "group G grants role R" as RBAC data (OGAR W-2, unanswered). Gap 3: OGAR V17 design only (#344, #347), no code. Gap 4: RESOLVED by lance-graph #1458 + #1461 + #1463 and spear #20 + #21 (`mailbox_members` now calls the upstream closure). Gap 5: EXCLUDED, never a dir-sim concern; D-FLOW-5 builds it here on Quack. Gap 6: OGAR V18 design only; regime 1 (attachment takes its ticket's or deal's access) is HubSPO-rs D-IAM-2/D-IAM-7, not an upstream ask; regime 2 (library ACL) waits on OGAR.
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

- **Status:** RESOLVED 2026-10-10 by spear #19 (`b7d82f4`): `ingest::thread_root_key` writes the thread key on every placed row (first `References` id, else `In-Reply-To`, else `Message-ID`); `RootThreads` implements `ThreadLinker` (feature `contract`); `STALWART_INTEGRATION.md` opens with a supersession note citing H-ADR-8; `directory.rs:47-51` now says the fork's dir-sim directory is neither constructed nor configurable. D-MAIL-4 consumes spear's key.
- **Where:** spear `src/mail/ingest.rs:192` (`thread_id` written as `FixedSizeBinary::new_null`); spear `docs/STALWART_INTEGRATION.md` (spear as a storage backend linked into stalwart).
- **What:** D-MAIL-4 needs a thread key; the contract seam is `lance_graph_contract::mail::ThreadLinker` (no impl in tree), and stalwart's `message-ingest.ham` webhook carries no ThreadId, so the key must be computed on our side. The stalwart doc conflicts with H-ADR-8 (stalwart stays a separate AGPL process) and should be marked superseded in spear.

## ISS-LG-REPORT-OWNED-LANES — the report seam copies each lane once

- **Status:** OPEN, dormant (audit 2026-10-11, lance-graph `1116c120`): `LaneData` unchanged. No HubSPO-rs crate depends on `lance-graph-report` yet (the D-QRY-3 board runs on Quack `GroupReduce`), so the second copy bites only when D-QRY-6 / D-ACT-4 publish a resident lane. Asked as handover W-5 (#1454) only, not on lance-graph's board. Minimal ask: an owner-erased `'static` lane (`Arc<dyn AsRef<[T]>>` or `ScalarBuffer<T>`); ask together with ISS-LG-REPORT-GENERATION-WIDTH only if that one stays upstream.
- **Where:** lance-graph `crates/lance-graph-report/src/batch.rs:26` (`LaneData` holds `Arc<[i32]>` / `Arc<[u32]>` / `Arc<[u64]>`).
- **What:** publishing a Lance version into an `AbiBatch` copies every lane once, so graph-flow-lance and z8run-lance (both on this seam) are not zero-copy from Lance. mask-risc itself borrows slices and is. Ask: a borrowed or Arrow-buffer-backed `LaneData` variant. Plan H-ADR-15 condition 1.

## ISS-LG-REPORT-GENERATION-WIDTH — `AbiBatch` generation is u32, Lance versions are u64

- **Status:** OPEN, reclassified 2026-10-11 from upstream ask to consumer decision: the generation is an equality staleness token (lance-graph `exec.rs:451` → `StaleSource`), so a fail-closed `u32::try_from(version)` (refuse past `u32::MAX`, never `as u32`) is lossless and stable across restarts. Lands with the first report/flow consumer. Also: `Envelope.generation` is z8run-lance's type, re-exported by graph-flow-lance.
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

- **Status:** REOPENED 2026-10-11, upstream text only; this workspace is correct. lance-graph #1436 set the preflight to `12.*`/`0.39.*`; #1437 then moved the fleet to lance 13 / lancedb 0.40 / Rust 1.99.0, so `ogar-consumer-preflight.md:45` is stale again. HubSPO-rs floats `13.*`/`0.40.*` (#6). Ask: the preflight points at lance-graph `CLAUDE.md` § Key Dependencies instead of restating numbers, so the next major bump cannot stale it.
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
- **Status note (2026-10-11):** stalwart's dir-sim backend changed in fork #2–#6 (`dirsim/mod.rs` only: groups, no mailbox for a non-receiving holder, `mail` as a label, the cloud-observed remote mailbox). Nothing that builds or configures it changed (`Directories::build`, the registry `Directory` enum, `crates/main` features). Option (d) of OD-12: point stalwart's stock LDAP directory at lance-graph-dir-sim's read-only `ldap::Server` (#1453); still needs a host that owns the listener.

## ISS-ORACLE-PORTAL — no HubSpot sandbox portal yet

- **Status:** OPEN (blocks D-ONT-1 against a live portal, D-STO-6, every value-oracle gate)
- **What:** OD-10 — the operator names the sandbox portal and holds its
  token. Public OpenAPI specs can be harvested without it.

## ISS-LG-METHOD-TABLE-PROBE-ONLY — the declared-method table is not consumable

- **Status:** OPEN (found 2026-10-12, lance-graph `42fa2a34`; upstream ask, not built here)
- **Where:** lance-graph `crates/lance-graph-glove-parity` (`method.rs`, `decl.rs`, D-CMH-1 #1501/#1504-#1509).
- **What:** `MethodTable` binds a declared method (`<concept>::<label> key= schema= reading= := COUNT(*) | SUM(f) | GROUP_SUM(v BY k IN n) [WHERE …]`) to canonical fields, checks seal and reading, and lowers it to one Quack program. That is the shape of D-ACT-4 (rollups, calculated properties), D-QRY-3 saved-view metrics and the D-QRY-6 report measures. The crate is a probe with its own `[workspace]`, `publish = false` and sibling path deps (sap, r2il → OGAR), so it cannot be consumed by git, and it links `lance-graph-ogar`, which iron rule 10 keeps out of the server binary.
- **Ask:** move the method table (spec, bind, prepare, Quack lowering, the text loader) into a git-consumable crate that does not need `lance-graph-ogar` at run time, with the concept resolved through the contract codebook. Until then HubSPO-rs does not hand-roll a parallel method format (STOP rule).

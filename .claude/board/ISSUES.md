# ISSUES — HubSPO-rs

> Open blockers and cross-repo follow-ups. Append new entries at the top;
> change only the `Status` line of an existing entry.

## ISS-ORDER-BY-RANK-SELECT — a page sorted by an `I32` key costs more than sorting everything

- **Status:** OPEN 2026-10-21. Measured by P-VIEW-1; the ask is posted to lance-graph on EXO-50 (operator's instruction: probe first, then post with numbers). HubSPO-rs builds nothing in place of it.
- **Where:** `hubspo-query::view::page`, the `I32` arm (`value_at` bisection).
- **What:** without a rank-select or `ORDER BY` terminal, the page's first and last value are found by bisection: 67 `Count` programs for every page that has valued records, at any offset. A page wholly inside a nullable key's null tail runs 2 programs and no bisection (Codex on #46: the first run's `amount` tail page was such a page). Measured (`cargo run --release -p hubspo-query --example probe_view_page`, 25-record pages, median of 5 per run, ranges over three runs on a shared 4-core container):

  | rows | one threshold scan | `I32` page (valued) | full edge sort | code-key page | null-tail page |
  |---|---|---|---|---|---|
  | 1M | 1.16–2.31 ms | 79–142 ms | 38–66 ms | 2.0–3.2 ms | 0.36–0.54 ms |
  | 4M | 5.34–5.85 ms | 352–421 ms | 196–242 ms | 8.9–13.9 ms | 1.5–2.3 ms |

  A valued `I32` page is 60–100 scans (same-run ratio) and 1.6–2.8× a full sort at the edge. A code-key page is ~1–2.7 scans but materialises the whole boundary code (~25 % of rows with 4 codes). Costs scale linearly with rows.
- **Ask:** one program that returns the value at a given rank of an `I32` lane over a filter (rank select), or a top-k / `ORDER BY` terminal returning the ordinals of ranks `[offset, offset + k)` with ties by ordinal. Either replaces the 66 bisection programs with one scan.


## ISS-PRESET-VERBS-PROVISIONAL — the preset roles' verbs are written from seat names, not from HubSpot

- **Status:** DECIDED 2026-10-21 (operator, EXO-50 message 006): AMENDED, not approved. The six role names stay. The verb table stays only as a tagged compatibility/simulation fixture, never as tenant runtime access. EffectiveAccess = seat/subscription ceiling ∧ tenant permission grants ∧ owner/team scope ∧ per-property outer+ordinal authorization ∧ action-specific gates. Super Admin is an explicit status, not a bypass of tenant isolation, schema/policy validation or seat gates. Admin is not blanket FULL/DELETE. View-only has no implicit read of sensitive fields. ACT (email send, enrolment, invoice, bulk), DELETE and EXPORT each need an explicit grant and are never inherited from WRITE/CREATE. A missing, unknown, stale or unread tenant grant fails closed and never falls back to the fixture. The presets are not reported as HubSpot-verified until a sandbox oracle exists (ISS-ORACLE-PORTAL). Implemented in the D-IAM-3 code PR (2026-10-21): the runtime bind grants nothing, the table is `PresetFixture`.
- **Where:** `hubspo-iam::roles` (the verb table in the module docs), pinned by `each_seat_edits_exactly_its_own_concepts` and `only_the_admins_delete`.
- **What:** every preset reads every plugged concept. Edit (write + create) and act: sales on contact, company, deal, lead, call, meeting, task, note, email, line item, quote/invoice (list edit only, sequence act only); service on contact, company, ticket, call, meeting, task, note, email (list edit); marketing on contact, company, campaign, email (list, note, task edit). Only the two admins delete; admin cannot delete users and teams (`PROJECT_ACTOR`). No HubSpot export backs this; the second oracle (rule 12) would.

## ISS-PERMISSION-SETS-NOT-ROLES — tenant permission sets cannot be `RoleId`s

- **Status:** DECIDED 2026-10-21 (operator, EXO-50 message 006): permission sets are versioned tenant DATA with stable numeric ids, separate from the static `RoleId` presets. A permission set may replace a user's configured permissions and is not assumed to only narrow a preset (HubSpot user-management docs, 2026-09). Seat, role template and grant are three different things. Implemented 2026-10-21: `hubspo_iam::tenant` (`TenantPolicy`, `PermissionSet`, `Seat`). Persistence is still open.
- **Where:** `lance_graph_contract::rbac::RoleId` is `&'static str`; `RbacPlug::roles` is a static list.
- **What:** a HubSpot permission set is created by the tenant at runtime, so it cannot be a plugged role without leaking strings. Candidate shape: a permission set is tenant data that narrows one preset (per-concept verbs ⊆ the preset's, field masks per D-IAM-3), applied after the preset's decision; it never widens. Needs the request path (D-IAM-3) to apply it.
## ISS-SPEAR-DOCUMENT-CONTRACTS — spear has a content store but no document record, reference or authorization

- **Status:** FILED UPSTREAM 2026-10-21 by the operator as spear#26 (contract and design request only, no implementation authorized): tenant-scoped DocumentRecord/DocumentRef, version witnesses, document ACL, folders, documented consumer/delivery API. HubSPO-rs builds none of it; D-DOC-1 stays Blocked until spear's contracts land.
- **Where:** spear `main` `c69cacb`: `src/content/mod.rs` (implemented), `src/sharepoint/{intent,drive_scope}.rs` (types only), `src/sharepoint/transport/mod.rs` (stub), `.claude/plans/onedrive-delivery-v1.md` (nothing built).
- **What HubSPO-rs needs from spear, and must not build itself:**
  1. A document record over the content store: tenant, content hash, name, media type, size, folder, provenance, version. The content store is a bare hash → bytes map written for message bodies.
  2. A document reference a consumer holds instead of a copy, resolved by spear.
  3. Document authorization: who may read, replace or link a document. spear's IAM plug (`mail::iam`) covers `email` only.
  4. Folders, if HubSpot folder operations are to be served; otherwise D-DOC-1 refuses them.
  5. Delivery: D-DELIVER-2..6 (`DeliveryIntent`, user drives, Entra resolution, delivery RBAC, transport, receipts) and `activity` / `send_receipts` (D-HUB-4/5, D-CONFIRM-1). Planned in spear, not built.
- **Until then:** D-DOC-1 is Blocked; no Files API operation is reported as served.

## ISS-SURFACE-COVERAGE-SEMANTICS — "mapped to a deliverable" read as "covered"

- **Status:** RESOLVED 2026-10-21 — `surface_map.tsv` has a `coverage` column: planned / implemented / deferred / out-of-scope / unclassified. `implemented` is accepted only for a D-id that is Shipped on STATUS_BOARD (`surface::check_board`); today no family is implemented, because no HubSpot-compatible endpoint is served yet. The committed-map test requires every one of the 140 families to carry a class that agrees with its deliverable (`UNPLANNED` ⇔ unclassified, `NONE` ⇔ out-of-scope, a D-id for planned or deferred), and pins the function split: planned 1,161, deferred 329, out of scope 16. A class says whether work is planned, never that it is implemented; implementation is the D-id's status on STATUS_BOARD.
- **Where:** `hubspo-harvest::surface::{Coverage, parse_map, by_coverage}`; `deliverables` command.
- **What:** the deliverable map assigned every family to a D-id or `UNPLANNED`, so a report of "families assigned" could be read as coverage. Deferred families (CMS, app platform, telephony) and out-of-scope ones were indistinguishable from planned ones.

## ISS-SURFACE-UNPLANNED — 134 documented HubSpot functions belong to no deliverable

- **Status:** DECIDED 2026-10-21 (operator). New D-ids: D-MKT-5 (marketing events), D-EXT-1 (app platform, deferred; a2ui, not HubSpot's React runtime), D-DOC-1 (file manager as a façade over spear), D-OPS-1 (tenant quotas), D-DATA-1 (data-studio ingestion). D-DOC-1 is compatibility in `hubspo-docs` over spear only (plan §2.5.1, `ISS-SPEAR-DOCUMENT-CONTRACTS`). Folded: custom behavioural events → D-FLOW-2, record tags → D-ONT-7 + D-STO-3, deal splits → D-COM-1, forecast types → D-QRY-6, AEO → D-MKT-4 (deferred). Out of scope: partner program, API metadata (HubSpot origin IPs). 118 functions assigned, 16 out of scope; `surface_map.tsv` carries the class per family (`ISS-SURFACE-COVERAGE-SEMANTICS`).
- **Where:** `data/hubspot/surface_map.tsv` rows with `UNPLANNED`.
- **What:** 19 spec families (134 functions) have no D-id: marketing events (37), app platform (22: CRM cards, extensions, feature flags, uninstall hooks), file manager (19), partner program (14), custom behavioural events (12), portal limits (9), record tags (6), answer-engine optimisation (5), data-studio ingestion (4), deal splits and forecasts (4), API metadata (2).
- **Decide per group:** a new D-id, folded into an existing one (the map row changes and the test re-pins), or out of scope (the row stays `UNPLANNED` with the reason). Candidates that fit existing work: custom behavioural events as workflow triggers (D-FLOW-2) and report inputs (D-QRY-6); file manager onto spear's document hub; record tags as a ClassView property.

## ISS-PACKED-RUN-RBAC-GRAIN — a packed run is granted as a whole, and contact's holds personal attributes

- **Status:** DECIDED 2026-10-21 (operator) — option (b); (c) is defense in depth only, never a substitute. Contract: `hubspo_iam::fields` (D-IAM-3). Storage grain may be a run; authorization grain is the property. Field 0 grants the container only; packed properties are checked one by one against an inner mask keyed by stable table ordinal, independent of the 256-position `WideFieldMask`. Requirements recorded on the D-IAM-3 row of STATUS_BOARD.
- **Where:** `hubspo-harvest::positions::PACKED_GROUPS`, `hubspo-schema::HubSpoClassView` (field 0).
- **What:** a role granted field 0 of contact sees all 27 packed properties; one not granted it sees none. HubSpot's `facebook_ads_properties` group holds `date_of_birth`, `gender`, `marital_status` and `military_status` next to `school` and `seniority`, so a role cannot get the job fields without the personal ones. HubSpot itself marks all of them `non_sensitive`; DSGVO may not.
- **Options for D-IAM-3:** (a) accept the grain and grant the run only to roles that may see all of it; (b) a second, run-internal mask keyed by table position (the run already stores positions); (c) keep the personal attributes out of the run by narrowing `PACKED_GROUPS` to names, with another group packed instead. Only (c) changes stored slots, and only before the first tenant data is written.

## ISS-FIELD-MASK-256 — HubSpot's contact object has more standard properties than a field mask can address

- **Status:** RESOLVED 2026-10-20 — operator chose option (c). Every object's field 0 is a packed run; `hubspo-harvest::positions::PACKED_GROUPS` sends contact's `facebook_ads_properties` and `socialmediainformation` groups there (27 properties), leaving 245 lanes. Slots are stored in the table and never move. RBAC grain follows: see `ISS-PACKED-RUN-RBAC-GRAIN`.
- **Where:** lance-graph-contract `WideFieldMask` (positions are `u8`, cap 256); `hubspo-schema::HubSpoClassView`.
- **What:** HubSpot's contact object-definition page defines 272 standard properties. Positions 256..271 are in the table, but no `WideFieldMask` can reach them, so no role can be granted them (RBAC fails closed: the field is absent from the wire). `HubSpoClassView` exposes the first 256 as fields and lists the rest via `unaddressable`, pinned at 16. Tenant custom properties (H-ADR-3) have the same ceiling. Company is next at 159.
- **Options:** (a) widen `WideFieldMask` positions upstream (a contract change; surface to the operator, do not file); (b) split a class's properties into two masks (core + extended); (c) move rarely-used properties to a packed tenant run (P-CUSTOM-1 measured both shapes).

## ISS-KB-PROPERTIES-PROVISIONAL — standard property positions come from public documentation

- **Status:** OPEN (opened 2026-10-20 by D-ONT-6; operator chose the provisional source)
- **Where:** `data/hubspot/standard_properties.tsv`, `hubspo-harvest::objdef`, `hubspo-schema::HubSpoClassView`.
- **What:** the table comes from HubSpot's public object-definition pages (internal name, type, field type, group, sensitivity, hidden flag, label for 18 object types, 1,337 properties) and from the activity API guides (internal names only, for calls, meetings, tasks, notes, emails and postal mail). So (1) activity rows have no type, group or label until a portal fills them in; (2) the counts are the documentation's, not a portal's; (3) quotes have no definition page and no table; (4) Invoice and Order converge with Quote on one concept, so none of the three is in the ClassView; `communication`, `postal_mail`, `appointment`, `cart`, `commerce_subscription`, `discount`, `feedback_submission`, `fee` and `goal` have tables but no HubSpot concept.
- **Finding:** every one of the 1,337 properties with a stated class is `non_sensitive`; data HubSpot treats as sensitive lives in tenant (custom) properties. Input for D-IAM-3.
- **Resolves with:** `ISS-ORACLE-PORTAL`. A portal's properties API run through `merge` fills the empty columns at the existing positions (append-only, by internal name).

## ISS-LG-GRAPHFLOW-LANCE-PATHS — graph-flow-lance cannot be consumed by git

- **Status:** OPEN (found 2026-10-14, rs-graph-llm `5b54301`; upstream ask)
- **Where:** rs-graph-llm `crates/graph-flow-lance` (#27), excluded from that workspace.
- **What:** the crate is the shape D-FLOW-1 needs: graph-flow tasks whose `Context` carries generation-checked plan and result handles, a fan-out as one mask-set coordinate folded in one pass, a checkpoint that fails closed on a newer source generation. It path-deps `lance-graph-report` and two `z8run` siblings, so a HubSPO-rs build cannot fetch it, and its handle generation is `z8run-lance`'s `Envelope.generation` (u32; see ISS-LG-REPORT-GENERATION-WIDTH).
- **Ask:** git coordinates for `graph-flow-lance` and `z8run-lance` (the same move mask-risc made in lance-graph #1457). Until then D-FLOW-1 designs against it without depending on it.

## ISS-LG-METHOD-TABLE-PROBE-ONLY — the declared-method table is not consumable

- **Status:** WITHDRAWN 2026-10-13: there is no table left to ask for. lance-graph #1524 (D-CMR-RESET, `9cfdf2a3`) moved the method layer to a backup branch: it was a second algebra beside the folds, lowered per call with fresh scratch, and `billable_hours` measured 430–480 µs against 283 µs for a scalar row loop. The rebuild (D-CMR, open) makes a class method a binding `classid × WideFieldMask → an existing compiled fold`, which is what HubSPO-rs's Quack programs already are. D-ACT-4 and the D-QRY-3/6 measures therefore build on compiled Quack programs now and gain a name when D-CMR lands. Earlier: OPEN (found 2026-10-12, lance-graph `42fa2a34`; upstream ask, not built here)
- **Where:** lance-graph `crates/lance-graph-glove-parity` (`method.rs`, `decl.rs`, D-CMH-1 #1501/#1504-#1509).
- **What:** `MethodTable` binds a declared method (`<concept>::<label> key= schema= reading= := COUNT(*) | SUM(f) | GROUP_SUM(v BY k IN n) [WHERE …]`) to canonical fields, checks seal and reading, and lowers it to one Quack program. That is the shape of D-ACT-4 (rollups, calculated properties), D-QRY-3 saved-view metrics and the D-QRY-6 report measures. The crate is a probe with its own `[workspace]`, `publish = false` and sibling path deps (sap, r2il → OGAR), so it cannot be consumed by git, and it links `lance-graph-ogar`, which iron rule 10 keeps out of the server binary.
- **Ask:** move the method table (spec, bind, prepare, Quack lowering, the text loader) into a git-consumable crate that does not need `lance-graph-ogar` at run time, with the concept resolved through the contract codebook. Until then HubSPO-rs does not hand-roll a parallel method format (STOP rule).

## ISS-LG-DIRSIM-NOT-GIT-CONSUMABLE — dir-sim and cypher-quack cannot be consumed by git

- **Status:** RESOLVED 2026-10-11 for dir-sim by lance-graph `c471e8b5` (OGAR crates by git; the sibling paths stay only in the crate's own `[patch]`). Verified from outside: a scratch crate depending on `lance-graph-dir-sim` (feature `rbac`) and `lance-graph-dir-sim-lance` by git, importing `actors::{GroupRoles, ViewActors}`, passes `cargo check` on toolchain 1.99.0 at lance-graph `31652c5c`, OGAR `9803380`. `cypher-quack`'s paths are in-repo and resolve by git; `arm-discovery` still names ndarray by path, which HubSPO-rs does not need.
- **Where:** lance-graph `crates/lance-graph-dir-sim/Cargo.toml:27-30` (four non-optional `../../../OGAR/crates/*` paths); `lance-graph-cypher-quack` and `lance-graph-arm-discovery` through `ndarray-simd`; `probes/p-qdb-1` stale paths.
- **What:** #1457 fixed mask-risc only. Any HubSPO-rs crate that depends on dir-sim (hubspo-iam, hubspo-sim, option (d)) fails to resolve by git. Ask: OGAR by git, as `lance-graph-ogar` did, plus the root-only `[patch]`.

## ISS-CRM-KEY-CLASSVIEW — CRM keys would bake the fossil render prefix `0x000B`

- **Status:** RESOLVED 2026-10-20 by operator decision: CRM keys carry classview `0x0000` (the shared canonical core), so a CRM classid is `concept << 16`. How a record is read comes from the authority by concept, never from the key; `HubSpoPort::APP_PREFIX` (`0x000B`) stays a render/port value and is never baked into a key. This answers lance-graph O1 (`v3-mandatory-hotplug-reading-v1`) for the CRM domain. Earlier: OPEN; evidence 2026-10-14: lance-graph #1550 (D-HOLO H5) measured a key-prefix `Range` at 292–573× a key sweep, valid only with a layout-order witness (a permuted key lane disagrees with the oracle). The classview choice therefore decides which CRM reads are prefix ranges. Earlier: OPEN (found 2026-10-11 by the audit; operator question + upstream ask)
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

- **Status:** 2026-10-20: OPEN for gaps 3 and 6. Gap 3 (mailbox delegation) is unchanged: OGAR V17 design only, no code. Gap 6 partly served — OGAR #359 `resolve_slot_authorized` / `resolve_doc_authorized` resolve a document under the reader's access decision (row scope applied, grids refused); the library ACL of regime 2 is still OGAR design. Earlier: OPEN for gaps 3, 6 only (2026-10-11): gap 1 RESOLVED by lance-graph #1479/#1484 (`lance-graph-dir-sim-lance`: the history as a log of verified append-only batches, one Lance batch per append, replay without per-batch copies; persisted rules resolve by exact (name, version)); gap 2 RESOLVED by lance-graph `8a4743d0` (`actors::ViewActors`, roles through the security groups a principal carries) and #1486 (`GroupRoles::from_policy` reads group grants from OGAR `ogar_rbac::group_policy`, the authority's versioned policy — this answers OGAR W-2); Graph groups and members are observed by OGAR `ogar-az::group` (`groups_url`, `members_url`, `ingest_members_page`). Earlier audit, superseded for gaps 1-2: OPEN for gaps 1, 2, 3, 6; gap 4 RESOLVED, gap 5 EXCLUDED (audit 2026-10-11 at lance-graph `1116c120`, OGAR `b4f5a36`, spear `0c1b4a2`). Gap 1: still in memory; V16's LDIF/LDAP (lance-graph #1450/#1452/#1453) is export only; asked as lance-graph W-3 / OGAR W-3. Gap 2: inputs landed (lance-graph #1458 `members_transitive`, #1463 `security_identifiers`, OGAR #348 V19) but no `ActorSource` over a `View` anywhere; ask a thin `ViewActors` in lance-graph-dir-sim, "group G grants role R" as RBAC data (OGAR W-2, unanswered). Gap 3: OGAR V17 design only (#344, #347), no code. Gap 4: RESOLVED by lance-graph #1458 + #1461 + #1463 and spear #20 + #21 (`mailbox_members` now calls the upstream closure). Gap 5: EXCLUDED, never a dir-sim concern; D-FLOW-5 builds it here on Quack. Gap 6: OGAR V18 design only; regime 1 (attachment takes its ticket's or deal's access) is HubSPO-rs D-IAM-2/D-IAM-7, not an upstream ask; regime 2 (library ACL) waits on OGAR.
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

- **Status:** STILL OPEN 2026-10-14 at lance-graph `c21bd165`: the preflight paragraph reads `lance 12.*` · `lancedb 0.39.*` while the fleet is on 13 / 0.40. Earlier: REOPENED 2026-10-11, upstream text only; this workspace is correct. lance-graph #1436 set the preflight to `12.*`/`0.39.*`; #1437 then moved the fleet to lance 13 / lancedb 0.40 / Rust 1.99.0, so `ogar-consumer-preflight.md:45` is stale again. HubSPO-rs floats `13.*`/`0.40.*` (#6). Ask: the preflight points at lance-graph `CLAUDE.md` § Key Dependencies instead of restating numbers, so the next major bump cannot stale it.
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

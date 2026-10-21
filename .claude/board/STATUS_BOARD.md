# STATUS_BOARD — HubSPO-rs

> Generated 2026-10-10 from the deliverable tables in
> `.claude/plans/hubspo-integration-v1.md`. When a D-id changes state, edit
> its `Status` and `Evidence` cells only; a new D-id is added to the plan
> first and then here.

> Status values: `Queued` → `In progress` → `In PR` → `Shipped` (or `Blocked`, with the blocker in `ISSUES.md`).


## Wave 0 — Make the repo honest (foundation)

| D-id | Deliverable | Size | Status | Evidence |
|---|---|---|---|---|
| D-W0-1 | Delete the non-compiling skeleton; new workspace per §4 with empty-but-compiling crates; `rust-toolchain.toml` 1.98.1; edition 2024 | S | Shipped | #2 |
| D-W0-2 | `Cargo.toml` workspace deps: lance-graph-* / OGAR / ndarray / spear / a2ui-rs / graph-flow on `branch="main"`; lance family floated | S | Shipped | #2 |
| D-W0-3 | CI workflow (test, clippy `--no-deps -D warnings`, fmt, debug-0, `config-v3.toml`) | S | Shipped | #2 |
| D-W0-4 | `.claude/` board: `LATEST_STATE.md`, `STATUS_BOARD.md` (all D-ids), `ISSUES.md`, `EPIPHANIES.md` (admission gate per lance-graph closeout rule) | S | Shipped | #2 |
| D-W0-5 | CLAUDE.md rewrite: iron rules from §3, read order, link to this plan | S | Shipped | #2 |
| D-W0-6 | README rewrite (drops "JanusGraph/ClickHouse/Gremlin replacement" framing in favour of §1) | S | Shipped | #2 |

## Wave 1 — Ontology and identity

| D-id | Deliverable | Size | Status | Evidence |
|---|---|---|---|---|
| D-ONT-1 | Harvest: `hubspo-harvest` pulls HubSpot's public OpenAPI specs + the properties/pipelines/association-label APIs of a sandbox portal → normalised schema JSON (zone 3, tool only) | M | Blocked | public-catalog half shipped (#3, `f019e4a`); sandbox half blocked on `ISS-ORACLE-PORTAL` |
| D-ONT-2 | OGAR: `ogar-from-schema` OpenAPI front end (queued upstream, `lib.rs:48`) — **requested from OGAR, not built in HubSPO-rs** | L | Queued |  |
| D-ONT-3 | Mint dossier: per candidate concept (H-ADR-2 table) the METHOD/STORAGE/STRUCTURE evidence + OGIT source + convergence argument; submit to the 5+3 codebook council | M | Shipped | ratified v3 `.claude/plans/hubspo-concept-mint-dossier-v3.md` (5+3 council, 2026-10-10) |
| D-ONT-4 | OGAR PR: `ConceptDomain::Crm` (`0x05XX`), 15 concepts in `class_ids` + `CODEBOOK` + `ALL`, `HubSpoPort` (`0x000B`) + aliases, layout doc row; lance-graph `ogar_codebook` mirror rows + `AppPrefix` 0x0008–0x000B. `crm_actions` deferred to D-ACT-1 (dossier v3 §7) | M | Shipped | OGAR#330, lance-graph#1438 |
| D-ONT-5 | `hubspo-port`: `RbacPlug` binding; `HotPlug` over Crm ids asserted to fail `NoCapabilitiesFor` until D-ACT-1 (dossier v3 §7); deliberately-wrong-plug test proving each drift arm can fire | S | Shipped | `hubspo-port`, 13 tests (`04edf87` on main) |
| D-ONT-6 | `hubspo-schema`: ClassViews for contact, company, deal, ticket, lead, product, line item, quote, activity kinds; standard property position tables via the generator into `hubspo-generated` | L | Shipped | 2026-10-20 provisional (operator: public docs): `hubspo-harvest` objdef/guide/merge/emit from HubSpot's object-definition pages and activity guides, `data/hubspot/standard_properties.tsv` (24 object types, 1,413 properties by internal name), generated tables, `hubspo-schema::HubSpoClassView` keyed by concept for 13 object types (counts pinned); contact passes the 256-position mask ceiling (`ISS-FIELD-MASK-256`); see `ISS-KB-PROPERTIES-PROVISIONAL`. Gate G1 stays open until the portal. Unblocked 2026-10-20: `ISS-CRM-KEY-CLASSVIEW` decided, CRM keys carry classview `0x0000` (`concept << 16`); ClassViews keyed by concept. Earlier: `ISS-CRM-KEY-CLASSVIEW`: no ClassView or key on the `0x000B` prefix until answered (plan H-ADR-1 note) |
| D-ONT-7 | Custom-object/-property design (H-ADR-3) + probe P-CUSTOM-1 (lane-per-property vs packed run; fold latency at 50 / 500 / 2,000 custom properties) | M | In progress | P-CUSTOM-1 2026-10-10: lanes 0.73–1.61× std, packed 15–35×; the 33–96 s is a fresh K-column `Dataset::write`, not updates. Both measured paths are off the shipped path since #23/#25: re-probe as P-CUSTOM-2 under the resident fold; 2026-10-21 ruling: also owns record-tag definitions (`crm-object-tags`, 6 functions); record-tag links are D-STO-3. |

## Wave 2 — Store, versions, history

| D-id | Deliverable | Size | Status | Evidence |
|---|---|---|---|---|
| D-STO-1 | `hubspo-store` write lane: batch image → one Lance version per publish; owner-stamped (`mailbox_owner`) | L | Shipped | resident half #22; persistence half #23: `hubspo-store::log` (one Lance version per batch, replay, feature `lance`) |
| D-STO-2 | KV value store for variable-length properties; zero-copy lens reads | M | Queued |  |
| D-STO-3 | Edge lanes + association labels (H-ADR-4); cardinality invariants as Quack `GroupReduce` | L | Queued | 2026-10-21 ruling: record-tag relations (tag definitions are D-ONT-7). |
| D-STO-4 | Tenant layout + P-TEN-1 probe | M | Queued | P-TEN-1 passed 2026-10-10 (7.8 ms p99 at 10k tenants, local disk) |
| D-STO-5 | Property history via version log + P-HIST-1 probe | M | Shipped | #25: `Log::history` folds one Quack program over the resident log records; Lance is read once, by replay. **0.72 ms** for 1,001 entries over 1.2M records (release, `examples/probe_history.rs`, 101 reads, 2026-10-10) — passes P-HIST-1's < 20 ms; replay 532 ms once at startup. #24 shipped the read (Lance scan, 336 ms); #25 made it resident. Pattern documented for every consumer: lance-graph `.claude/knowledge/split-tunnel-sparse-append-resident-fold.md` (#1466) |
| D-STO-6 | Importer (value oracle, part 1): sandbox portal → crm-sim plan → commit; counts compared per object type | L | Queued |  |
| D-DATA-1 | Schema-agnostic, versioned ingestion of structured data into Lance (incremental), separate from CRM object imports; HubSpot data-studio ingestion API on top | M | Queued | Operator ruling 2026-10-21 (`ISS-SURFACE-UNPLANNED`). Surface: `data-studio-datasource-ingestion`, 4 functions |

## Wave 3 — Query, lists, views, reports

| D-id | Deliverable | Size | Status | Evidence |
|---|---|---|---|---|
| D-QRY-1 | Filter compiler: HubSpot filter groups (AND within group, OR across groups; operators EQ/NEQ/LT/…/HAS_PROPERTY/CONTAINS_TOKEN/IN/BETWEEN) → Quack program | L | Shipped | 2026-10-14: lance-graph #1546 `lance-graph-relalg` (D-RA-1) normalizes a filter before it runs and answers a contradictory one as `Empty` without a program; D-CMR-1 moves that set algebra into the contract. Adopt it there, not from the prototype crate; `hubspo-store::catalog` + `hubspo-query::filter`; 600 random filter sets equal a plain-loop oracle on mask-risc; #19 |
| D-QRY-2 | Lists: active (incremental on version delta) + static (stored mask); membership history | L | Shipped | `hubspo-query::list` (refresh, static add/remove/members, added/removed); tests vs full recomputation. P-LIST-1 2.6 ms at 10M; P-SPARSE-1 1.93 ms, 257 KiB flat at 10M. Storage of list masks lands with D-STO-1 |
| D-QRY-3 | Saved views + board view (report pivot over pipeline × stage) | M | In progress | board half shipped (#21): `hubspo-query::board` (count + amount per stage as two `GroupReduce` programs); saved-view half open: `SavedView` value type + sort at the edge over existing Quack programs (`Agg::Rows` storage order, `Count` threshold bisection for I32, `GroupReduce Count` for coded lanes); ORDER BY is asked upstream only if a page-cost probe fails |
| D-QRY-4 | Quack lowering of callcenter's `ParsedQuery` (the upstream DataFusion `parsed_query_to_plan` is an unimplemented stub) | M | Queued |  |
| D-QRY-5 | Association-traversing filters ("contacts whose company has an open deal"): Quack `Semijoin` over edge lanes for one hop; multi-hop needs cypher-quack shapes that do not exist yet → upstream request first | L | Queued | 2026-10-20: lance-graph #1632/#1633 (D-SRAM-2 lab spike, `crepe-plan-probe`) measured a semijoin reduction before preparation and an addressed semijoin over attested lanes with masked Lance 13 reads; lab only, not authorization; 2026-10-14: the hop is D-CMR-5, preimage `f⁻¹(S)` under a proven mapping (lance-graph `d-cmr-concept-chain-normalization-v1`), not chained key reads (withdrawn in #1524 / ndarray #359). One hop is ready on the existing Quack `Semijoin`; multi-hop waits on D-CMR-5 rather than on new cypher-quack shapes |
| D-QRY-6 | Reports: single-object, cross-object, funnel, time-series, attribution skeleton on `lance-graph-report` | XL | Queued | 2026-10-21 ruling: also owns forecast types (`crm-forecast-types`, 2 functions) and reads D-FLOW-2's custom behavioural events. |
| D-QRY-7 | Full-text search over records (Tantivy, reference-indexed, reconciled from the store — tesseract-paperless `search` pattern) | M | Queued |  |

## Wave 4 — Behaviour: actions, pipelines, crm-sim

| D-id | Deliverable | Size | Status | Evidence |
|---|---|---|---|---|
| D-ACT-1 | `crm_actions` table in OGAR (§4.2 fence) + ActionDef tables: read / create / update / delete / archive / restore / merge / associate / dissociate / assign owner / move stage / GDPR delete / enroll; full `HotPlug` round trip | L | Queued |  |
| D-ACT-2 | Executor through `GatedOgarHandler`; `ActionInvocation` audit; idempotency keys | M | Queued |  |
| D-ACT-3 | Pipeline guards + required-properties-per-stage (`Constrains`) | M | Queued |  |
| D-ACT-4 | Calculated, rollup and formula properties (`Depends`/`Onchange` + report folds; formulas as loco programs) | L | Queued |  |
| D-SIM-1 | `hubspo-sim`: snapshot + overlay versions, rules, invariant programs, diff, `ExecutionPlan` review payload | L | Queued |  |
| D-SIM-2 | Bulk edit, import, delete, merge all routed through D-SIM-1; revert = version restore | M | Queued |  |
| D-SIM-3 | Dedup merge semantics (HubSpot rules: oldest record wins id, property conflict policy, association union, timeline union) | M | Queued |  |

## Wave 5 — Identity, teams, permissions

| D-id | Deliverable | Size | Status | Evidence |
|---|---|---|---|---|
| D-IAM-1 | Role catalogue: Super Admin, Admin, Sales/Service/Marketing user, view-only, custom permission sets; grant source with `field_mask_of` | M | In progress | 2026-10-21 (authorized by the operator): the six presets ship. Role names are in `hubspo_port::RBAC_ROLES`; the grants are `hubspo_iam::roles::Catalogue` (one grant per plugged concept, provisional verb table); `hubspo_iam::roles::bind` binds the plug green. A request's roles come from the directory: `hubspo_iam::roles::request_actors` reads security-group grants (OGAR group-role policy via dir-sim `GroupRoles`/`ViewActors`) once per request and refuses on an unread group. 12 unit + 3 directory tests. Open: tenant permission sets (`ISS-PERMISSION-SETS-NOT-ROLES`); the preset verbs await operator confirmation (`ISS-PRESET-VERBS-PROVISIONAL`). 2026-10-20 inputs from lance-graph: (1) #1635/#1638 (D-RMAP-DIR-1, lab only) measured a dir-sim `View` walk as a full membership scan per call, 0.4–0.7 ms at 35k–263k rows; resolve an actor's roles once per request, never per record. The compiled closure (32–49 ns) is not admitted (ADMISSION open, D-RMAP-DIR-2 not authorized), so do not depend on `dir-rmap-probe`. (2) #1636 ACTION-GATE: an `ActionDef` with `required_role = None` skips RBAC in both `commit` and `commit_via`; the upstream recommendation is `commit_via` canonical and `None` only for actions declared public. Give every CRM action a role. 2026-10-21 ruling (ISS-PRESET-VERBS-PROVISIONAL, amended): `Catalogue` is a fixture, not tenant runtime access; runtime grants come from versioned tenant permission sets (ISS-PERMISSION-SETS-NOT-ROLES) and fail closed. 2026-10-21 D-IAM-3 code PR: `bind()` grants nothing (`RoleNames`); the table is `PresetFixture`; runtime grants come from `hubspo_iam::tenant`. Open: tenant permission-set persistence and default sets for simulation. |
| D-IAM-2 | Ownership scopes (all / team / owned) as `ScopeSpec`; team hierarchy via `Dn128` subtree | M | Shipped | #30 + #31 (#31 fixes the nested-group P1 via lance-graph #1488 `members_transitive_within`; 8 tests). `hubspo-iam::scope` 2026-10-11: owner set from the dir-sim `View` (team = directly held team group + nested sub-teams), one Quack `IN` over the owner lane, stored as a mask; 6 tests, 2 disable runs red. P-AUTH-1 2026-10-10: scope as stored mask in the fold kernel ≤ 4.3 %; per-row owner check +43–49 %. Unblocked 2026-10-11: roles from directory groups via lance-graph-dir-sim `actors::ViewActors` + `GroupRoles::from_policy` (consumable by git) |
| D-IAM-3 | Field-level permissions (sensitive properties) as per-role `WideFieldMask`; absent from the wire | M | In progress | 2026-10-21 contract shipped (not yet wired to any output path): `hubspo_iam::fields` — outer `WideFieldMask` (lane, or position 0 for the run) ∧ inner `PropertyGrants` by stable ordinal; `Projection::compile` once per request, `project` drops hidden/unknown values, `require` refuses filters/sorts/aggregates on them, a projection compiled at another schema version refuses everything. 10 tests incl. `school` vs `date_of_birth`/`gender`/`marital_status`/`military_status`. Open: per-role default grants, wiring into REST/UI/export/events/workflow outputs. 2026-10-21 operator ruling (`ISS-PACKED-RUN-RBAC-GRAIN`, option b): storage grain may be a packed run, authorization grain is the property. Field 0 grants the container, never its contents; packed properties are authorized one by one by stable table ordinal, in a mask that is not the 256-position `WideFieldMask`. Requirements: compile authorized fields once per request; check each packed property before serialization, render, export and workflow use; refuse filters, sorts and aggregations on hidden properties; never hand a raw packed run to an untrusted caller (debug, cache, direct API included); fail closed on missing, unknown or stale inner policy, invalidated on grant or schema change; negative test `school` vs `date_of_birth`. Option (c), high-risk properties in their own lanes, allowed before first tenant data. 2026-10-20: also needs D-IAM-1 (roles) and D-ONT-6 (field positions per concept) in HubSPO-rs, which the upstream note below missed. 2026-10-20 unblocked upstream: lance-graph #1556 `RbacBinding::with_class_field_defaults` + `field_mask_for` (role mask, else class default, else None), OGAR #358 `GrantSource::default_field_mask_of` feeding it in `OgarRbac::bind`, lance-graph #1558 `walk_rails_authorized` (hidden field never emitted, hidden rail not followed; use `visible_fields()`). Read lance-graph `.claude/knowledge/access-control-model.md` first: policy data keyed by classid, no reasoning 2026-10-21 ruling: proceed with a first output path (contact lookup): actors resolved once per request; a projection stamped with schema, grant, role and seat versions; owner/team scope and per-ordinal grants before serialization, export, cache or diagnostics; ACT/DELETE/EXPORT gated individually. Required negative tests: missing policy, revocation, other concept or snapshot, view-only on sensitive fields, unauthorized ACT/DELETE, hidden-predicate inference. 2026-10-21 code PR: `hubspo_iam::access::RequestAccess` (tenant sets × seat × scope × ordinals × named action gates, stamped with schema, policy, seat and role versions) and the first output path `hubspo_compat::contacts::{lookup, check_search}`; all six negative tests plus seat/role invalidation; ten disable runs red. Open: store read for D-API-1, the other output paths. |
| D-IAM-4 | Directory ingest (ogar-ad / ogar-az / native), seat assignment as directory simulation plans | L | Queued |  |
| D-IAM-5 | Partitioning (business units / brands) as tenant sub-scopes | M | Queued |  |
| D-IAM-6 | Accumulation guard: exports / cohort reports require an extra grant | S | Queued |  |
| D-IAM-7 | RLS as a Quack predicate injected before evaluation (tenant + scope); the callcenter `RlsRewriter` is DataFusion-only and frozen. Upstream into lance-graph if a second consumer needs it | M | Queued |  |

## Wave 6 — Communications (spear + stalwart)

| D-id | Deliverable | Size | Status | Evidence |
|---|---|---|---|---|
| D-MAIL-1 | stalwart deployment profile (separate service, AGPL stays there); stalwart keeps its own directory; contacts join to mailboxes through spear's placement (OD-12) | M | Queued | re-scoped 2026-10-10 (OD-12) |
| D-MAIL-2 | Webhook receiver (HMAC verify) → spear `activity` rows for delivery / ingest events | M | Queued | wire surface mapped: `.claude/knowledge/comms-api-surface.md` §3 |
| D-MAIL-3 | One-to-one email send from a record via JMAP `EmailSubmission`; `send_receipts` row | M | Queued |  |
| D-MAIL-4 | Inbound logging: MTA hook BCC capture + mailbox sync → associate to contacts by address → company / deal by association rules. Implements the contract's `MailParser` / `ThreadLinker` seams (`lance-graph-contract/src/mail.rs`) | L | Queued | scope amended 2026-10-11 (plan note under the D-MAIL-4 row): implement `MailParser` only; `ThreadLinker` is spear `RootThreads` (#19) — consume its thread key, build none here |
| D-MAIL-5 | Shared inbox / conversations: new spear roles (team inbox member, inbox admin) + ownership routing | L | Queued |  |
| D-MAIL-6 | Meetings: CalDAV / JMAP Calendars, iTIP, booking pages (availability fold over calendars), meeting engagement records | XL | Queued |  |
| D-MAIL-7 | Contact sync (CardDAV / JMAP Contacts) — two-way, conflict policy via crm-sim | L | Queued |  |
| D-MAIL-8 | Templates, snippets, personalisation tokens (askama-compiled, validated against the ClassView) | M | Queued |  |
| D-MAIL-9 | Open / click tracking endpoints + bot/MPP heuristic flag | M | Queued |  |
| D-MAIL-10 | Calls: telephony is out of scope for v1 beyond call *logging* engagements; spear D-TELEPHONY reuse later | S | Queued | 2026-10-21: the telephony API families (`crm-calling-extensions`, `crm-transcriptions`; 17 functions) are deferred in the surface map; call logging stays planned under D-API-1 CRUD. |
| D-DOC-1 | HubSpot Files API compatibility in `hubspo-docs` over spear: ingest, delivery and lookup flows (plan §2.5.1). No second content store, version history, Graph client, authorization or receipt model | M | Blocked | Operator ruling 2026-10-21 (`ISS-SURFACE-UNPLANNED`). Surface: `files-files`, 19 functions. Waits on spear: document records, references and authorization (`ISS-SPEAR-DOCUMENT-CONTRACTS`, not planned there yet) and D-DELIVER-2..6 (planned, not built). spear verified at `c69cacb`: content store implemented; `UploadIntent`/`DriveScope` types only; Graph transport a stub. 2026-10-21: filed upstream as spear#26 (contract only). |

## Wave 7 — Automation: workflows, sequences, SLAs

| D-id | Deliverable | Size | Status | Evidence |
|---|---|---|---|---|
| D-FLOW-1 | Workflow model: trigger (criteria / event / schedule / webhook), enrolment, re-enrolment, actions, branches (if/then, value equals, A/B), delays (fixed, until date, until event) as graph-flow graphs | XL | Queued | 2026-10-14: rs-graph-llm #27 `graph-flow-lance` — graph-flow tasks over lance-graph-report folds, the Context holds generation-checked handles, never rows (ISS-LG-GRAPHFLOW-LANCE-PATHS) |
| D-FLOW-2 | Enrolment triggers: mask delta between published versions ∩ criteria from a stored offset; due-run mask for delays and schedules; watcher as wake-up only (H-ADR-7, amended 2026-10-10) | L | Queued | P-FLOW-1 2026-10-10: commit → enrolled 13.8 ms p50 / 27.1 ms p99 at 1k workflows; 2026-10-21 ruling: also owns custom behavioural events (definitions, ingestion, triggers; `events-*`, 12 functions); D-QRY-6 reads them. |
| D-FLOW-3 | Action library: set property, create record/task, send email, add to list, webhook, rotate owner, delay, branch, custom code (loco program) | L | Queued |  |
| D-FLOW-4 | Sequences: steps (email / task / call), stalwart future-release for scheduled sends, unenrol on reply/meeting booked (MTA hook / webhook) | L | Queued |  |
| D-FLOW-5 | SLAs + ticket routing (round robin, skills) as timers + rules; reuse spear D-ROUTE patterns | M | Queued | no upstream primitive needed (audit 2026-10-11): Quack eligibility mask, `MinI32` last-assigned, `GroupReduce` load; assign-owner ActionDef via `GatedOgarHandler` |
| D-FLOW-6 | Workflow test mode = crm-sim dry run over a sample population | M | Queued |  |
| D-FLOW-7 | Ops-hub-style data sync connectors as flows (out of v1 scope beyond webhooks) | — | Queued |  |

## Wave 8 — UI

| D-id | Deliverable | Size | Status | Evidence |
|---|---|---|---|---|
| D-UI-1 | Six-region frame; record page (about card / timeline / association cards) as ClassView projections | L | Queued |  |
| D-UI-2 | Index pages (table + board) with saved views; inline edit = ActionInvoke | L | Queued |  |
| D-UI-3 | Settings surfaces: properties, pipelines, association labels, users/teams, permission sets | L | Queued |  |
| D-UI-4 | List builder and workflow canvas (a2ui skins over the same projections) | XL | Queued |  |
| D-UI-5 | Structure oracle: Klickwege graph of HubSpot UI vs ours; parity test | M | Queued |  |
| D-UI-6 | Realtime: `version_watcher` → WebSocket → `NodeDelta`. The callcenter Phoenix server is commented out upstream, so the WS endpoint lives in `hubspo-server` (axum) until lance-graph ships one | M | Queued |  |

## Wave 9 — Intelligence

| D-id | Deliverable | Size | Status | Evidence |
|---|---|---|---|---|
| D-INT-1 | Duplicate detection: blocking via `cam_index` / `cascade` over record fingerprints (normalised name + domain + email + phone shingles), exact re-rank, `clam_search` for radius clusters; candidates → crm-sim merge plan, never auto-merge | L | Queued |  |
| D-INT-2 | Lookalike companies / contacts: CAM-PQ semantic codec trained on firmographic + engagement features; `top_k` | L | Queued |  |
| D-INT-3 | Lead / deal scoring: logistic model via BLAS L2 + activations; training via ndarray; calibration and rank fidelity reported with `reliability.rs` (Spearman vs exact) | L | Queued |  |
| D-INT-4 | AUTO suggestions (associate email → deal, set lifecycle stage, assign owner) via `lance-graph-arm-discovery`, lift-filtered, with `NarsTruth`; suggestions, never assignments (tesseract-paperless `auto_match` pattern) | M | Queued |  |
| D-INT-5 | Enrichment: spider crawl of company domain → `spider_doc_ir` → intake → facts proposed as a crm-sim plan with provenance | L | Queued |  |
| D-INT-6 | Text understanding of emails/notes: deepnsm-v2 SPO extraction + sentence beliefs (measured coverage on real CRM text before any default) | M | Queued |  |
| D-INT-7 | Copilot / agent surfaces: compiled thinking templates (elixir-template / template-runtime); LLM runs compile into templates, never the reverse | XL | Queued |  |

## Wave 10 — Marketing and Service breadth

| D-id | Deliverable | Size | Status | Evidence |
|---|---|---|---|---|
| D-MKT-1 | Forms (fields, dependent fields, progressive profiling, hidden fields, consent) → submissions create/update contacts through ActionDefs | L | Queued |  |
| D-MKT-2 | Marketing email: drag-and-drop doc IR, send to lists via stalwart bulk submission, subscription types, unsubscribe, bounce handling | XL | Queued |  |
| D-MKT-3 | Campaigns + attribution (first/last/linear/U/W/time-decay as report folds over touch points) | L | Queued |  |
| D-MKT-4 | Landing pages / CMS: askama/typst render of doc IR; out of v1 beyond forms + email | — | Deferred | 2026-10-21 ruling: also owns answer-engine optimisation (`marketing-aeo`, 5 functions); the whole D-id is deferred in the surface map. |
| D-SVC-1 | Tickets, help desk inbox, SLAs (from W7), feedback surveys (NPS/CSAT/CES) | L | Queued |  |
| D-SVC-2 | Knowledge base (doc IR + search) and customer portal (RLS-scoped to the contact's company) | L | Queued |  |
| D-COM-1 | Quotes, products, line items, invoices, payments on the commerce concepts (convergence with odoo-rs) | L | Queued | 2026-10-21 ruling: also owns deal splits (`crm-deal-splits`, 2 functions). |
| D-MKT-5 | Marketing events: event lifecycle, registrations, attendance, attribution | M | Queued | Operator ruling 2026-10-21 (`ISS-SURFACE-UNPLANNED`). Surface: `marketing-marketing-events`, `marketing-events-tracking`, 37 functions |

## Wave 11 — Compatibility API and migration

| D-id | Deliverable | Size | Status | Evidence |
|---|---|---|---|---|
| D-API-0 | The documented API surface as data: every operation of legacy, 2026-09 and 2027-03-beta, with coverage per area (the map of functions to rebuild) | M | Shipped | 2026-10-20: `hubspo-harvest surface`/`coverage`, `scripts/fetch-hubspot-surface.sh`, `data/hubspot/api_surface.tsv` — 2,279 endpoints (legacy 1,194, 2026-09 980, beta 105; 2,325 doc entries, generic endpoints listed by several specs); ~1,506 distinct functions (980 current + 440 legacy-only + 86 beta-only; corrected from 1,518, 12 beta pages double-counted). Table in LATEST_STATE. Linear claim: EXO-50 `session_01PPcakjk7ioQsnasmNDtc5z/002` (D-API-SURFACE). Shipped in #39. Deliverable map: `data/hubspot/surface_map.tsv` + `hubspo-harvest deliverables` (140 families → 33 D-ids, 134 functions unplanned: `ISS-SURFACE-UNPLANNED`) |
| D-API-1 | `/crm/v3/objects/*` CRUD + batch + search on the façade; association v4; properties; pipelines; owners | XL | Queued |  |
| D-API-2 | Webhook subscriptions (outbound) from version deltas | M | Queued |  |
| D-API-3 | OAuth apps / private app tokens mapped to RBAC actors with delegation | M | Queued |  |
| D-API-4 | Migration runbook: shadow mode (read-only mirror of a live portal, nightly diff), then cut-over | L | Queued |  |
| D-API-5 | Contract tests: replay recorded HubSpot API traffic against the façade | L | Queued |  |
| D-EXT-1 | App platform: extension definitions, CRM cards, feature flags, install / uninstall lifecycle; a2ui integration. HubSpot's React extension runtime is not promised | L | Deferred | Operator ruling 2026-10-21 (`ISS-SURFACE-UNPLANNED`). Surface: 4 families, 22 functions |
| D-OPS-1 | Tenant quotas and capacity introspection: report the limits actually configured, never invented HubSpot defaults | S | Queued | Operator ruling 2026-10-21 (`ISS-SURFACE-UNPLANNED`). Surface: `crm-limits-tracking`, 9 functions |

## Cross-cutting — H-ADR-15 (Kafka-shaped log outside, mask algebra inside)

| D-id | Deliverable | Size | Status | Evidence |
|---|---|---|---|---|
| D-LOG-1 | Log record (`NodeDelta` body) + batch header `(topic, base_offset, record_count)`, RBAC-projected per consumer; zone 3 | M | Queued |  |
| D-LOG-2 | Consumer offsets as `(consumer, last_version)` rows; replay over `VersionRange` by fragment diff (not `delta()`) | M | Queued | P-LOG-1 passed 2026-10-10 |
| D-LOG-3 | Optional `rdkafka` edge producing the same records to a Kafka topic | M | Queued |  |

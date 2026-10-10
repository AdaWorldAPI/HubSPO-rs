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
| D-ONT-1 | Harvest: `hubspo-harvest` pulls HubSpot's public OpenAPI specs + the properties/pipelines/association-label APIs of a sandbox portal → normalised schema JSON (zone 3, tool only) | M | In PR | public-catalog half done; sandbox half blocked on `ISS-ORACLE-PORTAL` |
| D-ONT-2 | OGAR: `ogar-from-schema` OpenAPI front end (queued upstream, `lib.rs:48`) — **requested from OGAR, not built in HubSPO-rs** | L | Queued |  |
| D-ONT-3 | Mint dossier: per candidate concept (H-ADR-2 table) the METHOD/STORAGE/STRUCTURE evidence + OGIT source + convergence argument; submit to the 5+3 codebook council | M | Shipped | ratified v3 `.claude/plans/hubspo-concept-mint-dossier-v3.md` (5+3 council, 2026-10-10) |
| D-ONT-4 | OGAR PR: `ConceptDomain::Crm` (`0x05XX`), 15 concepts in `class_ids` + `CODEBOOK` + `ALL`, `HubSpoPort` (`0x000B`) + aliases, layout doc row; lance-graph `ogar_codebook` mirror rows + `AppPrefix` 0x0008–0x000B. `crm_actions` deferred to D-ACT-1 (dossier v3 §7) | M | Shipped | OGAR#330, lance-graph#1438 |
| D-ONT-5 | `hubspo-port`: `RbacPlug` binding; `HotPlug` over Crm ids asserted to fail `NoCapabilitiesFor` until D-ACT-1 (dossier v3 §7); deliberately-wrong-plug test proving each drift arm can fire | S | In PR | `hubspo-port`, 13 tests |
| D-ONT-6 | `hubspo-schema`: ClassViews for contact, company, deal, ticket, lead, product, line item, quote, activity kinds; standard property position tables via the generator into `hubspo-generated` | L | Queued |  |
| D-ONT-7 | Custom-object/-property design (H-ADR-3) + probe P-CUSTOM-1 (lane-per-property vs packed run; fold latency at 50 / 500 / 2,000 custom properties) | M | Queued |  |

## Wave 2 — Store, versions, history

| D-id | Deliverable | Size | Status | Evidence |
|---|---|---|---|---|
| D-STO-1 | `hubspo-store` write lane: batch image → one Lance version per publish; owner-stamped (`mailbox_owner`) | L | Queued |  |
| D-STO-2 | KV value store for variable-length properties; zero-copy lens reads | M | Queued |  |
| D-STO-3 | Edge lanes + association labels (H-ADR-4); cardinality invariants as Quack `GroupReduce` | L | Queued |  |
| D-STO-4 | Tenant layout + P-TEN-1 probe | M | Queued |  |
| D-STO-5 | Property history via version log + P-HIST-1 probe | M | Blocked | P-HIST-1 failed 2026-10-10 (best 46 ms vs 20 ms); next: reader reuse vs value-carrying index |
| D-STO-6 | Importer (value oracle, part 1): sandbox portal → crm-sim plan → commit; counts compared per object type | L | Queued |  |

## Wave 3 — Query, lists, views, reports

| D-id | Deliverable | Size | Status | Evidence |
|---|---|---|---|---|
| D-QRY-1 | Filter compiler: HubSpot filter groups (AND within group, OR across groups; operators EQ/NEQ/LT/…/HAS_PROPERTY/CONTAINS_TOKEN/IN/BETWEEN) → Quack program | L | Queued |  |
| D-QRY-2 | Lists: active (incremental on version delta) + static (stored mask); membership history | L | Queued |  |
| D-QRY-3 | Saved views + board view (report pivot over pipeline × stage) | M | Queued |  |
| D-QRY-4 | Quack lowering of callcenter's `ParsedQuery` (the upstream DataFusion `parsed_query_to_plan` is an unimplemented stub) | M | Queued |  |
| D-QRY-5 | Association-traversing filters ("contacts whose company has an open deal"): Quack `Semijoin` over edge lanes for one hop; multi-hop needs cypher-quack shapes that do not exist yet → upstream request first | L | Queued |  |
| D-QRY-6 | Reports: single-object, cross-object, funnel, time-series, attribution skeleton on `lance-graph-report` | XL | Queued |  |
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
| D-IAM-1 | Role catalogue: Super Admin, Admin, Sales/Service/Marketing user, view-only, custom permission sets; grant source with `field_mask_of` | M | Queued |  |
| D-IAM-2 | Ownership scopes (all / team / owned) as `ScopeSpec`; team hierarchy via `Dn128` subtree | M | Queued |  |
| D-IAM-3 | Field-level permissions (sensitive properties) as per-role `WideFieldMask`; absent from the wire | M | Queued |  |
| D-IAM-4 | Directory ingest (ogar-ad / ogar-az / native), seat assignment as directory simulation plans | L | Queued |  |
| D-IAM-5 | Partitioning (business units / brands) as tenant sub-scopes | M | Queued |  |
| D-IAM-6 | Accumulation guard: exports / cohort reports require an extra grant | S | Queued |  |
| D-IAM-7 | RLS as a Quack predicate injected before evaluation (tenant + scope); the callcenter `RlsRewriter` is DataFusion-only and frozen. Upstream into lance-graph if a second consumer needs it | M | Queued |  |

## Wave 6 — Communications (spear + stalwart)

| D-id | Deliverable | Size | Status | Evidence |
|---|---|---|---|---|
| D-MAIL-1 | stalwart deployment profile (separate service, AGPL stays there); stalwart keeps its own directory; contacts join to mailboxes through spear's placement (OD-12) | M | Queued | re-scoped 2026-10-10 (OD-12) |
| D-MAIL-2 | Webhook receiver (HMAC verify) → spear `activity` rows for delivery / ingest events | M | Queued |  |
| D-MAIL-3 | One-to-one email send from a record via JMAP `EmailSubmission`; `send_receipts` row | M | Queued |  |
| D-MAIL-4 | Inbound logging: MTA hook BCC capture + mailbox sync → associate to contacts by address → company / deal by association rules. Implements the contract's `MailParser` / `ThreadLinker` seams (`lance-graph-contract/src/mail.rs`, no impl in tree) | L | Queued |  |
| D-MAIL-5 | Shared inbox / conversations: new spear roles (team inbox member, inbox admin) + ownership routing | L | Queued |  |
| D-MAIL-6 | Meetings: CalDAV / JMAP Calendars, iTIP, booking pages (availability fold over calendars), meeting engagement records | XL | Queued |  |
| D-MAIL-7 | Contact sync (CardDAV / JMAP Contacts) — two-way, conflict policy via crm-sim | L | Queued |  |
| D-MAIL-8 | Templates, snippets, personalisation tokens (askama-compiled, validated against the ClassView) | M | Queued |  |
| D-MAIL-9 | Open / click tracking endpoints + bot/MPP heuristic flag | M | Queued |  |
| D-MAIL-10 | Calls: telephony is out of scope for v1 beyond call *logging* engagements; spear D-TELEPHONY reuse later | S | Queued |  |

## Wave 7 — Automation: workflows, sequences, SLAs

| D-id | Deliverable | Size | Status | Evidence |
|---|---|---|---|---|
| D-FLOW-1 | Workflow model: trigger (criteria / event / schedule / webhook), enrolment, re-enrolment, actions, branches (if/then, value equals, A/B), delays (fixed, until date, until event) as graph-flow graphs | XL | Queued |  |
| D-FLOW-2 | Enrolment triggers: mask delta between published versions ∩ criteria from a stored offset; due-run mask for delays and schedules; watcher as wake-up only (H-ADR-7, amended 2026-10-10) | L | Queued |  |
| D-FLOW-3 | Action library: set property, create record/task, send email, add to list, webhook, rotate owner, delay, branch, custom code (loco program) | L | Queued |  |
| D-FLOW-4 | Sequences: steps (email / task / call), stalwart future-release for scheduled sends, unenrol on reply/meeting booked (MTA hook / webhook) | L | Queued |  |
| D-FLOW-5 | SLAs + ticket routing (round robin, skills) as timers + rules; reuse spear D-ROUTE patterns | M | Queued |  |
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
| D-MKT-4 | Landing pages / CMS: askama/typst render of doc IR; out of v1 beyond forms + email | — | Queued |  |
| D-SVC-1 | Tickets, help desk inbox, SLAs (from W7), feedback surveys (NPS/CSAT/CES) | L | Queued |  |
| D-SVC-2 | Knowledge base (doc IR + search) and customer portal (RLS-scoped to the contact's company) | L | Queued |  |
| D-COM-1 | Quotes, products, line items, invoices, payments on the commerce concepts (convergence with odoo-rs) | L | Queued |  |

## Wave 11 — Compatibility API and migration

| D-id | Deliverable | Size | Status | Evidence |
|---|---|---|---|---|
| D-API-1 | `/crm/v3/objects/*` CRUD + batch + search on the façade; association v4; properties; pipelines; owners | XL | Queued |  |
| D-API-2 | Webhook subscriptions (outbound) from version deltas | M | Queued |  |
| D-API-3 | OAuth apps / private app tokens mapped to RBAC actors with delegation | M | Queued |  |
| D-API-4 | Migration runbook: shadow mode (read-only mirror of a live portal, nightly diff), then cut-over | L | Queued |  |
| D-API-5 | Contract tests: replay recorded HubSpot API traffic against the façade | L | Queued |  |

## Cross-cutting — H-ADR-15 (Kafka-shaped log outside, mask algebra inside)

| D-id | Deliverable | Size | Status | Evidence |
|---|---|---|---|---|
| D-LOG-1 | Log record (`NodeDelta` body) + batch header `(topic, base_offset, record_count)`, RBAC-projected per consumer; zone 3 | M | Queued |  |
| D-LOG-2 | Consumer offsets as `(consumer, last_version)` rows; replay over `VersionRange` by fragment diff (not `delta()`) | M | Queued | P-LOG-1 passed 2026-10-10 |
| D-LOG-3 | Optional `rdkafka` edge producing the same records to a Kafka topic | M | Queued |  |

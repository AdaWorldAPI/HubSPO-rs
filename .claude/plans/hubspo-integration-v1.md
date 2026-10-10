# HubSPO-rs integration plan v1 — HubSpot reimagined on lance-graph · OGAR · ndarray · spear · stalwart · dir-sim

> **Status:** APPROVED 2026-10-10 — all open decisions OD-1..OD-11 (§10)
> approved as recommended. Wave 0 in PR; every later D-id `Queued`. Live
> status per D-id: `.claude/board/STATUS_BOARD.md`.
>
> **Companion:** `.claude/plans/hubspo-feature-matrix-v1.md` maps every
> HubSpot feature (160 rows, all five hubs plus platform and AI) onto a
> substrate and a D-id. This file holds the architecture, decisions,
> waves and gates; the matrix holds the coverage.
>
> **Evidence base:** a read-only survey of the sibling checkouts on
> 2026-10-10 (lance-graph, OGAR, ndarray, spear, stalwart, a2ui-rs,
> rs-graph-llm, odoo-rs, openproject-nexgen-rs, MedCare-rs,
> tesseract-rs/tesseract-paperless, spider, ruff). Paths cited as
> `repo/path:line` were read, not grepped. Anything not cited is a design
> proposal and is labelled as such.

---

## §0 Reading order

1. This file, §1–§4 (thesis, ground truth, decisions).
2. `hubspo-feature-matrix-v1.md` for the feature you are touching.
3. Before any classid / port / concept work: OGAR
   `docs/OGAR-CONSUMER-BEST-PRACTICES.md`, `docs/HOTPLUG-MIGRATION-GUIDE.md`,
   lance-graph `.claude/knowledge/ogar-consumer-preflight.md`.
4. Before any transcode / harvest work: ruff
   `.claude/knowledge/consumer-transcode-furnace-playbook.md` and
   `fuzzy-recipe-codebook.md`.
5. Before any query / report work: lance-graph `CLAUDE.md` §"planning
   migrates to ogar-loco / ogar-r2il; DataFusion is in grace period" and
   `crates/lance-graph-report/src/lib.rs` header.

---

## §1 Thesis

HubSpot is, under the marketing, four things:

| HubSpot's real shape | What it costs them | What we use instead |
|---|---|---|
| **An object graph** — standard + custom objects, typed properties, labelled associations, property history | HBase/MySQL (Vitess) rows + an association service + Elasticsearch for search + a property-history store | **lance-graph SoA rows** keyed by the canonical 16-byte GUID, typed by an OGAR **classid**, read through a **ClassView**; history is the **Lance version log** (no second store) |
| **A rule/automation engine** — workflows, sequences, lead scoring, SLAs, routing, calculated properties | a workflow service with enrollment queues, delay timers, Kafka between them | **OGAR `ActionDef` + `KausalSpec`** (behaviour as data), executed by **graph-flow** (rs-graph-llm) under **ractor** supervision, gated def-match → RBAC → state-guard → commit |
| **A communication hub** — email send/track, inbox, chat, meetings, calls | SMTP infra, tracking pixels, a calendar sync service, an inbox product | **stalwart** (out of process: SMTP/JMAP/CalDAV/CardDAV) + **spear** (in process: columnar mail store, recipient placement via dir-sim, mailbox IAM) |
| **A reporting + AI layer** — dashboards, custom reports, attribution, Breeze AI, enrichment, dedup | ClickHouse-style OLAP, ML services | **lance-graph-report** folds over **Quack / mask-risc**, **ndarray** SIMD/HPC (Hamming, CAM-PQ, cascade, BLAS) for dedup / lookalike / scoring, **arm-discovery** for suggestions with NARS truth |

The organising rule is the OGAR consumer invariant: **the classid is pure
address; the magic is what it resolves to.** HubSPO-rs does not own a
"Contact struct", it owns *what a contact means for HubSpot-shaped work*:
the ClassView skin, the action set, the RBAC grants, the screens. The
concept is shared; the skin is ours.

Second organising rule, from the BYOS endgame in lance-graph-java: the
customer-facing surface must be boring and familiar (HubSpot-API-shaped
REST, PostgREST-shaped reads, Supabase-shaped realtime). The novelty lives
underneath and is never something a customer must learn.

---

## §2 Ground truth (what exists on 2026-10-10)

### 2.1 HubSPO-rs itself

- Six crates (`Cargo.toml`): `hubspo-ontology`, `-graph`, `-models`,
  `-engine`, `-compatibility`, `-invariants`. Edition 2021. **No
  `rust-toolchain.toml`.**
- **The workspace does not compile.** Every crate's `lib.rs` declares
  modules whose files do not exist (e.g. `hubspo-engine/src/lib.rs`
  declares `pipeline_engine`, `association_rules`, `property_computation`;
  `hubspo-ontology/src/objects/mod.rs` declares `ticket`, `engagement`,
  `line_item`, `association` — none present). Only `company.rs`,
  `contact.rs`, `deal.rs`, `associations.rs`, `properties.rs` have bodies.
- Existing types (`Company { id: String, properties: Vec<Property> }`,
  `Property { name, value: String, … }`, `AssociationType` closed enum)
  are the **anti-pattern** the stack forbids: String ids instead of GUIDs,
  stringly-typed values instead of typed lanes, a closed enum of
  association kinds instead of classid-addressed edges. They are replaced
  wholesale in Wave 0, not extended.

### 2.2 OGAR (the IR / codebook)

- Ports: `OpenProjectPort 0x0001`, `OdooPort 0x0002`, `WoaPort 0x0003`,
  `SmbPort 0x0004`, `HealthcarePort 0x0005`, `RedminePort 0x0007`,
  `OsmPort 0x0008`, `WeatherNextPort 0x0009`, `SpearPort 0x000A`
  (`crates/ogar-vocab/src/ports.rs:136-652`). q2 holds `0x0006` reserved
  (`docs/APP-CLASS-CODEBOOK-LAYOUT.md` §2). **No CRM / HubSpot port.**
  Next free prefix: `0x000B`.
- Classid canon: `render_classid = (concept << 16) | APP_PREFIX`
  (`crates/ogar-vocab/src/app.rs:42`); domain decided by the high byte
  (`lib.rs:1772`).
- **No CRM concept domain.** Concepts that a CRM can *converge onto*
  today (`crates/ogar-vocab/src/lib.rs:1935-2334`):

  | CRM idea | Existing concept | hex |
  |---|---|---|
  | Account / party (company *or* person, Odoo `res.partner`) | `BILLING_PARTY` | 0x0204 |
  | Quote / order / invoice | `COMMERCIAL_DOCUMENT` | 0x0202 |
  | Line item | `COMMERCIAL_LINE_ITEM` | 0x0201 |
  | Product / price book / rule | `PRODUCT` / `PRICELIST` / `PRICELIST_RULE` | 0x0207 / 0x0209 / 0x020A |
  | Payment / currency / tax | `PAYMENT_RECORD` / `CURRENCY_POLICY` / `TAX_POLICY` | 0x0205 / 0x0206 / 0x0203 |
  | Owner / user (employee) | `HR_EMPLOYEE` | 0x0D01 |
  | Team | `HR_DEPARTMENT` | 0x0D02 |
  | Email | `EMAIL` | 0x0B05 |
  | Document / file | `DOCUMENT` | 0x080B |
  | Note (comment) | `PROJECT_COMMENT` | 0x010F |
  | Timeline / journal entry | `PROJECT_JOURNAL` | 0x0109 |
  | Follower | `PROJECT_WATCHER` | 0x0113 |
  | Custom field / value | `PROJECT_CUSTOM_FIELD` / `PROJECT_CUSTOM_VALUE` | 0x0110 / 0x0119 |
  | Saved view | `PROJECT_QUERY` | 0x010D |
  | Auth profiles | `AUTH_STORE`…`AUTH_ORY_KETO` | 0x0B01–0x0B04 |

  Absent: contact (person distinct from party), company (organisation
  distinct from party), deal, ticket, lead, task, meeting, call,
  campaign, pipeline, pipeline-stage, stage transition, form, list,
  sequence, workflow.
- **OGIT upstream vocabulary is already imported** under
  `OGAR/vocab/imports/ogit/NTO/` and covers most of the gap:
  `CustomerSupport/{Ticket, Organization, User, Group, Comment,
  ChatSession, ChatMessage}`, `SalesDistribution/{Customer, SalesOrder,
  SalesOrderItem, Invoice, Payment, Delivery, …}`,
  `Advertising/{Campaign, Deal, Creative, LineItem, Advertiser, …}`,
  `Forms/{Form, FormField, FormFieldGroup}`, `Survey/{Survey, Reply,
  Iteration}`, `Schedule/{Activity, Event, Attendee, Calendar}`,
  `SaaS/{Webhook, Integration, EventHandler}`,
  `Documents/{Document, …}`. Status in the lift catalogue
  (`docs/OGIT-DOMAIN-LIFT-CATALOGUE.md:89-165`): `SalesDistribution`
  Lift-tested; the others Imported. Promotion to a codebook id is
  **demand-driven and ownership-gated** (same file, :73-87) — HubSPO-rs
  is the demand.
- Behaviour IR: `Class` (`lib.rs:123`), `ActionDef` (`:396`, with
  `on_enter: EnterEffect`, `guard_failure_policy`, `state_timeout_millis`),
  `ActionInvocation` (`:515`, `ActionState{Pending,Committed,Failed,Cancelled}`,
  `idempotency_key`), `KausalSpec` (`:610`: `StateGuard`,
  `LifecycleTrigger`, `Depends`, `Constrains`, `Onchange`,
  `ContextDepends`, `External`). Domain workflow = a guarded effect, not
  machine state ("lifecycle formalized; workflow as data",
  `docs/OGAR-AST-CONTRACT.md` §3).
- Hot-plug authority: `capability_registry::resolve_hotplug` (`:432`)
  resolves only against `domain_tables()` (`:189`) = ocr, geo,
  healthcare, document. **A CRM plug would fail `NoCapabilitiesFor` until
  a `crm_actions` table exists.**
- RBAC: `ogar-rbac::OgarRbac<S: GrantSource>` (`:111`) with
  `field_mask_of(role, concept) -> Option<WideFieldMask>`, `bind(&RbacPlug)`,
  `authorize_user`, RFC 8693 delegation. Grant data is fixture-injected.
- Render: `ogar-render-askama` (`FieldView`, `render_field_view`,
  `list_view`, `form_view`; ~61 tests), `ogar-a2ui-frame`
  (`NodeDelta` / `ActionInvoke`, LE wire), `ogar-doc-ir`.
- Programs: `ogar-loco` (low-code call ABI, ~84 tests), `ogar-r2il`.
  `docs/LOCO-ORCHESTRATION-GAP.md` (2026-09-14): graph-flow orchestration
  on loco needs `Inventory`-addressed branches and suspension; **"nothing
  implements it"**.
- Producers: `ogar-from-ruff` (`compile_graph_python/ruby/csharp`),
  `ogar-from-rails`, `ogar-from-schema` (OGIT TTL + XSD shipped;
  **OpenAPI / JSON-Schema listed as queued**, `lib.rs:48`).
- Directory: `ogar-dir-core`, `ogar-ad`, `ogar-az`, `ogar-dir-sim`
  (meaning only: `Change`, provenance, `Violation`, `ExecutionPlan`;
  `NodeKind{User,Group}`; PoC 2026-10-03, `docs/DIRECTORY-SIMULATION-POC.md`).

### 2.3 lance-graph (the spine)

- `lance-graph-contract`: `canonical_node` (`NodeGuid`/`NodeRow`, 16|16|480,
  V3 `classid(4)+12` facet for new mints), `class_view` (`ClassView`,
  `FieldMask`, `WideFieldMask`), `soa_envelope` (`mailbox_owner`,
  write-on-behalf), `hotplug` (`HotPlug`, `CapabilityAuthority`),
  `rbac` (`ClassGrant`, `OpMask`, `grants_permit`), temporal vocabulary
  (`LanceVersion`, `TemporalPov`).
- `lance-graph-callcenter` (339 tests): `ExternalMembrane` impl
  (`LanceMembrane`, `lance_membrane.rs:356`; `subscribe` ignores its
  `CommitFilter`, "Phase A"). Status by piece:
  - **PostgREST (DM-8): stub.** `postgrest.rs:1-31` — "NO live HTTP
    server … pure-function dispatcher". `parse_path` / `ParsedQuery` /
    `Filter` work; POST/PATCH/DELETE return 501; `parsed_query_to_plan`
    (`:957`) returns "not yet implemented".
  - **RLS (DM-7): shipped, but on DataFusion.** `RlsRewriter` is a
    DataFusion `OptimizerRule` (`rls.rs:334`); `RlsPolicyRegistry` defaults
    to `Sealed`. Under the grace period it cannot be extended — a Quack-side
    RLS lowering is needed (D-IAM-7).
  - **Realtime: partial.** `LanceVersionWatcher` (`version_watcher.rs:57`)
    ships; the Phoenix WS server is commented out (`// pub mod phoenix;`)
    and the `serve` feature has no axum code yet.
  - **Audit: shipped.** `AuditSink`, `LanceAuditSink`, merkle-chained
    `UnifiedAuditEvent`, `bin/audit_verify.rs`.
  - Also: JWT `auth.rs`, `savant_reasoners.rs`
    (`CustomerCategoryReasoner`, `NextBestActionReasoner` — CRM-flavoured
    and unused), `odoo_alignment.rs` (`res.partner` → OGIT).
- Contract trait seams with **no implementation in tree**: `mail.rs`
  (`MailParser`, `ThreadLinker::thread_key`), `repository.rs`
  (`EntityStore`, `EntityWriter`), `tax.rs` (`TaxEngine`),
  `transaction/*`, `kanban.rs` (`LIBET_COMMIT_WINDOW_US = 550_000`).
- `ogar_codebook::AppPrefix` mirror lists Core 0 … Redmine 7 only
  (`:297`); a HubSpo row is needed alongside the OGAR port.
- Planner write path: `BatchWriter::cast(on_behalf, moves, payload)`
  (`batch_writer.rs:132`) — its own doc says zero production call sites;
  the supervisor's `cycle_driver` is the real caller.
- `lance-graph-rbac` (depends only on the contract; `ClassGrant`,
  `OpMask`).
- `lance-graph-dir-sim`: execution half of directory simulation — `Arc<Snapshot>`
  + delta overlay versions, rules over borrowed views, invariants as Quack
  programs (`Semijoin` anti-joins, `GroupReduce` counts), diff → plan,
  audit. ≤ 65,536 nodes per population (`u16` ordinals).
- `lance-graph-quack` + `lance-graph-mask-risc`: the one evaluator.
  `lance-graph-report`: "a coordinate-native fold algebra that reporting
  falls out of … NOT a second engine" (`src/lib.rs:1-20`).
  `lance-graph-cypher-quack` is **experimental**: it lowers only
  `MATCH (n:L) WHERE … RETURN count/sum/min/max`, one-hop `count(*)` and
  `count(DISTINCT)`; everything else returns a typed `Refusal`.
- **BBB barrier:** a customer binary must not depend on
  `lance-graph-ogar`, `-planner` or the engine. Spine-only crates
  (`OgarAuthority::activate`) run in HubSPO-rs tests and the admin tool;
  the server binary reads classids through `contract::ogar_codebook`
  (consumer preflight Q4). MedCare-rs is the worked example
  (`medcare-rbac/src/hotplug.rs:70`, `rbac_plug.rs:66`).
  `lance-graph-sap`: precedent for a CATS boundary adapter over the same
  ABI ("No SAP evaluator").
- `lance-graph-arm-discovery` (Aerial+ association rules → `TruthU8`,
  which lives in that crate, not the contract; `NarsTruth` is in
  `contract::exploration`),
  `deepnsm` / `deepnsm-v2` (PoS FSM → SPO triples; KJV-trained space),
  `lance-graph-supervisor` (ractor actor tree, one-for-one),
  `lance-graph-cypher-quack`, `lance-graph-planner`, `elixir-template` /
  `template-runtime` (compiled thinking templates), `lance-graph-osint`.
- **Ruling (lance-graph `CLAUDE.md`, 2026-09-05):** *"Every planning is in
  migration to ogar-loco and ogar-r2il, especially datafusion is out of the
  picture, what exists gets a grace period, nothing new will migrate to
  it."* → HubSPO-rs writes **no new DataFusion code**. (spear's own
  `CLAUDE.md` still mandates "DataFusion `Expr` API only"; that rule
  predates the ruling — see Open Decision OD-7.)
- Pins (lance-graph `CLAUDE.md` § Key Dependencies): `lance = "12.*"`,
  `lancedb = "0.39.*"`, `arrow = "58"`, `datafusion = "54"`; never a
  `.0.0` exact pin; toolchain `1.98.1` (`rust-toolchain.toml:2`).
  ⊘ Superseded 2026-10-10: the fleet moved to toolchain `1.99.0` and
  `lance = "13.*"` / `lancedb = "0.40.*"` (arrow 58 / datafusion 54
  unchanged); H-ADR-14 follows.

### 2.4 ndarray (the foundation)

- Toolchain floor `1.98.1` (`rust-toolchain.toml:2`, `Cargo.toml:13`).
  Default `target-cpu=native`; ship builds use `.cargo/config-v3.toml`.
  A `RUSTFLAGS` env var silently replaces cargo-config rustflags.
- `ndarray::simd` facade (`src/simd.rs`): runtime tier dispatch, F32x16 …
  U64 lanes, BF16 RNE conversion, `hamming_distance_raw`,
  `popcount_raw`, `Fingerprint*`, `moments_u32`, `quantize_f32_to_i8/i4/i2`,
  `kmeans`, `squared_l2`, `BlasLevel3`, `cosine_f32_to_f64_simd`,
  `cascade`, `matmul_i8_to_i32`, mask/blend ops.
- `hpc/` relevant to a CRM: `fingerprint.rs`, `cam_index.rs` (multi-probe
  LSH + exact re-rank), `clam_search.rs` (rho-NN, no false negatives),
  `cascade.rs` (σ-band adaptive Hamming search), `cam_pq.rs`
  (`train_geometric/semantic/hybrid`, `top_k`), `prefilter.rs`,
  `blas_level{1,2,3}.rs`, `activations.rs`, `reductions.rs`,
  `statistics.rs` (mergeable `MomentsU32`), `reliability.rs`
  (`pearson`/`spearman`/`cronbach_alpha`/`icc_a1`), `udf_kernels.rs`,
  `arrow_bridge.rs`. Measured (README): 1M 32-byte fingerprints, one
  query, 15.5 ms single core. Hamming is a DataFusion UDF in lance-graph,
  **not** wired into Lance ANN.
- Consumer rule: all SIMD from `ndarray::simd`, never raw intrinsics,
  never `hpc::*` kernels re-implemented locally.

### 2.5 spear (mail + document hub, in process)

- Shipped (`src/`): five Lance schemas (`messages` 24 cols, `folders`,
  `accounts`, `events` CalDAV-shaped, `contacts` CardDAV-shaped —
  `schema/mod.rs:45-96`), RFC 5322 ingest (`mail/ingest.rs`), write path
  for `messages` only (`mail/store.rs:31-98`), recipient placement via
  dir-sim (`mail/directory.rs`, `Delivery` enum), owner-scoped mailbox
  IAM (`mail/iam.rs`: `RBAC_PLUG`, `DirectoryActors`, `visible_rows`;
  only role `MAILBOX_OWNER`), Cypher mail graph (`mail/graph.rs`),
  SHA-256 content store (`content/mod.rs`), SharePoint `UploadIntent` with
  an empty transport.
- Not built: `activity`, `send_receipts`, `hub/`, every transport, events
  / contacts writers, shared-inbox or team roles.
- Its plan `.claude/plans/spear-outer-integration-v1.md` §3.1 already
  reserved **Pillar A — HubSpot UX parity (D-HUB-1..8)**: OGIT `NTO/Hub`
  seed, `src/hub/` intents, unified `activity` table keyed
  `(tenant_id, correlation_id)`, `send_receipts`, timeline query. All
  Queued/Backlog, and partly superseded (bridges → `SpearPort`
  aliases, plan line 7). **This plan re-homes D-HUB-1/2/3/7 into HubSPO-rs
  and leaves D-HUB-4/5/6/8 in spear** (decision H-ADR-12).

### 2.6 stalwart (mail server, out of process)

- AdaWorldAPI fork of Stalwart 0.16.25. Only fork change: a feature-gated
  `dir-sim` recipient backend in `crates/directory`
  (`src/backend/dirsim/mod.rs`, 494 lines). Not wired into config or
  `crates/main` features; cannot authenticate (must pair with OIDC/LDAP).
- License `AGPL-3.0-only OR LicenseRef-SEL`; default features include
  `enterprise` (SEL snippets). **Linking any stalwart crate into the
  HubSPO-rs binary makes the combined work AGPL** (H-ADR-8).
- Seams, least to most invasive: telemetry **webhooks** (HMAC-signed trc
  events — `common/src/telemetry/webhooks/mod.rs`), **MTA hooks** + milter
  (`smtp/src/inbound/hooks/mod.rs:87-173`, stages connect…data, can add
  recipients / headers), **JMAP** + push (EventSource / WebSocket), the
  `Directory` enum, the `Store` enum, the in-process `EmailIngest` trait.
- Capabilities that map to HubSpot: submission + outbound queue with
  DKIM/DANE/MTA-STS; **future release / delayed delivery** (sequences);
  DSN + `DeliveryEvent`s (delivery receipts); threading; CalDAV + RFC 6638
  scheduling + iTIP over mail (meetings); CardDAV / JMAP Contacts; SCIM v2;
  Sieve; spam filter with reply-to analysis. **No open / click tracking.**

### 2.7 Other reusable consumers

- **a2ui-rs** — screen addressing (`NodeDelta` down, `ActionInvoke` up by
  ordinal), RBAC by projection (`WideFieldMask ∩ role`, fail-closed),
  `a2ui-paint` skins `Form/Flow/Grid/Tile`. The record page, board and
  pipeline kanban are projections, not widgets.
- **rs-graph-llm / graph-flow** — `Task`, `NextAction{Continue,
  ContinueAndExecute, WaitForInput, End, GoTo}`, `FlowRunner`,
  `graph-flow-action-ogar::GatedOgarHandler` (def-match → RBAC →
  state-guard → MUL impact → commit), `graph-flow-kanban`. The workflow /
  sequence engine.
- **tesseract-rs `tesseract-paperless`** — S-2 content dedup gate
  (`kv.rs`), `intake::ingest_doc_ir` (any `DocIr` producer),
  `LanceStore`, Tantivy search with per-language stemming, reconcile,
  paperless-ngx `matching` (None/Any/All/Literal/Regex/Fuzzy), AUTO
  suggestions via arm-discovery (`auto_match`), `axes` report lanes. The
  "Files / Documents / auto-associate" tier, nearly as-is.
- **spider** + `spider_doc_ir` — website crawl → `DocIr` → intake. The
  enrichment front end (company website facts).
- **odoo-rs / openproject-nexgen-rs / MedCare-rs** — the transcode
  precedents (thin ontology caller of `compile_graph::<Port>`, additive
  `-generated` crate from a checked-in generator, hand-written adapter
  only for the essential 15%, axum server on `$PORT`, test-only parity
  fixtures, branch=main git deps, no internal pins).

---

## §3 Architecture decisions (H-ADR)

Each decision states the rule, the reason, and what it forbids. Operator
review required for those marked **[OPERATOR]**.

### H-ADR-1 — Identity: GUID + classid, never `String` ids

- Every record is a `NodeRow` (512 B: key 16 | edge facet 16 | value 480)
  keyed by `NodeGuid`. New mints use the **V3 content-blind 4+12 facet**
  via `mint_for(...)`, never `NodeGuid::new` (V1 tail is forbidden for new
  units, OGAR `CLAUDE.md` P0).
- The classid is `concept << 16 | HUBSPO_APP_PREFIX`. The concept half is
  pulled from `ogar_vocab::class_ids` through `HubSpoPort::class_id(name)`
  (spine) or `contract::ogar_codebook::canonical_concept_id` (membrane).
  HubSPO-rs **never** hard-codes a concept hex and **never** keeps a local
  codebook copy (consumer preflight).
- HubSpot's numeric `hs_object_id` is a **foreign key**, stored as a typed
  lane on the record and indexed for the compatibility façade (§H-ADR-11),
  never as identity.
- Forbids: `id: String`, `HashMap<String, Company>`, a local `CRM_CODEBOOK`.

### H-ADR-2 — Ports and concepts: one port, converge first, mint last **[OPERATOR]**

- Author `HubSpoPort` in `ogar-vocab/src/ports.rs`
  (`NAMESPACE "HubSpot"`, `BRIDGE_ID "hubspo"`, `APP_PREFIX 0x000B`
  proposed) plus its row in `APP-CLASS-CODEBOOK-LAYOUT.md` §2.
- Alias HubSpot names onto existing concepts wherever the meaning truly
  converges (`Product → PRODUCT`, `LineItem → COMMERCIAL_LINE_ITEM`,
  `Quote/Invoice/Order → COMMERCIAL_DOCUMENT`, `Payment → PAYMENT_RECORD`,
  `User/Owner → HR_EMPLOYEE`, `Team → HR_DEPARTMENT`, `Email → EMAIL`,
  `File → DOCUMENT`, `Note → PROJECT_COMMENT`).
- *Refined 2026-10-10 by the mint dossier (`hubspo-concept-mint-dossier-v1.md`):
  users and owners → `PROJECT_ACTOR` (the IAM actor), not `HR_EMPLOYEE`;
  also `projects` → `PROJECT`. Council question Q5.*
- Mint a new **CRM concept domain** for what does not converge. Candidate
  list, each to pass the three-axis gate METHOD ∧ STORAGE ∧ STRUCTURE
  (furnace playbook §3) and the 5+3 codebook council *before* a hex is
  spent:

  | Candidate concept | OGIT source | Why not an alias |
  |---|---|---|
  | `crm_contact` (person) | `CustomerSupport/User` | `BILLING_PARTY` merges person and company; HubSpot never does |
  | `crm_company` (organisation) | `CustomerSupport/Organization`, `SalesDistribution/Customer` | same |
  | `crm_deal` | `Advertising/Deal` (closest), new | revenue opportunity with pipeline stage; no existing analogue |
  | `crm_ticket` | `CustomerSupport/Ticket` | `PROJECT_WORK_ITEM` carries project semantics HubSpot tickets lack |
  | `crm_lead` | new | HubSpot Leads object (prospecting) |
  | `crm_pipeline` / `crm_pipeline_stage` | new; `PROJECT_STATUS` is the near miss | stage carries probability + object-type binding |
  | `crm_activity` family (`call`, `meeting`, `task`, `communication`, `postal_mail`) | `Schedule/{Activity,Event,Attendee}` | engagement kinds with their own fields |
  | `crm_campaign` | `Advertising/Campaign` | marketing attribution root |
  | `crm_list` (segment) | new | population definition (see H-ADR-5) |
  | `crm_form` / `crm_form_field` | `Forms/*` | |
  | `crm_sequence`, `crm_workflow` | new | behaviour containers |
  | `crm_feedback_survey` | `Survey/*` | |
  | `crm_custom_record` | new | the custom-object carrier (H-ADR-3) |

  Domain byte: `0x10XX` is unassigned (`lib.rs:1763`); needs a
  `ConceptDomain::Crm` arm and a `"crm"` tag in `source_domain_concept`
  (which today knows only `"project"` and `"erp"`, `lib.rs:1845`).
- Add `crm_actions` to `capability_registry::domain_tables()` so
  `resolve_hotplug` can activate CRM classids.
- Forbids: minting in HubSPO-rs; minting a concept that passes only 2 of 3
  axes (lifecycle-stage enums, property groups, association labels are
  *data*, not concepts).

### H-ADR-3 — Custom objects and custom properties are data, not mints **[OPERATOR]**

- HubSpot lets every portal define custom objects and hundreds of custom
  properties at runtime. Minting an OGAR concept per tenant would burn the
  shared codebook on private schema.
- Proposal: one concept `crm_custom_record`; a tenant's custom object type
  is a **tenant-scoped schema row** (name, plural, primary display
  property, property definitions) and a **tenant ClassView** computed from
  it at load time. Custom *properties on standard objects* are positions
  in a per-tenant `WideFieldMask`-addressed extension tenant on the row's
  value slab.
- Lane layout: standard properties occupy fixed ClassView positions
  (compiled); custom properties occupy a tenant-scoped position range
  resolved by a `(tenant, object_type, property_name) → position` map held
  as data. Positions past 64 use `WideFieldMask` (never `FieldMask` — the
  MedCare-rs `FieldMask`-silently-drops-≥64 incident).
- Values that do not fit 480 B (long text, multi-select over many options,
  rich text) live in the KV value store addressed by `(guid, position)`;
  the row carries the reference. Zero-copy lens reads, never `Vec<Property>`.
- Open: whether the extension tenant is one lane per property (fast fold,
  wide) or a packed `(position,value)` run (narrow, slower fold). Decide by
  probe P-CUSTOM-1 (§6).

### H-ADR-4 — Associations are edges with labels as data

- An association is an **edge** between two GUIDs whose *kind* is a
  classid reference (relation = class; "types exist only before the bake",
  MedCare-rs commitment #10). HubSpot's association *labels*
  (e.g. "Decision maker", "Billing contact") and their per-direction
  names are tenant data on that relation, not new concepts.
- Storage: sparse edge lanes `(from_ordinal, to_ordinal, label_id)` per
  object-type pair, sorted, Quack-joinable — the same sparse-membership
  shape `lance-graph-dir-sim` uses for `user × group`
  (`docs/DIRECTORY-SIMULATION-POC.md` §3). The 16-byte edge facet on the
  `NodeRow` is the **content-blind facet cascade**, resolved through the
  class's `ClassView::edge_codec_flavor`; HubSPO-rs never reads it as
  "12 in-family + 4 out-of-family" (lance-graph CANON note, 2026-08-04 /
  2026-09-17).
- Primary company of a contact, deal's primary company, etc. are labels
  with a cardinality invariant checked by a Quack `GroupReduce Count`
  (like dir-sim UPN uniqueness).
- Forbids: `enum AssociationType` (the current `hubspo-ontology` code).

### H-ADR-5 — Lists, views, filters and segments are masks

- An active list / saved view / filter / report selection is a **predicate
  program** compiled to Quack and evaluated by mask-risc into a population
  mask; a static list is a stored mask (bitmap over ordinals) plus a
  version. Membership counts, "contacts in list A but not B", and
  enrolment criteria are mask algebra — no row materialisation until the
  terminal view ("filter IDs first, hydrate raw values last",
  `lance-graph-report/src/lib.rs`).
- Active-list re-evaluation is incremental: evaluate the predicate over the
  overlay delta of the new Lance version, OR/ANDNOT into the stored mask.
- Forbids: `Vec<ContactId>` as a list, per-row Rust filtering loops in the
  engine.

### H-ADR-6 — Query surfaces: Quack/mask-risc + report folds; no new DataFusion

- Reads: `lance-graph-quack` programs and `lance-graph-report` plans
  (pivot / fold / materialise). Graph reads (e.g. "contacts of companies
  with an open deal > €50k owned by team X"): `lance-graph-cypher-quack`.
- The PostgREST-shaped read path reuses `callcenter::postgrest`'s
  **parser** (`parse_path` / `ParsedQuery`, shipped) and supplies its own
  Quack lowering (D-QRY-4); the upstream `parsed_query_to_plan` is an
  unimplemented DataFusion stub and must not be completed on DataFusion.
- Graph reads beyond cypher-quack's current shapes are a **missing
  capability**: they land upstream in lance-graph first (STOP rule), never
  as a HubSPO-rs-local traversal engine.
- Programs / user-defined logic (calculated properties, custom-code
  workflow actions) are **ogar-loco** programs, never embedded JS/Python
  in the hot path.

### H-ADR-7 — Behaviour: ActionDef + KausalSpec, run by graph-flow under ractor

- Every mutation the product exposes ("create deal", "move stage",
  "enrol in sequence", "assign owner", "merge records") is an
  `ActionDef` on the concept, invoked as an `ActionInvocation` with an
  `idempotency_key`, through `GatedOgarHandler`: def-match → RBAC →
  `StateGuard` → commit.
- Pipelines: a stage move is `StateGuard{guard_field: stage,
  guard_values: allowed_from}` + `on_enter: EnterEffect{field: stage,
  to_value}`; stage *configuration* (order, probability, required
  properties, closed-won/lost flags) is data. Required-property-on-stage
  is a `Constrains` spec.
- Workflows / sequences: graph-flow graphs. Delays use
  `NextAction::WaitForInput` + a timer actor, not sleeping tasks.
  Branches use conditional edges whose conditions are mask predicates
  over the enrolled record. Enrolment triggers are `LifecycleTrigger` /
  `Onchange` specs matched by a trigger actor tailing `LanceVersionWatcher`.
- Calculated / rollup properties: `Depends` / `Onchange` specs; the value
  is a fold (`lance-graph-report`) over associated rows, recomputed on the
  dependency's version delta. Rollups are **not** stored as a second truth
  unless they sit at a strictly higher awareness rung (zero-copy warden's
  one carve-out); default is computed-on-read with a cached mask.
- Actors: `lance-graph-supervisor` (ractor). One writer per mailbox:
  each tenant×object-type write lane has one owning actor; writes from
  workflows go **on behalf of** that owner (`SoaEnvelope::mailbox_owner`).
  No actor-per-message write (the deleted `kanban_actor` shape); writes
  are staged into the batch image and published as one Lance version.
- Forbids: DDL events, `DEFINE EVENT`, SurrealQL anywhere (deprecated
  workspace-wide), behaviour inside a UI component, per-record RPC
  "advance" calls.

### H-ADR-8 — Mail: stalwart out of process, spear in process **[OPERATOR: licensing]**

- stalwart runs as its own service (AGPL stays inside it). HubSPO-rs talks
  to it only over network protocols: JMAP (send, inbox, threads,
  calendars, contacts), SMTP submission, CalDAV/CardDAV, signed telemetry
  **webhooks** (delivery / ingest events → activity rows), **MTA hooks**
  (BCC-to-CRM capture, tracking-header injection, reply detection for
  sequences).
- spear is a crate dependency (MIT/Apache) and owns the columnar mail
  store, recipient placement through dir-sim, mailbox IAM and the
  `activity` / `send_receipts` tables (spear D-HUB-4/5/6).
- The join key between a CRM contact and a mailbox is the directory GUID
  that both spear (`mail/directory.rs`) and stalwart's dir-sim backend
  resolve identically; a contact's email addresses resolve to `Delivery`
  placements through the same directory version.
- Open / click tracking is ours to build (stalwart has none): link
  rewriting at render time (template engine) + a redirect endpoint +
  pixel endpoint, each writing a `send_receipts` row; Apple MPP-style
  machine opens flagged by heuristic (D-MAIL-9).
- Forbids: linking `stalwart/crates/*` into HubSPO-rs; `lettre` or a second
  SMTP client inside HubSPO-rs (send goes through stalwart's submission).

### H-ADR-9 — Users, teams, permissions: dir-sim + ogar-rbac

- Seats / users / teams come from the directory (`ogar-dir-core` records
  ingested from AD / Entra via `ogar-ad` / `ogar-az`, or authored
  natively). HubSpot teams/sub-teams → `HR_DEPARTMENT` hierarchy;
  `Dn128` subtree selection gives "my team and below".
- Permission sets / seat types → roles in `ogar-rbac` with
  `GrantSource::{grants_of, scope_of, field_mask_of}`. HubSpot's three
  ownership scopes (everything / team-owned / owned-only) map to
  `ScopeSpec` over the owner GUID and the team subtree. Field-level
  permissions (sensitive properties) → per-role `WideFieldMask`.
  RBAC happens **before** framing (a2ui doctrine): an unauthorised field is
  absent from the wire.
- User provisioning, team moves, seat changes are **directory
  simulations**: observed G0 → rules → desired → validate → diff →
  `ExecutionPlan` (no write until the plan is approved). Same pipeline as
  `lance-graph-dir-sim`.
- Sensitivity grows with accumulation (MedCare-rs commitment #11): bulk
  exports and cohort reports over contacts require an extra grant; a
  single record view does not.

### H-ADR-10 — Every bulk change is simulated first ("crm-sim")

- Generalise the dir-sim pipeline to CRM records: imports, dedup merges,
  property mass edits, workflow backfills ("enrol existing records"),
  pipeline restructures, property deletions.
- Version = `Arc<Snapshot>` + delta overlay; rules run over borrowed views;
  invariants are Quack programs (required properties, association
  cardinality, unique email, valid enum values); diff produces an
  `ExecutionPlan` the user reviews (counts, samples, violations) before
  commit. Commit publishes one Lance version; revert = the previous
  version (time travel), audited.
- This is the substrate's answer to HubSpot's weakest UX point (irreversible
  bulk operations). New crate `hubspo-sim`; meaning types upstreamed to OGAR
  only if a second consumer appears (prefer generalising `ogar-dir-sim`'s
  `Change`/`ExecutionPlan` with a `NodeKind::Record(concept)` arm —
  Open Decision OD-4).

### H-ADR-11 — Compatibility façade: HubSpot-API-shaped, boring, thin

- `hubspo-compatibility` serves a HubSpot CRM v3-shaped REST surface
  (`/crm/v3/objects/{type}`, `/search`, `/batch/*`, associations v4,
  properties, pipelines, owners, webhooks subscriptions) so existing HubSpot
  integrations and Zapier-style tools work unchanged (BYOS).
- It is zone 3 (serde allowed); it lowers every call into an
  `ActionInvocation` or a Quack read. It holds no state.
- The same crate holds the **importer** from a real HubSpot portal (API
  client, OAuth / private-app token) used both for migration and as the
  **value oracle** (H-ADR-13).

### H-ADR-12 — Ownership split with spear **[OPERATOR]**

| Concern | Owner | Why |
|---|---|---|
| CRM object graph, pipelines, associations, lists, workflows, sequences, marketing, service, reporting UX | **HubSPO-rs** | the product |
| `HubSpoPort` + CRM concept mint requests | **OGAR** (requested by HubSPO-rs) | codebook authority |
| Mail store, placement, mailbox IAM, `activity`, `send_receipts`, document hub, SharePoint | **spear** | already its scope |
| SMTP / IMAP / JMAP / CalDAV / CardDAV serving | **stalwart** (process) | AGPL boundary |
| Directory meaning / execution | **OGAR / lance-graph** dir-sim | shared |
| Query / fold / RBAC kernels | **lance-graph** | spine |
| SIMD / similarity / BLAS | **ndarray** | foundation |

spear D-HUB-1 (`NTO/Hub` seed), D-HUB-2 (`hub/` intents), D-HUB-3 /
D-HUB-7 (bridges, obsolete) move to HubSPO-rs as D-ONT-* / D-ACT-*;
spear keeps D-HUB-4/5/6/8 (activity, receipts, timeline query, test).
HubSPO-rs consumes spear's activity table rather than defining a second.

### H-ADR-13 — Two parity oracles from day one

- **Value oracle:** a real HubSpot portal (sandbox), exported through the
  API. Same input → same records, same associations, same computed
  properties, same list memberships, same workflow outcomes.
- **Structure oracle (Klickwege):** the HubSpot UI's screen graph —
  object index pages, record page left/middle/right sidebars, board view,
  pipeline settings, list builder, workflow canvas — captured as a
  navigation graph and compared to ours (same screens, regions,
  reachability). Shipping only one oracle is the documented anti-pattern
  (furnace playbook §2).

### H-ADR-14 — Build hygiene inherited from the fleet

- `rust-toolchain.toml` `channel = "1.99.0"` (was 1.98.1 until the fleet bump of 2026-10-10); edition 2024 (matches OGAR,
  a2ui-rs).
- Git deps on `branch = "main"` for lance-graph-*, OGAR crates, ndarray,
  a2ui-rs, spear, rs-graph-llm (ndarray's default branch is `master`, not
  `main`); lance/lancedb/arrow/datafusion floated per
  lance-graph's family (`13.*`, `0.40.*`, `58`, `54`), never exact `.0.0`.
  One ndarray in the graph (cargo-substrate law). No `vendor/` symlinks,
  no relative paths that escape the repo in committed manifests.
- CI: `cargo test --workspace`, `clippy --workspace --all-targets --no-deps
  -D warnings`, `fmt --check`, with `CARGO_PROFILE_DEV_DEBUG=0` and an
  explicit target-cpu config. Every feature gets a CI line (tesseract-rs
  lesson: excluded crates rot).
- Worker agents never run cargo; the orchestrator builds once.

### H-ADR-15 — Kafka-shaped log outside, zero-copy mask algebra inside (checked 2026-10-10)

Question (operator, 2026-10-10): can HubSPO-rs speak a Kafka-inspired wire
format externally and use the zero-copy masking algebra internally, in
line with the other session's stack (rs-graph-llm as the LangGraph-shaped
orchestrator, Rig as the LangChain-shaped model layer, z8run as the
n8n-shaped flow tool, all Quack / DuckDB-shaped)?

**Verdict: yes, with five conditions.** The two halves meet cleanly
because Kafka's semantics are already the shape of a Lance dataset:

| Kafka | here |
|---|---|
| topic | one dataset per tenant × object type (§5.5) |
| partition, total order | one writer per dataset (H-ADR-7, one-writer-per-mailbox) |
| offset | `LanceVersion` (`u64`, monotonic per dataset) |
| record key | the 16-byte `NodeGuid` |
| compacted topic (latest value per key) | the dataset at HEAD |
| full log, replay from an offset | the version history, read with `TemporalPov::at` / `VersionRange` |
| retention | the Lance version cleanup policy |
| consumer group offset | a stored `(consumer, last_version)` row |
| idempotent producer | `ActionInvocation::idempotency_key` |

**Inside (verified in code).**
- mask-risc evaluates over borrowed `LaneRef::{I32, U32, U64}` slices and
  `Strided` views straight into 512-byte `NodeRow` bytes, and never
  allocates (`lance-graph-mask-risc/src/ir.rs:16`, `lib.rs` A1). An Arrow
  array read from Lance can be handed over as `values()` with no copy.
- Sums widen `i32` to `i64` (`Terminal::MaskedSumI32`); `EqU32Via` /
  `GroupSumViaI32` give one-hop foreign-key filters and rollups — the
  association filters (D-QRY-5) and rollup properties (D-ACT-4) inside
  the algebra.
- The other session's `graph-flow-lance` (rs-graph-llm branch
  `claude/graph-flow-lance`) keeps only a generation-checked handle in a
  graph-flow `Context`; a LangGraph `Send` is an axis over a mask set,
  and the model sees only the materialized export. HubSPO-rs workflows
  (D-FLOW-*) and copilot surfaces (D-INT-7) adopt that seam instead of a
  second one.

**Outside (designed here; nothing exists yet).** The log record body is
the frame the UI already receives: `ogar-a2ui-frame::NodeDelta { key,
mask_words, values }` — the changed fields as a `WideFieldMask`, their
LE bytes, nothing else. One wire for screens, integrations, z8run flows
and webhooks. A batch header `(topic, base_offset, record_count)` wraps
it. The payload is RBAC-projected per consumer before framing (mask AND
with the consumer's role mask), so an unauthorised field is absent, never
filtered client-side. A real Kafka cluster is an optional edge: the
action daemon's `Transport` trait already reserves a `rdkafka` edge
(`graph-flow-action-ogar/src/daemon.rs`), and the same records can be
produced to a topic. Zone 3 only (`hubspo-server`, `hubspo-compat`).

**Conditions.**
1. **One copy at the report seam.** `lance-graph-report::AbiBatch` owns
   its lanes as `Arc<[i32]>` / `Arc<[u32]>` / `Arc<[u64]>`
   (`batch.rs:26`), so publishing a Lance version into it copies each lane
   once. graph-flow-lance and z8run-lance inherit this. Direct Quack /
   mask-risc calls do not. Upstream ask: a borrowed or Arrow-buffer-backed
   `LaneData` variant (STOP rule — not built here).
2. **Generation width.** `AbiBatch::generation` is `u32`; `LanceVersion`
   is `u64`. Mapping a version to a generation must fail closed past
   `u32::MAX`, or the seam widens upstream.
3. **The realtime watcher is always-latest.** `LanceVersionWatcher` skips
   stale revisions (`version_watcher.rs` header), so it is a wake-up,
   not the log. Consumers catch up by reading the version range from
   their stored offset.
4. **Change records come from the data, not the audit witness.** The
   audit record is examined in place and never egresses (MedCare-rs
   commitment #7, lance-graph doctrine). Which rows and fields changed
   between two versions must be derivable from the datasets themselves.
   Probe P-LOG-1 measures whether Lance's row-version tracking gives that
   directly; the fallback is diffing the touched fragments.
5. **Lane types are `i32`, `u32`, `u64`.** Money is two `i32` lanes (whole
   units, minor units), summed separately and combined; timestamps are
   `i32` seconds against a fixed epoch or a day + second pair; strings are
   `u32` dictionary ids, hydrated last.

New deliverables: D-LOG-1 log record + batch header encoding (zone 3);
D-LOG-2 consumer offsets and replay over version ranges; D-LOG-3
optional `rdkafka` edge; probe P-LOG-1. Upstream asks for conditions 1
and 2 are tracked in `.claude/board/ISSUES.md`.

---

## §4 Target crate layout

The six current crates are replaced (they do not compile and model the
forbidden shapes). Proposed workspace, all edition 2024:

| Crate | Zone | Holds | Depends on |
|---|---|---|---|
| `hubspo-port` | 2 | `HUBSPO_PLUG: HotPlug`, `RBAC_PLUG`, `class_id(name)` lookups over `HubSpoPort` — **no codebook** | `ogar-vocab`, `lance-graph-contract` |
| `hubspo-schema` | 2 | ClassView skins per object type, standard property position tables (generated), tenant custom-schema rows, lane layouts | `hubspo-port`, `ogar-class-view` |
| `hubspo-generated` | 2 | output of the checked-in generator (`@generated` headers, never hand-edited) | `lance-graph-contract` only |
| `hubspo-store` | 2 | write lanes, batch image, publish-one-version, KV values, edge lanes, list masks | `lance-graph`, `lance-graph-contract`, `lancedb` |
| `hubspo-query` | 2 | Quack programs for filters / lists / views, report plans, Cypher-quack reads | `lance-graph-quack`, `-report`, `-cypher-quack` |
| `hubspo-actions` | 2 | `ActionDef` tables (`crm_actions`), executors, pipeline guards, calculated properties | `ogar-vocab`, `graph-flow-action-ogar` |
| `hubspo-flow` | 2 | workflows, sequences, SLA timers, enrolment triggers (graph-flow graphs, ractor actors) | `graph-flow`, `lance-graph-supervisor` |
| `hubspo-sim` | 2 | crm-sim: versions, rules, invariants, diff, plan (H-ADR-10) | `lance-graph-dir-sim` patterns, `ogar-dir-sim` |
| `hubspo-iam` | 2 | role catalogue, grant source, field masks, directory actors | `ogar-rbac`, `lance-graph-rbac`, dir-sim |
| `hubspo-intel` | 2 | dedup, lookalike, scoring, suggestions, enrichment ingest | `ndarray` (`simd`, `hpc`), `lance-graph-arm-discovery`, `deepnsm-v2` |
| `hubspo-comms` | 3 | stalwart JMAP / webhook / MTA-hook adapters, tracking endpoints; spear wiring | `spear` (features `iam`, `mail-graph`) |
| `hubspo-docs` | 2/3 | files, attachments, document library, auto-association | `tesseract-paperless` (`store`, `search`, `auto-match`) |
| `hubspo-compat` | 3 | HubSpot v3-shaped REST + importer + oracle client | everything above, serde |
| `hubspo-ui` | 3 | a2ui surfaces, askama templates, six-region frame, board / record / list / settings | `a2ui-server`, `ogar-render-askama` |
| `hubspo-server` | 3 | axum host on `0.0.0.0:$PORT`, callcenter membrane (PostgREST, realtime, JWT, RLS) | `lance-graph-callcenter` (`serve`, `auth`) |
| `hubspo-parity-fixtures` | test | value + structure oracle fixtures | — |
| `hubspo-harvest` | tool | OpenAPI / properties-API harvester → OGAR `ogar-from-schema` openapi front end input | — |
| `hubspo-guards` | test | Gate G0 as tests: forbidden dependencies (stalwart, surrealdb, datafusion, lettre), zone discipline, `id: String`, V1 mints; every member classified | — |

`hubspo-invariants` is folded into `hubspo-sim` (invariants are Quack
programs, not a separate type layer). `hubspo-models` disappears (there
are no hand-written model structs).

---

## §5 Data model detail

### 5.1 Record row

```
NodeRow (512 B)
  key   (16)  NodeGuid — classid(4: concept<<16 | 0x000B) + V3 12-byte facet
  edge  (16)  content-blind facet cascade (edge_codec_flavor per ClassView)
  value (480) ClassView-carved standard property lanes
              [owner_guid_ref][create_ts][modify_ts][lifecycle_stage_id]
              [pipeline_id][stage_id][amount_minor][currency_id]…  (per object type)
KV            (guid, position) → long / variable values (text, rich text, multi-select)
Extension     tenant custom properties (H-ADR-3)
Edges         per object-type pair: sorted (from_ord, to_ord, label_id) lanes
```

Strings are presentation metadata: enumeration values, owners, stages,
countries and currencies are **ids** into tenant dictionaries; the string
is hydrated last.

### 5.2 Property history

HubSpot shows per-property history ("value, source, timestamp, changed
by"). We do not store a history table: every change lands in a Lance
version, and history is `QueryReference::at(version)` reads over the
version log, filtered by the property's position. Change *source* and
*actor* are carried in the version's audit record
(`lance-graph-callcenter::unified_audit`) — the audit witness is examined
in place, never exported as JSON (MedCare-rs commitment #7).

Probe P-HIST-1 measures "history of one property on one record over N
versions" latency; if it fails the budget, a per-property change index
(positions × versions bitmap) is added — an index, not a second truth.

### 5.3 Timeline

The record timeline is spear's `activity` table (D-HUB-4) joined on
`correlation_id`, unioned with property-change events derived from the
version log and with engagement records (calls, meetings, notes, tasks)
associated to the record. One ordered feed; RBAC applied per row before
it reaches the wire.

### 5.4 Pipelines

- `crm_pipeline` row: object type, ordered stage ids, per-stage probability,
  per-stage required properties, `is_closed`, `outcome ∈ {won, lost,
  open}`.
- The stage move is the `MoveStage` ActionDef (H-ADR-7). The board view is
  a `lance-graph-report` pivot over `(pipeline, stage)` with folds for
  count / sum(amount) / weighted amount — a terminal view, not a query
  per column.

### 5.5 Multi-tenancy

- Tenant = HubSpot portal. Tenant id is part of the RLS context
  (`RlsContext`), part of every mailbox owner, and the scope root in
  `ogar-rbac`. Registries run `sealed` (fail closed); `fail_open` is only
  for tests.
- Physical layout: one Lance dataset per tenant × object type (blast-radius
  and deletion story: a tenant deletion is dropping datasets, GDPR-clean).
  Probe P-TEN-1 checks the per-dataset overhead at 10k tenants; fallback
  is a tenant column with partitioned fragments.

---

## §6 Waves

Waves are sequential; D-ids inside a wave may run in parallel on disjoint
files. Every wave ends with a **gate**: named tests green, measured
numbers recorded with the command that produced them. No wave starts
before the previous gate is green.

Size: S ≤ 1 day, M ≤ 3 days, L ≤ 1 week, XL > 1 week.

### Wave 0 — Make the repo honest (foundation)

| D-id | Deliverable | Size | Deps |
|---|---|---|---|
| D-W0-1 | Delete the non-compiling skeleton; new workspace per §4 with empty-but-compiling crates; `rust-toolchain.toml` 1.98.1; edition 2024 | S | — |
| D-W0-2 | `Cargo.toml` workspace deps: lance-graph-* / OGAR / ndarray / spear / a2ui-rs / graph-flow on `branch="main"`; lance family floated | S | D-W0-1 |
| D-W0-3 | CI workflow (test, clippy `--no-deps -D warnings`, fmt, debug-0, `config-v3.toml`) | S | D-W0-1 |
| D-W0-4 | `.claude/` board: `LATEST_STATE.md`, `STATUS_BOARD.md` (all D-ids), `ISSUES.md`, `EPIPHANIES.md` (admission gate per lance-graph closeout rule) | S | — |
| D-W0-5 | CLAUDE.md rewrite: iron rules from §3, read order, link to this plan | S | — |
| D-W0-6 | README rewrite (drops "JanusGraph/ClickHouse/Gremlin replacement" framing in favour of §1) | S | — |

**Gate G0:** `cargo check --workspace` and CI green on an empty
workspace; no `String` id, no `serde` outside zone-3 crates
(`grep`-located, then read).

### Wave 1 — Ontology and identity

| D-id | Deliverable | Size | Deps |
|---|---|---|---|
| D-ONT-1 | Harvest: `hubspo-harvest` pulls HubSpot's public OpenAPI specs + the properties/pipelines/association-label APIs of a sandbox portal → normalised schema JSON (zone 3, tool only) | M | W0 |
| D-ONT-2 | OGAR: `ogar-from-schema` OpenAPI front end (queued upstream, `lib.rs:48`) — **requested from OGAR, not built in HubSPO-rs** | L | D-ONT-1 |
| D-ONT-3 | Mint dossier: per candidate concept (H-ADR-2 table) the METHOD/STORAGE/STRUCTURE evidence + OGIT source + convergence argument; submit to the 5+3 codebook council | M | D-ONT-1 |
| D-ONT-4 | OGAR PR: `ConceptDomain::Crm` (`0x10XX`), approved concepts in `class_ids` + `CODEBOOK` + `ALL`, `HubSpoPort` (`0x000B`) + aliases, `crm_actions` domain table, layout doc row; lance-graph `ogar_codebook` mirror rows in the same arc | M | D-ONT-3 |
| D-ONT-5 | `hubspo-port`: `HotPlug` + `RbacPlug` consts; `activate()` test that reports zero drift; deliberately-wrong-plug test proving each drift arm can fire | S | D-ONT-4 |
| D-ONT-6 | `hubspo-schema`: ClassViews for contact, company, deal, ticket, lead, product, line item, quote, activity kinds; standard property position tables via the generator into `hubspo-generated` | L | D-ONT-4 |
| D-ONT-7 | Custom-object/-property design (H-ADR-3) + probe P-CUSTOM-1 (lane-per-property vs packed run; fold latency at 50 / 500 / 2,000 custom properties) | M | D-ONT-6 |

**Gate G1:** a sandbox portal's schema round-trips harvest → ClassView →
render of an empty record page; `activate` drift-free; every standard
HubSpot property of the five core objects has a position (count pinned
`== N`, not `>= N`).

### Wave 2 — Store, versions, history

| D-id | Deliverable | Size | Deps |
|---|---|---|---|
| D-STO-1 | `hubspo-store` write lane: batch image → one Lance version per publish; owner-stamped (`mailbox_owner`) | L | G1 |
| D-STO-2 | KV value store for variable-length properties; zero-copy lens reads | M | D-STO-1 |
| D-STO-3 | Edge lanes + association labels (H-ADR-4); cardinality invariants as Quack `GroupReduce` | L | D-STO-1 |
| D-STO-4 | Tenant layout + P-TEN-1 probe | M | D-STO-1 |
| D-STO-5 | Property history via version log + P-HIST-1 probe | M | D-STO-1 |
| D-STO-6 | Importer (value oracle, part 1): sandbox portal → crm-sim plan → commit; counts compared per object type | L | D-STO-3, W4-sim minimal |

**Gate G2:** import of a seeded sandbox (≥ 10k contacts, 2k companies,
3k deals, 5k engagements) reproduces record counts, association counts
per label and property values byte-for-byte (value oracle); allocation
of a list-count query independent of population size (measured).

### Wave 3 — Query, lists, views, reports

| D-id | Deliverable | Size | Deps |
|---|---|---|---|
| D-QRY-1 | Filter compiler: HubSpot filter groups (AND within group, OR across groups; operators EQ/NEQ/LT/…/HAS_PROPERTY/CONTAINS_TOKEN/IN/BETWEEN) → Quack program | L | G2 |
| D-QRY-2 | Lists: active (incremental on version delta) + static (stored mask); membership history | L | D-QRY-1 |
| D-QRY-3 | Saved views + board view (report pivot over pipeline × stage) | M | D-QRY-1 |
| D-QRY-4 | Quack lowering of callcenter's `ParsedQuery` (the upstream DataFusion `parsed_query_to_plan` is an unimplemented stub) | M | D-QRY-1 |
| D-QRY-5 | Association-traversing filters ("contacts whose company has an open deal"): Quack `Semijoin` over edge lanes for one hop; multi-hop needs cypher-quack shapes that do not exist yet → upstream request first | L | D-QRY-1, D-STO-3 |
| D-QRY-6 | Reports: single-object, cross-object, funnel, time-series, attribution skeleton on `lance-graph-report` | XL | D-QRY-1 |
| D-QRY-7 | Full-text search over records (Tantivy, reference-indexed, reconciled from the store — tesseract-paperless `search` pattern) | M | G2 |

**Gate G3:** list-membership parity with the sandbox for ≥ 30 list
definitions covering every operator (value oracle); each filter operator
has a can-fire and a can-stay-silent test; report folds match exported
HubSpot report CSVs.

### Wave 4 — Behaviour: actions, pipelines, crm-sim

| D-id | Deliverable | Size | Deps |
|---|---|---|---|
| D-ACT-1 | `crm_actions` ActionDef tables: create / update / delete / archive / restore / merge / associate / dissociate / assign owner / move stage | L | G1 |
| D-ACT-2 | Executor through `GatedOgarHandler`; `ActionInvocation` audit; idempotency keys | M | D-ACT-1 |
| D-ACT-3 | Pipeline guards + required-properties-per-stage (`Constrains`) | M | D-ACT-2 |
| D-ACT-4 | Calculated, rollup and formula properties (`Depends`/`Onchange` + report folds; formulas as loco programs) | L | D-ACT-2, D-QRY-6 |
| D-SIM-1 | `hubspo-sim`: snapshot + overlay versions, rules, invariant programs, diff, `ExecutionPlan` review payload | L | G2 |
| D-SIM-2 | Bulk edit, import, delete, merge all routed through D-SIM-1; revert = version restore | M | D-SIM-1 |
| D-SIM-3 | Dedup merge semantics (HubSpot rules: oldest record wins id, property conflict policy, association union, timeline union) | M | D-SIM-1, D-INT-1 |

**Gate G4:** every mutating API path goes through an ActionInvocation
(grep-located, read-verified); a disable run of the RBAC stage fails a
named test; merges reproduce HubSpot's merge outcome on 50 fixture pairs.

### Wave 5 — Identity, teams, permissions

| D-id | Deliverable | Size | Deps |
|---|---|---|---|
| D-IAM-1 | Role catalogue: Super Admin, Admin, Sales/Service/Marketing user, view-only, custom permission sets; grant source with `field_mask_of` | M | G1 |
| D-IAM-2 | Ownership scopes (all / team / owned) as `ScopeSpec`; team hierarchy via `Dn128` subtree | M | D-IAM-1 |
| D-IAM-3 | Field-level permissions (sensitive properties) as per-role `WideFieldMask`; absent from the wire | M | D-IAM-1 |
| D-IAM-4 | Directory ingest (ogar-ad / ogar-az / native), seat assignment as directory simulation plans | L | D-IAM-1 |
| D-IAM-5 | Partitioning (business units / brands) as tenant sub-scopes | M | D-IAM-2 |
| D-IAM-6 | Accumulation guard: exports / cohort reports require an extra grant | S | D-IAM-1 |
| D-IAM-7 | RLS as a Quack predicate injected before evaluation (tenant + scope); the callcenter `RlsRewriter` is DataFusion-only and frozen. Upstream into lance-graph if a second consumer needs it | M | D-IAM-2, D-QRY-1 |

**Gate G5:** permission matrix test (role × object × op × scope) with
fixture users; RLS registry runs sealed; a fail-open registry fails a test.

### Wave 6 — Communications (spear + stalwart)

| D-id | Deliverable | Size | Deps |
|---|---|---|---|
| D-MAIL-1 | stalwart deployment profile (separate service, AGPL stays there). stalwart keeps its own directory (internal store, or LDAP/OIDC) for delivery and login; the CRM joins contacts to mailboxes through spear's placement over the dir-sim version (`spear/src/mail/directory.rs`). No dir-sim backend in stalwart (OD-12) | M | G5 |
| D-MAIL-2 | Webhook receiver (HMAC verify) → spear `activity` rows for delivery / ingest events | M | spear D-HUB-4/5 |
| D-MAIL-3 | One-to-one email send from a record via JMAP `EmailSubmission`; `send_receipts` row | M | D-MAIL-2 |
| D-MAIL-4 | Inbound logging: MTA hook BCC capture + mailbox sync → associate to contacts by address → company / deal by association rules. Implements the contract's `MailParser` / `ThreadLinker` seams (`lance-graph-contract/src/mail.rs`, no impl in tree) | L | D-MAIL-2 |
| D-MAIL-5 | Shared inbox / conversations: new spear roles (team inbox member, inbox admin) + ownership routing | L | D-MAIL-4, D-IAM-2 |
| D-MAIL-6 | Meetings: CalDAV / JMAP Calendars, iTIP, booking pages (availability fold over calendars), meeting engagement records | XL | D-MAIL-1 |
| D-MAIL-7 | Contact sync (CardDAV / JMAP Contacts) — two-way, conflict policy via crm-sim | L | D-MAIL-1, D-SIM-1 |
| D-MAIL-8 | Templates, snippets, personalisation tokens (askama-compiled, validated against the ClassView) | M | D-ONT-6 |
| D-MAIL-9 | Open / click tracking endpoints + bot/MPP heuristic flag | M | D-MAIL-3 |
| D-MAIL-10 | Calls: telephony is out of scope for v1 beyond call *logging* engagements; spear D-TELEPHONY reuse later | S | D-ACT-1 |

**Gate G6:** end-to-end: send from a record → delivered event → timeline
row → reply arrives → logged and associated, all in one test against a
containerised stalwart; no stalwart crate in `cargo tree`.

### Wave 7 — Automation: workflows, sequences, SLAs

| D-id | Deliverable | Size | Deps |
|---|---|---|---|
| D-FLOW-1 | Workflow model: trigger (criteria / event / schedule / webhook), enrolment, re-enrolment, actions, branches (if/then, value equals, A/B), delays (fixed, until date, until event) as graph-flow graphs | XL | G4 |
| D-FLOW-2 | Trigger actor tailing `LanceVersionWatcher`; enrolment = mask delta ∩ criteria | L | D-FLOW-1, D-QRY-2 |
| D-FLOW-3 | Action library: set property, create record/task, send email, add to list, webhook, rotate owner, delay, branch, custom code (loco program) | L | D-FLOW-1 |
| D-FLOW-4 | Sequences: steps (email / task / call), stalwart future-release for scheduled sends, unenrol on reply/meeting booked (MTA hook / webhook) | L | D-FLOW-1, G6 |
| D-FLOW-5 | SLAs + ticket routing (round robin, skills) as timers + rules; reuse spear D-ROUTE patterns | M | D-FLOW-1 |
| D-FLOW-6 | Workflow test mode = crm-sim dry run over a sample population | M | D-FLOW-1, D-SIM-1 |
| D-FLOW-7 | Ops-hub-style data sync connectors as flows (out of v1 scope beyond webhooks) | — | — |

**Gate G7:** 20 fixture workflows produce identical enrolment and action
logs as the sandbox portal over a scripted event tape (value oracle);
every delay survives an actor restart (supervisor test).

### Wave 8 — UI

| D-id | Deliverable | Size | Deps |
|---|---|---|---|
| D-UI-1 | Six-region frame; record page (about card / timeline / association cards) as ClassView projections | L | G3, G5 |
| D-UI-2 | Index pages (table + board) with saved views; inline edit = ActionInvoke | L | D-UI-1, D-QRY-3 |
| D-UI-3 | Settings surfaces: properties, pipelines, association labels, users/teams, permission sets | L | D-UI-1 |
| D-UI-4 | List builder and workflow canvas (a2ui skins over the same projections) | XL | D-UI-1, D-FLOW-1 |
| D-UI-5 | Structure oracle: Klickwege graph of HubSpot UI vs ours; parity test | M | D-UI-1 |
| D-UI-6 | Realtime: `version_watcher` → WebSocket → `NodeDelta`. The callcenter Phoenix server is commented out upstream, so the WS endpoint lives in `hubspo-server` (axum) until lance-graph ships one | M | D-UI-1 |

**Gate G8:** structure oracle reachability parity for the core object
screens; an unauthorised field verified absent from the `NodeDelta`
bytes (not merely hidden).

### Wave 9 — Intelligence

| D-id | Deliverable | Size | Deps |
|---|---|---|---|
| D-INT-1 | Duplicate detection: blocking via `cam_index` / `cascade` over record fingerprints (normalised name + domain + email + phone shingles), exact re-rank, `clam_search` for radius clusters; candidates → crm-sim merge plan, never auto-merge | L | G2 |
| D-INT-2 | Lookalike companies / contacts: CAM-PQ semantic codec trained on firmographic + engagement features; `top_k` | L | D-INT-1 |
| D-INT-3 | Lead / deal scoring: logistic model via BLAS L2 + activations; training via ndarray; calibration and rank fidelity reported with `reliability.rs` (Spearman vs exact) | L | G3 |
| D-INT-4 | AUTO suggestions (associate email → deal, set lifecycle stage, assign owner) via `lance-graph-arm-discovery`, lift-filtered, with `NarsTruth`; suggestions, never assignments (tesseract-paperless `auto_match` pattern) | M | G4 |
| D-INT-5 | Enrichment: spider crawl of company domain → `spider_doc_ir` → intake → facts proposed as a crm-sim plan with provenance | L | G4 |
| D-INT-6 | Text understanding of emails/notes: deepnsm-v2 SPO extraction + sentence beliefs (measured coverage on real CRM text before any default) | M | G6 |
| D-INT-7 | Copilot / agent surfaces: compiled thinking templates (elixir-template / template-runtime); LLM runs compile into templates, never the reverse | XL | D-INT-4 |

**Gate G9:** dedup precision/recall on a labelled fixture of ≥ 1,000
pairs; lookalike rank fidelity Spearman vs exact cosine recorded; every
suggestion carries truth + evidence; zero direct writes from D-INT-*.

### Wave 10 — Marketing and Service breadth

| D-id | Deliverable | Size | Deps |
|---|---|---|---|
| D-MKT-1 | Forms (fields, dependent fields, progressive profiling, hidden fields, consent) → submissions create/update contacts through ActionDefs | L | G4 |
| D-MKT-2 | Marketing email: drag-and-drop doc IR, send to lists via stalwart bulk submission, subscription types, unsubscribe, bounce handling | XL | G6, D-QRY-2 |
| D-MKT-3 | Campaigns + attribution (first/last/linear/U/W/time-decay as report folds over touch points) | L | D-QRY-6 |
| D-MKT-4 | Landing pages / CMS: askama/typst render of doc IR; out of v1 beyond forms + email | — | — |
| D-SVC-1 | Tickets, help desk inbox, SLAs (from W7), feedback surveys (NPS/CSAT/CES) | L | G7 |
| D-SVC-2 | Knowledge base (doc IR + search) and customer portal (RLS-scoped to the contact's company) | L | D-SVC-1 |
| D-COM-1 | Quotes, products, line items, invoices, payments on the commerce concepts (convergence with odoo-rs) | L | G4 |

**Gate G10:** feature-matrix rows for these hubs move to their target
phase; each new surface passes both oracles for its scope.

### Wave 11 — Compatibility API and migration

| D-id | Deliverable | Size | Deps |
|---|---|---|---|
| D-API-1 | `/crm/v3/objects/*` CRUD + batch + search on the façade; association v4; properties; pipelines; owners | XL | G4 |
| D-API-2 | Webhook subscriptions (outbound) from version deltas | M | D-API-1 |
| D-API-3 | OAuth apps / private app tokens mapped to RBAC actors with delegation | M | D-API-1, G5 |
| D-API-4 | Migration runbook: shadow mode (read-only mirror of a live portal, nightly diff), then cut-over | L | D-API-1, D-STO-6 |
| D-API-5 | Contract tests: replay recorded HubSpot API traffic against the façade | L | D-API-1 |

**Gate G11:** a recorded integration (e.g. a sync tool's traffic) replays
with identical responses modulo ids/timestamps.

---

## §7 Probes (measure before building)

| Probe | Question | Pass criterion (pre-registered) |
|---|---|---|
| P-CUSTOM-1 | custom property layout | fold over 500 custom properties × 1M rows within 2× of a standard property fold |
| P-TEN-1 | dataset per tenant×type overhead | open + first read at 10k tenants < 50 ms p99 |
| P-HIST-1 | property history from version log | one property, one record, 1,000 versions < 20 ms |
| P-LIST-1 | active list incremental re-eval | delta of 1k changed rows on 10M population < 10 ms; allocation independent of population |
| P-DEDUP-1 | blocking recall | ≥ 99% of labelled duplicates share a block |
| P-FLOW-1 | enrolment latency | version publish → enrolled < 1 s p99 at 1k workflows |
| P-AUTH-1 | RBAC overhead on reads | < 10% vs unauthorised scan |
| P-LOG-1 | changed rows and fields between two Lance versions, from the datasets alone | derivable without the audit record; cost proportional to changed rows, not dataset size |

Thresholds are policy pins; each is replaced by a measured value with
its command in `.claude/board/` when run. A probe that fails stops the
dependent D-ids; the response is a design change, not a threshold change.

---

## §8 Test discipline

- Falsifiability rule (lance-graph `CLAUDE.md`): every guard has a
  can-fire **and** a can-stay-silent test on non-trivial input; every
  threshold has an inertness test; prefer `== N`.
- Disable runs: commit first, then disable the guard, see red, restore
  (ruff AGENTS.md trap: `git checkout` reverts uncommitted work).
- Fixtures are generated in Rust, committed; no `/tmp` fixtures; external
  sandbox exports are fetched by a hash-free manifest into a gitignored
  cache and never become committed tests (tesseract-rs bench rule).
- Oracles: value (sandbox export) and structure (Klickwege) on every
  surface.
- Contact data is PII: fixtures are synthetic; no real portal data
  committed; logs never print property values.

---

## §9 Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| Concept mints rejected or slow at the OGAR council | M | converge first (H-ADR-2 aliases); W1 can proceed on aliased objects while CRM concepts wait |
| Custom objects at HubSpot scale (many tenants × many schemas) strain the classid model | M | H-ADR-3 keeps them as data; probe P-CUSTOM-1 |
| AGPL contamination via stalwart | L if H-ADR-8 kept | `cargo deny` license check in CI; `cargo tree` gate in G6 |
| Workflow engine depends on loco orchestration that "nothing implements" | H | v1 uses graph-flow directly; loco only for formula/custom-code programs |
| DataFusion grace period ends while callcenter PostgREST still lowers through it | M | D-QRY-4 first |
| dir-sim limit of 65,536 nodes per population | M for users (fine), H if reused for records | crm-sim uses its own population sizing; only borrows the pattern |
| Over-reach: 160 features | H | matrix marks v1 / v2 / later; waves 0–8 are the CRM core |
| Stale docs (recurring defect in odoo-rs, surrealdb, spear) | H | methods in CLAUDE.md, measurements dated in board, plan status lines updated per wave |

---

## §10 Decisions

**DECISION (2026-10-10):** the operator approved every recommendation
below. Cross-repo follow-ups are tracked in `.claude/board/ISSUES.md`.

| ID | Question | Recommendation (approved) |
|---|---|---|
| OD-1 | `HubSpoPort` app prefix `0x000B`? | yes (next free after Spear) |
| OD-2 | New `ConceptDomain::Crm` at `0x10XX` vs folding into Commerce `0x02XX` | new domain; CRM is not commerce |
| OD-3 | Custom objects as data (H-ADR-3) vs per-tenant concepts | data |
| OD-4 | crm-sim: generalise `ogar-dir-sim` meaning types vs HubSPO-rs-local types | local first, upstream when a second consumer appears |
| OD-5 | Re-home spear D-HUB-1/2/3/7 into HubSPO-rs (H-ADR-12) | yes |
| OD-6 | stalwart out of process (AGPL stays there) | yes |
| OD-7 | spear's "DataFusion `Expr` only" rule vs the 2026-09-05 grace-period ruling | update spear's rule to "Quack for new code" |
| OD-8 | License of HubSPO-rs (currently `MIT OR Apache-2.0`) | keep, given OD-6 |
| OD-9 | Edition 2024 for HubSPO-rs | yes |
| OD-10 | Which HubSpot sandbox portal is the value oracle, and who holds its token | operator |
| OD-11 | lance-graph `ogar-consumer-preflight.md` still whitelists `lance =9.0.0 / lancedb =0.33.0`; lance-graph `CLAUDE.md` floats `12.*` / `0.39.*` | follow `CLAUDE.md`; fix the preflight upstream |
| OD-12 | How stalwart gets users and teams (`ISS-STALWART-DIRSIM-WIRING`): (a) build the dir-sim backend outside stalwart's config model, (b) dir-sim persistence + stalwart's schema generator first, (c) stalwart keeps its own directory and the CRM join key comes from spear's placement | (c) — approved 2026-10-10 |

---

## §11 Status

- Wave 0 (D-W0-1..6): **Shipped**, 2026-10-10 (#2). Gate G0 evidence in
  `.claude/board/LATEST_STATE.md`.
- D-ONT-1 (public-catalog half) and D-ONT-3 (dossier): **In PR**.
- Every other D-id: **Queued**.

This file is updated (status lines + measured numbers with their commands)
at every gate.

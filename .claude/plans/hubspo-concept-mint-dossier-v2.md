# HubSPO-rs concept mint dossier v2 (D-ONT-3, council draft)

> ⊘ **Superseded by v3** (ratified 2026-10-10). Kept as the draft the
> three adversarial reviewers attacked.

> **Supersedes:** `hubspo-concept-mint-dossier-v1.md` (kept as the input the
> council reviewed).
> **Status:** COUNCIL DRAFT v2, 2026-10-10. Five research agents reviewed v1
> (shipped code, canon consistency, convergence, every number, external
> sources). This draft folds their findings in. Three adversarial reviewers
> attack it next; their fixes produce v3, which is what D-ONT-4 lands.
> **Approved context (operator, 2026-10-10):** port `HubSpoPort` at app
> prefix `0x000B`; concept domain `ConceptDomain::Crm` at **`0x05XX`**
> (OD-2, byte amended from `0x10`, see §1.2); custom objects and properties
> are tenant data (plan H-ADR-3).

Evidence grades: `[G]` shipped code, a measurement with its command, or a
primary document; `[H]` plausible, with the oracle that would settle it;
`[S]` analogy only.

---

## 1. What changed from v1

### 1.1 Verdict changes

| Item | v1 | v2 | Why (grade) |
|---|---|---|---|
| `crm_activity` (abstract parent, Q1) | MINT | **dropped** | No screen of its own: it fails STRUCTURE. The furnace playbook refuses a two-of-three concept (`user_right` precedent, ruff `consumer-transcode-furnace-playbook.md:117-127`); a timeline is a union of kinds, i.e. a mask, not a concept. OGIT `Schedule/Activity` is a schedule item, not a timeline parent. [G] |
| `notes` (Q4) | ALIAS → `PROJECT_COMMENT` | **MINT `crm_note`** | `project_comment` has one author edge and one parent (`ogar-vocab/src/lib.rs:3767-3780`); a HubSpot note is an engagement with an owner and a timestamp, associated to many records at once, shown on every one of their timelines. Different mechanism. Aliasing would also put CRM notes inside ProjectMgmt's domain-scoped RBAC coverage (`concepts_in_domain`, `lib.rs:1806-1832`). HubSpot lists notes among its seven activity objects (`0-46`). [G] code / [H] multi-association in practice |
| `communications`, `postal_mail` | instances of `crm_activity` | **PARK** | With no parent they need their own home; no v1 deliverable authorizes or routes on them yet. |
| teams | ALIAS → `HR_DEPARTMENT` | **ALIAS → `PROJECT_ACTOR`** | `HR_DEPARTMENT` is an employment unit with `manager_ref`/`parent_ref` and employee members (`lib.rs:4900-4913`). HubSpot team members are users (already `PROJECT_ACTOR`, Q5), a user has one primary and several secondary teams, and the Teams API exposes no hierarchy (9 ops, none parent/child). OpenProject `Group` already folds into `PROJECT_ACTOR` (`ports.rs:165-167`). [G] code / [H] secondary teams |
| `taxes` (CRM object) | ALIAS → `TAX_POLICY` | **PARK** with discounts and fees | The `taxes` object is a tax line attached to a quote (`crm-taxes@2026-03.json:215`), the same kind of record as discounts and fees. The tax-rate library (`settings-tax-rates@2026-03`) is the `TAX_POLICY` analogue and takes the alias instead. [G] |
| `crm_lead` rationale | "must associate to a contact" | "associates to a contact and/or company; lives in the prospecting workspace behind a Sales seat" | The contact requirement is not supported by any primary source found. [H] |
| `crm_pipeline_stage` METHOD citation | Pipelines API | **Pipeline Governance API** (`stage-edit-permissions`, 2026-09 `LATEST`) | That API carries the per-stage permissions. Per-stage `permissionMode` OPEN/RESTRICTED with `userIds`/`teamIds` is documented (developers.hubspot.com pipelines rules guide). [G] |

### 1.2 Domain byte: `0x05`, not `0x10`

Found by the council, decided by the operator on 2026-10-10:

- `0x10` sits outside the business/reference band `0x00`–`0x0F`. The
  operator's altitude ruling (2026-08-18; `ogar-vocab/src/lib.rs:1440-1465`,
  `docs/DISCOVERY-MAP.md` D-CBAND-ALTITUDE) makes the top nibble an
  altitude selector, with `0x17` the orchestration tier. [G]
- `0x1000`, the root of domain `0x10`, is read as the V3 marker by lance-graph
  `classid_canon_compat` and `classid_scan`. [G]
- Legacy ids of the form `0x1000_DDCC` route to `Unassigned` today through
  `classid_concept_domain`; with a `0x10 => Crm` arm they would route to CRM. [G]
- `0x05` is unassigned in OGAR and the lance-graph mirror, reserved by no
  ruling, and used as a concept domain by no consumer (closed search over
  OGAR, lance-graph, MedCare-rs, q2, odoo-rs, spear, a2ui-rs, tesseract-rs,
  openproject-nexgen-rs, openstreetmap-website-rs, stalwart, rs-graph-llm). [G]
- Not a conflict: the recipe-family bytes `0x05`/`0x06` (Scope/Concern,
  reserved in `recipe.rs`) are a different numbering, and the Healthcare app
  prefix `0x0005` sits in the other classid half.

### 1.3 Corrections to v1's evidence

- Forecasts are not beta-only: `crm-forecast-submissions` (2026-09 `LATEST`)
  was selected and read. [G]
- PARK verbs: `subscriptions` also has `read_associations`; `contracts` is
  list, read and batch_read. [G]
- "Properties are an enum of 7 types" is the create schema; the read schema
  has 11. [G]
- OGIT sources: `CustomerSupport/User` is a support-desk actor and
  `CustomerSupport/Organization` a helpdesk tenant; neither is a CRM contact
  or company. `Advertising/Campaign` is ad buying. There is no OGIT Person,
  Contact, Lead, Opportunity, Note or Pipeline entity (closed filename search
  of `OGAR/vocab/imports/ogit/NTO/`). Only `CustomerSupport/Ticket` is a real
  match. No OGIT entity is marked Cross-walked by this mint (ownership-gated,
  `OGIT-DOMAIN-LIFT-CATALOGUE.md:77-87`). [G]
- HubSpot's own type ids (developers.hubspot.com, understanding the CRM)
  name every object: contacts `0-1`, companies `0-2`, deals `0-3`, tickets
  `0-5`, leads `0-136`, calls `0-48`, meetings `0-47`, notes `0-46`, tasks
  `0-27`, users `0-115`. [G]
- **Council rule:** a `LATEST` release counts as committed METHOD evidence.
  The harvester already selects it as the fallback when no `STABLE` release
  exists; beta and preview never count. Pipeline Governance and Teams rest on
  `LATEST`.

All other v1 numbers were re-measured against `.cache/harvest.json` and
match (123 / 107 / 0 missing / 1,198 / 472 / 726 / 35 types; per-API op
counts 14, 8, 30, 8, 24, 4, 9, 4; every MINT verb claim).

## 2. Verdicts

### 2.1 MINT — `ConceptDomain::Crm`, `0x05XX`

`0x0500` is the domain root and never a concept (slot `CC == 0x00` is
forbidden by the zero-slot test). Slots follow promotion order. Every name
keeps the `crm_` prefix: OGAR's alias normalizer maps bare words such as
`status`, `type`, `query`, `group` and `board` to ProjectMgmt concepts
(`lib.rs:2875-3039`).

| Hex | Concept | HubSpot (type id) | METHOD | STORAGE | STRUCTURE | Why not an alias |
|---|---|---|---|---|---|---|
| `0x0501` | `crm_contact` | contacts `0-1` | full + merge + gdpr_delete [G] | own collection [G] | index + record [H] | `BILLING_PARTY` (`0x0204`) is a **role in a sale** ("counterparty", `lib.rs:4104-4119`, target of the commerce `party` edge). Most contacts are prospects never billed. HubSpot keeps contacts and companies behind separate OAuth scopes (`crm.objects.contacts.*` vs `crm.objects.companies.*`), so they need separate grants. WoA, SMB and Odoo converge "customer" on `BILLING_PARTY`; that stays right for them, because their record *is* the billed party. |
| `0x0502` | `crm_company` | companies `0-2` | full + merge [G] | own collection [G] | index + record [H] | as above |
| `0x0503` | `crm_deal` | deals `0-3` | full + merge [G] | own collection [G] | index, board, record [H] | No concept is a revenue opportunity moving through stages; `COMMERCIAL_DOCUMENT` is what a deal produces. |
| `0x0504` | `crm_ticket` | tickets `0-5` | full + merge [G] | own collection [G] | index, board, record [H] | `PROJECT_WORK_ITEM` is defined by its project edge and journals (`lib.rs:3496-3506`); a ticket has no project and is authorized by owner, team and associations. OGIT `CustomerSupport/Ticket` matches. |
| `0x0505` | `crm_lead` | leads `0-136` | full [G] | own collection [G] | prospecting workspace + record [H] | A qualification record associated to a contact and/or company; not a contact lifecycle stage. |
| `0x0506` | `crm_pipeline` | Pipelines API (14 ops) | 14 ops + governance [G] | per object type [G] | pipeline settings, board [H] | `PROJECT_TYPE` carries no ordered stage set bound to an object type. |
| `0x0507` | `crm_pipeline_stage` | stages | Pipeline Governance `stage-edit-permissions` [G] | child of one pipeline [G] | board columns [H] | Per-stage permissions are the demand rule met directly. `PROJECT_STATUS` is a global lookup (`lib.rs:3602-3614`); a stage belongs to exactly one pipeline. |
| `0x0508` | `crm_call` | calls `0-48` | full [G] | own collection [G] | calls index + record [H] | own fields (duration, disposition, recording) |
| `0x0509` | `crm_meeting` | meetings `0-47` | full [G] | own collection [G] | meetings index + record [H] | An engagement record. A calendar event is its backing, not the same concept (see §4 O-3). |
| `0x050A` | `crm_task` | tasks `0-27` | full [G] | own collection [G] | tasks index, task queues [H] | Same reason as tickets: no project semantics (Q2). |
| `0x050B` | `crm_note` | notes `0-46` | full [G] | own collection [G] | record timelines [H] | See §1.1. |
| `0x050C` | `crm_list` | Lists API (30 ops; "segments" in the UI) | 30 ops [G] | own records [G] | lists index + list page [H] | A population with membership, static or active; `PROJECT_QUERY` (`0x010D`) is a saved query with no membership. In our substrate a list is a mask, and still a record people own and authorize on. |
| `0x050D` | `crm_campaign` | Campaigns API (24 ops) | 24 ops [G] | own records [G] | campaigns index + page [H] | the marketing attribution root |
| `0x050E` | `crm_sequence` | Sequences API (4 ops) | list, read, enroll, enrollments [G]; no authoring ops in the public API | own records [G] | sequences index + editor [H] | Shared, permissioned records: enrollment fails unless the sequence is shared with the sender (HubSpot KB) [G]. Behaviour lives in `ActionDef`s, the address only names the container, as for `knowledge_item` `0x0C05` (Q3). |
| `0x050F` | `crm_custom_record` | every custom object (`2-XXX`; Schemas API, 8 ops) | 8 ops [G] | tenant-defined [G] | per-schema index + record [H] | H-ADR-3: one concept; each tenant's schemas are data and ClassViews. |

Every STRUCTURE cell stays `[H]` until the Klickwege oracle (plan D-UI-5)
runs; no primary document describes HubSpot's index and record pages.

### 2.2 ALIAS — onto existing concepts

| HubSpot | Concept | Hex | Basis |
|---|---|---|---|
| `emails` (`0-49`) | `EMAIL` | `0x0B05` | the message spear stores (`SpearPort` "Email"); one identity across HubSPO-rs and spear, if the engagement carries the RFC Message-ID (§4 O-2) |
| `products` (`0-7`) | `PRODUCT` | `0x0207` | converges with Odoo `product.*` |
| `line_items` (`0-8`) | `COMMERCIAL_LINE_ITEM` | `0x0201` | converges with Odoo `sale.order.line` |
| `quotes`, `invoices`, `orders`, `carts` | `COMMERCIAL_DOCUMENT` | `0x0202` | many models, one document concept (Odoo `account.move` / `sale.order` pattern; a cart is a draft order) |
| `commerce_payments` | `PAYMENT_RECORD` | `0x0205` | converges with Odoo `account.payment` |
| tax-rate library (`/tax-rates`) | `TAX_POLICY` | `0x0203` | converges with Odoo `account.tax` |
| `projects` (`0-970`) | `PROJECT` | `0x0101` | a project container; HubSpot describes it as centralised team work. [H] |
| users (`0-115`), owners | `PROJECT_ACTOR` | `0x0104` | the IAM actor: `auth_store` maps `sub` → `0x0104` (`lib.rs:1339`; `ogar-auth` `user.rs:19,66`). HubSpot users cannot be associated to records, unlike every CRM object. Refines plan H-ADR-2 (which said `HR_EMPLOYEE`). |
| teams | `PROJECT_ACTOR` | `0x0104` | the Group fold (§1.1) |

**Commerce `party` edge.** `COMMERCIAL_DOCUMENT` and `PAYMENT_RECORD` point
their `party` edge at `BILLING_PARTY` (`lib.rs:4132`). A HubSpot invoice
associates to contacts and companies. The rule: `BILLING_PARTY` stays the
role, `crm_contact` / `crm_company` stay the identity, and an aliased HubSpot
commerce document carries a party edge only when a billing party is resolved
for it. How a contact or company becomes a billing party is decided at
D-COM-1, with a test that a HubSpot invoice's party resolves to a classid the
edge accepts. Nothing in this mint changes the commerce edges.

### 2.3 DATA — never a concept

| Item | Where it lives |
|---|---|
| properties and property groups | ClassView positions + tenant schema rows (create schema: 7 types, 12 field types; read schema: 11 types) |
| association labels | tenant data on the relation |
| business units | a tenant sub-scope (H-ADR-9) |
| property validation rules (4 ops) | `Constrains` specs |
| pipeline governance rules | `ActionDef` guards on `crm_pipeline_stage` |
| saved index views | masks over a ClassView; whether they converge on `PROJECT_QUERY` is §4 O-5 |

### 2.4 PARK — no v1 consumer yet

| HubSpot objects | Revisit at |
|---|---|
| `communications` (`0-18`), `postal_mail` (`0-116`) | the comms wave, when a deliverable routes or authorizes on them |
| `discounts`, `fees`, `taxes` (quote adjustment lines) | D-COM-1; likely one adjustment concept or an alias onto `COMMERCIAL_LINE_ITEM` |
| `subscriptions` (full + merge + gdpr_delete + read_associations), `contracts` (list, read, batch_read) | D-COM-1 |
| `goal_targets` | D-QRY-6 |
| `feedback_submissions` (read-only) | D-SVC-1 |
| `partner_clients`, `partner_services` | not planned |
| `0-162` Services, `0-410` Courses, `0-420` Listings | when a vertical needs them; until then `crm_custom_record` |

### 2.5 DEFER — no committed contract

Workflows (Automation V4), Forms, Lead Scoring and Reporting exist only as
beta or preview. Workflows are decided at D-FLOW-1, forms at D-MKT-1 (OGIT
`Forms/*` exists), lead scoring is a computed property.

### 2.6 Accounting

All 35 harvested object-type names are placed:

- MINT: contacts, companies, `0-3`, tickets, leads, calls, meetings, tasks,
  notes (9);
- ALIAS: emails, products, line_items, quotes, invoices, orders, carts,
  commerce_payments, projects, users (10);
- PARK: communications, postal_mail, discounts, fees, taxes, subscriptions,
  contracts, goal_targets, feedback_submissions, partner_clients,
  partner_services, `0-162`, `0-410`, `0-420`, services, courses (16).

9 + 10 + 16 = 35. The other six MINT concepts (pipeline, stage, list,
campaign, sequence, custom record) and the three non-object aliases (owners,
teams, tax rates) come from APIs outside `/objects/`.

## 3. Landing checklist (D-ONT-4, one arc across OGAR and lance-graph)

**OGAR**
- `crates/ogar-vocab/src/lib.rs`: `ConceptDomain::Crm` with doc; the
  `canonical_concept_domain` arm `0x05`; the layout table and the
  `Unassigned` doc; `source_domain_concept` `"crm"`; `CODEBOOK` rows with a
  section comment; `class_ids` constants and `ALL`; one
  `pub fn crm_*() -> Class` per concept; tests: re-pin
  `canonical_concept_domain(0x0500)` (`:6055`) and the email test's
  "no Mail domain" assert on `0x0501` (`:7245`), add the
  `concepts_in_domain(Crm)` count.
- `crates/ogar-vocab/src/ports.rs`: `HubSpoPort` (`0x000B`) and
  `HUBSPO_ALIASES`; prefix tests; add WeatherNext, Spear and HubSpo to the
  collision list and the `0x1000` guard (both lists already omit the first
  two).
- `crates/ogar-vocab/src/capability_registry.rs` + a new `crm_actions.rs`:
  `domain_tables()` row; hot-plug test.
- `crates/ogar-class-view/src/lib.rs`: `all_canonical_classes()` rows (the
  bijection tests require them).
- Docs: `APP-CLASS-CODEBOOK-LAYOUT.md` domain list and a `0x000B` row in §2;
  `DISCOVERY-MAP.md` a new `D-CRM-DOMAIN` entry.

**lance-graph** (`crates/lance-graph-contract/src/ogar_codebook.rs`)
- `ConceptDomain::Crm` (same variant name: the parity test compares Debug
  strings), its arm, `CODEBOOK` rows, the `0x0500` re-pin (`:881`).
- `AppPrefix::HubSpo` (`0x000B`). The mirror stops at `0x0007` today; add
  `Osm` `0x0008`, `WeatherNext` `0x0009` and `Spear` `0x000A` in the same
  change so the table has no hole, and re-pin the test that asserts
  `from_prefix(0x0008) == None`.
- `lance-graph-ogar` parity tests green against the OGAR branch.

**HubSPO-rs** (D-ONT-5): `HotPlug` / `RbacPlug` declarations, an `activate`
test with zero drift, and a deliberately wrong plug per drift arm.

## 4. Open after this mint

- **O-1** Sandbox checks (needs `ISS-ORACLE-PORTAL`): do notes carry two or
  more associations; do users have secondary teams.
- **O-2** Do logged email engagements carry the RFC Message-ID? The `EMAIL`
  alias depends on it.
- **O-3** A shared calendar-event concept has three candidate consumers:
  spear's events (iCalendar UID), Odoo `calendar.event`, and the event behind
  a HubSpot meeting. It belongs in its own proposal, not in the CRM domain.
- **O-4** Odoo `crm.lead` holds lead and opportunity in one record. Which
  of `crm_lead` / `crm_deal` it aliases to is decided when Odoo's CRM is
  harvested (no Odoo CRM model is harvested yet); `mail.activity` is a
  prospective second consumer for `crm_task`.
- **O-5** Saved index views: masks only, or converge on `PROJECT_QUERY`.

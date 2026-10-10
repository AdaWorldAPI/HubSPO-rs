# HubSPO-rs concept mint dossier v1 (D-ONT-3)

> **For:** the OGAR 5+3 codebook council, before D-ONT-4 (the OGAR PR that
> adds `HubSpoPort`, `ConceptDomain::Crm` and the CRM concepts).
> **Status:** DRAFT 2026-10-10. Nothing here is minted; every row is a
> proposal with its evidence.
> **Approved context (2026-10-10):** port `HubSpoPort` at app prefix
> `0x000B`; a new concept domain at `0x05XX` (OD-2, byte amended from `0x10` on 2026-10-10); custom objects as tenant data
> (plan H-ADR-2, H-ADR-3, §10).

---

## 1. The gate this dossier answers

A concept earns a codebook id only when it passes all three axes
(ruff `consumer-transcode-furnace-playbook.md` §3):

- **METHOD:** a service family operates on it.
- **STORAGE:** something persists it as its own record.
- **STRUCTURE:** a screen gives it a home.

The OGIT lift catalogue adds a demand rule
(`OGAR/docs/OGIT-DOMAIN-LIFT-CATALOGUE.md`, "Lift-tested → Cross-walked"):
a consumer must need to authorize or route on it. HubSPO-rs is that
consumer for every row marked MINT.

Before minting, every HubSpot object is first tried against an existing
OGAR concept. If the meaning truly converges, the verdict is ALIAS.

## 2. Evidence and its grade

| Axis | Source | Grade |
|---|---|---|
| METHOD | `hubspo-harvest` over HubSpot's public API catalog | **MEASURED** (command below) |
| STORAGE | the same harvest: a type with its own `/objects/<type>` collection is its own record; property definitions come from the generic `/crm/properties/{objectType}` API | **MEASURED** for the collection; property lists need a sandbox portal (`ISS-ORACLE-PORTAL`) |
| STRUCTURE | HubSpot's UI: whether the type has its own index page and record page | **CLAIMED** — public product knowledge. The Klickwege structure oracle (D-UI-5) has not run. |

Reproduce the METHOD and STORAGE numbers:

```sh
scripts/fetch-hubspot-specs.sh                      # into .cache/ (gitignored)
cargo run -q -p hubspo-harvest -- report .cache/hubspot-specs > .cache/harvest.json
```

Measured 2026-10-10:
- 123 APIs in the catalog; 107 have a selectable version and all 107 were
  read (0 missing).
- 1,198 operations: 472 on `/objects/` paths, all classified (0 unknown);
  726 outside `/objects/` (settings, CMS, marketing and so on), not
  classified.
- 35 object types. Four are named only by numeric type id in paths, each
  used by exactly one API: `0-3` Deals, `0-162` Services, `0-410` Courses,
  `0-420` Listings.
- Not selectable, because only a beta or preview version exists: Automation
  V4 (workflows), Forms, Lead Scoring, Reporting, Forecasts, Subscription
  Lifecycle, Payments. No METHOD evidence is taken from an uncommitted
  contract.

**Verb column key.** Every type marked "full" has all 11 standard object
verbs: list, create, read, update, archive, the five batch verbs, and search.
Extra verbs are named.

## 3. Verdicts

### 3.1 MINT — new concepts in `ConceptDomain::Crm` (`0x05XX`)

The council assigns the hex. The order below is a suggestion only.

| # | Concept | HubSpot objects | METHOD (measured) | STORAGE | STRUCTURE (claimed) | OGIT source | Why not an alias |
|---|---|---|---|---|---|---|---|
| 1 | `crm_contact` | `contacts` | full + merge + gdpr_delete | own collection | contacts index + record page | `CustomerSupport/User` (partial) | `BILLING_PARTY` (Odoo `res.partner`) is one concept for persons **and** companies. HubSpot keeps them as two object types with their own APIs, screens and association label sets. Converging would make every contact↔company association a self-edge on one concept. |
| 2 | `crm_company` | `companies` | full + merge | own collection | companies index + record page | `CustomerSupport/Organization`, `SalesDistribution/Customer` | as above |
| 3 | `crm_deal` | `0-3` (Deals API) | full + merge | own collection | deals index, board, record page | `Advertising/Deal` is advertising inventory, not a sales opportunity | No existing concept is a revenue opportunity moving through a pipeline. `COMMERCIAL_DOCUMENT` is the quote/order a deal produces, not the deal. |
| 4 | `crm_ticket` | `tickets` | full + merge | own collection | tickets index, board, record page | `CustomerSupport/Ticket` | `PROJECT_WORK_ITEM` (OpenProject work package / Redmine issue) carries project membership, versions and journals. A HubSpot ticket has none of those, and has pipeline and SLA semantics instead. |
| 5 | `crm_lead` | `leads` | full | own collection | leads index (prospecting) + record page | — | HubSpot's Leads object is a qualification record that must associate to a contact. It is not a lifecycle stage of the contact. |
| 6 | `crm_activity` (abstract parent) | parent of 7–9; `communications` and `postal_mail` are instances with a kind | `communications` full; `postal_mail` full | each kind has its own collection | no index of its own; lives on the record timeline | `Schedule/Activity` | It is the RBAC and route key for "everything on a record's timeline". **Council question Q1:** is an abstract parent with no screen of its own admissible, or should communications and postal mail wait until they need it? |
| 7 | `crm_call` | `calls` | full | own collection | calls index + record | `Schedule/Activity` | engagement with its own fields (duration, disposition, recording) |
| 8 | `crm_meeting` | `meetings` | full | own collection | meetings index + record | `Schedule/Event`, `Schedule/Attendee` | calendar-backed engagement (iTIP via stalwart, D-MAIL-6) |
| 9 | `crm_task` | `tasks` | full | own collection | tasks index + task queues | — | **Council question Q2:** converge onto `PROJECT_WORK_ITEM`? The recommendation is no, for the same reason as tickets (no project, version or journal semantics). |
| 10 | `crm_pipeline` | Pipelines API (`/crm/pipelines/{objectType}`, 14 ops including audit) | 14 ops, plus Pipeline Governance (8 ops) | own records per object type | pipeline settings, board | — | `PROJECT_TYPE` / `PROJECT_STATUS` are near misses: OpenProject types do not carry an ordered stage set bound to an object type. |
| 11 | `crm_pipeline_stage` | stages under each pipeline | read/create/update/delete stages; **stage edit permissions** (`stage-edit-permissions`, `PipelineStage.writePermissions` enum) | child records of a pipeline | board columns, stage settings | — | HubSpot authorizes per stage, so a consumer must authorize on it: the demand rule is met directly. `PROJECT_STATUS` has no probability and no object-type binding. |
| 12 | `crm_list` | Lists API (30 ops including folders) | 30 ops | own records | lists index + list page | — | A list is a population (static or dynamic), with folders and membership. `PROJECT_QUERY` (a saved view) is the nearest concept but has no membership. In our substrate a list is a mask (plan H-ADR-5), and it is still a record people own, share and authorize on. |
| 13 | `crm_custom_record` | every custom object (Schemas API, 8 ops) | 8 ops on schemas; custom objects use the generic object API | tenant-defined | per-schema index + record | — | Plan H-ADR-3 (approved): one concept; each tenant's schemas are data and ClassViews. Never one concept per tenant. |
| 14 | `crm_campaign` | Marketing Campaigns API (24 ops) | 24 ops including batch | own records | campaigns index + campaign page | `Advertising/Campaign` | the attribution root for marketing |
| 15 | `crm_sequence` | Sequences API (4 ops) | list, read, enroll, read enrollments | own records | sequences index + editor | — | Behaviour container. **Council question Q3:** should the sequence be a concept, or only graph-flow data whose enrolment is an `ActionDef` on `crm_contact`? The recommendation is to mint it: users own, share and permission sequences as records. |

### 3.2 ALIAS — HubSpot names onto existing OGAR concepts

| HubSpot | Existing concept | hex | Basis |
|---|---|---|---|
| `emails` (logged email engagement) | `EMAIL` | 0x0B05 | the same message spear stores (`SpearPort` "Email"); converges HubSPO-rs and spear on one identity |
| `notes` | `PROJECT_COMMENT` | 0x010F | free-text comment attached to a record. **Council question Q4:** is "comment on a record" domain-neutral enough? |
| `products` | `PRODUCT` | 0x0207 | converges with Odoo `product.template`/`product.product` |
| `line_items` | `COMMERCIAL_LINE_ITEM` | 0x0201 | converges with Odoo `sale.order.line` |
| `quotes`, `invoices`, `orders`, `carts` | `COMMERCIAL_DOCUMENT` | 0x0202 | many models → one document concept, same pattern as Odoo `account.move` / `sale.order` |
| `commerce_payments` | `PAYMENT_RECORD` | 0x0205 | converges with Odoo `account.payment` |
| `taxes` | `TAX_POLICY` | 0x0203 | converges with Odoo `account.tax` |
| `projects` | `PROJECT` | 0x0101 | HubSpot's Projects object is a project container; converges with OpenProject `Project` / Redmine `Project` |
| `users`, owners (`/crm/owners`) | `PROJECT_ACTOR` | 0x0104 | the IAM actor (`AUTH_STORE` maps `sub` → `0x0104`). **This refines plan H-ADR-2,** which proposed `HR_EMPLOYEE`: a HubSpot user is a login and assignee, not an employment record. |
| teams (`/settings/teams`, 9 ops) | `HR_DEPARTMENT` | 0x0D02 | team hierarchy = organisational unit (plan H-ADR-9) |

### 3.3 DATA — never a concept

| Item | Where it lives | Why |
|---|---|---|
| Properties and property groups (`/crm/properties/{objectType}`) | ClassView positions + tenant schema rows | Types are an enum of 7 (`bool, date, datetime, enumeration, number, phone_number, string`); field types are an enum of 12. They describe columns, not things. |
| Association labels (`/crm/associations/{from}/{to}/labels`) | tenant data on the relation | `category` ∈ {HUBSPOT_DEFINED, INTEGRATOR_DEFINED, USER_DEFINED, WORK}: labels are configuration (plan H-ADR-4) |
| Communications, postal mail | instances of `crm_activity` with a kind | no screen of their own |
| Business units | tenant sub-scope (plan H-ADR-9, D-IAM-5) | a partition, not a record type |
| Property validation rules (4 ops) | `Constrains` specs | behaviour on a property, not an entity |
| Pipeline governance rules | `ActionDef` guards on `crm_pipeline_stage` | behaviour |

### 3.4 PARK — real objects, no v1 consumer yet

The demand rule is not met until a v1 deliverable needs to authorize or
route on them. Each is revisited when its wave starts.

| HubSpot objects | Revisit at |
|---|---|
| `discounts`, `fees` (full CRUD; quote adjustments) | D-COM-1. Likely ALIAS → `COMMERCIAL_LINE_ITEM`. |
| `subscriptions` (full + merge + gdpr_delete), `contracts` (list/read only) | D-COM-1 |
| `goal_targets` (full) | D-QRY-6 (goals and forecasting) |
| `feedback_submissions` (read-only: list, read, batch_read, search) | D-SVC-1 (surveys) |
| `partner_clients`, `partner_services` (partial) | not planned |
| `0-162` Services, `0-410` Courses, `0-420` Listings (full; vertical object library). `services` and `courses` also appear by name, on their merge endpoints only. | when a vertical needs them; until then they are custom records (`crm_custom_record`) |

### 3.5 DEFER — no committed contract yet

Workflows (Automation V4), Forms, Lead Scoring, Reporting and Forecasts
exist in HubSpot's catalog only as beta or preview. Their concepts are not
proposed from METHOD evidence HubSpot has not committed to:

- **Workflows:** graph-flow graphs whose steps are `ActionDef`s (plan
  H-ADR-7). Whether a workflow is also a concept is decided at D-FLOW-1.
- **Forms:** OGIT `Forms/{Form, FormField, FormFieldGroup}` exist upstream;
  decided at D-MKT-1.
- **Lead scoring:** a computed property (plan D-ACT-4 / D-INT-3), so DATA
  unless proven otherwise.

## 4. Summary

| Verdict | Count | Items |
|---|--:|---|
| MINT | 15 | `crm_contact`, `crm_company`, `crm_deal`, `crm_ticket`, `crm_lead`, `crm_activity`, `crm_call`, `crm_meeting`, `crm_task`, `crm_pipeline`, `crm_pipeline_stage`, `crm_list`, `crm_custom_record`, `crm_campaign`, `crm_sequence` |
| ALIAS | 10 rows (12 object types, plus owners and teams) | §3.2 |
| DATA | 6 | §3.3 |
| PARK | 6 rows (13 object-type names: 11 types, two of them also named on merge endpoints) | §3.4 |
| DEFER | 5 | §3.5 |

All 35 harvested object-type names are accounted for (checked by script against `.cache/harvest.json`). The 15 MINT concepts plus
the ALIAS rows cover every object behind a v1 row of the feature matrix.

## 5. Questions for the council

- **Q1** `crm_activity` as an abstract parent with no screen of its own: admit it, or defer the communications and postal-mail kinds?
- **Q2** `tasks`: mint `crm_task` (recommended) or converge onto `PROJECT_WORK_ITEM`?
- **Q3** `crm_sequence`: concept (recommended) or graph-flow data only?
- **Q4** `notes` → `PROJECT_COMMENT`: is that concept domain-neutral enough to carry CRM notes?
- **Q5** users and owners → `PROJECT_ACTOR` (recommended, refines H-ADR-2) rather than `HR_EMPLOYEE`?
- **Q6** Hex order inside `0x05XX`: assigned by the council; this dossier proposes only names.

## 6. What happens after the council

1. OGAR PR (D-ONT-4): `ConceptDomain::Crm`, the `source_domain_concept`
   `"crm"` tag, the approved `class_ids` with `CODEBOOK` and `ALL` rows,
   `HubSpoPort` (`0x000B`) with every ALIAS and MINT name, a `crm_actions`
   table in `capability_registry::domain_tables()`, and the layout-doc row.
2. Same arc: the lance-graph `ogar_codebook` mirror rows (`AppPrefix` +
   concepts), so `MirrorDrift` stays silent for the ids HubSPO-rs plugs.
3. HubSPO-rs D-ONT-5: `HotPlug` / `RbacPlug` declarations and an `activate`
   test that reports zero drift, plus a deliberately wrong plug proving each
   drift arm can fire.

# HubSPO-rs feature matrix v1 — every HubSpot feature → our substrate → D-id

> Companion to `hubspo-integration-v1.md` (architecture, decisions H-ADR-*,
> waves, gates). Status 2026-10-10: every row is **Queued**.
>
> **Phase:** `v1` = waves 0–9 (the CRM core people switch for);
> `v2` = wave 10–11 breadth; `later` = deliberately out of scope until v2
> ships; `n/a` = not reimplemented (external product, or a HubSpot
> business we do not replicate).
>
> **Substrate keys:**
> LG = lance-graph (`contract` / `quack` / `mask-risc` / `report` /
> `callcenter` / `supervisor` / `arm-discovery` / `deepnsm-v2`),
> OG = OGAR (`ogar-vocab` / `ogar-rbac` / `ogar-render-askama` /
> `ogar-a2ui-frame` / `ogar-doc-ir` / `ogar-loco` / `ogar-dir-*`),
> ND = ndarray (`simd` / `hpc`), SP = spear, ST = stalwart (out of process),
> DS = dir-sim (OGAR meaning + LG execution), GF = graph-flow
> (rs-graph-llm), A2 = a2ui-rs, TP = tesseract-paperless, SD = spider.

---

## 1. Smart CRM (platform core)

| # | HubSpot feature | How HubSpot behaves | Our design | D-id | Phase |
|---|---|---|---|---|---|
| 1.1 | Contacts | person records, email-unique | `crm_contact` concept (mint) or alias; NodeRow + ClassView; email uniqueness = Quack `GroupReduce Count > 1` invariant | D-ONT-6, D-STO-1 | v1 |
| 1.2 | Companies | org records, domain-unique, parent/child | `crm_company`; domain uniqueness invariant; parent/child = labelled self-edge | D-ONT-6, D-STO-3 | v1 |
| 1.3 | Deals | opportunities in pipelines | `crm_deal`; amount in minor units + currency id lane; stage via ActionDef | D-ONT-6, D-ACT-3 | v1 |
| 1.4 | Tickets | service requests in pipelines | `crm_ticket` (OGIT `CustomerSupport/Ticket`) | D-ONT-6, D-SVC-1 | v1 |
| 1.5 | Leads object | prospecting stage before contact/deal | `crm_lead`; association to contact + company required (invariant) | D-ONT-6 | v1 |
| 1.6 | Products / product library | catalogue with price, SKU, recurring billing | alias → `PRODUCT` (0x0207), `PRICELIST` (0x0209); converges with odoo-rs | D-ONT-4, D-COM-1 | v1 |
| 1.7 | Line items | product instances on deals/quotes | alias → `COMMERCIAL_LINE_ITEM` (0x0201) | D-COM-1 | v1 |
| 1.8 | Quotes | priced proposals with e-sign/payment | alias → `COMMERCIAL_DOCUMENT` (0x0202) + doc IR template; e-sign later | D-COM-1 | v2 |
| 1.9 | Custom objects | portal-defined object types | `crm_custom_record` + tenant schema rows + tenant ClassView (H-ADR-3) | D-ONT-7 | v1 |
| 1.10 | Standard properties | ~100+ per object, typed | ClassView fixed positions, generated | D-ONT-6 | v1 |
| 1.11 | Custom properties | text, number, date, enum, checkbox, file, calc, rollup | tenant extension positions via `WideFieldMask`; long values in KV | D-ONT-7, D-STO-2 | v1 |
| 1.12 | Property groups | UI grouping | data on tenant schema; not a concept | D-UI-3 | v1 |
| 1.13 | Property validation rules | regex, min/max, unique values | `Constrains` KausalSpec; uniqueness = Quack invariant in crm-sim | D-ACT-1, D-SIM-1 | v1 |
| 1.14 | Calculated / formula properties | formula over same record | `Depends` + ogar-loco program | D-ACT-4 | v1 |
| 1.15 | Rollup properties | min/max/sum/count over associated | `lance-graph-report` fold over edge lanes, recompute on version delta | D-ACT-4 | v1 |
| 1.16 | Property history | per-change value/source/actor | Lance version log + audit record; probe P-HIST-1 | D-STO-5 | v1 |
| 1.17 | Associations | many-to-many between objects | sparse sorted edge lanes per type pair | D-STO-3 | v1 |
| 1.18 | Association labels | named, directional, with limits | tenant data on the relation; cardinality invariant | D-STO-3 | v1 |
| 1.19 | Primary company | one per contact/deal | label + `== 1` invariant | D-STO-3 | v1 |
| 1.20 | Record page (about / timeline / association cards) | three-column record UI | a2ui six-region frame, ClassView projections | D-UI-1 | v1 |
| 1.21 | Record customisation (cards, tabs per team) | per-team layouts | layouts as data, team-scoped; RBAC by projection | D-UI-3 | v2 |
| 1.22 | Index page table view | sortable, filterable, column-pickable | report terminal view over a mask; `list_view` | D-UI-2 | v1 |
| 1.23 | Board view | kanban per pipeline | report pivot pipeline × stage + folds | D-QRY-3 | v1 |
| 1.24 | Saved views | per user/team/everyone | `PROJECT_QUERY`-shaped data + stored predicate | D-QRY-3 | v1 |
| 1.25 | Filters | AND groups OR'd, many operators | filter compiler → Quack | D-QRY-1 | v1 |
| 1.26 | Active lists (segments) | auto-updating membership | predicate → mask, incremental on version delta | D-QRY-2 | v1 |
| 1.27 | Static lists | manual membership | stored mask + version | D-QRY-2 | v1 |
| 1.28 | List membership history | when added/removed | mask diff per version | D-QRY-2 | v1 |
| 1.29 | Global search | across objects | Tantivy reference index, reconciled from store (TP pattern) | D-QRY-7 | v1 |
| 1.30 | Import (CSV/Excel, multi-object) | mapping, dedup on email/domain | crm-sim plan → review → one version | D-STO-6, D-SIM-2 | v1 |
| 1.31 | Export | CSV of view | terminal materialisation; accumulation grant | D-IAM-6 | v1 |
| 1.32 | Duplicate management | suggested pairs, merge | ND blocking + exact re-rank; merge via crm-sim | D-INT-1, D-SIM-3 | v1 |
| 1.33 | Merge records | combines properties, associations, timeline | `Merge` ActionDef; HubSpot merge semantics reproduced | D-SIM-3 | v1 |
| 1.34 | Bulk edit / delete | mass changes | crm-sim plan; revert = version restore | D-SIM-2 | v1 |
| 1.35 | Restore deleted records (90 days) | recycle bin | archived flag lane + time travel | D-ACT-1 | v1 |
| 1.36 | Data quality command center | property insights, formatting issues | invariant programs reported as `Violation`s; fix = crm-sim plan | D-SIM-1 | v2 |
| 1.37 | Data formatting automation | capitalise names, normalise phone | `Normalize` recipe ActionDefs (fuzzy-recipe centroid) | D-FLOW-3 | v2 |
| 1.38 | Timeline / activity feed | ordered engagements + changes | spear `activity` ∪ version-log changes ∪ engagements | D-STO-5, SP D-HUB-4 | v1 |
| 1.39 | Notes | rich text on records | alias → `PROJECT_COMMENT` or `crm_activity` kind; doc IR body in KV | D-ONT-6 | v1 |
| 1.40 | Tasks + queues | to-dos with due dates, queues | `crm_activity(task)`; queue = saved view | D-ONT-6 | v1 |
| 1.41 | Calls (logging) | engagement records | `crm_activity(call)` | D-MAIL-10 | v1 |
| 1.42 | Meetings (logging) | engagement records | `crm_activity(meeting)` from CalDAV/iTIP | D-MAIL-6 | v1 |
| 1.43 | Communications (SMS/WhatsApp/LinkedIn logging) | engagement records | `crm_activity(communication)`; channel = data | D-ONT-6 | v2 |
| 1.44 | Postal mail logging | engagement | `crm_activity(postal_mail)` | D-ONT-6 | later |
| 1.45 | Files on records | attachments | TP dedup gate + `DOCUMENT` + edge to record | D-STO-3, hubspo-docs | v1 |
| 1.46 | Followers / record notifications | follow a record | alias → `PROJECT_WATCHER`; notifications from version deltas | D-UI-6 | v2 |
| 1.47 | @mentions in notes/comments | notify users | doc IR mention node → notification | D-UI-6 | v2 |
| 1.48 | Lifecycle stage | subscriber → customer, forward-only by default | enum id lane; forward-only = `StateGuard` | D-ACT-3 | v1 |
| 1.49 | Lead status | | enum id lane | D-ONT-6 | v1 |
| 1.50 | Record ownership + owner rotation | | owner GUID lane; rotation ActionDef | D-ACT-1, D-FLOW-3 | v1 |

## 2. Sales Hub

| # | HubSpot feature | How HubSpot behaves | Our design | D-id | Phase |
|---|---|---|---|---|---|
| 2.1 | Deal pipelines + stages | ordered stages, win probability | `crm_pipeline` data + `MoveStage` ActionDef | D-ACT-3 | v1 |
| 2.2 | Required properties per stage | block stage move | `Constrains` spec, `guard_failure_policy: Reject` | D-ACT-3 | v1 |
| 2.3 | Stage automation (create task on stage) | simple triggers | workflow with `Onchange` trigger | D-FLOW-1 | v1 |
| 2.4 | Deal tags / deal health | indicators | rules → fold → tag lane; health = scoring | D-INT-3 | v2 |
| 2.5 | Email from CRM (1:1) | send via connected inbox | JMAP `EmailSubmission` on ST; receipt row on SP | D-MAIL-3 | v1 |
| 2.6 | Email tracking (open/click) | pixel + link rewrite | own endpoints; MPP/bot flag | D-MAIL-9 | v1 |
| 2.7 | Inbox connection / email logging | BCC + connected inbox | MTA hook BCC capture + JMAP sync; contract `MailParser`/`ThreadLinker` impl | D-MAIL-4 | v1 |
| 2.8 | Email templates | reusable with tokens | askama-compiled, validated against ClassView | D-MAIL-8 | v1 |
| 2.9 | Snippets | text blocks | same as templates | D-MAIL-8 | v1 |
| 2.10 | Personalisation tokens | `{{contact.firstname}}` | compile-time-validated template slots | D-MAIL-8 | v1 |
| 2.11 | Sequences | timed email/task steps, auto-unenrol | graph-flow; ST future release; unenrol on reply/meeting | D-FLOW-4 | v1 |
| 2.12 | Meetings scheduler / booking links | availability + booking page | fold over CalDAV free/busy; iTIP invite | D-MAIL-6 | v1 |
| 2.13 | Round-robin / group meetings | | rotation over team availability | D-MAIL-6 | v2 |
| 2.14 | Calling (VoIP) | dial, record, transcribe | out of scope; log only; spear telephony later | D-MAIL-10 | later |
| 2.15 | Conversation intelligence | call transcript analysis | deepnsm-v2 on transcripts | D-INT-6 | later |
| 2.16 | Documents (sales content tracking) | tracked PDFs | TP intake + view-tracking endpoint | hubspo-docs | v2 |
| 2.17 | Playbooks | guided call scripts | doc IR + form → properties | D-MKT-1 | later |
| 2.18 | Prospecting workspace | daily lead queue | saved views + tasks + scoring | D-QRY-3, D-INT-3 | v2 |
| 2.19 | Forecasting | weighted pipeline by period/owner | report fold: amount × probability by close date × owner/team | D-QRY-6 | v1 |
| 2.20 | Goals / quotas | targets per user/team | goal records + report progress fold | D-QRY-6 | v2 |
| 2.21 | Sales analytics (deal velocity, win rate) | | report plans | D-QRY-6 | v1 |
| 2.22 | Quotes + e-signature + payments | | `COMMERCIAL_DOCUMENT`; payments via `PAYMENT_RECORD`; e-sign later | D-COM-1 | v2 |
| 2.23 | Predictive lead scoring | ML score | logistic model on ND BLAS; rank fidelity via `reliability.rs` | D-INT-3 | v1 |
| 2.24 | Manual lead scoring (points rules) | | rules → fold → score lane | D-ACT-4 | v1 |
| 2.25 | Buyer intent | site visits by company | tracking endpoint + reverse-IP enrichment | D-INT-5 | later |

## 3. Marketing Hub

| # | HubSpot feature | How HubSpot behaves | Our design | D-id | Phase |
|---|---|---|---|---|---|
| 3.1 | Forms (embedded, pop-up, standalone) | create/update contacts | OGIT `Forms/*`; submission → ActionInvocation | D-MKT-1 | v2 |
| 3.2 | Progressive profiling / dependent fields | | form definition data; render-time projection | D-MKT-1 | v2 |
| 3.3 | Consent / subscription types (GDPR) | per-type opt-in, legal basis | subscription status lanes + audit; enforced at send | D-MKT-2 | v2 |
| 3.4 | Marketing email (drag-and-drop) | design, send to lists | doc IR → askama/typst render; ST bulk submission | D-MKT-2 | v2 |
| 3.5 | A/B tests | variant split | mask split by hash; report fold | D-MKT-2 | v2 |
| 3.6 | Send-time optimisation | | per-contact engagement fold → schedule | D-MKT-2 | later |
| 3.7 | Bounce / unsubscribe handling | suppression | ST DSN events → suppression mask | D-MKT-2 | v2 |
| 3.8 | Marketing contacts (billing tier) | marketable flag | flag lane; counts fold | D-MKT-2 | v2 |
| 3.9 | Campaigns | group assets, attribution | `crm_campaign` (OGIT `Advertising/Campaign`) | D-MKT-3 | v2 |
| 3.10 | Attribution reports (multi-touch) | first/last/linear/U/W/decay | report folds over touch points | D-MKT-3 | v2 |
| 3.11 | Landing pages / website / blog (CMS) | | doc IR render; out of scope beyond forms + email | D-MKT-4 | later |
| 3.12 | HubDB | tables for CMS | custom objects cover it | D-ONT-7 | later |
| 3.13 | SEO tools | | n/a | — | n/a |
| 3.14 | Social publishing | | n/a (third-party) | — | n/a |
| 3.15 | Ads (audience sync, attribution) | | list → audience export via connector flow | D-FLOW-7 | later |
| 3.16 | ABM (target accounts, tiers) | | company properties + lists + report | D-QRY-2 | v2 |
| 3.17 | Marketing workflows | | same engine as §6 | D-FLOW-1 | v2 |
| 3.18 | Web analytics / tracking code | page views on contacts | tracking endpoint → activity rows | D-INT-5 | later |
| 3.19 | Lead capture chatflows | chatbot | §4.3 | D-SVC-1 | later |
| 3.20 | Customer journey analytics | stage sequences | report fold over ordered events | D-QRY-6 | later |

## 4. Service Hub

| # | HubSpot feature | How HubSpot behaves | Our design | D-id | Phase |
|---|---|---|---|---|---|
| 4.1 | Ticket pipelines | | same pipeline machinery as deals | D-ACT-3 | v1 |
| 4.2 | Help desk / shared inbox | channels into tickets | SP shared-inbox roles + ST JMAP; email → ticket ActionDef | D-MAIL-5, D-SVC-1 | v1 |
| 4.3 | Live chat + chatbots | website chat into inbox | OGIT `CustomerSupport/{ChatSession,ChatMessage}`; WS endpoint | D-SVC-1 | v2 |
| 4.4 | Ticket routing (round robin, skills) | | rules + rotation; spear D-ROUTE patterns | D-FLOW-5 | v1 |
| 4.5 | SLAs (first response, close time) | timers + breach alerts | timer actors under supervisor; breach = event | D-FLOW-5 | v1 |
| 4.6 | Knowledge base | articles + search | doc IR + Tantivy | D-SVC-2 | v2 |
| 4.7 | Customer portal | customers view their tickets | RLS scoped to contact's company; auth via OIDC | D-SVC-2 | v2 |
| 4.8 | Feedback surveys (NPS/CSAT/CES) | | OGIT `Survey/*`; responses → contact properties | D-SVC-1 | v2 |
| 4.9 | Customer success workspace / health score | | scoring + views | D-INT-3 | later |
| 4.10 | Service analytics | | report plans | D-QRY-6 | v2 |

## 5. Commerce Hub

| # | HubSpot feature | How HubSpot behaves | Our design | D-id | Phase |
|---|---|---|---|---|---|
| 5.1 | Invoices | | `COMMERCIAL_DOCUMENT`; converge with odoo-rs (`account.move`) | D-COM-1 | v2 |
| 5.2 | Payments / payment links | Stripe-backed | `PAYMENT_RECORD`; PSP connector flow | D-COM-1 | later |
| 5.3 | Subscriptions | recurring billing | data + scheduled flows | D-COM-1 | later |
| 5.4 | Orders / carts | e-commerce sync | `COMMERCIAL_DOCUMENT` aliases | D-COM-1 | later |
| 5.5 | Tax | | `TAX_POLICY`; contract `TaxEngine` seam | D-COM-1 | later |

## 6. Automation (Operations / Data Hub)

| # | HubSpot feature | How HubSpot behaves | Our design | D-id | Phase |
|---|---|---|---|---|---|
| 6.1 | Workflows (record-based) | trigger → actions → branches → delays | graph-flow graphs; gated ActionDefs | D-FLOW-1 | v1 |
| 6.2 | Enrolment triggers (criteria, event, schedule, webhook) | | trigger actor on `LanceVersionWatcher`; mask delta ∩ criteria | D-FLOW-2 | v1 |
| 6.3 | Re-enrolment | | per-workflow enrolment mask + version | D-FLOW-2 | v1 |
| 6.4 | Branches (if/then, value equals, A/B) | | conditional edges = mask predicates | D-FLOW-1 | v1 |
| 6.5 | Delays (fixed, until date, until event) | | `WaitForInput` + timer actor | D-FLOW-1 | v1 |
| 6.6 | Action library (set property, create record, email, list, webhook, rotate) | | `crm_actions` table | D-FLOW-3 | v1 |
| 6.7 | Custom code actions (Node/Python) | sandboxed code | ogar-loco programs; never embedded JS/Python in hot path | D-FLOW-3 | v2 |
| 6.8 | Workflow test mode | | crm-sim dry run | D-FLOW-6 | v1 |
| 6.9 | Workflow history / logs | per-enrolment action log | `ActionInvocation` audit (merkle-chained) | D-ACT-2 | v1 |
| 6.10 | Webhooks (outbound) | | version delta → signed POST | D-API-2 | v2 |
| 6.11 | Data sync (two-way app sync) | | connector flows; out of v1 | D-FLOW-7 | later |
| 6.12 | Datasets (reporting) | | report plans saved as data | D-QRY-6 | later |
| 6.13 | Scheduled workflows | | cron trigger actor | D-FLOW-2 | v1 |
| 6.14 | Approval steps | human gate | `WaitForInput`; GF kanban | D-FLOW-1 | v2 |

## 7. Reporting

| # | HubSpot feature | How HubSpot behaves | Our design | D-id | Phase |
|---|---|---|---|---|---|
| 7.1 | Dashboards | grid of reports | a2ui projection of report results | D-QRY-6, D-UI-1 | v1 |
| 7.2 | Single-object reports | | report plan | D-QRY-6 | v1 |
| 7.3 | Cross-object reports | joins across associations | report over edge lanes (Quack `Semijoin`) | D-QRY-6 | v1 |
| 7.4 | Funnel reports | stage conversion | ordered fold over stage transitions (version log) | D-QRY-6 | v1 |
| 7.5 | Time-series / comparison | | fold by time bucket | D-QRY-6 | v1 |
| 7.6 | Custom report builder | arbitrary dims/measures | `ReportPlan` authored by UI | D-QRY-6, D-UI-4 | v2 |
| 7.7 | Report sharing / permissions | | RBAC on report + underlying masks | D-IAM-1 | v1 |
| 7.8 | Scheduled email of dashboards | | flow + render to doc IR | D-FLOW-2 | v2 |

## 8. Platform, admin, security

| # | HubSpot feature | How HubSpot behaves | Our design | D-id | Phase |
|---|---|---|---|---|---|
| 8.1 | Users + seats | | directory records (DS); seat = role grant | D-IAM-4 | v1 |
| 8.2 | Teams + sub-teams | hierarchy | `HR_DEPARTMENT` tree, `Dn128` subtree | D-IAM-2 | v1 |
| 8.3 | Permission sets | per-object CRUD + scope | `ogar-rbac` roles + `ClassGrant`/`OpMask` | D-IAM-1 | v1 |
| 8.4 | Ownership-based access (all / team / owned) | | `ScopeSpec` over owner + team subtree | D-IAM-2 | v1 |
| 8.5 | Field-level permissions (sensitive data) | | per-role `WideFieldMask`; absent from wire | D-IAM-3 | v1 |
| 8.6 | Partitioning (business units / brands) | | tenant sub-scopes | D-IAM-5 | v2 |
| 8.7 | SSO / 2FA | SAML/OIDC | OIDC; `ogar-auth` TOTP | D-IAM-4 | v1 |
| 8.8 | Audit logs (login, security, property changes) | | merkle-chained audit, examined in place | D-ACT-2 | v1 |
| 8.9 | Multi-portal / multi-tenant | | tenant = portal; datasets per tenant × type | D-STO-4 | v1 |
| 8.10 | Sandboxes | copy of portal for testing | crm-sim branch / Lance version fork | D-SIM-1 | v2 |
| 8.11 | Data privacy (GDPR delete, export) | permanent delete + audit | delete = tombstone + dataset compaction; export accumulation-gated | D-IAM-6 | v1 |
| 8.12 | CRM API v3 (objects, search, batch, associations v4, properties, pipelines, owners) | | `hubspo-compat` façade | D-API-1 | v2 |
| 8.13 | OAuth apps / private app tokens | | RBAC actors with RFC 8693 delegation | D-API-3 | v2 |
| 8.14 | App marketplace / UI extensions | | n/a for v1; a2ui ObjectSlot portals later | — | later |
| 8.15 | Mobile app | | a2ui-paint `DeviceClass::Mobile` | D-UI-1 | later |
| 8.16 | Notifications (in-app, email) | | version deltas → notification fold | D-UI-6 | v2 |
| 8.17 | Currencies (multi-currency, exchange rates) | | `CURRENCY_POLICY`; rate table data | D-COM-1 | v2 |
| 8.18 | Data residency | | dataset location per tenant | D-STO-4 | later |

## 9. AI (Breeze) and enrichment

| # | HubSpot feature | How HubSpot behaves | Our design | D-id | Phase |
|---|---|---|---|---|---|
| 9.1 | Breeze Intelligence enrichment | firmographics from domain | SD crawl → `spider_doc_ir` → TP intake → crm-sim plan with provenance | D-INT-5 | v2 |
| 9.2 | Duplicate suggestions | | ND `cam_index` / `cascade` / `clam_search` | D-INT-1 | v1 |
| 9.3 | Lookalike companies | | ND `cam_pq` semantic/hybrid + `top_k` | D-INT-2 | v2 |
| 9.4 | Association / owner / stage suggestions | | arm-discovery rules, lift-filtered, `NarsTruth`; never auto-applied | D-INT-4 | v1 |
| 9.5 | Email / note summarisation | LLM | deepnsm-v2 SPO + compiled templates; LLM only as oracle | D-INT-6, D-INT-7 | later |
| 9.6 | Copilot (chat over CRM) | | thinking templates over Quack reads; RBAC-scoped | D-INT-7 | later |
| 9.7 | Agents (customer, prospecting, content, data) | autonomous LLM agents | compiled templates + GF flows; every write a gated ActionInvocation | D-INT-7 | later |
| 9.8 | Next-best-action | | callcenter `NextBestActionReasoner` (exists, unused) + scoring | D-INT-3 | v2 |
| 9.9 | Customer categorisation | | callcenter `CustomerCategoryReasoner` (exists, unused) | D-INT-4 | v2 |
| 9.10 | Data agent (fill properties from web) | | 9.1 + D-INT-4 suggestion path | D-INT-5 | later |

---

## Coverage summary

| Hub | Rows | v1 | v2 | later | n/a |
|---|--:|--:|--:|--:|--:|
| Smart CRM | 50 | 42 | 7 | 1 | 0 |
| Sales | 25 | 15 | 6 | 4 | 0 |
| Marketing | 20 | 0 | 11 | 7 | 2 |
| Service | 10 | 4 | 5 | 1 | 0 |
| Commerce | 5 | 0 | 1 | 4 | 0 |
| Automation | 14 | 9 | 3 | 2 | 0 |
| Reporting | 8 | 6 | 2 | 0 | 0 |
| Platform | 18 | 9 | 6 | 3 | 0 |
| AI | 10 | 2 | 4 | 4 | 0 |
| **Total** | **160** | **87** | **45** | **26** | **2** |

Counts were computed by parsing this file's rows (2026-10-10); recompute them when a row's phase changes rather than editing by hand.

# HubSPO-rs concept mint dossier v3 (D-ONT-3, ratified)

> **Supersedes:** v2 (council draft) and v1 (the input), both kept in this
> folder.
> **Status:** RATIFIED 2026-10-10 by the 5+3 council: five research reviews
> of v1 (shipped code, canon consistency, convergence, every number,
> external sources) → v2 → three adversarial reviews of v2 (grade
> inflation, dilution/collapse, non-negotiables and landing safety) → this
> v3. Every blocker the reviewers raised is resolved below (§6). D-ONT-4
> lands exactly this.
> **Operator decisions (2026-10-10):** port `HubSpoPort` at app prefix
> `0x000B`; `ConceptDomain::Crm` at **`0x05XX`** (OD-2, amended from
> `0x10`); custom objects and properties are tenant data (H-ADR-3).

Evidence grades: `[G]` shipped code, a measurement with its command, or a
primary document; `[H]` plausible, with the oracle that settles it; `[S]`
analogy only — never built on.

---

## 1. The gate, as the council applied it

A concept needs all three axes plus a consumer that authorizes or routes on
it (furnace playbook §3; OGIT lift catalogue demand rule):

- **METHOD** — a service family operates on it. Measured by
  `hubspo-harvest` over HubSpot's public API catalog.
- **STORAGE** — persisted as its own record (its own collection).
- **STRUCTURE** — **rendered as a distinct item type on some screen**: its
  own index or record page, *or* its own card type on another record's
  timeline. This is the definition the council fixed after v2 applied two
  different ones. It is `[H]` for every row until the Klickwege oracle
  (D-UI-5) runs; no primary document describes HubSpot's pages.

**LATEST counts as METHOD evidence (council DECISION).** Basis: the
harvester's selection policy picks a `LATEST` release only when no `STABLE`
one exists and never a beta or preview (`hubspo-harvest/src/catalog.rs`
module doc and `select`). Revisit if HubSpot publishes a versioning document
that defines `LATEST`.

## 2. MINT — `ConceptDomain::Crm`, `0x05XX`

`0x0500` is the domain root and never a concept (the zero-slot test forbids
`CC == 0x00`). Slots follow promotion order. Every name keeps the `crm_`
prefix, because OGAR's alias normalizer maps bare words (`status`, `type`,
`query`, `group`, `board`, …) to ProjectMgmt concepts.

| Hex | Concept | HubSpot (type id) | METHOD | STRUCTURE [H] | Why not an alias |
|---|---|---|---|---|---|
| `0x0501` | `crm_contact` | contacts `0-1` | full + merge + gdpr_delete [G] | index + record page | see §2.1 |
| `0x0502` | `crm_company` | companies `0-2` | full + merge [G] | index + record page | see §2.1 |
| `0x0503` | `crm_deal` | deals `0-3` | full + merge [G] | index, board, record page | No concept is a revenue opportunity moving through stages; `COMMERCIAL_DOCUMENT` is what a deal produces. |
| `0x0504` | `crm_ticket` | tickets `0-5` | full + merge [G] | index, board, record page | `PROJECT_WORK_ITEM` is defined by its project edge and journals; a ticket has no project and is authorized by owner, team and associations [G]. OGIT `CustomerSupport/Ticket` matches. |
| `0x0505` | `crm_lead` | leads `0-136` | full [G] | prospecting workspace + record page | A qualification record associated to a contact and/or company; not a lifecycle stage of the contact. |
| `0x0506` | `crm_pipeline` | Pipelines API, 14 ops [G] | settings, board | `PROJECT_TYPE` carries no ordered stage set bound to an object type. |
| `0x0507` | `crm_pipeline_stage` | stages; per-stage permissions (`permissionMode` OPEN/RESTRICTED with user and team ids), documented in HubSpot's pipeline-rules developer guide [G] | board column, stage settings | `PROJECT_STATUS` is a global lookup; a stage belongs to exactly one pipeline and is authorized per stage. |
| `0x0508` | `crm_call` | calls `0-48` | full [G] | calls index + record; timeline card | own fields (duration, disposition, recording) |
| `0x0509` | `crm_meeting` | meetings `0-47` | full [G] | meetings index + record; timeline card | An engagement record. A calendar event is its backing, not the same concept (§5 O-3). |
| `0x050A` | `crm_task` | tasks `0-27` | full [G] | tasks index, task queues | Same reason as tickets: no project semantics. |
| `0x050B` | `crm_note` | notes `0-46` | full [G] | its own card type on record timelines; no page of its own | see §2.2 |
| `0x050C` | `crm_list` | Lists API, 30 ops ("segments" in the UI) [G] | lists index + list page | see §2.3 |
| `0x050D` | `crm_campaign` | Campaigns API, 24 ops [G] | campaigns index + page | the marketing attribution root; OGIT `Advertising/Campaign` is ad buying, a loose match only |
| `0x050E` | `crm_sequence` | Sequences API, 4 ops: list, read, enroll, enrollments [G]; no authoring ops in the public API | sequences index + editor | A shared, permissioned record: enrollment fails unless the sequence is shared with the sender (HubSpot knowledge base) [G]. Behaviour lives in `ActionDef`s; the address only names the container, as for `knowledge_item` `0x0C05`. |
| `0x050F` | `crm_custom_record` | tenant custom objects (`2-XXX`); Schemas API, 8 ops [G] | per-schema index + record | H-ADR-3: one concept; each tenant's schemas are data and ClassViews. **Default deny** (§4.3). |

Every MINT row also has STORAGE `[G]`: its own collection in the harvest.

### 2.1 Contact and company (resolves reviewer 1 BLOCK 1, reviewer 2 FIX 6)

- `BILLING_PARTY` (`0x0204`) is the target of the commerce `party` edges
  (`commercial_document` and `payment_record`). Its attributes are identity
  fields (`name`, `email`, `is_company`), and Odoo `res.partner`, prospects
  included, already aliases onto it. So reading it as "a role in a sale" is
  an interpretation, `[H]`, not what the code says.
- The mint rests on HubSpot's mechanism: person and organization are two
  object types with separate APIs, merge rules and screens, joined by a
  labelled, many-to-many contact→company association. Aliasing both onto
  `BILLING_PARTY` would turn that association into a self-edge discriminated
  by an attribute flag. Mainstream CRMs split the two as HubSpot does
  (Salesforce Contact/Account; schema.org Person/Organization); Odoo and
  vCard do not. `[H]`
- No second consumer splits contact from company yet. **O-6** decides
  where Odoo `res.partner` lands once `crm_contact` exists.
- On commerce records (§3), HubSpot's contact and company associations are
  always kept as CRM associations. A `party` edge is additional, set only
  when a billing party is resolved; that rule is decided at D-COM-1, with a
  test that a HubSpot invoice's party resolves to a classid the edge
  accepts.

### 2.2 Notes, and why there is no `crm_activity` (resolves reviewer 1 BLOCK 2)

Under the §1 definition of STRUCTURE:

- A note has its own card type on the timelines of the records it is
  associated with; it passes. Two legs, graded separately:
  - aliasing onto `PROJECT_COMMENT` would put CRM notes inside
    ProjectMgmt's domain-scoped RBAC coverage (`concepts_in_domain`) `[G]`;
  - a note is associated to several records at once and owned, while
    `project_comment` has one `author` edge and polymorphic attachment by
    doc only `[H]`, settled by sandbox check O-1.
- "Activity" is never rendered as an item type of its own: the activity
  feed shows calls, notes, meetings, tasks and emails, each as its own card.
  An abstract `crm_activity` therefore fails STRUCTURE `[H]` (same oracle).
  It is **not minted and not reserved**; it is revisited only if D-UI-5
  shows an activity item type. The record timeline is DATA (§3.3).
- `communications` and `postal_mail` do have their own cards, so they pass
  STRUCTURE; they are parked on the demand rule (§3.4).

### 2.3 Lists (reviewer 2 FIX 9)

`crm_list` addresses the **list definition**: owner, filter, static or
active, and its id. Membership is a mask over the member population, never
one row per member (iron rule 5). What separates a list from a saved view is
that a list is a referenceable, owned record that people share and authorize
on; a saved view is a mask over a ClassView (§3.3).

## 3. Everything else

### 3.1 ALIAS — onto existing concepts

Port-scoped, PascalCase names in `HUBSPO_ALIASES`; nothing is added to the
global alias normalizer, so "Users", "Teams" or "Notes" never remap for
another consumer.

| HubSpot | Concept | Hex | Basis |
|---|---|---|---|
| `emails` (`0-49`) | `EMAIL` | `0x0B05` | the message spear stores; one identity across HubSPO-rs and spear, if the engagement carries the RFC Message-ID (O-2) |
| `products` (`0-7`) | `PRODUCT` | `0x0207` | converges with Odoo `product.*` |
| `line_items` (`0-8`) | `COMMERCIAL_LINE_ITEM` | `0x0201` | converges with Odoo `sale.order.line` |
| `quotes`, `invoices`, `orders` | `COMMERCIAL_DOCUMENT` | `0x0202` | the Odoo `sale.order` / `account.move` pattern. The ClassView tells them apart by a document-kind discriminator, never by state alone. |
| `commerce_payments` | `PAYMENT_RECORD` | `0x0205` | converges with Odoo `account.payment` |
| tax-rate library | `TAX_POLICY` | `0x0203` | converges with Odoo `account.tax` |
| users (`0-115`) | `PROJECT_ACTOR` | `0x0104` | the IAM actor (`auth_store` maps `sub` → `0x0104`); HubSpot users cannot be associated to records, unlike every CRM object. Refines plan H-ADR-2. |
| teams | `PROJECT_ACTOR` | `0x0104` | the OpenProject `Group` fold: a group is a principal one grants to, and stage permissions grant to team ids. Teams carry the STI discriminator `type = "Team"`; user→team membership (primary and secondary) is an edge; grant lists stay typed by the discriminator. |

### 3.2 Not aliased after review

| Item | v2 | v3 | Why |
|---|---|---|---|
| owners (`/crm/owners`) | alias → `PROJECT_ACTOR` | DATA: record ownership | An owner is a role on a record (`ownerId` is separate from `userId`; an archived owner may have no login). Ownership is an edge onto a `PROJECT_ACTOR`, resolved through the user id. |
| `carts` | alias → `COMMERCIAL_DOCUMENT` | PARK | "A cart is a draft order" was analogy `[S]`; a cart may have no party, date or line-item contract. |
| `projects` (`0-970`) | alias → `PROJECT` | PARK | `[H]` only, and it would pull HubSpot records into ProjectMgmt's RBAC coverage — the reason notes were not aliased onto `PROJECT_COMMENT`. |

### 3.3 DATA — never a concept

| Item | Where it lives |
|---|---|
| properties and property groups | ClassView positions + tenant schema rows |
| association labels | tenant data on the relation |
| record ownership | an edge onto `PROJECT_ACTOR` (§3.2) |
| record timeline | a mask over `crm_call`, `crm_meeting`, `crm_note`, `crm_task`, `EMAIL` and the parked communications/postal mail; it spans two domains |
| saved index views | a mask over a ClassView; whether they ever converge on `PROJECT_QUERY` is O-5 |
| business units | a tenant sub-scope (H-ADR-9) |
| property validation rules | `Constrains` specs |
| pipeline governance rules | `ActionDef` guards and row scope on `crm_pipeline_stage` |

The Timeline Events API (app-defined event types) is not placed yet; it is
decided with D-MAIL-2.

### 3.4 PARK — no v1 consumer yet

| HubSpot | Revisit at |
|---|---|
| `communications` (`0-18`), `postal_mail` (`0-116`) | the comms wave |
| `discounts`, `fees`, `taxes` (quote adjustment lines; a tax line keeps a rate edge onto `TAX_POLICY`) | D-COM-1 |
| `carts` | D-COM-1 |
| `subscriptions`, `contracts` | D-COM-1 |
| `projects` | when a deliverable needs them; then argued against `PROJECT` with RBAC coverage in view |
| `goal_targets` | D-QRY-6 |
| `feedback_submissions` (read-only) | D-SVC-1 |
| `partner_clients`, `partner_services` | not planned |
| Services `0-162`, Courses `0-410`, Listings `0-420` (HubSpot standard objects) | when a vertical needs them. Unaddressed until then: they are **not** routed to `crm_custom_record`, which stays tenant-defined only. |

### 3.5 DEFER

Workflows (Automation V4), Forms, Lead Scoring and Reporting exist only as
beta or preview. Workflows are decided at D-FLOW-1, forms at D-MKT-1, lead
scoring is a computed property.

### 3.6 Accounting

The harvest has 35 object-type keys for 33 objects: `services` and
`courses` are the merge-only paths of `0-162` and `0-410`.

- MINT, 9 keys: contacts, companies, `0-3`, tickets, leads, calls,
  meetings, tasks, notes.
- ALIAS, 8 keys: emails, products, line_items, quotes, invoices, orders,
  commerce_payments, users.
- PARK, 18 keys (16 objects): communications, postal_mail, discounts, fees,
  taxes, carts, subscriptions, contracts, projects, goal_targets,
  feedback_submissions, partner_clients, partner_services, `0-162`,
  `0-410`, `0-420`, services, courses.

9 + 8 + 18 = 35 keys; 9 + 8 + 16 = 33 objects. The other six MINT concepts
and the non-object items (teams, tax rates, owners) come from APIs outside
`/objects/`.

## 4. Landing (D-ONT-4)

### 4.1 Order (resolves reviewer 3 BLOCK 1)

lance-graph CI runs the `lance-graph-ogar` parity tests against OGAR's
default branch, in both directions. So:

1. Open the OGAR PR and the lance-graph mirror PR together, cross-linked;
   verify the mirror locally against the OGAR branch.
2. Merge OGAR first (OGAR CI does not read the mirror).
3. Merge the mirror immediately after. Between the two merges every
   lance-graph PR is red on the reverse parity check — the `osm_street_node`
   window. Name it in both PR bodies.
4. HubSPO-rs D-ONT-5 starts only when both are on `main`.

### 4.2 `crm_actions` (resolves reviewer 3 BLOCK 2)

`resolve_hotplug` fails a consumer that is not an expected executor of any
table contributing to the ids it plugs, and fails an id with no rows. So:

- `crm_actions` subjects are **only** `0x0501`–`0x050F`; never an aliased
  concept, so other apps' hot-plugs on `EMAIL`, `PROJECT_ACTOR` or the
  commerce concepts are untouched.
- `CRM_EXPECTED_EXECUTORS = ["hubspo-rs"]`; capability names carry a `crm_`
  prefix.
- Seed rows, facts only, each backed by a measured endpoint: `crm_read` on
  all fifteen; `crm_merge` on contact, company, deal and ticket;
  `crm_gdpr_delete` on contact; `crm_enroll` on sequence. Create, update,
  archive, associate, assign and move-stage arrive with D-ACT-1, together
  with their guards.
- A test asserts every `crm_actions` subject is in
  `concepts_in_domain(Crm)`, plus the hot-plug round trip and a wrong
  consumer rejected.
- HubSPO-rs D-ONT-5's `HotPlug` lists Crm ids only; aliased concepts are
  bound through `RbacPlug`, as spear does with `EMAIL`.

### 4.3 RBAC requirements carried into D-ONT-5 (reviewer 3 FIX 4–6)

- Grants match the canon half and field masks are keyed by concept, so a
  role holding `PROJECT_ACTOR` also covers HubSpot users and teams.
  HubSPO-rs uses its own role namespace; its boot coverage set is
  `concepts_in_domain(Crm)` plus every `HUBSPO_ALIASES` target; a test proves
  no non-HubSpot role's mask applies under prefix `0x000B`.
- Users and teams share `0x0104`: team administration is separated from
  user administration by row scope on the `type` discriminator, not by
  class grant.
- `crm_custom_record` is default deny; per-schema access is row scope plus
  field mask, never a class grant alone; a test shows a grant on one schema
  does not reveal another.
- `EMAIL` sits in the Auth domain, which `is_access_control_class` treats as
  access control; logging a HubSpot email will need that tier. Known caveat.

### 4.4 Checklist

**OGAR**
- `ogar-vocab/src/lib.rs`: `ConceptDomain::Crm` with doc; arm `0x05`; the
  layout table and `Unassigned` doc; `source_domain_concept` `"crm"`;
  `CODEBOOK` rows; `class_ids` constants and `ALL`; one minimal
  `pub fn crm_*() -> Class` per concept (no attributes, no associations, no
  steps, no status enum — the address carries no behaviour), with a test
  asserting that; re-pin `canonical_concept_domain(0x0500)` and re-pin the
  email test's `0x0501` assert to `Crm` (keeping "email stays in Auth");
  the `concepts_in_domain(Crm)` count.
- `ogar-vocab/src/ports.rs`: `HubSpoPort` (`0x000B`), `HUBSPO_ALIASES`;
  prefix tests; add WeatherNext, Spear and HubSpo to the collision list;
  add Osm, WeatherNext, Spear and HubSpo to the `0x1000` guard.
- `ogar-vocab/src/crm_actions.rs` + `capability_registry::domain_tables()`.
- `ogar-class-view`: `all_canonical_classes()` rows.
- Docs: `APP-CLASS-CODEBOOK-LAYOUT.md` domain list and a `0x000B` row;
  `DISCOVERY-MAP.md` a `D-CRM-DOMAIN` entry recording the `0x05` choice and
  that `0x05`/`0x06` recipe-family bytes are a different numbering;
  `.claude/board/LATEST_STATE.md` concept-domain table corrected (it lists
  `0x04`–`0x06` as unassigned and calls `0x05` "Scope-kind reserved",
  conflating recipe families with domains).
- Public-repo hygiene: cite HubSpot's public developer docs and type ids
  only; no harvested JSON, spec filenames, sandbox/portal references,
  product-tier wording or "reverse-engineered" framing; fixtures synthetic.

**lance-graph** (`lance-graph-contract/src/ogar_codebook.rs`)
- `ConceptDomain::Crm` (same variant name; the parity test compares Debug
  strings), its arm, `CODEBOOK` rows, the `0x0500` re-pin.
- `AppPrefix`: add `Osm` `0x0008`, `WeatherNext` `0x0009`, `Spear` `0x000A`,
  `HubSpo` `0x000B` (confirm the mirror still stops at `0x0007` on `main`).
  `0x0006` stays unallocated: keep `from_prefix(0x0006) == None`, re-pin the
  `0x0008` assert, update the round-trip hand list.
- `lance-graph-ogar` parity green against the OGAR branch.

## 5. Open

- **O-1** Sandbox checks: do notes carry two or more associations; do users
  have secondary teams; do teams have a hierarchy (response schemas were not
  harvested).
- **O-2** Do logged email engagements carry the RFC Message-ID?
- **O-3** A shared calendar-event concept has three candidate consumers
  (spear events with iCalendar UID, Odoo `calendar.event`, the event behind
  a HubSpot meeting). Its own proposal; `crm_meeting` gains an edge to it
  then.
- **O-4** Odoo `crm.lead` holds lead and opportunity in one record; which
  of `crm_lead` / `crm_deal` it aliases to is decided when Odoo's CRM is
  harvested. `mail.activity` is a prospective second consumer for
  `crm_task`, `crm.team` one for a separate team concept.
- **O-5** Saved views: masks only, or converge on `PROJECT_QUERY`.
- **O-6** Odoo `res.partner` stays on `BILLING_PARTY`, which merges person,
  company and party; one real customer then has two concepts across
  HubSPO-rs and Odoo. Decided with O-4.

## 6. Council record

| Review | Verdict | Resolution |
|---|---|---|
| Research 1 — shipped code | alias hexes exact; no existing concept for any MINT row; `0x000B` free | used throughout |
| Research 2 — canon | `0x10` contradicts the altitude ruling; slot convention; checklist | operator moved CRM to `0x05`; §4.4 |
| Research 3 — convergence | notes ≠ comment; teams ≠ department; taxes object ≠ tax policy; party-edge gap | §2.2, §3.1, §3.4, §2.1 |
| Research 4 — numbers | every headline number matches; two PARK verb lists and the Forecasts claim corrected; `0x1000` hazard bounded to exactly `0x1000` | §3.4, §1 |
| Research 5 — external | HubSpot type ids; seven activity objects; stage permissions; sequences are shared records; OGIT sources weak | §2, §2.2 |
| Review 1 — grade inflation | BLOCK: `BILLING_PARTY` "role" reading; BLOCK: notes vs activity rule | §2.1, §1 + §2.2 |
| Review 2 — dilution/collapse | owners, carts, list definition vs membership, standard objects in custom record | §3.2, §2.3, §3.4 |
| Review 3 — non-negotiables | BLOCK: landing order; BLOCK: `crm_actions` subject fence; RBAC widening; custom-record default deny; hygiene | §4 |

**`0x05` availability, recorded.** Command, run 2026-10-10 against each
repo's `origin/main`, Rust sources only:
`git grep -nE '\b0x05[0-9A-Fa-f]{2}(_[0-9A-Fa-f]{4})?\b|ConceptDomain::Crm|0x05XX' origin/main -- '*.rs'`.
Hits: OGAR 5 (the layout doc, the `Unassigned` doc, the two tests re-pinned
in §4.4, and `recipe.rs`'s unrelated recipe-family doc); lance-graph 5 (the
`Unassigned` doc, the `0x0500` test, an `OwlIdentity` literal, a HIP/TWIG
tier value, both other axes); a2ui-rs 4 (RBAC tests using `0x0500` as an
ungranted concept — still ungranted, since `0x0500` is never minted); 0 in
HubSPO-rs, MedCare-rs, q2, odoo-rs, spear, tesseract-rs,
openproject-nexgen-rs, openstreetmap-website-rs, stalwart and rs-graph-llm.
woa-rs and smb-office-rs are not available in this environment and were not
searched.

---

## §7 Amendment (2026-10-10, during D-ONT-4) — `crm_actions` deferred to D-ACT-1

**DECISION.** The D-ONT-4 OGAR PR (AdaWorldAPI/OGAR#330) ships no
`crm_actions` table. §4.2's seed rows (`crm_read`, `crm_merge`,
`crm_gdpr_delete`, `crm_enroll`) move to D-ACT-1.

**BASIS.** OGAR's `geo_actions` rule: every action entry must correspond to a
primitive the consumer implements today. "Backed by a measured endpoint"
means HubSpot has the operation; it does not mean HubSPO-rs does, and today
HubSPO-rs implements none of them. Seeding the rows would declare a surface
nobody implements.

**SCOPE.** Everything else in §4.2 still applies when D-ACT-1 lands the table:
subjects only `0x0501`–`0x050F`, executors `["hubspo-rs"]`, `crm_` prefix, the
subject-fence test, the round trip and the wrong-consumer rejection.

**Consequence for D-ONT-5.** `resolve_hotplug` reports `NoCapabilitiesFor` for
a plugged id that has no capability rows. Until D-ACT-1, D-ONT-5 binds
`RbacPlug` and asserts that a `HotPlug` over the Crm ids fails with
`NoCapabilitiesFor`. The full `HotPlug` round trip moves to D-ACT-1.

**REVISIT WHEN** HubSPO-rs implements its first CRM operation (D-ACT-1).

**Landed:** OGAR#330 (domain, 15 concepts, `HubSpoPort`, docs) and the
lance-graph mirror AdaWorldAPI/lance-graph#1438 (codebook rows,
`AppPrefix::{Osm, WeatherNext, Spear, HubSpo}`). Merge order per §4.1.


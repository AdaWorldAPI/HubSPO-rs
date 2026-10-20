# LATEST_STATE — HubSPO-rs

> What exists right now. Update in the same commit as the change.

**As of 2026-10-10 (Wave 0 merged in #2; harvester merged in #3; mint dossier ratified as v3 by the 5+3 council; D-ONT-4 merged as OGAR#330 + lance-graph#1438; H-ADR-7 amended to drop ractor):**

## Workspace

- 18 crates, edition 2024, toolchain 1.99.0 (`rust-toolchain.toml`; bumped with the fleet 2026-10-10).
- 17 of them are empty on purpose: the crate layout from plan §4, with a
  module doc naming each crate's zone and deliverables.
- `hubspo-port` (D-ONT-5): `RBAC_PLUG` over the 15 CRM concepts + the 7
  core concepts HubSpot objects converge on; `bind()` resolves it against
  `OgarRbac` (minted in `ogar-vocab`, agrees with the contract mirror). No
  roles until D-IAM-1. `HOT_PLUG` over the CRM concepts is pinned to
  `NoCapabilitiesFor(crm_contact)` until D-ACT-1. `class_id` /
  `render_class_id` resolve HubSpot names through `HubSpoPort`. 13 tests.
  Disable runs 2026-10-10: dropping a CRM id (2 tests red), dropping an
  alias target (1 red), a `bind()` that skips the authority (2 red).
  **Known non-binding:** a stray `COVERED_CAPABILITIES` entry is not
  caught yet — `resolve_hotplug` stops at `NoCapabilitiesFor` before the
  coverage check; OGAR's `Undeclared` arm catches it once D-ACT-1 lands.
- `hubspo-guards` enforces Gate G0 as tests (rules in its `lib.rs` header).
  9 unit tests (each rule can fire and can stay silent) + 1 workspace scan.
  All six disable runs measured red on 2026-10-10:

  | disable | failing test |
  |---|---|
  | `id: String` added to `hubspo-schema` | `the_workspace_breaks_no_guard` |
  | `serde` added to `hubspo-store` | `the_workspace_breaks_no_guard` |
  | `datafusion` added to root deps | `the_workspace_breaks_no_guard` |
  | `hubspo-ui` removed from `ZONES` | `the_workspace_breaks_no_guard` |
  | `has_string_id` forced to `false` | `string_ids_fire` |
  | stalwart-source check removed | `forbidden_dependency_fires_in_every_spelling` |

  Added 2026-10-10: `ractor*`, `lance-graph-supervisor` and any `surreal*`
  dependency feature are forbidden. On its first run the rule found the root
  manifest still declaring `lance-graph-supervisor` (removed). Five more
  disable runs, all red (`cargo test -p hubspo-guards --lib`):

  | disable | failing test |
  |---|---|
  | `ractor` removed from the list | `forbidden_dependency_fires_in_every_spelling` |
  | `lance-graph-supervisor` removed | `forbidden_dependency_fires_in_every_spelling` |
  | `ractor` matched exactly, not as prefix | `forbidden_dependency_fires_in_every_spelling` (`ractor_cluster`) |
  | surreal-feature check never fires | `forbidden_dependency_fires_in_every_spelling` |
  | surreal-feature check fires on any `surreal` text | `allowed_dependencies_stay_silent` |

- CI: `.github/workflows/ci.yml` runs fmt, clippy (`--no-deps -D warnings`)
  and `cargo test --workspace` with debug info off.

## Filter compiler (D-QRY-1)

- `hubspo-store::catalog`: the production `Registrar` + `Binder` for Quack.
  One `Codebook` per coded field (label `i` → code `i`, unknown labels bind
  to nothing), plane `0` of every table is its live plane, nullable fields
  name a validity plane. Refuses duplicate fields / lanes / labels, a
  codebook on an integer field, and live-as-validity. 4 unit tests.
- `hubspo-query::filter::compile(object, groups, &binder) -> Filter`:
  HubSpot filter groups (AND within, OR across), operators EQ NEQ LT LTE GT
  GTE BETWEEN IN NOT_IN HAS_PROPERTY NOT_HAS_PROPERTY; CONTAINS_TOKEN is
  refused (text search is the Tantivy index). Values are HubSpot strings;
  integers are decimal `i32` until D-ONT-6 fixes lane encodings (amounts,
  timestamps). Three-valued over nullable properties: a record with no
  value is kept by no comparison, NEQ and NOT_IN included — whether HubSpot
  agrees is a value-oracle question (`ISS-ORACLE-PORTAL`).
- `tests/filter_oracle.rs`: 600 random filter-group sets over 5,000
  records (3 % deleted, 20–30 % missing values with random payloads left in
  the lanes) are compiled, lowered and run on mask-risc; every kept-row set
  equals a plain-loop reading (456 non-trivial, 122 empty, 22 all live).
  Disable runs 2026-10-10 (`cargo test -p hubspo-query --test filter_oracle`):

  | disable | red |
  |---|---|
  | `sql_where` dropped | oracle + missing-value test |
  | NOT_IN compiled as IN | oracle |
  | BETWEEN upper bound exclusive | oracle |
  | groups ANDed instead of ORed | oracle |
  | live plane dropped | oracle + presence test |
  | NOT_HAS_PROPERTY on a required property true | oracle + presence test |

  A seventh guard (`reads_value`, keeping presence-only properties off the
  nullable list) stayed green and was deleted: `sql_where` gates only
  comparison leaves, so it guarded nothing.
- Quack became consumable by git with lance-graph #1457
  (`ISS-LG-MASKRISC-PATH-NDARRAY`, resolved).

## Store, resident half (D-STO-1)

- `hubspo-store::table::Table`: one dataset (tenant × object type) as
  resident lanes (`U32` / `I32`), a live plane (plane 0) and one validity
  plane per nullable lane. `planes()` borrows them for mask-risc; no read
  copies a lane.
- Writes are `Batch`es (inserts, updates, deletes), validated whole and
  applied as exactly one version or not at all, and only for the table's
  owner (`MailboxId`, write on behalf). Refused: wrong owner, wrong arity,
  wrong kind, missing required value, unknown lane, a deleted or absent
  row, and a row written twice in one batch (update + update of one lane,
  update + delete, delete + delete; order inside a batch means nothing).
- `apply()` returns `Applied { version, first_insert, changed }`; `changed`
  is the plane `list::refresh` takes. Ordinals are never reused: inserts
  append, deletes clear the live bit and leave the row inert.
- Tests: 60 random batches match a plain model (every lane and plane, 200+
  updates, 30+ deletes); 12 refusals leave version, lanes and planes
  untouched (each refused batch also carries a valid insert, so a partial
  apply would show); `hubspo-query/tests/store_list.rs` runs catalog →
  filter → `list::refresh` on the store's own `changed` plane over 50
  batches and equals a full recomputation.
  Disable runs 2026-10-10, all red: owner unchecked, deletes missing from
  `changed` (red in the store and in the list test), delete keeps `live`,
  a cleared value keeps validity, version frozen, conflict check off,
  updates to deleted rows allowed, lane kind unchecked.
- Not yet: persistence (one Lance version per published batch, P-SPARSE-1
  base + delta) — the next half of D-STO-1.

## Store, persistence half (D-STO-1, feature `lance`)

- `hubspo-store::log::Log`: every batch is one Lance version of an
  append-only log (P-SPARSE-1's delta shape; nothing updated in place).
  Records: `BORN` (row), `SET` (row, lane, present, value as the lane's 32
  bits), `DELETE` (row), each stamped with version and owner.
- `commit(table, batch)`: check → append one Lance version → apply. Lance
  version = table version; the log's version is checked before the append
  (and after it). A refused batch writes nothing; an empty batch is
  refused (`StoreError::EmptyBatch`).
- `replay(owner, specs)`: rebuilds the table batch by batch through the
  same `Table::apply`; refuses a foreign owner, an undecodable record, and
  a log whose Lance version is not its batch count.
- Tests (`cargo test -p hubspo-store --features lance`, CI job
  `store-lance`, which installs `protoc`): 25 committed batches replay to
  an identical image (deletes and negative values included); one Lance
  version per batch; a second, stale writer is refused before it applies
  anything; refused and empty batches leave the log at its version;
  replay under another owner is refused.
  Disable runs 2026-10-10, red: no pre-append version check, owner ignored
  on replay, the I32 sign bit lost, deletes not logged, apply before the
  append. Green, and kept on purpose: the post-append version check — it
  catches only a writer that appends between the check and the write, a
  race no sequential test can schedule.
- Not yet: object stores. No index on `row`: only `Log::commit` writes a
  log dataset (ISS-LOG-NON-BATCH-VERSION). The log is never compacted
  (decision 2026-10-10): the append order is the version order.

## Property history (D-STO-5, feature `lance`)

- `Log::history(specs, row, col) -> Vec<Entry>`: every write of one
  property on one record, oldest first — the inserted value (absent
  included), each update, the deletion — each with its version and owner.
  No history store and no Lance read: one Quack program,
  `row = r ∧ ((lane = c ∧ op = SET) ∨ op = DELETE)`, on mask-risc over the
  log's resident records; only matches are decoded. `Log` keeps every record
  it commits or replays resident (29 B each). Lance is the persistence
  clock: `replay` reads it once, `commit` writes one version per batch.
  Rule applied: one write costs about a million folds (lance-graph
  2026-09-24); a Lance scan is recovery cost. Documented as a shared
  pattern in lance-graph `.claude/knowledge/split-tunnel-sparse-append-resident-fold.md`
  (#1466), with this crate as the reference implementation. The log's `op`
  and `lane` columns widened to `u32` for this (no data existed yet).
- Tests: for every record and both lanes over 30 random batches, the
  history equals one written down from the batches (anti-vacuity: updates,
  deletions, negative and absent values); unknown lane refused; empty log
  reads empty. Disable runs 2026-10-10, red: drop the row term, drop the
  DELETE arm, drop `op = SET`, lose the I32 sign. A sort by version was
  inert under a disable run and is removed: append order is version order.
  Replay dropped its sort too; a log out of order is still refused there,
  as a batch whose version is not the next one. Resident: a second log
  replayed from disk gives identical histories; a log whose records are not
  at the table's version refuses to commit; a refused replay leaves the
  resident records unchanged, so a corrupt record can never reach history.
  Disable runs 2026-10-10, all red: resident-version check off, records
  installed before replay validates, commit not appending to them.
- Timing (release, `examples/probe_history.rs`, 100k records, 1,000 versions × 1,000 updates, 1.2M log records, 2026-10-10): history **0.72 ms** median over 101 reads (0.64–1.99), inside P-HIST-1's < 20 ms; replay from Lance 532 ms, once at startup. Superseded: reading history from Lance took 336 ms (#24), 320 ms of it the scan.

## Board view (D-QRY-3, board half)

- `hubspo-query::board::Board`: per (pipeline, stage), record count and
  amount sum as two `GroupReduce` programs over the composite address
  `pipeline × stages + stage` (no row ids, no composite key lane).
  Records without an amount are counted, not summed; a stage whose records
  all lack one reads `amount: None`, never `0`. `weighted()` is exact in
  minor units × basis points (stage total × p = Σ record × p, tested).
- `tests/board_oracle.rs`: unfiltered, filtered and narrow boards equal a
  plain per-cell tally over 6,000 deals (3 pipelines × 7 stages, 4 % deleted,
  25 % without amount, one stage with no amounts at all).
  Disable runs 2026-10-10: sum without the amount validity (3 red), empty
  sum read as 0 (2 red), pipeline/stage swapped (3 red), zero group
  universe accepted (1 red), empty stages reported (1 red — the narrow-board
  test, added after this disable first stayed green: every earlier fixture
  occupied all 21 stages).
- Open (the saved-view half): view definitions as data, and sorting.
  Quack has no `ORDER BY` (order is storage order), so a table view sorted
  by a property needs its own decision.

## Lists (D-QRY-2)

- `hubspo-query::list`: every list operation is a Quack filter over planes
  the caller names, run by mask-risc (no row ids, no population loop).
  - `refresh(predicate, prev, changed)` = `(prev ∧ ¬changed) ∨
    (predicate ∧ changed)`: an active list re-reads the predicate only
    where records changed. Deletes need no special case (the compiled
    predicate carries `live`).
  - static lists: `add`, `remove`, and `members` (`∧ live` on every read);
  - history: `added` / `removed` between two stored versions.
- Invariant this rests on: **ordinals are never reused** (records are
  appended, the base is never rewritten; P-SPARSE-1). A store that reused
  ordinals would have to clear a deleted record's bit in every static list.
- `tests/list_oracle.rs`: 40 rounds of updates / inserts / deletes keep a
  refreshed list equal to full recomputation and to a plain loop (added /
  removed equal plain set differences, both > 100 over the run); a static
  list equals a plain set through adds, removes and deletes; refresh keeps
  stored bits outside the changed rows (> 1,000 of them disagree with the
  predicate on purpose).
  Disable runs 2026-10-10 (`cargo test -p hubspo-query --test list_oracle`):
  refresh keeping stale bits (2 red), refresh re-reading every row (1 red),
  `members` without `live` (1 red), `added` computing `removed` (1 red),
  `remove` keeping rows (1 red). A `live` gate inside `add` stayed green
  and was removed: `members` already applies it on every read.

## Harvest (D-ONT-1, public-catalog half)

- `scripts/fetch-hubspot-specs.sh` → `.cache/` (gitignored), then
  `cargo run -q -p hubspo-harvest -- report .cache/hubspot-specs`.
- Measured 2026-10-10: 123 APIs, 107 selected and read, 0 missing; 1,198
  operations, 472 on `/objects/` (0 unclassified), 726 outside; 35 object
  types; type ids `0-3` Deals, `0-162` Services, `0-410` Courses,
  `0-420` Listings.
- 13 unit tests. Five disable runs measured red (dated-release preference,
  preview exclusion, missing-spec recording, type-id recognition,
  dated version segment).
- Not covered yet: per-portal property, pipeline and label definitions
  (needs `ISS-ORACLE-PORTAL`).

## Standard property positions (D-ONT-6, provisional)

- `scripts/fetch-hubspot-properties.sh` fetches HubSpot's public
  object-definition pages (18 object types) and activity API guides (6) into
  `.cache/hubspot-properties` (gitignored), then runs `hubspo-harvest objdef`
  or `guide`, `merge` (append-only by internal name into
  `data/hubspot/standard_properties.tsv`, 1,413 rows) and `emit`
  (`crates/hubspo-generated/src/standard_properties.rs`). A test fails when
  the generated file and the table disagree.
- Each row: internal name, type, field type, group, sensitivity, hidden,
  label. Activity rows carry names only.
- Each row also stores a slot: `lane n` (mask position `n`, 1..=255) or
  `packed` (the object's packed run, mask position 0 in every object).
  New rows in a group listed in `PACKED_GROUPS` are packed; stored slots
  never move (`ISS-FIELD-MASK-256`, resolved by operator decision).
- `hubspo-schema::HubSpoClassView` keys each table by concept alone
  (classview `0x0000`). Fields, run included: contact 246 (27 packed),
  company 160, deal 98, ticket 108, lead 94, product 31, line item 59,
  call 17, meeting 30, task 9, note 5, email 16, commerce payment 93
  (pinned `== N`). Field 0 is the packed run, field `n` is lane `n`; the
  predicate is `hubspot-property:<object>/<name>`. Every contact property
  has a mask position. Every stated sensitivity is `non_sensitive`; the
  run's RBAC grain is open (`ISS-PACKED-RUN-RBAC-GRAIN`).
- Slot disable runs (2026-10-20): packed-group routing, dense lanes,
  lanes-only fields, packed position 0, run at field 0 all measured red.
  Breaking merge's own lane-overflow check left the test green: the table
  check refuses the 256th lane on its own, so that test pins the backstop.
- Provisional source (`ISS-KB-PROPERTIES-PROVISIONAL`). Measured 2026-10-20
  from the pages of that day; a re-run on the same pages left the table
  byte-identical.
- 18 tests (objdef 4, positions 6, schema 8). Eleven disable runs measured
  red (truncated page, guide table filter, non-blanking merge, append-only
  merge, duplicate name, name shape, past-255 addressing, field cap,
  shared concept, name predicate, generated-file drift).

## The documented API surface (D-API-0)

- `scripts/fetch-hubspot-surface.sh` fetches the API-reference overview page
  of each version tab (`legacy`, `latest`, `2027-03-beta`) into
  `.cache/hubspot-surface` (gitignored). Each page embeds its tab's whole
  navigation; `hubspo-harvest surface` reads every operation entry (title,
  `specs/<v>/<spec>.json METHOD /path`, page) into
  `data/hubspot/api_surface.tsv` (version, area, method, path, specs, title,
  page). Only coordinates and titles are committed.
- 2,279 endpoints: legacy 1,194, 2026-09 980, 2027-03-beta 105. The docs
  list 2,325 entries; 28 endpoints (generic `{objectType}` ones) are listed
  under several specs and are one row each, with all specs kept.
- `hubspo-harvest coverage <surface.tsv> <harvest.json>` counts, per version
  and area, the operations the stable OpenAPI harvest (`report`, 2026-03
  specs) holds by method and unversioned path, and those whose reference
  page also exists under the current tab. A legacy endpoint's path often
  changes shape in 2026-09 (`/crm/v3/objects/x` → `/crm/objects/2026-09/x`),
  so the page slug, not the path, is what says it is the same function.
- Measured 2026-10-20 (`coverage data/hubspot/api_surface.tsv
  .cache/harvest.json`): about **1,518 distinct functions** — 980 current,
  440 legacy-only, 98 beta-only. The stable harvest holds 912 of the 980
  current ones.

| area | current (2026-09) | in stable harvest | legacy only | beta only |
|---|---|---|---|---|
| crm | 506 | 473 | 212 | 3 |
| cms | 208 | 208 | 77 | 15 |
| marketing | 74 | 71 | 43 | 19 |
| automation | 31 | 24 | 20 | 10 |
| revenue | 22 | 22 | 0 | 28 |
| account | 42 | 41 | 6 | 0 |
| conversations | 22 | 6 | 25 | 0 |
| reporting | 0 | 0 | 13 | 22 |
| files | 6 | 6 | 13 | 0 |
| webhooks-journal | 19 | 19 | 0 | 0 |
| communication-preferences | 6 | 6 | 12 | 0 |
| webhooks | 8 | 0 | 9 | 0 |
| events | 12 | 12 | 0 | 0 |
| app-management | 11 | 11 | 0 | 0 |
| authentication | 3 | 3 | 5 | 0 |
| meetings | 6 | 6 | 0 | 1 |
| scheduler | 0 | 0 | 5 | 0 |
| data-studio | 4 | 4 | 0 | 0 |
| **total** | 980 | 912 | 440 | 98 |

- "Legacy only" is a count of reference pages with no current counterpart;
  it includes superseded versions of a function whose page was renamed, so
  it is an upper bound on legacy-only functions.
- 7 surface tests; six disable runs measured red (damaged entry, method
  check, spec merge, version-segment replacement, `v<digits>` rule,
  page-slug match). A Haiku `census-counter` run confirmed the per-version
  counts (AGENT_LOG 2026-10-20).

## The API surface by deliverable (D-API-0)

- `data/hubspot/surface_map.tsv` maps each of the 140 spec families
  (`crm-contacts`, `cms-pages`, …; `surface::family` strips directory,
  `.json` and version) to the D-id that owns it, or `UNPLANNED` with a
  reason. `hubspo-harvest deliverables <surface.tsv> <map.tsv>` counts the
  functions per D-id and refuses a family with no row.
- A test pins it to the committed data: every family in the surface has a
  row, no row names a family that is not there, every D-id is on
  STATUS_BOARD, and the functions sum to 1,518.
- The mapping is a judgement, recorded so it can be argued with row by row.
  Object CRUD of every object type (including types with no concept, like
  courses and listings) is D-API-1, which is why it holds a third of the
  surface. Measured 2026-10-20:

| deliverable | spec families | current | legacy only | beta only | functions |
|---|---|---|---|---|---|
| D-API-1 | 43 | 377 | 168 | 0 | 545 |
| D-MKT-4 | 21 | 208 | 77 | 0 | 285 |
| UNPLANNED | 19 | 99 | 30 | 5 | 134 |
| D-COM-1 | 10 | 39 | 3 | 28 | 70 |
| D-MKT-2 | 7 | 27 | 32 | 0 | 59 |
| D-MAIL-5 | 3 | 22 | 25 | 0 | 47 |
| D-QRY-2 | 1 | 31 | 1 | 0 | 32 |
| D-QRY-6 | 2 | 0 | 8 | 22 | 30 |
| D-MKT-3 | 1 | 26 | 1 | 0 | 27 |
| D-FLOW-1 | 2 | 0 | 16 | 10 | 26 |
| D-UI-1 | 1 | 0 | 24 | 0 | 24 |
| D-MKT-1 | 3 | 0 | 17 | 6 | 23 |
| D-IAM-4 | 2 | 19 | 2 | 0 | 21 |
| D-LOG-2 | 1 | 19 | 0 | 0 | 19 |
| D-API-2 | 1 | 8 | 9 | 0 | 17 |
| D-FLOW-3 | 1 | 17 | 0 | 0 | 17 |
| D-MAIL-10 | 2 | 13 | 4 | 0 | 17 |
| D-ACT-3 | 2 | 11 | 4 | 0 | 15 |
| D-SVC-2 | 1 | 0 | 0 | 15 | 15 |
| D-MAIL-6 | 2 | 8 | 5 | 1 | 14 |
| D-FLOW-4 | 1 | 9 | 4 | 0 | 13 |
| D-STO-3 | 1 | 9 | 2 | 0 | 11 |
| D-IAM-2 | 1 | 9 | 0 | 0 | 9 |
| D-API-3 | 1 | 3 | 5 | 0 | 8 |
| D-INT-3 | 1 | 0 | 0 | 8 | 8 |
| D-ONT-7 | 1 | 6 | 0 | 0 | 6 |
| D-MAIL-8 | 1 | 5 | 0 | 0 | 5 |
| D-SIM-2 | 1 | 5 | 0 | 0 | 5 |
| D-ACT-2 | 2 | 3 | 1 | 0 | 4 |
| D-ONT-6 | 1 | 2 | 2 | 0 | 4 |
| D-IAM-6 | 1 | 3 | 0 | 0 | 3 |
| D-INT-4 | 1 | 0 | 0 | 3 | 3 |
| D-IAM-5 | 1 | 1 | 0 | 0 | 1 |
| D-INT-6 | 1 | 1 | 0 | 0 | 1 |
| **total** | 140 | 980 | 440 | 98 | 1,518 |

- Unplanned, 134 functions in 11 groups (`ISS-SURFACE-UNPLANNED`):
  marketing events 37, app platform 22 (cards, extensions, feature flags,
  uninstalls), file manager 19, partner program 14, custom behavioural
  events 12, portal limits 9, record tags 6, answer-engine optimisation 5,
  data-studio ingestion 4, deal splits and forecasts 4, API metadata 2.
- 4 mapping tests; seven disable runs measured red (unmapped family,
  duplicate row, version-suffix strip, legacy-only by page slug, D-id not on
  the board, missing row, stale row).
- D-ids with no surface functions are internal (store, simulation, query
  engine, UI, CI) or have their API inside another D-id's families: D-SVC-1's
  tickets and feedback are D-API-1 CRUD, D-IAM-1's roles have no public API.

## Probe P-QDB-1 (Quack as the database; plan H-ADR-6, D-QRY-1)

`probes/p-qdb-1`. Command:
`target/probes/release/p-qdb-1 <tickets> [none|code|via|live] [clustered]`.
Five tables (`user` 20k, `ticket` N, `message` 4N, `delivery` 4N,
`document` N) are written to Lance, reloaded once into resident Arrow
columns, and read only through Quack: HubSPO-rs implements Quack's
`Registrar` and `Binder` (catalog + per-field codebook; upstream has no
production implementation), text binds once to `u32` codes, and mask-risc
executes. Every answer is asserted against a plain-loop oracle. Measured
2026-10-10, median of 15, release:

| query | 1M tickets, clustered | 3M tickets, clustered | 3M, random fk |
|---|---|---|---|
| Q1 tickets open ∧ pipeline (bound text) | 0.79 ms | 3.9 ms | — |
| Q2 … ∧ owner.team (fk join `EqU32Via`) | 3.1 ms | 9.3 ms | — |
| Q3 tickets GROUP BY status | 2.0 ms | 4.0 ms | — |
| Q4 unseen in one mailbox (full scan) | 5.7 ms | 15.1 ms | — |
| Q5 one ticket's thread (rows) | 2.8 ms | 7.3 ms | — |
| Q6 delivery GROUP BY outcome | 8.3 ms | 18.6 ms | — |
| Q7 bounced ∧ message.mailbox (fk join) | 8.6 ms | 25.1 ms | 253 ms |
| Q8 deliveries GROUP BY message.mailbox (20k groups) | 6.6 ms | 24.1 ms | 1,942 ms |
| Q9 invoices on open tickets (fk join, random fk) | 10.8 ms | 58.6 ms | — |

- Load: 3M tickets (30M rows across tables) write to Lance in 1.3 s and
  reload to resident columns in 1.0 s; bind + lower takes 0.013 ms.
- **The fk join cost is the foreign key's storage order, not Quack.** With
  `delivery.message` random, Q7/Q8 are 10–80× slower (253 ms / 1.9 s at 3M)
  and the plain loop is just as slow. Q9 (documents' ticket fk is random)
  is 58.6 ms at 3M. Rows that are gathered through an fk must be stored in
  fk order; delivery events arrive grouped by message, so this holds for
  delivery if the write lane keeps arrival order.
- Q7 under a random fk is 2× slower than the loop: the survivor skip is
  per 64-row word, and a 5% predicate leaves a survivor in ~96% of words,
  so the gather runs over nearly all rows; the loop short-circuits per row.
- Q4 scans all mail for one mailbox (15 ms at 12M rows); per-mailbox reads
  want the mailbox as a storage partition or sort key.
- Binding an unknown literal (`status = 'escalated'`) is `UnknownValue` and
  an unknown field is `UnknownField`; nothing is minted.
- Disable runs, each red at 200k tickets: `code` (swap two status codes in
  the codebook) fails Q1 (18,131 vs 47,941); `via` (join through the wrong
  foreign lane) fails Q2 (0 vs 910); `live` (drop the last live-plane word)
  fails Q1 (47,922 vs 47,941).

Verdict: Quack serves as the read engine for tickets, mail, delivery and
documents over Lance-resident columns, provided HubSPO-rs owns the catalog
and codebooks and stores fk-gathered tables in fk order. The API map this
rests on is `.claude/knowledge/comms-api-surface.md`.

## Probe P-LOG-1 (plan H-ADR-15 condition 4)

`probes/p-log-1` (standalone, needs `protoc`). Command:
`CARGO_TARGET_DIR=target/probes cargo build --release --manifest-path probes/p-log-1/Cargo.toml && target/probes/release/p-log-1 100000,1000000,4000000,16000000`.
Fixed change set at every N: 500 updates, 250 inserts, 250 deletes;
median of 5 runs, ms; every arm asserts the exact counts. Measured 2026-10-10:

| N | frags | `delta` inserted | `delta` updated | `delta` deleted | field diff | fragment diff |
|---|---|---|---|---|---|---|
| 100k | 3 | 8.86 | 8.75 | 1.09 | 1.91 | 1.38 |
| 1M | 3 | 63.86 | 61.74 | 1.12 | 9.06 | 1.51 |
| 4M | 6 | 231.60 | 228.16 | 1.26 | 22.39 | 1.39 |
| 16M | 18 | 843.49 | 912.89 | 1.80 | 39.20 | 1.38 |

Verdict: changes are derivable from the datasets alone. `delta()`'s
inserted/updated readers are table scans and fail the cost criterion; the
fragment diff (scan fragments added since v0, row id below v0's
`next_row_id` = update; diff shared fragments' deletion vectors) is flat and
exact, and is what D-LOG-2 uses. The field diff (`take_rows` at v0) grows
with fragment count, not N (39 ms at 16M). Disable run: scanning every
fragment instead of only the added ones → assertion red (99,750 "updates").
Not probed: compaction (`Rewrite`) versions between the two offsets.

## Probe P-SPARSE-1 (split-tunnel persistence arm; re-runs P-HIST-1 and P-LIST-1)

`probes/p-sparse-1`. Commands:
`target/probes/release/p-sparse-1 hist 1000000 1000 {100,1000}` and
`target/probes/release/p-sparse-1 list {1000000,10000000} {0,100} 1000`.
The alpha split tunnel (lance-graph `contract/src/alpha_tunnel.rs`) reads
a read-only base and writes a sparse overlay at the same addresses;
`AlphaOverlay` stamps attention and carries no payload, so this probe builds
the persistence half (lance-graph board entry 2026-09-23, "third probe
arm"): the base dataset is written once and never updated; each writer
version appends sparse records `(row, a, version, op)` to an append-only
delta dataset; reads merge base and delta, newest version wins. Measured
2026-10-10:

| hist (1M rows, 1,000 versions) | 100 per version | 1,000 per version |
|---|---|---|
| delta records | 100k | 1M |
| write all versions | 9.2 s | 12.4 s |
| scan, 1,000 fragments | 218.5 ms | 256.0 ms |
| BTree index on `row`, 1,000 fragments | 16.9 ms (build 0.22 s) | 15.0 ms (build 0.46 s) |
| compacted to 1 fragment, scan | 6.0 ms | 27.5 ms |
| compacted, indexed (compact + index 0.43 / 1.69 s) | 3.3 ms | 5.9 ms |

| list | prefill versions | delta fragments | step ms | alloc KiB |
|---|---|---|---|---|
| 1M | 0 | 1 | 0.85 | 251 |
| 1M | 100 | 101 | 1.45 | 257 |
| 10M | 0 | 1 | 1.68 | 252 |
| 10M | 100 | 101 | 1.93 | 257 |

Verdict: **P-HIST-1 passes** (16.9 ms indexed vs 20 ms; 3.3–5.9 ms once
the delta is compacted), and writes are ~25–45× cheaper than in-place
updates (~300 s). **P-LIST-1 passes on both counts**: allocation is flat in
population and in delta history, because there are no deletion vectors to
read. Compacting the delta is safe (append-only, no deletions); the base is
never rewritten, so history does not depend on keeping old Lance versions.
Every history read is asserted (101 values) and every list step against a
merge-on-read oracle. Disable runs: reading the wrong row's delta → red;
ignoring the delete flag → red (the first fixture's deletes carried
`a = 0`, which cleared the bit either way, so that disable stayed green;
deletes now target list members and carry the old value). Not covered:
records appended after the last index build are scanned unindexed; one
value column only (several properties = one column each, P-CUSTOM-1); the
delta grows without bound until a policy folds it into a new base.

## Probe P-FLOW-1 (plan H-ADR-7)

`probes/p-flow-1`. Command:
`CARGO_TARGET_DIR=target/probes cargo build --release --manifest-path probes/p-flow-1/Cargo.toml && target/probes/release/p-flow-1 1000000 200`.
1M contacts `(id, stage, score, region)`, stable row ids; 1,000 workflows,
each triggering on `stage == s AND score >= t AND region == r`. Per trial a
writer commits 1,000 updates as one version; the consumer then opens the
latest version, fragment-diffs from its stored offset (P-LOG-1), evaluates
all 1,000 criteria over the changed rows, appends the enrolments as one
Lance version and advances its offset. Every trial's enrolments are
asserted against the generator. Measured 2026-10-10:

| contacts | trials | p50 ms | p99 ms | max ms | enrolments / version | fragments at end |
|---|---|---|---|---|---|---|
| 1M | 50 | 11.20 | 19.44 | 19.44 | 2,326 | 51 |
| 1M | 200 | 13.82 | 27.13 | 31.10 | 2,445 | 201 |

Verdict: **passes** (budget 1 s p99), with ~35× headroom. The time grows
mildly with fragment count (each catch-up loads two manifests). Not in
the measurement: starting the graph-flow runs (an enrolment here is a
row), the version watcher's own wake-up latency, and re-enrolment rules.
The criteria are a plain loop (1,000 × 1,000 evaluations per version)
standing in for Quack programs. Disable run: `>=` → `>` in the criteria
→ assertion red at trial 0; restored green.

## Probe P-AUTH-1 (plan H-ADR-9)

`probes/p-auth-1`. Command:
`CARGO_TARGET_DIR=target/probes cargo build --release --manifest-path probes/p-auth-1/Cargo.toml && target/probes/release/p-auth-1`.
10M rows `(id, owner, amount)`, 1,000 owners, read = `sum(amount)`. Arms
run interleaved, 21 iterations; overhead = per-iteration ratio to the
unscoped read, median [p25, p75]. Every arm's sum is asserted. Measured
2026-10-10:

| scope | share | unscoped ms | owner check | word-wise scope mask | take masked rows | kernel only: plain / mask ms |
|---|---|---|---|---|---|---|
| everything | 100 % | 81.0 | +45.7 % [+40.7, +62.0] | −7.3 % [−17.2, +12.8] | — | 6.80 / 6.90 |
| team-owned | 20 % | 82.5 | +42.7 % [+37.6, +59.5] | −1.7 % [−13.4, +12.2] | — | 6.83 / 10.30 |
| owned-only | 0.1 % | 81.0 | +49.2 % [+37.5, +67.9] | −4.5 % [−10.6, +9.0] | −80.9 % [−82.5, −78.3] | 7.77 / 3.85 |

Verdict: **passes, under conditions.** The scan itself varies by more than
the 10 % budget between iterations on this VM (IQR ±15 %), so the
end-to-end ratios cannot resolve it; the in-memory kernel delta can: the
word-wise mask adds 0.1–3.5 ms to a ~81 ms read (≤ 4.3 %) and is faster
than the plain sum for a sparse scope. Conditions: the scope is a stored
mask (H-ADR-5 algebra), not a per-row owner check (+43–49 %, it reads the
`owner` column); and a scope under 5 % of rows takes the masked rows
instead of scanning (−81 %). A bit-by-bit mask loop (branchy or
branchless) lands at +12–25 %, so the kernel shape matters. The role
decision per request (`ogar-rbac` authorize) is not in the measurement.
Disable run: summing zero mask words as if full → assertion red;
restored green.

## Probe P-TEN-1 (plan §5.5)

`probes/p-ten-1`. Command:
`CARGO_TARGET_DIR=target/probes cargo build --release --manifest-path probes/p-ten-1/Cargo.toml && target/probes/release/p-ten-1 1000,10000`.
One dataset of 1,000 rows per tenant; for 1,000 random tenants, time open
+ the first 100 rows of a column, each tenant opened for the first time in
the process; the OS page cache is dropped before each arm. Every read
asserts the tenant's own data. Measured 2026-10-10, ms:

| tenants | fresh session p50 / p99 / max | shared session p50 / p99 / max |
|---|---|---|
| 1,000 | 1.91 / 5.20 / 64.17 | 2.22 / 5.27 / 15.48 |
| 10,000 | 2.50 / 7.77 / 51.70 | 2.13 / 5.01 / 14.04 |

Verdict: **passes** (budget 50 ms p99). A shared `Session` removes the
fresh-session tail (max 14 ms vs 52–64 ms). Not covered: object storage
(the real latency floor in production), and tenant datasets with many
versions and fragments (P-HIST-1 shows the first read of a version costs
more as fragments grow). Disable run: reading tenant t+1's expected data
from tenant t → assertion red; restored green.

## Probe P-CUSTOM-1 (plan H-ADR-3)

`probes/p-custom-1`. Command:
`CARGO_TARGET_DIR=target/probes cargo build --release --manifest-path probes/p-custom-1/Cargo.toml && target/probes/release/p-custom-1 1000000 50,500,2000`.
1M rows; each row fills 20 of the K custom properties plus a standard
`amount`. Lanes: one nullable `i32` column per property. Packed: parallel
`List<u16>` positions and `List<i32>` values. Fold = sum of one property,
asserted against the generator; median of 5 runs. Measured 2026-10-10:

| K | standard ms | lanes ms (× std) | packed ms (× std) | write lanes s | write packed s | lanes MB | packed MB |
|---|---|---|---|---|---|---|---|
| 50 | 16.17 | 15.66 (0.97) | 565.75 (35.0) | 1.2 | 1.3 | 54.7 | 42.6 |
| 500 | 12.08 | 19.40 (1.61) | 218.23 (18.1) | 33.2 | 1.3 | 268.4 | 50.1 |
| 2,000 | 43.34 | 31.61 (0.73) | 639.53 (14.8) | 96.1 | 1.7 | 347.3 | 55.1 |

Verdict: **lanes pass the fold budget at every K; the packed run fails at
every K** (it scans all 20M entries and filters by position). Lanes pay at
write time (33–96 s vs ~1.5 s) and on disk (5–6×), and the standard fold
itself slows on a 2,000-column dataset (43 ms vs 12–16 ms). The sums are
a scalar loop in both layouts, not a Quack fold. Not probed: adding a
custom property to a live dataset (a new column) and bulk import time at
tenant scale. Disable run: folding the wrong position in the packed arm →
assertion red; restored green.

## Probe P-LIST-1 (plan H-ADR-5)

`probes/p-list-1`. Command:
`CARGO_TARGET_DIR=target/probes cargo build --release --manifest-path probes/p-list-1/Cargo.toml && target/probes/release/p-list-1 100000,1000000,4000000,10000000`.
Stored mask = one bit per stable row id; predicate `a >= 900`; 1,000
changes (500 updates flipping membership, 250 inserts, 250 deletes);
median of 5 runs. Each run asserts the incremental mask equals a full
recompute. Measured 2026-10-10:

| N | fragments | members | full scan ms | step ms | alloc KiB (deletes + added) |
|---|---|---|---|---|---|
| 100k | 3 | 9,988 | 5.7 | 1.70 | 971 |
| 1M | 3 | 99,988 | 24.2 | 2.24 | 743 + 224 |
| 4M | 6 | 399,990 | 69.3 | 2.07 | 1,357 + 224 |
| 10M | 12 | 999,990 | 196.4 | 2.64 | 2,580 + 224 |

Verdict: **time passes** (2.6 ms against 10 ms). **Allocation is not
independent of the population**: the added-row scan is flat (224 KiB), but
`delta().get_deleted_row_ids()` costs ~200 KiB per fragment whose deletion
vector changed, and 250 random deletes touch every fragment. It is bounded
by min(changes, fragments), not by rows. A leaner delete path (deletion-
vector diff + row-id sequence per touched fragment, no anti-join) needs
Lance's row-id sequence loader, which is not public. The predicate is a
plain loop over 750 changed rows standing in for the Quack program.
Disable run: skipping the delete clears → assertion red; restored green.

## Probe P-HIST-1 (plan §5.2)

`probes/p-hist-1`. Command:
`CARGO_TARGET_DIR=target/probes cargo build --release --manifest-path probes/p-hist-1/Cargo.toml && target/probes/release/p-hist-1 100000,1000000`.
1,000 versions × 100 random updates (`a = a + 1`); the target record is in
every 10th batch, so it changes 100 times; median of 3 runs, ms. Every arm
asserts the same 101 values. Measured 2026-10-10:

| N | frags | time travel | change chain | address index | concurrent (32) | checkouts |
|---|---|---|---|---|---|---|
| 100k | 1001 | 38,626 | 2,803 | 115.8 | 45.4 | 0 |
| 1M | 1001 | 147,166 | 14,887 | 109.0 | 46.1 | 0 |

Verdict: **fails the 20 ms budget.** Where the time goes (split, 1M rows):
checkout 0.2–3 ms; the first `take_rows` on a freshly checked-out version
91–178 ms (it builds that version's stable-row-id map); a second take
about 1 ms. The address index (per-record `(version, row address)`; old
copies read with `read_physical_slice`, which ignores deletions) skips the
map and is flat in N, but costs about 1 ms per read sequentially and 0.45
ms concurrently, one fragment open each. A record changed in all 1,000
versions would take about 450 ms. Disable run: pointing every index entry
at the latest copy → assertion red (100 × 101). Not probed: compaction
(fragment leaves the manifest; the arm then checks out the version, a path
these runs never hit) and version cleanup, which deletes the old files.

## Mint dossier (D-ONT-3)

`.claude/plans/hubspo-concept-mint-dossier-v1.md`: 15 MINT, 10 ALIAS rows,
6 DATA, 6 PARK rows, 5 DEFER, six questions for the OGAR council. Every
harvested object-type name is accounted for (script-checked).

## Decisions

All open decisions OD-1..OD-11 in plan §10 were approved by the operator on
2026-10-10, as recommended. Cross-repo follow-ups they create are tracked in
`ISSUES.md`.

## Not built

Everything from Wave 1 on. No record can be stored, queried or rendered yet.

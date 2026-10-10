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

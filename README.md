# HubSPO-rs

**HubSpot, reverse-engineered and rebuilt on the AdaWorldAPI stack.**

Not a connector and not a clone of HubSpot's internals. HubSpot is, under
the hood, four things — an object graph, a rule engine, a communication
hub, and a reporting/AI layer. HubSPO-rs keeps those shapes and replaces
the machinery:

| HubSpot's shape | Here |
|---|---|
| Objects, properties, associations, property history | lance-graph rows keyed by a 16-byte GUID and typed by an OGAR classid; history is the Lance version log |
| Workflows, sequences, pipelines, scoring, SLAs | OGAR `ActionDef` + `KausalSpec` (behaviour as data), run by graph-flow under ractor supervision |
| Email, inbox, meetings, contact sync | stalwart (separate process) + spear (columnar mail and document hub) |
| Lists, views, reports, dashboards | masks and folds over Quack / mask-risc / `lance-graph-report` |
| Dedup, lookalikes, scoring, suggestions | `ndarray::simd` / `hpc` and lance-graph-arm-discovery |
| Users, teams, permissions | dir-sim + ogar-rbac, field-level masks applied before anything reaches the wire |

Every bulk change (imports, merges, mass edits, workflow backfills) is
simulated first and shown as a reviewable plan; reverting is restoring the
previous version.

The customer-facing surface stays boring on purpose: a HubSpot CRM
v3-shaped REST API, so existing integrations keep working.

## Status

Wave 0 (foundation) is in progress. See `.claude/board/STATUS_BOARD.md`.

- Plan: `.claude/plans/hubspo-integration-v1.md`
- Feature coverage (160 HubSpot features): `.claude/plans/hubspo-feature-matrix-v1.md`

## Crates

| Crate | Role |
|---|---|
| `hubspo-port` | OGAR port + hot-plug / RBAC declarations |
| `hubspo-schema` | ClassView skins, property positions, tenant custom schema |
| `hubspo-generated` | generator output (never hand-edited) |
| `hubspo-store` | write lanes, versions, KV values, association edges, list masks |
| `hubspo-query` | filters, lists, views, reports |
| `hubspo-actions` | the CRM action set, pipeline guards, calculated properties |
| `hubspo-flow` | workflows, sequences, SLAs |
| `hubspo-sim` | simulate → validate → diff → plan for bulk changes |
| `hubspo-iam` | roles, scopes, field masks, directory actors |
| `hubspo-intel` | dedup, lookalikes, scoring, suggestions |
| `hubspo-comms` | stalwart and spear adapters, tracking |
| `hubspo-docs` | files and documents |
| `hubspo-compat` | HubSpot-API-shaped REST, importer, oracle client |
| `hubspo-ui` | screens |
| `hubspo-server` | the HTTP host |
| `hubspo-parity-fixtures` | oracle fixtures (test only) |
| `hubspo-harvest` | schema harvester (tool) |
| `hubspo-guards` | structural guards run as tests |

## Build

```sh
cargo test --workspace
```

Toolchain 1.99.0 is pinned in `rust-toolchain.toml`. See `CLAUDE.md` for
the full check set.

## License

MIT OR Apache-2.0. stalwart (AGPL) is never linked into this codebase; it
runs as a separate service.

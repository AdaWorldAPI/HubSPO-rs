# CLAUDE.md — HubSPO-rs

This repository follows the **deep reverse-engineering + reimagine on steroids** philosophy (same as Bardioc-rs and Hiro-rs).

## Core Principles

1. **Understand first, reimagine second**
   - Deeply reverse engineer HubSpot’s real data shapes, workload patterns, state machines, and supervision needs.
   - Do **not** try to replicate HubSpot’s internal architecture.

2. **Reimagine using our stack**
   - Use `lance-graph` + `ndarray` as the unified substrate (replacing JanusGraph + ClickHouse + Gremlin).
   - Use `Ractor` for actor/supervision patterns (replacing OTP/BEAM).
   - Apply `spear` OGIT + OWL + DOLCE aggressively for strong invariants.

3. **Touch grass**
   - The value of reverse engineering is to understand the *shape* of the problem so we can build something significantly better, simpler, and more powerful.

4. **Clean separation**
   - Keep ontology (`hubspo-ontology`), graph implementation (`hubspo-graph`), models, engine, invariants, and compatibility layer clearly separated.
## Active plan (read first)

`.claude/plans/hubspo-integration-v1.md` — architecture decisions (H-ADR-1..14),
ground truth of every sibling repo, waves W0–W11 with gates, probes, open
decisions for the operator. `.claude/plans/hubspo-feature-matrix-v1.md` maps
all 160 HubSpot features to a substrate and a D-id.

Short form of the rules it sets:

- Identity is `NodeGuid` + OGAR classid (`concept << 16 | HubSpoPort` prefix),
  pulled through the port — never `String` ids, never a local codebook.
- Behaviour is OGAR `ActionDef` + `KausalSpec`, run by graph-flow under ractor;
  no DDL events, no SurrealQL.
- Queries are Quack / mask-risc / `lance-graph-report`; no new DataFusion code.
- stalwart runs out of process (AGPL); spear is the in-process mail hub.
- Every bulk change is simulated first (crm-sim: version → validate → diff → plan).
- The current six crates do not compile and are replaced in Wave 0.

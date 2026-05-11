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
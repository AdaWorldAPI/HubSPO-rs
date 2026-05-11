# AdaWorldAPI/HubSPO-rs

**HubSpot Reverse Engineering** using the modern spear + lance-graph stack.

## Philosophy

This repository is **not** a normal HubSpot connector.

It is a deep reverse-engineering effort to model HubSpot’s domain ontologically, using:

- Palantir Foundry-style ontology modeling
- `lance-graph` as the core graph + columnar storage layer
- `spear` OGIT + OWL + DOLCE for strong semantic invariants
- `ndarray` for analytical workloads

## Structure

- `hubspo-ontology` — Core ontological modeling of HubSpot objects, associations, properties, and pipelines
- `hubspo-graph` — lance-graph specific implementations and traversals
- `hubspo-models` — Strongly typed domain models aligned with OGIT/OWL/DOLCE
- `hubspo-engine` — Reverse-engineered business logic and behavior
- `hubspo-compatibility` — Thin layer to talk to real HubSpot APIs (for validation & comparison)
- `hubspo-invariants` — OWL/DOLCE-style invariants and validation rules
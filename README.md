# AdaWorldAPI/HubSPO-rs

**HubSpot Reverse Engineering** — reimagined on steroids using the modern spear stack.

## Core Philosophy

This is **not** a normal HubSpot connector or integration layer.

**HubSPO-rs** follows the same approach as **Bardioc-rs** and **Hiro-rs**:

1. **Deep 1:1 reverse engineering** of the original system to understand its real shape:
   - Data model (objects, associations, properties, pipelines, timelines)
   - Workload patterns and access shapes
   - State machine / pipeline / rule behavior
   - Concurrency and supervision needs

2. **Reimagine it on steroids** using our modern stack:
   - `lance-graph` + `ndarray` instead of JanusGraph + ClickHouse + Gremlin
   - `Ractor` instead of OTP/BEAM
   - `spear` OGIT + OWL + DOLCE for strong semantic invariants
   - Much simpler, faster, more correct, and more powerful

The goal is to **touch grass** — deeply understand what HubSpot actually does under the hood, then build something significantly better using our stack instead of trying to replicate the old architecture.

## Structure

| Crate                    | Purpose                                              |
|--------------------------|------------------------------------------------------|
| `hubspo-ontology`        | Core ontological modeling of HubSpot domain          |
| `hubspo-graph`           | lance-graph implementation + traversals              |
| `hubspo-models`          | Strongly typed models (OGIT/OWL/DOLCE aligned)       |
| `hubspo-engine`          | Reverse-engineered business logic & behavior         |
| `hubspo-compatibility`   | Thin adapter to real HubSpot APIs (validation only)  |
| `hubspo-invariants`      | OWL/DOLCE-style semantic invariants & validation     |
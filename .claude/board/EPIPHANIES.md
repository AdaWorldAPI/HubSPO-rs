# EPIPHANIES — HubSPO-rs

> Admission gate (lance-graph closeout rule): an entry must be NEW,
> LOAD-BEARING and DURABLE, and must still matter if the mistake that led to
> it had never happened. Corrections and lessons are one closeout line, not
> an entry here. Prepend new entries; edit only `Status`.

## E-HUBSPO-BULK-CHANGE-IS-A-DIRECTORY-SIMULATION-1 (2026-10-10)

- **Status:** WORKING-MODEL (H-ADR-10; untested until D-SIM-1)
- **Claim:** the dir-sim pipeline (observed version → rules → validate →
  diff → `ExecutionPlan`) is not specific to directories. Any bulk CRM
  mutation — import, merge, mass edit, workflow backfill, pipeline
  restructure — has the same shape, and running it that way makes every
  bulk change reviewable before commit and reversible after it, which
  HubSpot's bulk operations are not.
- **Basis:** `OGAR/docs/DIRECTORY-SIMULATION-POC.md` §1–§3;
  `lance-graph-dir-sim` (versions as `Arc<Snapshot>` + overlay, invariants
  as Quack programs).
- **Revisit when:** P-LIST-1 or the D-SIM-1 gate shows overlay versions do
  not scale to record populations (dir-sim caps each population at 65,536).

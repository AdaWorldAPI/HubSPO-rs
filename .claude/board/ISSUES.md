# ISSUES — HubSPO-rs

> Open blockers and cross-repo follow-ups. Append new entries at the top;
> change only the `Status` line of an existing entry.

## ISS-OGAR-CRM-PORT — HubSpoPort and the CRM concept domain do not exist

- **Status:** OPEN (blocks D-ONT-5 and every classid in Wave 1)
- **Where:** OGAR `crates/ogar-vocab` (ports, `class_ids`, `ConceptDomain`,
  `capability_registry::domain_tables`), lance-graph `ogar_codebook` mirror.
- **What:** approved 2026-10-10 (OD-1, OD-2): port `0x000B`, domain
  `0x10XX`, a `crm_actions` domain table. Concepts mint only after the
  D-ONT-3 dossier passes the 5+3 codebook council.
- **Next:** the dossier (`hubspo-concept-mint-dossier-v1.md`) goes to the
  OGAR 5+3 council; then the OGAR PR (D-ONT-4).

## ISS-SPEAR-HUB-REHOME — spear's D-HUB-1/2/3/7 move to HubSPO-rs

- **Status:** OPEN
- **Where:** spear `.claude/plans/spear-outer-integration-v1.md` §3.1,
  `.claude/board/STATUS_BOARD.md`.
- **What:** approved 2026-10-10 (OD-5, H-ADR-12). spear keeps D-HUB-4/5/6/8
  (activity, send receipts, timeline query, round-trip test); the object
  model moves here. spear's plan and board need a supersession note.

## ISS-SPEAR-DATAFUSION-RULE — spear still mandates DataFusion `Expr`

- **Status:** OPEN
- **Where:** spear `CLAUDE.md`, "Architecture Invariants" rule 1.
- **What:** approved 2026-10-10 (OD-7): new code uses Quack; DataFusion is
  in a grace period (lance-graph `CLAUDE.md`, 2026-09-05).

## ISS-LG-PREFLIGHT-STALE-PINS — consumer preflight whitelists old lance pins

- **Status:** OPEN
- **Where:** lance-graph `.claude/knowledge/ogar-consumer-preflight.md`.
- **What:** it whitelists `lance =9.0.0 / lancedb =0.33.0`; lance-graph
  `CLAUDE.md` floats `12.*` / `0.39.*`. Approved 2026-10-10 (OD-11): follow
  `CLAUDE.md`, fix the preflight upstream.

## ISS-STALWART-DIRSIM-WIRING — stalwart's dir-sim backend is unreachable

- **Status:** OPEN (blocks D-MAIL-1)
- **Where:** stalwart `crates/directory` (fork), `crates/main` features,
  config.
- **What:** the `dir-sim` recipient backend exists but nothing constructs
  it, `crates/main` does not forward the feature, and it cannot
  authenticate (needs OIDC/LDAP alongside). Fork-only work; stalwart's
  `AGENTS.md` forbids upstream contributions by agents.

## ISS-ORACLE-PORTAL — no HubSpot sandbox portal yet

- **Status:** OPEN (blocks D-ONT-1 against a live portal, D-STO-6, every value-oracle gate)
- **What:** OD-10 — the operator names the sandbox portal and holds its
  token. Public OpenAPI specs can be harvested without it.

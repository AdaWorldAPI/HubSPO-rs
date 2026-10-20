# AGENT_LOG — HubSPO-rs

> Haiku runs and their checks (`.claude/rules/agent-model-tiers.md` rule 6).
> Newest first; one writer, the orchestrating session.

## Retired

(none: no card has failed twice on one kind of task)

## 2026-10-20 — merge harvest, lance-graph `a1abdf8f..abfa9e2b` (79), OGAR (5), ndarray (2), a2ui-rs (2)

- **Cards:** 4 × `harvest-scout` (git-diff harvest), 1 × `quote-harvester` (D-IAM-3 sources).
- **harvest-scout × 4: not run — orchestrator error, not a card failure.** The briefs asked for `git log` / `git diff`; the card has Read, Glob, Grep only. All four reported BLOCKED, wrote nothing and invented nothing. Lesson for briefs: a git-history harvest is a script the orchestrator runs (the index here came from one bash pass); a Haiku card gets files to read. Not counted toward rule 7.
- **quote-harvester:** 3 ranges FOUND. Checked by reading (2): `rbac_plug.rs:184` `with_class_field_defaults`, `selection.rs:335` `walk_rails_authorized` — both exact. Wrong: 0. Missed: 0.

## 2026-10-14 — merge harvest, lance-graph `c21bd165..a1abdf8f` + ndarray `3842ed6..1ecbccb`

- **Card:** ad-hoc read-only harvest brief (before the cards were installed here); model haiku.
- **Asked:** per merge, PR, summary, stat, name hits for 19 HubSPO-relevant names, plan/entry status lines; two yes/no questions.
- **Checked by reading (3):** preflight line 45 still `lance 12.*` — correct; `D-CMR` absent from `lance-graph-contract/src` (0 files) — correct; Haiku cards present in lance-graph — present.
- **Wrong (1):** #1552 reported as adding four cards; it added five (`census-counter.md` came in `170aa6ae`). Cause: the stat tail was truncated.
- **Missed:** none found.

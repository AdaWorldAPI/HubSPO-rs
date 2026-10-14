# Agent Model Tiers — Haiku for supervised harvest and mapping

DECISION (operator, 2026-10-14): use Haiku agents for grindwork, scoped to
cheap harvest discovery and deep mapping, and watch them closely. The
canonical statement lives in lance-graph
(`.claude/knowledge/tiered-agent-execution-protocol.md` § Haiku harvest
tier); this file is the HubSPO-rs copy of the rules that apply here. The
five cards in `.claude/agents/` are ndarray's, unchanged.

## Tiers

| tier | does | never |
|---|---|---|
| Opus (orchestrator) | plans, decides, reviews, compiles, lints, tests, commits | mechanical loops |
| Sonnet | edits from a spec, drafting from a complete brief | synthesis across sources, gate decisions |
| Haiku | read-only harvest, deep mapping, exact spec edits | authoring content, conclusions, review, cargo, git |

Only the orchestrator runs cargo (CLAUDE.md § Build: worker agents never run
cargo; the orchestrating session builds once).

## The five Haiku cards

- `harvest-scout`: candidates as `path:line` + the matching line.
- `census-counter`: raw counts, each with the exact command.
- `quote-harvester`: verbatim text with line numbers; FOUND / NOT FOUND.
- `substrate-mapper`: a fact map of one module or directory, written to one
  file in the session scratchpad.
- `spec-editor`: exact edits to one file, each applied only if its anchor
  occurs exactly once.

## Watching closely

1. Harvest is navigation: search finds, reading decides. The orchestrator
   Reads every item it relies on; a Haiku list is never cited as evidence.
2. Re-run at least one reported count per `census-counter` reply.
3. A `hits: 0` is never taken as absence; Read the place the thing would live.
4. For every map, Read at least two mapped items at their stated lines.
5. Read the diff of every `spec-editor` edit before committing.
6. Record each run in `.claude/board/AGENT_LOG.md`: card, items checked,
   items wrong, items missed.
7. A card that misses, miscounts or concludes twice on one kind of task is
   not used for that kind again; that kind goes to Sonnet.

## What makes a brief Haiku-sized

One question; a closed search space (exact paths or commit ranges); patterns
written out with their aliases; a hit cap; nothing that needs two sources
held in mind together.

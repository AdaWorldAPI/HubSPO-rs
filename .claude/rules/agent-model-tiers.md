# Agent Model Tiers — Haiku for supervised harvest and mapping

DECISION (operator, 2026-10-14): use Haiku agents for grindwork, scoped to
cheap harvest discovery and deep mapping, and watch them closely. The
canonical statement lives in lance-graph
(`.claude/knowledge/tiered-agent-execution-protocol.md` § Haiku harvest
tier); this file is the HubSPO-rs copy of the rules that apply here. The
five cards in `.claude/agents/` are ndarray's, except that
`census-counter` has no shell here (see § Access).

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
- `census-counter`: raw counts, each with the exact Grep, Glob or Read query.
- `quote-harvester`: verbatim text with line numbers; FOUND / NOT FOUND.
- `substrate-mapper`: a fact map of one module or directory, written to one
  file in the session scratchpad.
- `spec-editor`: exact edits to one file, each applied only if its anchor
  occurs exactly once.

## Access

Haiku cards may write local files only, never to GitHub or the network.

- No card has `Bash`. A shell can run `git push`, `gh` or `curl`, and a tool
  allowlist cannot restrict which commands run; so the shell stays with the
  orchestrator. Anything needing git history or a pipeline is run by the
  orchestrator, not delegated.
- No card lists an MCP tool (no `mcp__github__*`, no Linear). The `tools:`
  line is an allowlist; a card that needs one is out of scope for Haiku.
- Writes: `spec-editor` edits the one file named in its brief;
  `substrate-mapper` writes the one output file named in its brief, in the
  session scratchpad. No other card writes.
- Commits, pushes, PRs and comments are the orchestrator's alone.

## Watching closely

1. Harvest is navigation: search finds, reading decides. The orchestrator
   Reads every item it relies on; a Haiku list is never cited as evidence.
2. Re-run at least one reported count per `census-counter` reply, using the
   query it reports.
3. A `hits: 0` is never taken as absence; Read the place the thing would live.
4. For every map, Read at least two mapped items at their stated lines.
5. Read the diff of every `spec-editor` edit before committing.
6. Record each run in `.claude/board/AGENT_LOG.md`: card, items checked,
   items wrong, items missed.
7. A card that misses, miscounts or concludes twice on one kind of task is
   not used for that kind again; that kind goes to Sonnet.
8. Before dispatching a card, read `.claude/board/AGENT_LOG.md` and its
   **Retired** list. A retirement under rule 7 is recorded there as one
   line, `card × task kind → Sonnet`, so a fresh session sees it.

## What makes a brief Haiku-sized

One question; a closed search space (exact paths or commit ranges); patterns
written out with their aliases; a hit cap; nothing that needs two sources
held in mind together.

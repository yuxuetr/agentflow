# A7 — changelog-writer

**Status**: TODO (scaffold only)
**Tracking entry**: [`EXAMPLES_TODOs.md` § A7](../../../EXAMPLES_TODOs.md#a7--changelog-writer)

## Business

Input: a git tag range (e.g. `v1.0.0..HEAD`).
Output: a markdown changelog section grouping commits by
conventional-commits type (`feat`, `fix`, `docs`, `chore`, `refactor`,
`test`, `perf`, `ci`), with each entry rewritten in user-facing tone.

**This is AgentFlow eating its own dogfood** — every release of
AgentFlow itself uses this application to draft the changelog
section in `docs/RELEASE_NOTES_<version>.md`.

## Architecture (planned)

```
shell_node: git log <range> --pretty=format:'%h|%s|%b' --no-merges →
  llm classify_and_rewrite (group by conventional-commits type,
                            rewrite each into user-facing prose) →
  template render (markdown section) →
  file write / append to CHANGELOG.md or RELEASE_NOTES_<ver>.md
```

## External dependencies

| Dep | Why |
| --- | --- |
| `git` on PATH | Source of truth for commit log |
| LLM provider | Classification + rewriting (mock provider works for dry-runs) |

**No paid API key required for development** — the mock provider
suffices for testing the workflow shape; real provider only kicks in
when you want a usable changelog draft.

## What this validates in AgentFlow

- `shell` node admission + OS sandbox limiting to `git`-only commands
  (verifies sandbox actually constrains shell, not just claims to)
- LLM with structured output (commits → categorised JSON)
- Template node rendering markdown from structured input
- File append vs overwrite semantics
- **Promotion path**: if this becomes part of every release, it
  graduates to a first-class `agentflow changelog` CLI subcommand
  (would be tracked under a new `P3.x` line in `TODOs.md`).

## Findings during dogfooding

_Pending implementation._

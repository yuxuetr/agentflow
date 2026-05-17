# A6 — doc-translator

**Status**: TODO (scaffold only)
**Tracking entry**: [`EXAMPLES_TODOs.md` § A6](../../../EXAMPLES_TODOs.md#a6--doc-translator)

## Business

Input: a markdown folder (e.g. `docs/`) + target language list
(e.g. `["en", "ja", "zh"]`).
Output: per-language sibling folders (`docs.en/`, `docs.ja/`, `docs.zh/`)
preserving original directory structure, with markdown formatting,
code fences, links, and heading hierarchy untouched and only the
prose translated.

## Architecture (planned)

```
discover *.md files in input folder →
  map (parallel, one task per file × language) →
    template → llm translate →
    file write to <target_lang>/<relative_path>
  fan in: emit summary table (files translated × languages × status)
```

Translation pairs are independent so the `map` node runs them in
parallel up to the configured concurrency cap, respecting LLM provider
rate limits.

## External dependencies

| Dep | Why |
| --- | --- |
| LLM provider | Translation engine (Anthropic / OpenAI recommended for long-form quality) |

## What this validates in AgentFlow

- `map` node parallel execution (real fan-out: N files × K languages)
- Template rendering for system prompts
- Concurrency cap + rate-limit backoff
- Partial-failure tolerance (one file's failure doesn't abort the rest)
- Checkpoint resume — kill mid-batch, restart, skip already-completed
  file/lang pairs
- Stability at scale (verify with 100+ file fanout before DONE)

## Findings during dogfooding

_Pending implementation._

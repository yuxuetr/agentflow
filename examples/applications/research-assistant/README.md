# A3 — research-assistant

**Status**: TODO (scaffold only)
**Tracking entry**: [`EXAMPLES_TODOs.md` § A3](../../../EXAMPLES_TODOs.md#a3--research-assistant)

## Business

Configure a list of arxiv topics + keywords (e.g. `["LLM agents",
"Rust async runtime", "diffusion models"]`); the assistant periodically
fetches new papers, reads abstracts, diffs against the local RAG
index of papers it has already seen, and produces a weekly markdown
briefing covering "what's new this week, why it matters, related to
papers X / Y you've already read".

## Architecture (planned)

```
schedule (weekly) →
  arxiv_node (fetch new papers by topic) →
  diff against EntityFactStore ("which papers have I already seen?") →
  rag_ingest (new ones) →
  llm summarize + cross-reference →
  file write briefing.md
```

## External dependencies

| Dep | Why |
| --- | --- |
| Arxiv API | Free, no key |
| LLM provider | For abstracts → summaries + cross-references |
| Optional: Qdrant | Vector RAG; BM25 mode works without it |

## What this validates in AgentFlow

- `arxiv` node (existing AgentFlow node)
- RAG ingest + retrieve loop using actual user-authored content
- `MemoryStore` layers:
  - `SqlitePreferenceStore` → keeps the topic list per user
  - `SqliteEntityFactStore` → "I've already seen paper X"
- Scheduled runs (via cron or AgentFlow's own scheduling once that
  ships for non-interactive contexts)
- LLM long-form writing

## Findings during dogfooding

_Pending implementation._

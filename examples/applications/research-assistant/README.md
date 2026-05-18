# A3 — research-assistant

**Status**: WIP — live ✅ (2026-05-18, iteration 1: cs.AI / 5 papers, ~20s first run, ~1s dedup-only re-run).
**Tracking entry**: [`EXAMPLES_TODOs.md` § A3](../../../EXAMPLES_TODOs.md#a3--research-assistant)

## Business

Fetch recent papers from an arxiv category, dedupe against a local
"seen papers" SQLite store so periodic runs only see genuinely new
arrivals, and use one LLM call to render a structured markdown
briefing with Highlights / per-paper blocks / Clusters.

## Architecture: L1 binary with persistent state

```
┌──────────────────┐    Vec<Paper>    ┌──────────────┐    Vec<Paper>    ┌──────────────┐
│ FetchArxivNode   │ ──── via bus ──▶ │ DiffSeenNode │ ──── via bus ──▶ │ BriefingNode │
│  GET arxiv API   │                  │   SQLite     │                  │ one-shot LLM │
│   parse Atom     │                  │  diff +      │                  │  → markdown  │
│                  │                  │  mark seen   │                  │  → file      │
└──────────────────┘                  └──────────────┘                  └──────────────┘
                                              │
                                              ▼
                                      ┌─────────────────────────────────────────┐
                                      │ ~/.agentflow/state/research-assistant.db│
                                      │  EntityFact rows:                       │
                                      │   entity_id = "arxiv:<category>"        │
                                      │   fact_id   = arxiv paper id            │
                                      │   value     = {title, published, url}   │
                                      └─────────────────────────────────────────┘
```

Three custom `AsyncNode`s share an `Arc<Mutex<Option<Vec<Paper>>>>`
bus to hand off the in-memory paper list without round-tripping
through `FlowValue::Json` serialization. The Flow's `dependencies`
ordering ensures fetch → diff → briefing serial execution.

## Why L1 binary (not L3 skill)

Per the [L1+L3 R2 reflection rule](../../../docs/L1_L3_REFLECTION_R2_2026-05-18.md):

- **Pass-through axis: HIGH**. The category (`cs.AI`), max-results
  count, state-db path, and output path all need to reach tool calls
  verbatim. Per A7's lesson, L3 skill agents substitute these with
  hallucinated examples.
- **Decision density: LOW**. The pipeline is fixed: fetch → dedupe →
  summarize → write. No agent branching.

Both axes point at L1. Wraps one HTTP fetch + one SQLite read/write
+ one LLM call.

## What this validates in AgentFlow

- `agentflow-memory::SqliteEntityFactStore` end-to-end (open, query,
  insert, persist across runs) for a real "track-by-id" use case
- `agentflow_llm::AgentFlow::model(...).prompt(...).execute()`
  one-shot LLM pattern (also used by A7) on `moonshot-v1-128k`
- Cross-workspace path deps to `agentflow-memory` (in addition to
  `agentflow-core` + `agentflow-llm` already used by A1/A7)
- 3-node DAG with a shared in-memory bus pattern for handing off
  non-JSON data (avoids serialization roundtrip cost when nodes are
  same-process and the payload is structured Rust types)

## External dependencies

| Dep | How to satisfy |
| --- | --- |
| Arxiv search API | Public, no auth, free (no key needed) |
| LLM API key | Default model `moonshot-v1-128k`; needs `MOONSHOT_API_KEY`. Auto-loaded from `~/.agentflow/.env` (P9.3). |
| SQLite | Bundled with `agentflow-memory` deps |

## Files

```
research-assistant/
├── README.md                 # ← this file
├── Cargo.toml                # standalone Cargo project, path deps + reqwest + quick-xml + chrono
├── src/
│   ├── main.rs               # CLI + 3-node Flow + shared bus + tokio entry
│   ├── arxiv_fetch.rs        # HTTP GET arxiv Atom + serde XML parse
│   ├── seen_store.rs         # SqliteEntityFactStore wrapper for dedup
│   └── briefing.rs           # one-shot LLM prompt + render
```

## Run

```bash
cd examples/applications/research-assistant
# MOONSHOT_API_KEY auto-loaded from ~/.agentflow/.env

# First run — every paper is "new", LLM summarizes all:
cargo run --release -- \
  --category cs.AI \
  --max-results 30 \
  --output /tmp/arxiv-cs-AI.md

# Subsequent runs — only NEW papers since last run get summarized.
# State persists in ~/.agentflow/state/research-assistant.db
cargo run --release -- --category cs.AI

# Override state file (useful for testing or per-user separation):
cargo run --release -- \
  --category cs.CL \
  --state /tmp/test-state.db \
  --output /tmp/arxiv-cs-CL.md
```

CLI flags:

| Flag | Default | Notes |
| --- | --- | --- |
| `--category <cat>` | (required) | e.g. `cs.AI`, `cs.CL`, `math.ST`, `stat.ML` |
| `--max-results <N>` | 30 | Arxiv API max is 2000; > 30 is rarely useful for daily briefings |
| `--output <path>` | `/tmp/arxiv-briefing.md` | Markdown briefing file |
| `--state <path>` | `~/.agentflow/state/research-assistant.db` | Dedup SQLite (created on first run) |
| `--model <name>` | `moonshot-v1-128k` | Any agentflow-llm provider model |

## First-run observations (2026-05-18, cs.AI, 5 papers)

- **Wall clock**: ~20s for the first run (1.3s arxiv fetch + 21ms
  SQLite dedup + 19s LLM briefing call). The LLM dominates.
- **Re-run wall clock**: 0.9s (all-dedup, no LLM call) — confirms
  the SQLite-backed dedup loop works as designed for cron-style
  periodic invocation.
- **Output quality**: Coherent briefing markdown with all 5 sections
  (Highlights / All papers / Clusters). Each per-paper block has
  authors / published / link / summary. Minor: the prompt's
  `<abs_url>` placeholder got copied as literal markdown
  `[abs_url](http://...)` syntax — model rendered the placeholder
  text rather than substituting cleanly. Cosmetic; could tighten
  the prompt to say "link as bare URL".
- **EntityFact storage**: 5 facts written to
  `~/.agentflow/state/research-assistant.db`, per-paper denormalized
  snapshot (title + published + abs_url) so future tools can
  reference seen papers without re-fetching arxiv.
- **No regressions in tests**: 11 unit tests covering all 3 modules
  (Atom parse / id extraction / whitespace / store dedup / store
  isolation / store idempotence / prompt building) pass hermetically.

## What's not in iteration 1

Tracked as future iterations to keep first cut shippable:

- **Cross-reference via RAG**: original A3 spec called for RAG
  indexing of paper abstracts so the briefing can call out "this
  builds on paper X from last week". Adds `agentflow-rag` +
  embeddings (probably Qdrant or local ONNX). Iteration 2.
- **Scheduled run**: original spec called for weekly cron-style
  scheduling. Current binary is one-shot; schedule via OS cron /
  systemd timer / `agentflow harness /schedule`. Iteration 2 (or
  just document the cron line in this README).
- **Per-user preference store**: original spec mentioned
  `SqlitePreferenceStore` for "which topics am I subscribed to".
  Currently configured via CLI flag. Could move to a preferences
  file for multi-category runs. Iteration 2.
- **Smarter "since" tracking**: currently dedup is by paper-id only;
  no notion of "since date X". For arxiv this is fine because
  papers don't get republished, but for other sources (RSS feeds)
  date-based filtering would be needed.

## Findings during dogfooding

See [`EXAMPLES_TODOs.md` § A3 Findings](../../../EXAMPLES_TODOs.md#a3--research-assistant)
for the live list.

# Yanshi SDK Example Matrix

This directory and the per-crate `examples/` folders together form the
canonical SDK example matrix for v1. Each row below maps a spec capability
to one (or more) runnable example. The matrix is the index — open the
referenced file for the runnable code, comments, and run commands.

## Conventions

- **Offline by default.** Every example below runs against the mock
  LLM provider out of the box (`Yanshi::init_with_config(...)` with
  `vendor: mock`). No network calls leave the machine.
- **Opt into live providers.** Set `YANSHI_LIVE_PROVIDER=1` and
  configure real API keys (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`,
  etc.) when an example documents a live path. The mock path is what
  CI exercises.
- **Per-crate compile contract.** Each example must compile under its
  owning crate's default + relevant feature set. The Quality CI
  `features` matrix (`.github/workflows/quality.yml`) covers the most
  common combinations; `cargo check --workspace --examples` is the
  catch-all locally.
- **LLM-judgement output is non-deterministic — run multiple times,
  union the findings** (F-A2-5). Examples whose value depends on the
  LLM's *judgement* (code review, content critique, evaluation,
  scoring) will produce materially different outputs on repeated runs
  against the same input — A2's dogfooding caught 5 issues on run 1
  and 7 different issues on run 2 with only **1 in common**, both
  runs correct. This is intrinsic to LLM sampling, not a bug. Don't
  treat any single run as definitive: for human consumption, prefer
  3-5 runs and union the findings; for automated gates, define
  acceptance on quorum (e.g. "≥2 of 3 runs flag the same issue") not
  on a single pass. Examples whose value comes from the LLM's
  *generation* (summarisation, translation, briefing) are usually
  fine on one run because the output is wholly produced rather than
  filtered down from a larger candidate space.
- **Translation workflows: always guard `source_lang != target_lang`
  before LLM dispatch** (F-A6-4). Asking a translation model to
  "translate to {lang}" when the source is already in `{lang}` is a
  null-op that the model can't represent — moonshot-v1-128k's
  observed behaviour on en→en was to switch to Chinese (any non-en
  language reads as a more plausible request than the
  unintelligible identity). The trap doesn't show as a crash; it
  silently corrupts one cell of an N×K fan-out. Fix at the
  workflow level: either filter `input_list` to exclude degenerate
  pairs (the doc-translator example does this for its English
  source — see [`examples/applications/doc-translator/workflow.yml`](applications/doc-translator/workflow.yml)),
  or add a Tera guard in the per-iteration prompt builder
  (`{% if item.lang != source_lang %} ... {% endif %}`). Either
  way, validate the language pair before paying for the LLM call.

## Matrix

| # | Capability | Example | Crate | Status |
| -- | --- | --- | --- | --- |
| 1 | DAG workflow with Map / While | [`yanshi-cli/examples/ai_research_assistant.yml`](../yanshi-cli/examples/ai_research_assistant.yml) | `yanshi-cli` | ✓ |
| 2 | DAG workflow embedding `AgentNode` | [`yanshi-cli/examples/workflows/skill_agent_hybrid.yml`](../yanshi-cli/examples/workflows/skill_agent_hybrid.yml), [`hybrid_workflow_agent.rs`](../yanshi-agents/examples/hybrid_workflow_agent.rs) | `yanshi-cli`, `yanshi-agents` | ✓ |
| 3 | ReAct agent with native tool calling | [`agent_native_react.rs`](../yanshi-agents/examples/agent_native_react.rs), [`react_agent.rs`](../yanshi-agents/examples/react_agent.rs) | `yanshi-agents` | ✓ |
| 4 | PlanExecute agent | [`plan_execute_agent.rs`](../yanshi-agents/examples/plan_execute_agent.rs) | `yanshi-agents` | ✓ |
| 5 | Multi-agent handoff supervisor | [`multi_agent_handoff.rs`](../yanshi-agents/examples/multi_agent_handoff.rs) | `yanshi-agents` | ✓ |
| 6 | Multi-agent blackboard supervisor | [`multi_agent_blackboard.rs`](../yanshi-agents/examples/multi_agent_blackboard.rs) | `yanshi-agents` | ✓ |
| 7 | Multi-agent debate supervisor | [`multi_agent_debate.rs`](../yanshi-agents/examples/multi_agent_debate.rs) | `yanshi-agents` | ✓ |
| 8 | SkillBuilder direct API | [`skill_calls_mcp_tool.rs`](../yanshi-skills/examples/skill_calls_mcp_tool.rs) | `yanshi-skills` | ✓ |
| 9 | MCP client + tool invocation | [`simple_client.rs`](../yanshi-mcp/examples/simple_client.rs) | `yanshi-mcp` | ✓ |
| 10 | RAG ingest + query + (eval via CLI) | [`phase4_indexing_demo.rs`](../yanshi-rag/examples/phase4_indexing_demo.rs), [`phase5_advanced_retrieval.rs`](../yanshi-rag/examples/phase5_advanced_retrieval.rs), `yanshi rag eval <dataset>` | `yanshi-rag`, `yanshi-cli` | ✓ |
| 11 | Tracing JSONL (and OTel export hook) | [`simple_tracing.rs`](../yanshi-tracing/examples/simple_tracing.rs) | `yanshi-tracing` | ✓ JSONL; OTel exporter wired but no dedicated example yet (follow-up) |
| 12 | Tool policy + sandbox capability decision | [`tool_policy_sandbox_demo.rs`](../yanshi-tools/examples/tool_policy_sandbox_demo.rs) | `yanshi-tools` | ✓ (added under P3.1) |

## Ecosystem / scenario-level demos

The `examples/ecosystem/` tree pulls multiple capabilities together for
end-to-end scenarios:

- `skills/` — official `SKILL.md` samples (`code-reviewer`,
  `research-assistant`, `multimodal-content-analyzer`).
- `plugins/` — official subprocess plugin samples (`echo`,
  `data-transform`).
- `marketplace/` — remote marketplace manifest example.
- `workflows/` — config-first hybrid workflow tying DAG / Agent / MCP /
  RAG / Skill / Trace together.

See [`examples/ecosystem/README.md`](ecosystem/README.md) for the full
walk-through and the dry-run + live-run commands.

## Running every example locally

```bash
# Compile every example in every workspace crate without running them.
cargo check --workspace --examples

# Run a specific Rust example (each one auto-initialises the mock provider).
cargo run -p yanshi-agents --example agent_native_react
cargo run -p yanshi-tools --example tool_policy_sandbox_demo

# Validate every YAML workflow example without execution.
cargo run -p yanshi-cli -- workflow validate examples/ecosystem/workflows/hybrid_offline_demo.yml --strict
```

## Follow-ups (tracked as P3.1 follow-ups, not blocking)

- Dedicated OTel-export example showing how to wire the OTLP exporter
  documented under `yanshi_tracing::otel`. Today the JSONL example
  exercises the most common path; the OTel exporter is already covered
  by the `trace_context_propagation` integration test in
  `yanshi-llm/tests/`.
- `yanshi rag eval` is the canonical entry for the eval row; a
  small Rust example invoking the eval runner directly would round
  out the RAG row.
- Per-example smoke CI lands under P3.2 / P3.10 / P7.3.

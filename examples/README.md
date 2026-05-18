# AgentFlow SDK Example Matrix

This directory and the per-crate `examples/` folders together form the
canonical SDK example matrix for v1. Each row below maps a spec capability
to one (or more) runnable example. The matrix is the index — open the
referenced file for the runnable code, comments, and run commands.

## Conventions

- **Offline by default.** Every example below runs against the mock
  LLM provider out of the box (`AgentFlow::init_with_config(...)` with
  `vendor: mock`). No network calls leave the machine.
- **Opt into live providers.** Set `AGENTFLOW_LIVE_PROVIDER=1` and
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

## Matrix

| # | Capability | Example | Crate | Status |
| -- | --- | --- | --- | --- |
| 1 | DAG workflow with Map / While | [`agentflow-cli/examples/ai_research_assistant.yml`](../agentflow-cli/examples/ai_research_assistant.yml) | `agentflow-cli` | ✓ |
| 2 | DAG workflow embedding `AgentNode` | [`agentflow-cli/examples/workflows/skill_agent_hybrid.yml`](../agentflow-cli/examples/workflows/skill_agent_hybrid.yml), [`hybrid_workflow_agent.rs`](../agentflow-agents/examples/hybrid_workflow_agent.rs) | `agentflow-cli`, `agentflow-agents` | ✓ |
| 3 | ReAct agent with native tool calling | [`agent_native_react.rs`](../agentflow-agents/examples/agent_native_react.rs), [`react_agent.rs`](../agentflow-agents/examples/react_agent.rs) | `agentflow-agents` | ✓ |
| 4 | PlanExecute agent | [`plan_execute_agent.rs`](../agentflow-agents/examples/plan_execute_agent.rs) | `agentflow-agents` | ✓ |
| 5 | Multi-agent handoff supervisor | [`multi_agent_handoff.rs`](../agentflow-agents/examples/multi_agent_handoff.rs) | `agentflow-agents` | ✓ |
| 6 | Multi-agent blackboard supervisor | [`multi_agent_blackboard.rs`](../agentflow-agents/examples/multi_agent_blackboard.rs) | `agentflow-agents` | ✓ |
| 7 | Multi-agent debate supervisor | [`multi_agent_debate.rs`](../agentflow-agents/examples/multi_agent_debate.rs) | `agentflow-agents` | ✓ |
| 8 | SkillBuilder direct API | [`skill_calls_mcp_tool.rs`](../agentflow-skills/examples/skill_calls_mcp_tool.rs) | `agentflow-skills` | ✓ |
| 9 | MCP client + tool invocation | [`simple_client.rs`](../agentflow-mcp/examples/simple_client.rs) | `agentflow-mcp` | ✓ |
| 10 | RAG ingest + query + (eval via CLI) | [`phase4_indexing_demo.rs`](../agentflow-rag/examples/phase4_indexing_demo.rs), [`phase5_advanced_retrieval.rs`](../agentflow-rag/examples/phase5_advanced_retrieval.rs), `agentflow rag eval <dataset>` | `agentflow-rag`, `agentflow-cli` | ✓ |
| 11 | Tracing JSONL (and OTel export hook) | [`simple_tracing.rs`](../agentflow-tracing/examples/simple_tracing.rs) | `agentflow-tracing` | ✓ JSONL; OTel exporter wired but no dedicated example yet (follow-up) |
| 12 | Tool policy + sandbox capability decision | [`tool_policy_sandbox_demo.rs`](../agentflow-tools/examples/tool_policy_sandbox_demo.rs) | `agentflow-tools` | ✓ (added under P3.1) |

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
cargo run -p agentflow-agents --example agent_native_react
cargo run -p agentflow-tools --example tool_policy_sandbox_demo

# Validate every YAML workflow example without execution.
cargo run -p agentflow-cli -- workflow validate examples/ecosystem/workflows/hybrid_offline_demo.yml --strict
```

## Follow-ups (tracked as P3.1 follow-ups, not blocking)

- Dedicated OTel-export example showing how to wire the OTLP exporter
  documented under `agentflow_tracing::otel`. Today the JSONL example
  exercises the most common path; the OTel exporter is already covered
  by the `trace_context_propagation` integration test in
  `agentflow-llm/tests/`.
- `agentflow rag eval` is the canonical entry for the eval row; a
  small Rust example invoking the eval runner directly would round
  out the RAG row.
- Per-example smoke CI lands under P3.2 / P3.10 / P7.3.

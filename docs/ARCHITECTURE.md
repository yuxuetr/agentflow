# AgentFlow Architecture

Last updated: 2026-05-09

> **Direction note (2026-06-20):** this document describes the *current* L1–L4
> layered structure. The workspace is migrating (in place, no rewrite) to a
> narrow-waist **contract kernel** that converges the four execution paradigms
> (static DAG / native loop / harness / dynamic workflow). See
> `docs/RFC_CRATE_ARCHITECTURE.md` for the target design,
> `docs/ARCHITECTURE_EVALUATION_2026-06-20.md` for the dependency-graph
> validation, and `TODOs.md` §P-A for execution. Until that track lands, the
> model below is accurate.

AgentFlow is a Rust workspace for deterministic workflow execution and agent-native
runtime loops. The project is organized around a small core engine and separate
crates for nodes, LLM access, tools, Skills, MCP, memory, tracing, visualization,
and the CLI/server surfaces.

All workspace crates use Rust 2024 edition.

## Layered Mental Model

The workspace crates fall into four layers:

```text
+----------------------------------------------------------+
| L4 Operations / Productization                          |
|   tracing · viz · server · db · worker                  |
+----------------------------------------------------------+
| L3 Agent / Orchestration                                 |
|   agents · skills · cli                                  |
+----------------------------------------------------------+
| L2 Capability Adapters                                   |
|   nodes · llm · tools · mcp · rag · memory               |
+----------------------------------------------------------+
| L1 Execution Core                                        |
|   core (Flow / GraphNode / FlowValue / scheduler)        |
+----------------------------------------------------------+
```

L1 is the only execution kernel. L2 capabilities reach L3 either as
`AsyncNode` implementations (DAG path) or as tools/clients consumed by
`AgentRuntime` (agent-native path). L4 is observation/operation cross-cutting.

## Runtime Model

AgentFlow supports two complementary execution styles:

- **DAG workflows**: `agentflow-core::Flow` runs explicit graph nodes with declared
  dependencies, input mappings, optional conditions, checkpoints, retry, timeout,
  resource limits, and health primitives. Two execution modes are available:
  - `FlowExecutionMode::Serial` (default): topological order, one node at a time.
  - `FlowExecutionMode::Concurrent`: dependency-ready dispatch via
    `FuturesUnordered` with a configurable `max_concurrency` window. Nodes whose
    dependencies are all `Ok(_)` or `NodeSkipped` are launched immediately.
- **Agent loops**: `agentflow-agents::AgentRuntime` records observe, plan, tool
  call, tool result, reflection, and final answer steps. ReAct, plan/execute, and
  multi-agent examples are built on this runtime. Each run produces an
  `AgentRunResult` with a structured `AgentStopReason` (one of: final answer,
  stop condition, max steps, max tool calls, timeout, cancelled, token budget,
  error).

The intended direction of composition is:

```text
Flow -> AgentNode -> AgentRuntime -> ToolRegistry -> Tool / MCP / WorkflowTool
```

Use workflows for deterministic production automation. Use agents when the next
step depends on model reasoning, tool feedback, memory, or reflection. Use
`AgentNode` when a workflow needs one agent-driven step, and use workflow tools
when an agent should call a stable DAG as a tool.

YAML can declare both styles: `llm` / `template` / `http` / `file` / `map` /
`while` and so on for DAG nodes; `agent` / `skill_agent` for agent-native
nodes that build a `ReActAgent` from a Skill manifest at run time.

## Workspace Crates

| Crate | Role |
| --- | --- |
| `agentflow-core` | Pure workflow engine, node abstractions, `FlowValue`, scheduling, retry, timeout, checkpoint recovery, resource controls, health checks, and execution events. |
| `agentflow-nodes` | Config-first node implementations such as `llm`, `template`, `http`, `file`, `arxiv`, audio, image, MCP, RAG, `map`, and `while`. |
| `agentflow-llm` | Model configuration, provider clients, streaming, multimodal helpers, discovery, and model registry support. |
| `agentflow-cli` | User-facing commands for workflow run/validate/debug, config, LLM model discovery, MCP, Skills, tracing, audio, image, and optional RAG operations. |
| `agentflow-agents` | Agent runtime plus ReAct, plan/execute, supervisor, `AgentNode`, workflow tool integration, and shared agent utilities. |
| `agentflow-tools` | Built-in tool interfaces, registry, sandbox and permission policy, file/http/shell/script tools. |
| `agentflow-skills` | Skill loading, `SKILL.md` parsing, manifests, registry indexes, marketplace files, MCP tool discovery, and Skill builder integration. |
| `agentflow-mcp` | MCP stdio transport, client sessions, tools, resources, prompts, retry, and builder APIs. |
| `agentflow-rag` | RAG abstractions including vector store and reranking modules. |
| `agentflow-memory` | Session, SQLite, semantic memory types, and memory store abstractions. |
| `agentflow-tracing` | Structured trace events, file storage, redaction, replay, OpenTelemetry integration, and terminal timeline inspection. |
| `agentflow-db` | SQLx database layer with migrations, models, and repository traits for runs, steps, events, artifacts, Skill installs, and MCP sessions. |
| `agentflow-server` | Axum gateway with health endpoints, run submission/query routes, SSE event streams, Skill routes, bearer auth, Web UI embedding, and distributed scheduler control-plane primitives. |
| `agentflow-worker` | Distributed worker runtime and binary built around the `WorkerProtocol` abstraction. |

`agentflow-config` is not an active workspace crate. Current config-first workflow
support lives in `agentflow-cli/src/config` and `agentflow-cli/src/executor`.

## CLI Surface

Current top-level commands are:

```bash
agentflow workflow run|validate|debug
agentflow config init|show|validate
agentflow llm models
agentflow mcp list-tools|call-tool|list-resources
agentflow skill init|install|validate|inspect|run|chat|list|list-tools|test|index|marketplace
agentflow marketplace search|install|update|verify
agentflow plugin install|list|inspect|uninstall   # when built with the plugin feature
agentflow trace replay|tui
agentflow audio asr|tts
agentflow image generate|understand
agentflow rag search|index|collections|eval        # when built with the rag feature
```

The old bare prompt/chat command is not part of the public CLI. Interactive model
use should go through Skills, agents, or workflows.

## Configuration And Secrets

The CLI reads model configuration from `~/.agentflow/models.yml`, falling back to
bundled defaults when no user config exists. Secret values belong in the process
environment or `~/.agentflow/.env`; model entries should reference them by
environment variable name instead of storing raw keys.

Useful commands:

```bash
agentflow config init
agentflow config show models
agentflow config show providers
agentflow config validate
agentflow llm models --provider openai --detailed
```

## Workflow YAML Contract

Config-first workflows use `FlowDefinitionV2`:

```yaml
name: Example
inputs:
  topic:
    description: Topic to process
    required: false
    default: "AgentFlow"
nodes:
  - id: render
    type: template
    parameters:
      template: "Explain {{topic}}"
  - id: answer
    type: llm
    dependencies: [render]
    input_mapping:
      prompt: "{{ nodes.render.outputs.output }}"
    parameters:
      model: gpt-4o-mini
```

Each node has `id`, `type`, optional `dependencies`, optional `input_mapping`,
optional `run_if`, and a `parameters` map. `agentflow workflow validate` checks
node support, required parameters, basic parameter types, dependency references,
and supported `input_mapping` expressions before execution.

See [WORKFLOW_SCHEMA.md](WORKFLOW_SCHEMA.md) for the current node parameter table.

## Persistence And Observability

- Workflow run artifacts default to `~/.agentflow/runs`; override with
  `agentflow workflow run --run-dir <dir>` or `AGENTFLOW_RUN_DIR`.
- Trace files default to `~/.agentflow/traces`; inspect them with
  `agentflow trace replay` or `agentflow trace tui`.
- Checkpoint recovery preserves completed workflow node outputs and serialized
  agent step history so interrupted runs can resume.

## Related Guides

- [CONFIGURATION.md](CONFIGURATION.md)
- [WORKFLOW_SCHEMA.md](WORKFLOW_SCHEMA.md)
- [AGENT_RUNTIME.md](AGENT_RUNTIME.md)
- [SKILLS.md](SKILLS.md)
- [MCP_SKILLS.md](MCP_SKILLS.md)
- [TRACING_USAGE.md](TRACING_USAGE.md)
- [CHECKPOINT_RECOVERY.md](CHECKPOINT_RECOVERY.md)

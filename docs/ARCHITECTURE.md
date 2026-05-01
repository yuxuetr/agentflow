# AgentFlow Architecture

Last updated: 2026-05-01

AgentFlow is a Rust workspace for deterministic workflow execution and agent-native
runtime loops. The project is organized around a small core engine and separate
crates for nodes, LLM access, tools, Skills, MCP, memory, tracing, visualization,
and the CLI/server surfaces.

## Runtime Model

AgentFlow supports two complementary execution styles:

- **DAG workflows**: `agentflow-core::Flow` runs explicit graph nodes with declared
  dependencies, input mappings, optional conditions, checkpoints, retry, timeout,
  resource limits, and health primitives.
- **Agent loops**: `agentflow-agents::AgentRuntime` records observe, plan, tool
  call, tool result, reflection, and final answer steps. ReAct, plan/execute, and
  multi-agent examples are built on this runtime.

The intended direction of composition is:

```text
Flow -> AgentNode -> AgentRuntime -> ToolRegistry -> Tool / MCP / WorkflowTool
```

Use workflows for deterministic production automation. Use agents when the next
step depends on model reasoning, tool feedback, memory, or reflection. Use
`AgentNode` when a workflow needs one agent-driven step, and use workflow tools
when an agent should call a stable DAG as a tool.

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
| `agentflow-viz` | Workflow graph conversion and DOT, JSON, and Mermaid renderers. |
| `agentflow-db` | Database abstraction used by the server. |
| `agentflow-server` | Axum gateway with health, liveness, and readiness endpoints. |

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
agentflow trace replay|tui
agentflow audio asr|tts
agentflow image generate|understand
agentflow rag search|index|collections   # when built with the rag feature
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

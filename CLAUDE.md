# AgentFlow Project - Claude Code Configuration

## Project Overview

AgentFlow is a Rust workspace that supports both deterministic DAG workflows and agent-native autonomous loops, with full LLM, MCP, RAG, Skill, and tracing support. The workspace has 16 Rust crates plus 1 Web UI crate (`agentflow-ui`, a Vite-built React SPA embedded by the server).

Recommended four-layer mental model:

- **L1 Execution Core**: `agentflow-core` (DAG engine, `AsyncNode`, `FlowValue`, scheduler, retry, timeout, checkpoint, resource manager, health, events)
- **L2 Capability Adapters**: `agentflow-nodes`, `agentflow-llm`, `agentflow-tools`, `agentflow-mcp`, `agentflow-rag`, `agentflow-memory`
- **L3 Agent / Orchestration**: `agentflow-agents`, `agentflow-skills`, `agentflow-harness`, `agentflow-cli`
- **L4 Operations / Productization**: `agentflow-tracing`, `agentflow-viz`, `agentflow-server`, `agentflow-db`, `agentflow-worker`, `agentflow-ui`

Two complementary execution styles:

- **DAG workflows** via `agentflow-core::Flow` (sequential or `FlowExecutionMode::Concurrent` dependency-ready scheduling) with explicit I/O, checkpoints, retry, timeout, conditional execution.
- **Agent-native loops** via `agentflow-agents::AgentRuntime` (ReAct, Plan-Execute, Reflection, Supervisor) with structured `AgentStep` / `AgentEvent` / `AgentStopReason`, tool calling, memory, cancellation.

The two compose via `AgentNode` (agent embedded in DAG) and `WorkflowTool` (DAG exposed as agent tool). Config-first YAML supports `agent` / `skill_agent` node types.

## Architecture Principles

### High Cohesion, Low Coupling
- Each crate has clearly defined responsibilities
- Minimal cross-crate dependencies, well-defined public APIs
- Feature flags isolate optional capabilities (mcp, rag, etc.)

### Crate Responsibilities

#### L1 — agentflow-core
DAG execution engine and core abstractions:
- `Flow` orchestrator with topological sort and `FlowExecutionMode::{Serial, Concurrent}` (dependency-ready dispatch via `FuturesUnordered` + `max_concurrency`)
- `AsyncNode` trait + `GraphNode` (dependencies, `input_mapping`, `run_if`, `initial_inputs`)
- `NodeType::{Standard, Map, While}` with parallel/sequential map and conditional loops
- `FlowValue::{Json, File, Url}` for explicit, namespaced state pool
- Production primitives: retry/retry_executor, timeout, checkpoint, resource_manager, resource_limits, health, state_monitor, events

#### L2 — agentflow-nodes
Built-in `AsyncNode` library: 16+ node types (`llm`, `template`, `http`, `file`, `arxiv`, `markmap`, `batch`, `conditional`, `while`, `mcp`, `rag`, `asr`, `tts`, `text_to_image`, `image_to_image`, `image_edit`, `image_understand`). Feature-gated; factory pattern in `factory_traits.rs`.

#### L2 — agentflow-llm
LLM provider abstraction:
- Unified fluent API: `AgentFlow::model(...).prompt(...).execute()`
- 6 providers: OpenAI, Anthropic, Google, StepFun, Moonshot, Mock
- Multimodal (text + image url/base64), streaming, model registry/discovery
- Native `tool_calls` / `tool_choice` first-class across all 6 providers
- W3C `traceparent` propagation through HTTP calls (via `LlmTraceContext`)

#### L2 — agentflow-tools
Unified tool abstraction:
- `Tool` trait + `ToolRegistry` + `SandboxPolicy` + `ToolPolicy`
- `ToolMetadata` with `source: ToolSource::{Builtin, Script, Mcp, Workflow}`, permissions, original MCP server/tool names
- Built-in `FileTool` / `HttpTool` / `ShellTool` (shell defaults to disabled)
- `ToolOutputPart::{Text, Image, Resource}` for typed multimodal output
- OS-level sandbox backends (macOS sandbox-exec / Linux seccomp) for `ShellTool` / `ScriptTool`

#### L2 — agentflow-mcp
Model Context Protocol integration: client + server + transport (stdio first), JSON-RPC 2.0, retry/timeout/reconnect, latency benchmarks. Adapter into `agentflow-tools::ToolRegistry`.

#### L2 — agentflow-rag
Retrieval-Augmented Generation: document chunking, embeddings (OpenAI/StepFun API or local ONNX), Qdrant vectorstore, retrieval, reranking. Sources: PDF, HTML, CSV, text. Eval harness (`eval` module): JSONL dataset format (`corpus`/`queries`/`qrels`), Recall@K / MRR / nDCG@K metrics, baseline comparison with paired sign test, CLI `agentflow rag eval`.

#### L2 — agentflow-memory
Agent conversation memory: `MemoryStore` trait with `SessionMemory` (token-windowed in-memory) and `SqliteMemory` (persistent). `SemanticMemory` for similarity search (interlocks with `agentflow-rag`).

#### L3 — agentflow-agents
Agent-native runtime and patterns:
- `AgentRuntime` trait with `AgentContext`, `RuntimeLimits` (max_steps, max_tool_calls, timeout_ms, token_budget), `AgentCancellationToken`
- `ReActAgent` (observe/plan/tool/result/reflect/final answer with memory summary)
- `PlanExecuteAgent` (structured plan JSON + sequential execution)
- `ReflectionStrategy` trait (`FailureReflection` / `FinalReflection` / `NoOpReflection`)
- `MemorySummaryBackend` trait (`RecentOnlyMemorySummary` / `CompactMemorySummary`)
- `AgentNode` (agent in DAG) + `WorkflowTool` (DAG as agent tool) + `AgentNodeResumeContract` (partial resume)
- Multi-agent collaboration: `HandoffSupervisor` / `BlackboardSupervisor` / `DebateSupervisor`

#### L3 — agentflow-skills
Declarative agent capability packages:
- `SKILL.md` (recommended) + `skill.toml` (compatibility) parsing
- `SkillBuilder` wires persona / model / tools / knowledge / memory / mcp_servers / security into a runnable agent
- Local registry (`skills.index.toml`) + marketplace catalog
- CLI: `init`, `install`, `list`, `inspect`, `list-tools`, `run`, `chat`, `test`, `validate`, `index`, `marketplace`

#### L3 — agentflow-harness
Harness Agent Mode contract crate (Phase H0 freeze):
- `HarnessEvent` line-delimited JSON envelope (closed kind set: `session_started`, `step_started`, `tool_call_requested`, `approval_requested`, `approval_decided`, `tool_call_completed`, `background_task_updated`, `memory_summary_added`, `stopped`)
- `ApprovalRequest` / `ApprovalDecision` / `ApprovalRisk` / `ApprovalScope` interactive approval protocol
- Async hook traits: `PreToolHook`, `PostToolHook`, `ApprovalProvider`, `ContextProvider`
- `HarnessContext`, `HarnessProfile`, `HarnessRuntimeKind` session descriptor
- Stability tier **experimental** until Phase H1 exercises the runtime end-to-end. See `docs/HARNESS_MODE.md` for the implementation spec.

#### L3 — agentflow-cli
Unified user interface:
- `workflow run|validate|debug` (with `--input`, `--dry-run`, `--output`, `--timeout`, `--max-retries`, `--model`, `--run-dir`, `--max-concurrency`)
- `config init|show|validate`, `llm models`
- `skill *`, `mcp list-tools|call-tool|list-resources`, `trace replay|tui`
- `audio asr|tts`, `image generate|understand`
- `rag search|index|collections` (feature-gated)

#### L4 — agentflow-tracing
Observability:
- Event collection via `EventListener` (non-invasive)
- Persistence: JSONL (default) or SQLite/Postgres (feature-gated)
- `agentflow trace replay` + TUI timeline
- OpenTelemetry exporter (OTLP) with W3C trace context propagation
- Redaction for API keys, env secrets, sensitive tool params
- `AGENTFLOW_TRACE_DIR` / `AGENTFLOW_RUN_DIR` for explicit storage roots

#### L4 — agentflow-viz
DAG visualization: YAML → VisualGraph → Mermaid / DOT / JSON. Static; not yet wired to live trace state.

#### L4 — agentflow-server
Axum gateway for platform mode. Implemented: `/v1/runs` (POST/GET), `/v1/runs/{id}/events` (SSE with backfill), `/v1/skills`, `/v1/skills/{name}:run`. Bearer-token auth, unified error envelope, `WorkflowEventListener` bridge to DB. Real Flow runner replacing `StubExecutor` lands in v0.4.0.

#### L4 — agentflow-db
PostgreSQL persistence for the gateway. 6-table schema (runs / steps / events / artifacts / skill_installs / mcp_sessions) via `sqlx::migrate!()`. Repository layer: `RunRepo` / `StepRepo` / `EventRepo` / `ArtifactRepo` / `SkillInstallRepo` / `McpSessionRepo`.

#### L4 — agentflow-worker
Standalone worker process for distributed DAG execution. Speaks `WorkerProtocol` over gRPC to the server control plane, pulls assigned tasks, executes the node payload locally, and streams events back with W3C `traceparent` continuity so worker spans stitch into the parent OTel trace. Today the supported node payloads are limited (template/file); extending to LLM / HTTP / MCP / agent payloads is tracked as P2.8.

#### L4 — agentflow-ui
React + Vite + TypeScript SPA embedded by the server at `/ui`. Implemented: run list, DAG status panel, event history replay, live SSE updates. It is a client of the same `/v1/*` and SSE contracts the CLI uses — never bypass server APIs for UI-only features. Productization beyond the alpha shell is tracked under P6.

## Development Guidelines

### Code Style
- **Indentation**: 2 spaces (NO TABS) — overrides Rust default
- snake_case for functions/variables, PascalCase for types
- Explicit error handling with custom error types (`error.rs` per crate, `thiserror`)
- `///` doc comments on public APIs
- `async/await` with Tokio runtime

### Testing Strategy
- Unit tests in each module (`#[cfg(test)]`)
- Integration tests in `tests/` directories
- Example-driven development with `examples/` directories
- CLI tests with `assert_cmd` crate

### Configuration Management
- YAML-based configuration for workflows and models
- Environment variable support with `.env` files
- Hierarchical config: project → user → built-in defaults
- Runtime configuration validation

## Current Implementation Status

### ✅ Production-Ready

- **Core DAG engine** — async/await, topological sort, concurrent dependency-ready scheduler, state management
- **Control flow** — Map (sequential/parallel), While loops, Conditional execution
- **16+ built-in nodes** — HTTP, File, Batch, Template (Tera), MarkMap, Arxiv, etc.
- **6 LLM providers** — OpenAI, Anthropic, Google, Moonshot, StepFun, Mock; native tool calling on all
- **Multimodal** — text, image (generation/understanding), audio (TTS/ASR)
- **MCP integration** — client, MCPNode, CLI commands (`list-tools`, `call-tool`, `list-resources`), workflow examples
- **Agent-native runtime** — ReAct, Plan-Execute, Reflection, memory summary backends, hybrid composition (`AgentNode` / `WorkflowTool`)
- **Multi-agent collaboration** — Handoff, Blackboard, Debate supervisors; `multi_agent` YAML node
- **RAG** — chunking, embeddings, Qdrant, retrieval, reranking; CLI `rag search|index|collections|eval`; eval harness with Recall@K / MRR / nDCG@K metrics + paired baseline comparison
- **Observability/reliability (Phase 1.5)** — timeout control, K8s-compatible health checks, checkpoint recovery, retry, resource management, structured logging, Prometheus metrics
- **Tracing** — `EventListener`, JSONL/SQLite/Postgres persistence, `trace replay` TUI, OTel OTLP exporter, W3C `traceparent` propagation through LLM HTTP calls
- **OS-level sandbox** — macOS sandbox-exec / Linux seccomp backends for shell/script tools (opt-in via `security.os_sandbox`); active backend name + `enforcement_level` (`enforcing` / `permissive` / `disabled`) is visible in `ToolCapabilityDecision` events and `agentflow doctor --format json` output
- **Platform skeleton** — server gateway routes (`/v1/runs`, SSE, skills) + DB schema/repos + auth
- **Distributed worker foundation** — `agentflow-worker` runtime/binary, gRPC `WorkerProtocol`, server control-plane façade, stitched worker traces mapped to OTel spans (node-payload coverage is partial; see P2.8)
- **Web UI alpha shell** — `agentflow-ui` SPA embedded at `/ui`, run list, DAG graph/status, event history, SSE updates

### 📋 Roadmap

**N8 — Platform skeleton + native tool calling (v0.3.0 candidate):** ✅ closed
- LLM `tool_calls` / `tool_choice` native ✅ / Server gateway core routes ✅ / DB schema ✅
- Pending: `Tool` idempotency metadata for partial-resume auto-replay; `FlowValue::File`/`Url` checkpoint round-trip type fidelity

**N9 — Multi-agent + ecosystem (v0.4.0 candidate):** mostly closed
- ✅ Handoff/blackboard/debate; ✅ OS sandbox; ✅ OTel `traceparent` propagation; ✅ RAG eval harness; ✅ LLM provider consistency suite (foundation)
- Pending: cross-provider streaming / multimodal / tool-calling consistency tests; live-LLM nightly CI

**N10 — Plugin / distributed / Web UI (v1.0.0-rc candidate):** in progress
- ✅ `docs/AGENT_SDK.md` extension guide + runnable examples (`custom_runtime` / `custom_reflection` / `custom_memory_summary`); core extension traits rustdoc-clean
- ✅ Plugin / Custom Node foundation: subprocess JSON-RPC runtime, manifest/lifecycle, sandbox bridge, `type: plugin` workflow node, plugin CLI, and marketplace signature/version handoff
- ✅ Distributed scheduling foundation: `WorkerProtocol`, gRPC transport choice, server control-plane façade, `agentflow-worker` runtime/binary, stitched worker traces mapped to OTel spans
- ✅ Web UI debugger: React + Vite + TypeScript SPA embedded at `/ui`, run list, DAG graph/status, event history replay, and SSE updates
- ✅ Plugin marketplace remote registry foundation: unified Skill/Plugin manifest, read-only HTTP client, artifact cache, signature verifier, marketplace CLI, and docs

See `RoadMap.md` for the full plan; `PROJECT_EVALUATION_2026-05-14.md` for the most recent evaluation (the 2026-05-01 evaluation is retained as historical context). For change history, prefer `git log` over a doc summary.

## File Organization

### Configuration Files
- `Cargo.toml` — workspace configuration
- `agentflow-cli/examples/workflows/` — example workflow definitions
- `agentflow-llm/config/models/` — LLM provider configurations
- `agentflow-llm/templates/` — default configuration templates

### Source Entry Points
- `agentflow-core/src/lib.rs` — core exports and module organization
- `agentflow-llm/src/lib.rs` — LLM API entry point and fluent interface
- `agentflow-cli/src/main.rs` — CLI command structure and routing

### Examples
- `agentflow-cli/examples/` — CLI usage examples (incl. `ai_research_assistant.yml`, skill-agent hybrid, RAG + Skill assistant, fixed DAG basic)
- `agentflow-agents/examples/` — agent-native ReAct, Plan-Execute, multi-agent (handoff/blackboard/debate)
- `agentflow-llm/examples/`, `agentflow-core/examples/`

## Common Development Tasks

### Adding New LLM Provider
1. Create provider module in `agentflow-llm/src/providers/`
2. Implement provider trait with authentication and API calls
3. Add configuration in `agentflow-llm/config/models/`
4. Update model registry in `agentflow-llm/src/registry/`
5. Add examples and tests

### Adding New Node Type
1. Create node module in `agentflow-nodes/src/nodes/`
2. Implement `AsyncNode` trait from `agentflow-core`
3. Add to factory in `agentflow-nodes/src/factories/`
4. Add configuration parsing and validation
5. Create examples and tests; update documentation

### Adding New CLI Command
1. Define command structure in `agentflow-cli/src/main.rs`
2. Implement command handler in appropriate `commands/` module
3. Add output formatting and error handling
4. Create examples and documentation

## Quality Standards

### Code Quality Checklist
- [ ] All public APIs documented with `///` comments
- [ ] Error handling with appropriate error types
- [ ] Unit tests for core functionality
- [ ] Integration tests for CLI commands
- [ ] Examples demonstrating usage
- [ ] Configuration validation
- [ ] Logging and observability support

### Pre-Commit Requirements
- [ ] `cargo fmt` — code formatting
- [ ] `cargo clippy` — lint checks (`-D warnings`)
- [ ] `cargo test` — all tests passing
- [ ] `cargo doc` — documentation builds
- [ ] Example workflows validate successfully

## Security Considerations

### API Key Management
- Never commit API keys to repository
- Use environment variables or secure config files
- Support multiple configuration precedence levels
- Mask sensitive data in logs and error messages

### Input Validation
- Validate all user inputs (prompts, file paths, URLs)
- Sanitize template inputs to prevent injection
- Validate workflow configurations before execution
- Secure MCP transport connections

## Performance Guidelines

- Use `tokio::spawn` for concurrent operations; proper backpressure handling
- Streaming for large responses; connection pooling for HTTP clients
- Avoid holding large responses in memory; streaming parsers for large files
- Implement proper cleanup in `Drop`; monitor memory in long-running workflows

---

**Last Updated**: 2026-05-14
**AgentFlow Version**: 0.2.0+ (targeting v0.3.0)
**Rust Edition**: 2024 (all workspace members)
**Composite Maturity Rating**: B+ (per `PROJECT_EVALUATION_2026-05-14.md`)

# AgentFlow Project - Claude Code Configuration

## Project Overview

AgentFlow is a Rust workspace that supports both deterministic DAG workflows and agent-native autonomous loops, with full LLM, MCP, RAG, Skill, and tracing support. The workspace has 22 Rust crates plus 1 Web UI crate (`agentflow-ui`, a Vite-built React SPA embedded by the server).

A narrow-waist **contract kernel** (L0) was extracted by the P-A track (`docs/RFC_CRATE_ARCHITECTURE.md`; validated by `docs/ARCHITECTURE_EVALUATION_2026-06-20.md`): the runtimes never depend on each other, only on shared contracts, enforced by `cargo xtask check-arch` (eight dependency laws). The four execution paradigms (static DAG / native loop / harness / dynamic workflow) and their three-axis mental model live in `docs/ARCHITECTURE.md` § Four Execution Paradigms.

Recommended five-layer mental model:

- **L0 Contract Kernel** (narrow waist): `agentflow-value` (`FlowValue`), `agentflow-graph` (the `Flow` IR / `AsyncNode` / `expr` / `AgentFlowError`), `agentflow-store-spi` (`MemoryStore` + `KnowledgeBackend`), `agentflow-agent-spi` (`AgentRuntime` / turn-driven façade / `Capability` lowering), `agentflow-async-util` (retry/timeout/`race_with_limits`), plus `agentflow-tools` (the `Tool` contract)
- **L1 Execution Core** (the executor): `agentflow-core` runs the L0 `Flow` IR — scheduler, checkpoint, retry-executor, resource manager, health, events — exposed via the `FlowExt` trait (`flow.run()`). IR ≠ executor; the L0 types are re-exported under `agentflow_core::*` for compatibility.
- **L2 Capability Adapters**: `agentflow-nodes` (tool-tier nodes), `agentflow-nodes-ai` (capability-backed nodes), `agentflow-llm`, `agentflow-tools`, `agentflow-mcp`, `agentflow-rag`, `agentflow-memory`
- **L3 Agent / Orchestration**: `agentflow-agents` (incl. the `dynamic` module: `compile_plan_to_flow` + `DynamicWorkflowAgent`), `agentflow-skills`, `agentflow-harness`, `agentflow-config` (shared config-first workflow assembly: YAML schema + `executor` + `diagnostics`, consumed by both `cli` and `server`), `agentflow-cli`
- **L4 Operations / Productization**: `agentflow-tracing`, `agentflow-server`, `agentflow-db`, `agentflow-worker`, `agentflow-ui`

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

#### L2 — agentflow-nodes (tool tier) + agentflow-nodes-ai (capability tier)
Split by the P-A nodes decomposition (`docs/RFC_NODES_DECOMPOSITION.md`) so the tool-tier crate carries no capability dependencies:
- **`agentflow-nodes`** — tool-tier `AsyncNode`s (`template`, `file`, `http`, `batch`, `conditional`, `arxiv`, `markmap`). Depends only on the IR (`agentflow-core`/graph) + `agentflow-tools`. Feature flags: defaults `["http", "file", "template"]`; `batch` / `conditional` opt-in.
- **`agentflow-nodes-ai`** — capability-backed adapters (`llm`, `asr`, `tts`, `text_to_image`, `image_to_image`, `image_understand`, `image_edit`, `mcp`, `rag`). Depends on `agentflow-nodes` (shared `common`/`error`) + the capabilities (`agentflow-llm` always; `agentflow-mcp` / `agentflow-rag` behind the `mcp` / `rag` features). The AI-modality nodes ship without per-modality gates.

The workflow YAML `type:` → node dispatch lives in `agentflow-config::executor::factory` (it imports tool nodes from `agentflow-nodes` and capability nodes from `agentflow-nodes-ai`); the `type:` strings are unchanged by the split. `agentflow-worker` keeps the tool tier and pulls `agentflow-nodes-ai` only for the `llm` / `mcp` payloads it dispatches.

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
Model Context Protocol integration: client + server + transport (stdio first), JSON-RPC 2.0, retry/timeout/reconnect, latency benchmarks. The MCP→`agentflow-tools::Tool` adapter (`McpToolAdapter` + `McpClientPool`) lives in `agentflow-skills/src/mcp_tools.rs`, not in this crate — `agentflow-skills` owns the conversion because the skill builder is the entry point that knows which MCP servers a skill manifest declares.

#### L2 — agentflow-rag
Retrieval-Augmented Generation: document chunking, embeddings (OpenAI API or local ONNX), Qdrant vectorstore, retrieval, reranking. Sources: PDF, HTML, CSV, text (PDF/HTML loaders carry a default 50 MiB / 10 MiB size cap, override via `with_max_bytes`). Eval harness (`eval` module): JSONL dataset format (`corpus`/`queries`/`qrels`), Recall@K / MRR / nDCG@K metrics, baseline comparison with paired sign test, CLI `agentflow rag eval`. (StepFun embedding provider mentioned in earlier drafts is not implemented; only OpenAI + local ONNX exist today.) **RAG repositioning (P-A4.1):** implements the L0 `KnowledgeBackend` SPI as `Bm25KnowledgeBackend` (in-memory keyword index, bundled-files tier) + `VectorStoreKnowledgeBackend` (vector tier), and exposes `RagSearchTool` — a registry-installable `rag_search` `Tool` (idempotent, read-only) wrapping any `Arc<dyn KnowledgeBackend>`. This puts RAG on the capability/tool axis behind a Skill's `knowledge:` declaration rather than as a top-level mode.

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
- `SkillBuilder` wires persona / model / tools / knowledge / memory / mcp_servers / security into a runnable agent. Tiered knowledge (P-A4.2): each `[[knowledge]]` entry's `backend` is `files` (default — inlined into the persona) or `rag` (indexed into a `Bm25KnowledgeBackend` + exposed as a shared `rag_search` tool, so large corpora retrieve on demand instead of bloating the prompt)
- `SkillCapability` implements the L0 `Capability` contract (P-A4.3): `lower()` produces the Skill's tool registry contents (built-in + MCP + `rag_search`) + its persona as a `Critical` `ContextItem`, so a surface can merge it with other capabilities into one registry + context bundle for a runtime
- Local registry (`skills.index.toml`) + marketplace catalog
- CLI: `init`, `install`, `list`, `inspect`, `list-tools`, `run`, `chat`, `test`, `validate`, `index`, `marketplace`

#### L3 — agentflow-harness
Harness Agent Mode crate (Phase H0 contract freeze + H1 runtime MVP + H2 hooks/approval, all closed):
- **Frozen contract surface (H0):** `HarnessEvent` line-delimited JSON envelope (closed kind set: `session_started`, `step_started`, `tool_call_requested`, `approval_requested`, `approval_decided`, `tool_call_completed`, `background_task_updated`, `memory_summary_added`, `stopped`); `ApprovalRequest` / `ApprovalDecision` / `ApprovalRisk` / `ApprovalScope` interactive approval protocol; async hook traits `PreToolHook` / `PostToolHook` / `ApprovalProvider` / `ContextProvider`; session descriptor `HarnessContext` / `HarnessProfile` / `HarnessRuntimeKind`
- **Runtime MVP (H1):** `HarnessRuntime` wrapping any `agentflow_agents::AgentRuntime` (typically `ReActAgent`) via `Box<dyn AgentRuntime>`; four default context providers (`AgentsMdProvider`, `TodosMdProvider`, `RoadmapMdProvider`, `WorkspaceLayoutProvider`) with priority + token-cost estimates and priority-aware budget trimming; `InMemoryEventSink` / `JsonlEventSink` / `StdoutEventSink` / `SinkChain` persistence; deterministic `AgentEvent` → `HarnessEvent` translation with monotonic `seq`; `tracing_bridge` honoring the `AGENTFLOW_TRACE_DIR` convention so Harness session logs co-locate with the rest of the trace tooling.
- **Hooks + approval (H2):** `HookedTool` + `wrap_registry(registry, HookConfig)` decorate every registered `Tool` with a pre/post hook + approval pipeline. Pre-hook timeouts and errors are fail-closed; post-hooks are advisory. Three `ApprovalProvider` implementations (`AutoAllow`, `AutoDeny`, `Cli`). Production profile escalates `NonIdempotent` calls to `RequireApproval` automatically. `Session` / `Run` scope decisions are cached per tool. `DenyAndStop` short-circuits every subsequent tool call. Approval-lifecycle events (`approval_requested` / `approval_decided`) flow through the existing `SinkChain`.
- **Parallel tool calls (H3):** `ReActAgent::run_with_context` adds a batch dispatcher (in `agentflow-agents/src/react/agent.rs`) that activates when the LLM returns `>= 2` native tool calls in one turn. Idempotent calls run concurrently via `futures::future::join_all`; `NonIdempotent` / `Unknown` calls run serially in LLM-returned order. `ToolPolicyDecision` / `ToolCapabilityDecision` / `ToolCallStarted` / `ToolCall` step rows all fire in LLM-returned order before any execution begins, so trace replay stays deterministic. Partial failures keep the batch moving; pre-cancel and `max_tool_calls` checks are atomic.
- **Background tasks (H4):** `agentflow-harness::tasks` provides `TaskRuntime` + `TaskHandle` + `TaskAgentFactory` plus 5 built-in tools (`task_create`, `task_get`, `task_list`, `task_stop`, `task_output`). Each task spawns a `tokio::task` running an inner agent; lifecycle transitions (`Pending → Running → Completed | Failed | Cancelled`) emit `BackgroundTaskUpdated` through the parent `SinkChain`. Nested task spawning is rejected via a `tokio::task_local!` flag. Output capture is bounded by `max_output_bytes` (default 64 KiB).
- **CLI surface:** `agentflow harness run|resume|list|inspect` with `--output text|json|stream-json` and the full flag set documented in `docs/HARNESS_MODE.md`.
- Stability tier **beta** as of P-H.5 closure: `HarnessEvent` envelope, `ApprovalRequest`, and `ApprovalDecision` are plumbed through both the in-process hook runtime and the HTTP surface (`/v1/harness/sessions/{id}/events`, `/approvals`). See `docs/HARNESS_MODE.md` for the implementation spec and `docs/STABILITY.md` for the wire-shape promise. `tracing_bridge` now ships **two** sink tiers: (a) JSONL-only via `open_tracing_sink(...)` (per-session `<base>/harness/sessions/<id>.jsonl` for raw replay), and (b) `ExecutionTrace` via `open_execution_trace_sink(storage)` which translates each `HarnessEvent` stream into an `agentflow_tracing::ExecutionTrace` and persists it through any `TraceStorage` backend (Q3.10.4). One related item remains **open**: first-party OTLP exporter transport (HTTP/gRPC + TLS + auth) is deferred (Q2.3.3) — operators bring their own `OtelSpanSink` impl.

#### L3 — agentflow-config
Shared config-first workflow assembly extracted from the CLI (P-A2.4) so the server can assemble/diagnose workflows without depending on the CLI binary crate:
- `config` — YAML workflow schema (`config::v2::{FlowDefinitionV2, NodeDefinitionV2}`, `config::schema`).
- `executor` — compiles a config into an `agentflow-core` `Flow` (`build_flow_from_yaml` + node factories); feature flags `plugin` / `rag` / `mcp` gate capability nodes.
- `diagnostics` — the `agentflow doctor` report builder (`build_report`, `DoctorReport`, `print_text_report`); the CLI's `doctor` command + the server's `/v1/diagnostics` both consume it.
- `agentflow-cli` re-exports `config` / `executor` under their original `agentflow_cli::{config, executor}` paths, and `commands::doctor` re-exports the diagnostics surface — consumers unchanged.

#### L3 — agentflow-cli
Unified user interface:
- `workflow run|validate|debug` (with `--input`, `--dry-run`, `--output`, `--timeout`, `--max-retries`, `--model`, `--run-dir`, `--max-concurrency`)
- `workflow dynamic --goal ... --model ...` — LLM authors a `WorkflowPlan`, compiled + executed under a restrictive built-in tool sandbox (`--allow-path` / `--allow-domain`); `--dry-run` prints the plan; `--approve` routes tool calls through the Harness approval pipeline
- `config init|show|validate`, `llm models`
- `skill *`, `mcp list-tools|call-tool|list-resources`, `trace replay|tui`
- `audio asr|tts`, `image generate|understand`
- `rag search|index|collections` (feature-gated)

#### L4 — agentflow-tracing
Observability:
- Event collection via `EventListener` (non-invasive); the in-process drain task processes events in arrival order so terminal node state cannot race the `WorkflowCompleted` save
- Persistence: JSONL (default) or SQLite/Postgres (feature-gated). Producer-side wiring is live in CLI (`agentflow workflow run` always writes file traces under `AGENTFLOW_TRACE_DIR` / `~/.agentflow/traces` by default) and in the gateway (`POST /v1/runs` writes file traces only when `AGENTFLOW_TRACE_DIR` is explicitly set, since the cleanup sweep does not cover that dir). Harness sessions (`HarnessEvent`) persist to Postgres + SSE only; file-backed trace integration would need a separate `HarnessEventListener → ExecutionTrace` adapter and is not wired today.
- `agentflow trace replay` + TUI timeline (read from the directories above)
- OpenTelemetry span model (`OtelSpan` / `OtelSpanSink` trait) + W3C trace context propagation (inbound `traceparent` honored via `context::scope`; outbound via `LlmTraceContext`). No OTLP HTTP/gRPC transport is built into the workspace — operators bring their own `OtelSpanSink` implementation backed by `opentelemetry-otlp` or similar. A first-party OTLP exporter with TLS / auth is deferred (Q2.3.3, `docs/audit/agentflow-tracing.md` M3).
- Redaction for API keys, env secrets, sensitive tool params
- `AGENTFLOW_TRACE_DIR` / `AGENTFLOW_RUN_DIR` for explicit storage roots

#### L4 — agentflow-server
Axum gateway for platform mode. Workflow surface: `/v1/runs` (POST/GET), `/v1/runs/{id}/events` (SSE with backfill), `/v1/skills`, `/v1/skills/{name}:run`. Harness Mode surface (P-H.5, closed): `/v1/harness/sessions` (POST/GET), `/v1/harness/sessions/{id}` (GET + `:cancel` POST + `:resume` POST), `/v1/harness/sessions/{id}/events` (SSE with backfill), `/v1/harness/sessions/{id}/events/history` (JSON), `/v1/harness/sessions/{id}/approvals` (GET pending) + `POST .../{request_id}` (decide), backed by `LiveHarnessExecutor` in production (wires `HarnessRuntime` + `ReActAgent` + hook-wrapped tool registry + `ServerApprovalProvider`) and `StubHarnessExecutor` in tests. `:resume` accepts `mode: "rerun" | "append"` (default `rerun` for backwards compat); rerun clears prior events and restarts the seq series at 0, append preserves the prior log and continues at `MAX(seq) + 1` via the upstream `HarnessRuntime::with_initial_seq` knob. Bearer-token auth, unified error envelope, `WorkflowEventListener` bridge to DB. `FlowRunExecutor` is the production default and runs config-first workflows in-process; `StubExecutor` remains as the test-only stand-in for route-plumbing tests that don't need real execution.

#### L4 — agentflow-db
PostgreSQL persistence for the gateway. Nine-table schema (runs / steps / events / artifacts / skill_installs / mcp_sessions + harness_sessions / harness_session_events + user_preferences) via `sqlx::migrate!()`. Repository layer: `RunRepo` / `StepRepo` / `EventRepo` / `ArtifactRepo` / `SkillInstallRepo` / `McpSessionRepo` / `HarnessSessionRepo` / `HarnessEventRepo` / `UserPreferenceRepo`.

#### L4 — agentflow-worker
Standalone worker process for distributed DAG execution. Speaks `WorkerProtocol` over gRPC to the server control plane, pulls assigned tasks, executes the node payload locally, and streams events back with W3C `traceparent` continuity so worker spans stitch into the parent OTel trace. Today the supported node payloads are limited (template/file); extending to LLM / HTTP / MCP / agent payloads is tracked as P2.8.

#### L4 — agentflow-ui
React + Vite + TypeScript SPA embedded by the server at `/ui`. Implemented: run list, DAG status panel, event history replay, live SSE updates. Harness Mode surface (P-H.5, closed): `/ui/harness/sessions` (list), `/ui/harness/sessions/new` (submit form), `/ui/harness/sessions/{id}` (detail with `EventSource`-backed event timeline, payload pane, pending approval cards with allow / deny / deny_and_stop × scope dropdown, cancel button, resume button with `rerun` / `append` mode dropdown gated on terminal status). It is a client of the same `/v1/*` and SSE contracts the CLI uses — never bypass server APIs for UI-only features. Productization beyond the alpha shell is tracked under P6.

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
- **Tracing** — `EventListener`, JSONL/SQLite/Postgres persistence, `trace replay` TUI, OTel span model + W3C `traceparent` propagation (inbound on workflow start + outbound through LLM HTTP calls). First-party OTLP transport (HTTP/gRPC + TLS + auth) is **deferred** (Q2.3.3) — operators wire their own `OtelSpanSink`.
- **OS-level sandbox** — macOS sandbox-exec / Linux seccomp backends for shell/script tools (opt-in via `security.os_sandbox`); active backend name + `enforcement_level` (`enforcing` / `permissive` / `disabled`) is visible in `ToolCapabilityDecision` events and `agentflow doctor --format json` output
- **Platform skeleton** — server gateway routes (`/v1/runs`, SSE, skills) + DB schema/repos + auth
- **Distributed worker foundation** — `agentflow-worker` runtime/binary, gRPC `WorkerProtocol`, server control-plane façade, stitched worker traces mapped to OTel spans (node-payload coverage is partial; see P2.8)
- **Web UI alpha shell** — `agentflow-ui` SPA embedded at `/ui`, run list, DAG graph/status, event history, SSE updates

### 📋 Roadmap

**N8 — Platform skeleton + native tool calling (v0.3.0 candidate):** ✅ closed
- LLM `tool_calls` / `tool_choice` native ✅ / Server gateway core routes ✅ / DB schema ✅
- ✅ `Tool` idempotency metadata bridge: `AgentNodeResumeContract::from_result_with_tools` consults `Tool::idempotency()` so registry-declared `Idempotent` tools auto-replay on partial-resume (DAG + skill_agent paths wired)
- ✅ `FlowValue::File`/`Url` checkpoint round-trip type fidelity: disk save→load preserves variant tags; tagged-but-corrupt payloads warn loudly instead of silently downgrading to `Json`

**N9 — Multi-agent + ecosystem (v0.4.0 candidate):** ✅ closed
- ✅ Handoff/blackboard/debate; ✅ OS sandbox; ✅ OTel `traceparent` propagation; ✅ RAG eval harness; ✅ LLM provider consistency suite (foundation)
- ✅ Cross-provider streaming / multimodal / tool-calling consistency tests: streaming covered by the `cross_provider_streaming_paths_yield_uniform_hello_world_concatenation` invariant; multimodal covered by `cross_provider_multimodal_paths_produce_uniform_response_shape`; tool-calling covered by `cross_provider_tool_call_paths_produce_uniform_canonical_shape` (basic) plus four `cross_provider_tool_choice_<variant>_is_honored_by_every_provider` invariants (`auto` / `none` / `required` / specific-tool)
- ✅ Live-LLM nightly CI: `.github/workflows/llm-live.yml` runs `provider_consistency_live` against all 9 providers (OpenAI / Anthropic / Google / Moonshot / StepFun / GLM·Zhipu / DashScope·Alibaba / DeepSeek / MiniMax) nightly at 09:30 UTC; per-provider tests self-skip when the corresponding API-key secret is absent; not wired into the `release-gate` aggregate so PRs are never gated on live API calls. `workflow_dispatch` accepts an optional comma-separated `providers` filter for ad-hoc subsets. The 4 OpenAI-compat vendors (GLM, DashScope, DeepSeek, MiniMax) share `OpenAIProvider` via the `create_provider` factory and the `default_models.yml` registry — no dedicated provider module needed because the wire shape matches.

**N10 — Plugin / distributed / Web UI (v1.0.0-rc candidate):** ✅ closed
- ✅ `docs/AGENT_SDK.md` extension guide + runnable examples (`custom_runtime` / `custom_reflection` / `custom_memory_summary`); core extension traits rustdoc-clean
- ✅ Plugin / Custom Node foundation: subprocess JSON-RPC runtime, manifest/lifecycle, sandbox bridge, `type: plugin` workflow node, plugin CLI, and marketplace signature/version handoff
- ✅ Distributed scheduling foundation: `WorkerProtocol`, gRPC transport choice, server control-plane façade, `agentflow-worker` runtime/binary, stitched worker traces mapped to OTel spans
- ✅ Web UI debugger: React + Vite + TypeScript SPA embedded at `/ui`, run list, DAG graph/status, event history replay, and SSE updates
- ✅ Plugin marketplace remote registry foundation: unified Skill/Plugin manifest, read-only HTTP client, artifact cache, signature verifier, marketplace CLI, and docs

Tag-cut + production deployment rehearsal (P7.4-FU4 checklist) remain the only operational steps before the actual `v1.0.0-rc.1` tag.

See `RoadMap.md` for the full plan; `docs/archive/PROJECT_EVALUATION_2026-05-19.md` for the most recent evaluation (2026-05-14 and 2026-05-01 evaluations are retained as historical context). For change history, prefer `git log` over a doc summary.

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
3. Register the `type:` string in `agentflow-config/src/executor/factory.rs` (capability-backed nodes live in `agentflow-nodes-ai`)
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

**Last Updated**: 2026-06-21 (P-A track sync: `agentflow-config` crate, dynamic-workflow CLI, `race_with_limits`, burned `server→cli` edge)
**AgentFlow Version**: 0.2.0+ (targeting v0.3.0)
**Rust Edition**: 2024 (all workspace members)
**Composite Maturity Rating**: A (per `docs/archive/PROJECT_EVALUATION_2026-05-19.md`)

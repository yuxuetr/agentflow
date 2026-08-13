# AgentFlow Project - Claude Code Configuration

## Project Overview

AgentFlow is a Rust workspace that supports both deterministic DAG workflows and agent-native autonomous loops, with full LLM, MCP, RAG, Skill, and tracing support. The workspace has 24 Rust crates plus 1 Web UI crate (`agentflow-ui`, a Vite-built React SPA embedded by the server).

A narrow-waist **contract kernel** (L0) was extracted by the P-A track (`docs/RFC_CRATE_ARCHITECTURE.md`; validated by `docs/ARCHITECTURE_EVALUATION_2026-06-20.md`): the runtimes never depend on each other, only on shared contracts, enforced by `cargo xtask check-arch` (eight dependency laws). The four execution paradigms (static DAG / native loop / harness / dynamic workflow) and their three-axis mental model live in `docs/ARCHITECTURE.md` § Four Execution Paradigms.

Recommended five-layer mental model:

- **L0 Contract Kernel** (narrow waist): `agentflow-value` (`FlowValue`), `agentflow-graph` (the `Flow` IR / `AsyncNode` / `expr` / `AgentFlowError`), `agentflow-store-spi` (`MemoryStore` + `KnowledgeBackend`), `agentflow-agent-spi` (`AgentRuntime` / turn-driven façade / `Capability` lowering), `agentflow-async-util` (retry/timeout/`race_with_limits`), plus `agentflow-tool` (the `Tool` contract: trait, `ToolRegistry`, `ToolMetadata`, `Capability`, `ToolPolicy`, `SecurityProfile`, the `SandboxBackend` trait + DTOs — split out of `agentflow-tools` in T3.3, `docs/RFC_TOOL_CONTRACT_SPLIT.md`, since the latter bundles concrete builtin tools + OS-sandbox backends that don't belong in a kernel crate)
- **L1 Execution Core** (the executor): `agentflow-core` runs the L0 `Flow` IR — scheduler, checkpoint, retry-executor, resource manager, health, events — exposed via the `FlowExt` trait (`flow.run()`). IR ≠ executor; the L0 types are re-exported under `agentflow_core::*` for compatibility.
- **L2 Capability Adapters**: `agentflow-nodes` (tool-tier nodes), `agentflow-nodes-ai` (capability-backed nodes), `agentflow-llm`, `agentflow-tools` (builtin tool implementations + concrete OS-sandbox backends; depends on and re-exports `agentflow-tool` in full), `agentflow-tools-ai` (capability-backed `Tool` adapters — TTS/ASR/image/video generation — mirroring `agentflow-nodes-ai`'s role for `Tool`-consuming callers instead of DAG nodes), `agentflow-mcp`, `agentflow-rag`, `agentflow-memory`
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
- Wired-in primitives (real callers in `Flow`/the executor): retry/retry_executor, timeout, checkpoint, events, `resource_limits` (W5.3 — `FlowExecutionConfig::resource_limits: Option<ResourceLimits>`, advisory-only: `notify_state_size` emits `WorkflowEvent::ResourceWarning` when the state pool exceeds the configured limit, never evicts or rejects)
- `resource_manager`, `concurrency`, `state_monitor` were deleted in W5.3 — zero real callers workspace-wide, and `state_monitor`'s LRU eviction was actively unsafe for `Flow`'s state pool (a node's output can be a real dependency for any later node via `input_mapping`, regardless of recency). `health` (`HealthChecker`) is wired-in for real too, but its consumer lives in `agentflow-server` (`/health/ready` runs a `SELECT 1` DB check, `503` on failure), not in `agentflow-core` itself

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

#### L0 — agentflow-tool
The `Tool` contract (T3.3, `docs/RFC_TOOL_CONTRACT_SPLIT.md`): the `Tool` trait, `ToolRegistry`, `ToolMetadata` (with `source: ToolSource::{Builtin, Script, Mcp, Workflow}`, permissions, original MCP server/tool names), `ToolIdempotency`, `ToolOutputPart::{Text, Image, Resource}`, `Capability`/`EffectiveCapabilities`, `ToolPolicy`, `SecurityProfile`, and the `SandboxBackend` trait + its DTOs (`SandboxScope`/`SandboxStatus`/`SandboxEnforcement`/`SandboxError`). Dependency-free — a genuine L0 kernel crate, unlike the crate below it split out of. Runtimes (`agentflow-agents`, `agentflow-harness`) depend on this crate directly, never on `agentflow-tools`.

#### L2 — agentflow-tools
Built-in tool implementations + concrete OS-sandbox backends. Depends on and re-exports `agentflow-tool` in full, so every existing `use agentflow_tools::{Tool, ToolRegistry, ...}` call site is unaffected:
- `SandboxPolicy` (the in-process allow-list `ShellTool`/`FileTool`/`HttpTool` consult)
- Built-in `FileTool` / `HttpTool` / `ShellTool` (shell defaults to disabled)
- `ToolOutputPart::{Text, Image, Resource}` for typed multimodal output
- OS-level sandbox backends (macOS sandbox-exec / Linux seccomp + Landlock + cgroup v2 resource limits) for `ShellTool` / `ScriptTool`, `SecurityConfig::os_sandbox` defaults `true` (S3.4) — a skill opts a tool *out* rather than in
- **`code_exec` (S4.2, `docs/RFC_LLM_CODE_EXECUTION.md`):** runs LLM-generated Python (v1, no other languages yet) inside `ContainerBackend` — a strongly-isolated tier shelling out to a real container engine (Apple's `container` CLI, preferred: genuine per-invocation Linux microVM; or rootless Podman) instead of the syscall-scoped OS sandbox above, since llm-generated content is adversarial by construction on every call (never author-signed like `ScriptTool`'s). Mandatory isolation — refuses to run rather than degrade when no engine is available — with a fresh ephemeral workdir per call, hardcoded resource limits (256 MiB / 30 CPU-seconds / 32 pids), zero network access (no egress allowlist proxy exists yet), and `ToolIdempotency::NonIdempotent` so harness's production-profile approval escalation applies automatically. `agentflow skill inspect --explain-permissions` and `agentflow doctor` both report the container engine's status independently from the OS-sandbox backend above.

#### L2 — agentflow-tools-ai
Capability-backed `Tool` adapters: `TtsTool` / `AsrTool` / `Text2ImageTool` / `Image2ImageTool` / `ImageEditTool` / `ImageUnderstandTool` / `Text2VideoTool`, registered under the stable names `tts` / `asr` / `text_to_image` / `image_to_image` / `image_edit` / `image_understand` / `text_to_video`. Each is a thin wrapper directly over the matching `agentflow_llm::AgentFlow::*` modality-dispatch function (`tts`, `asr`, `text2image_for`, `image2image`, `image_edit`, `text2video_for`; `image_understand` has no dedicated trait and routes through the ordinary chat multimodal path instead) — same model registry YAML, same per-vendor `modality_dispatch.rs` reconciliation the DAG nodes in `agentflow-nodes-ai` already use, zero duplicated vendor logic. Exists as its own adapter crate — depending on `agentflow-tool` (contract) + `agentflow-llm` (capability) — rather than adding `agentflow-llm` as a dependency of the tool-tier `agentflow-tools` crate, mirroring the P-A0.5 precedent that extracted `agentflow-nodes-ai` out of `agentflow-nodes` for the same reason. This is what makes TTS/ASR/image/video generation callable by `ToolRegistry`-consuming callers (`ReActAgent`, `DynamicWorkflowAgent`, and transitively Harness) that were previously limited to `WorkflowTool`'s coarse whole-`Flow` wrapping — before this crate, none of the six modalities were reachable outside a DAG at all. `register_all(&mut registry)` registers all seven at once; `agentflow-skills`' `SkillBuilder` also accepts them individually by name in a skill manifest's `[[tools]]` list, and `agentflow workflow dynamic --allow-modalities` opts an LLM-authored dynamic plan into them (off by default, same rationale as shell never being registered there — billed vendor calls an adversarial-by-construction plan shouldn't reach without an explicit operator grant).

#### L2 — agentflow-mcp
Model Context Protocol integration: client + server, JSON-RPC 2.0, retry/timeout/reconnect, latency benchmarks. Two client transports: `StdioTransport` (local process, Legacy era — `initialize()` handshake, protocol version `2024-11-05`) and `StreamableHttpTransport` (W5.8-3, Modern era — stateless per-request `_meta`-carried protocol version `2026-07-28`, hand-rolled per-request-scoped SSE parsing). `MCPClient::connect()` selects era by transport type (`client/era.rs`, W5.8-4) — `StreamableHttpTransport` speaks Modern (per-request `_meta`, `UnsupportedProtocolVersionError` retry, MRTR `InputRequiredResult` detection), every other transport stays Legacy, byte-for-byte unchanged. See `docs/RFC_MCP_PROTOCOL_MODERNIZATION.md`: this is the RFC's Phase 2 + Phase 4; Phase 3 (Modern-era *server* support) is still open. The MCP→`agentflow-tool::Tool` adapter (`McpToolAdapter` + `McpClientPool`) lives in `agentflow-skills/src/mcp_tools.rs`, not in this crate — `agentflow-skills` owns the conversion because the skill builder is the entry point that knows which MCP servers a skill manifest declares.

#### L2 — agentflow-rag
Retrieval-Augmented Generation: document chunking, embeddings (OpenAI API or local ONNX), Qdrant vectorstore, retrieval, reranking. Sources: PDF, HTML, CSV, text (PDF/HTML loaders carry a default 50 MiB / 10 MiB size cap, override via `with_max_bytes`). Eval harness (`eval` module): JSONL dataset format (`corpus`/`queries`/`qrels`), Recall@K / MRR / nDCG@K metrics, baseline comparison with paired sign test, CLI `agentflow rag eval`. (StepFun embedding provider mentioned in earlier drafts is not implemented; only OpenAI + local ONNX exist today.) **RAG repositioning (P-A4.1):** implements the L0 `KnowledgeBackend` SPI as `Bm25KnowledgeBackend` (in-memory keyword index, bundled-files tier) + `VectorStoreKnowledgeBackend` (vector tier), and exposes `RagSearchTool` — a registry-installable `rag_search` `Tool` (idempotent, read-only) wrapping any `Arc<dyn KnowledgeBackend>`. This puts RAG on the capability/tool axis behind a Skill's `knowledge:` declaration rather than as a top-level mode.

#### L2 — agentflow-memory
Agent conversation memory: `MemoryStore` trait with `SessionMemory` (token-windowed in-memory) and `SqliteMemory` (persistent). `SemanticMemory` for similarity search (interlocks with `agentflow-rag`).

#### L3 — agentflow-agents
Agent-native runtime and patterns:
- `AgentRuntime` trait with `AgentContext`, `RuntimeLimits` (max_steps, max_tool_calls, timeout_ms, token_budget), `AgentCancellationToken`
- `ReActAgent` (observe/plan/tool/result/reflect/final answer with memory summary)
- `PlanExecuteAgent` (structured plan JSON + sequential execution)
- `ReflectionStrategy` trait (`FailureReflection` / `FinalReflection` / `NoOpReflection`) — non-fatal, self-critique only; fires after a stop decision is already made and cannot change control flow
- `VerificationStrategy` trait (`AlwaysApprove` built-in) — gates a `ReActAgent` candidate final answer *before* it stops: a `Rejected { feedback }` verdict feeds the critique back into memory and loops the agent for another attempt (bounded by `ReActConfig::max_verification_attempts`, default 2; exhausting it force-accepts rather than erroring). Recorded as an `AgentStepKind::Verify` step / `AgentEvent::VerificationCompleted` event. See `agentflow-agents/examples/custom_verification.rs`.
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
- **Frozen contract surface (H0):** `HarnessEvent` line-delimited JSON envelope (closed kind set, 12 kinds today — 9 from H0 plus 3 additive kinds from later milestones: `session_started`, `step_started`, `tool_call_requested`, `approval_requested`, `approval_decided`, `interrupt_requested` (V2.3), `interrupt_answered` (V2.3), `tool_call_completed`, `token_delta` (streaming), `background_task_updated`, `memory_summary_added`, `stopped`); `ApprovalRequest` / `ApprovalDecision` / `ApprovalRisk` / `ApprovalScope` interactive approval protocol; async hook traits `PreToolHook` / `PostToolHook` / `ApprovalProvider` / `ContextProvider`; session descriptor `HarnessContext` / `HarnessProfile` / `HarnessRuntimeKind`
- **Runtime MVP (H1):** `HarnessRuntime` wrapping any `agentflow_agents::AgentRuntime` (typically `ReActAgent`) via `Box<dyn AgentRuntime>`; four default context providers (`AgentsMdProvider`, `TodosMdProvider`, `RoadmapMdProvider`, `WorkspaceLayoutProvider`) with priority + token-cost estimates and priority-aware budget trimming; `InMemoryEventSink` / `JsonlEventSink` / `StdoutEventSink` / `SinkChain` persistence; deterministic `AgentEvent` → `HarnessEvent` translation with monotonic `seq`; `tracing_bridge` honoring the `AGENTFLOW_TRACE_DIR` convention so Harness session logs co-locate with the rest of the trace tooling.
- **Hooks + approval (H2):** `HookedTool` + `wrap_registry(registry, HookConfig)` decorate every registered `Tool` with a pre/post hook + approval pipeline. Pre-hook timeouts and errors are fail-closed; post-hooks are advisory. Three `ApprovalProvider` implementations (`AutoAllow`, `AutoDeny`, `Cli`). Production profile escalates `NonIdempotent` calls to `RequireApproval` automatically. `Session` / `Run` scope decisions are cached per tool. `DenyAndStop` short-circuits every subsequent tool call. Approval-lifecycle events (`approval_requested` / `approval_decided`) flow through the existing `SinkChain`.
- **Parallel tool calls (H3):** `ReActAgent::run_with_context` adds a batch dispatcher (in `agentflow-agents/src/react/agent/batch.rs` — W5.4 split the former 8,254-line `react/agent.rs` into `react/agent/*.rs` by concern: `core`/`config`/`tool_dispatch`/`batch`/`memory`/`verification`/`checkpoint`/`prompt`/`turn_driven`/`support`, plus a relocated `tests.rs`; the `react::{ReActAgent, ReActConfig, ...}` public surface is unchanged) that activates when the LLM returns `>= 2` native tool calls in one turn. Idempotent calls run concurrently via `futures::future::join_all`; `NonIdempotent` / `Unknown` calls run serially in LLM-returned order. `ToolPolicyDecision` / `ToolCapabilityDecision` / `ToolCallStarted` / `ToolCall` step rows all fire in LLM-returned order before any execution begins, so trace replay stays deterministic. Partial failures keep the batch moving; pre-cancel and `max_tool_calls` checks are atomic.
- **Background tasks (H4):** `agentflow-harness::tasks` provides `TaskRuntime` + `TaskHandle` + `TaskAgentFactory` plus 5 built-in tools (`task_create`, `task_get`, `task_list`, `task_stop`, `task_output`). Each task spawns a `tokio::task` running an inner agent; lifecycle transitions (`Pending → Running → Completed | Failed | Cancelled`) emit `BackgroundTaskUpdated` through the parent `SinkChain`. Nested task spawning is rejected via a `tokio::task_local!` flag. Output capture is bounded by `max_output_bytes` (default 64 KiB).
- **Flow governance (P-A2.2):** `HarnessRuntime::for_flow()` + `run_flow(flow, runner, inputs, options)` govern a deterministic `agentflow-graph::Flow` run (not just an agent loop). It brackets a `FlowRunner`-driven execution with the Harness envelope (`session_started` runtime=`flow` → `stopped`, classifying completed/failed/timed-out via the per-node result map). Tool calls inside the Flow's nodes are governed by wrapping the node registry with `wrap_registry` + a `HookConfig` sharing the runtime's seq counter + sinks, so approval/hook/audit events interleave on the same monotonic stream. Adds a `harness → agentflow-graph` dep (runtime→contract; executor stays out via the injected `FlowRunner`). Node-level `step_started` events fire per node (via `Flow::with_event_listener`); The `agentflow harness run-flow <yaml>` CLI runs a config workflow under this governance (envelope + node events); a server route is the remaining follow-up.
- **CLI surface:** `agentflow harness run|run-flow|resume|list|inspect` with `--output text|json|stream-json` and the full flag set documented in `docs/HARNESS_MODE.md`.
- Stability tier **beta** as of P-H.5 closure: `HarnessEvent` envelope, `ApprovalRequest`, and `ApprovalDecision` are plumbed through both the in-process hook runtime and the HTTP surface (`/v1/harness/sessions/{id}/events`, `/approvals`). See `docs/HARNESS_MODE.md` for the implementation spec and `docs/STABILITY.md` for the wire-shape promise. `tracing_bridge` now ships **two** sink tiers: (a) JSONL-only via `open_tracing_sink(...)` (per-session `<base>/harness/sessions/<id>.jsonl` for raw replay), and (b) `ExecutionTrace` via `open_execution_trace_sink(storage)` which translates each `HarnessEvent` stream into an `agentflow_tracing::ExecutionTrace` and persists it through any `TraceStorage` backend (Q3.10.4). One related item remains **open**: first-party OTLP gRPC transport (+ TLS + auth) is deferred — W4.4 shipped the HTTP+JSON leg (`agentflow_tracing::otlp::OtlpHttpSpanSink`, `otlp-http` feature); operators still bring their own `OtelSpanSink` impl for gRPC.

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
- `rag ops search|index|collections` (operator vector-store ops) + `rag eval` (feature-gated)

#### L4 — agentflow-tracing
Observability:
- Event collection via `EventListener` (non-invasive); the in-process drain task processes events in arrival order so terminal node state cannot race the `WorkflowCompleted` save
- Persistence: JSONL (default, `FileTraceStorage` — the only implemented backend; SQLite/Postgres are DDL-only schema constants in `storage::schema` with no `TraceStorage` implementation, and the dead `postgres` Cargo feature that gestured at one was removed in W4.4). Producer-side wiring is live in CLI (`agentflow workflow run` always writes file traces under `AGENTFLOW_TRACE_DIR` / `~/.agentflow/traces` by default) and in the gateway (`POST /v1/runs` writes file traces only when `AGENTFLOW_TRACE_DIR` is explicitly set, since the cleanup sweep does not cover that dir). Harness sessions (`HarnessEvent`) persist to Postgres + SSE only; file-backed trace integration would need a separate `HarnessEventListener → ExecutionTrace` adapter and is not wired today.
- `agentflow trace replay` + TUI timeline (read from the directories above)
- OpenTelemetry span model (`OtelSpan` / `OtelSpanSink` trait) + W3C trace context propagation (inbound `traceparent` honored via `context::scope`; outbound via `LlmTraceContext`). W4.4: first-party OTLP/HTTP+JSON exporter (`agentflow_tracing::otlp::OtlpHttpSpanSink`, `otlp-http` feature) built on the official `opentelemetry-otlp` crate — `OtlpHttpConfig::from_env()` reads the standard `OTEL_EXPORTER_OTLP_ENDPOINT`/`OTEL_EXPORTER_OTLP_HEADERS` env vars. gRPC transport (+ TLS + auth) is still not built in — operators bring their own `OtelSpanSink` for that.
- Redaction for API keys, env secrets, sensitive tool params
- `AGENTFLOW_TRACE_DIR` / `AGENTFLOW_RUN_DIR` for explicit storage roots

#### L4 — agentflow-server
Axum gateway for platform mode. Workflow surface: `/v1/runs` (POST/GET), `/v1/runs/{id}/events` (SSE with backfill), `/v1/skills`, `/v1/skills/{name}:run`. Harness Mode surface (P-H.5, closed): `/v1/harness/sessions` (POST/GET), `/v1/harness/sessions/{id}` (GET + `:cancel` POST + `:resume` POST), `/v1/harness/sessions/{id}/events` (SSE with backfill), `/v1/harness/sessions/{id}/events/history` (JSON), `/v1/harness/sessions/{id}/approvals` (GET pending) + `POST .../{request_id}` (decide), backed by `LiveHarnessExecutor` in production (wires `HarnessRuntime` + `ReActAgent` + hook-wrapped tool registry + `ServerApprovalProvider`) and `StubHarnessExecutor` in tests. `:resume` accepts `mode: "rerun" | "append"` (default `rerun` for backwards compat); rerun clears prior events and restarts the seq series at 0, append preserves the prior log and continues at `MAX(seq) + 1` via the upstream `HarnessRuntime::with_initial_seq` knob. Bearer-token auth, unified error envelope, `WorkflowEventListener` bridge to DB. `FlowRunExecutor` is the production default and runs config-first workflows in-process; `StubExecutor` remains as the test-only stand-in for route-plumbing tests that don't need real execution.

#### L4 — agentflow-db
PostgreSQL persistence for the gateway. Thirteen-table schema (runs / steps / events / artifacts / skill_installs / mcp_sessions / harness_sessions / harness_session_events / user_preferences / harness_loop_checkpoints / run_cancellation_intents / approval_decision_intents / tenant_admission_windows) via `sqlx::migrate!()` — the last three back W4.2's cross-replica cancellation/approval/admission intents (`RunRepo::record_cancellation_intent`/`create_if_admitted`, the standalone `ApprovalIntentRepo`), not a separate top-level feature. Repository layer: `RunRepo` / `StepRepo` / `EventRepo` / `ArtifactRepo` / `SkillInstallRepo` / `McpSessionRepo` / `HarnessSessionRepo` / `HarnessEventRepo` / `UserPreferenceRepo` / `ApprovalIntentRepo`, plus `DbLoopCheckpointer` for `harness_loop_checkpoints`.

#### L4 — agentflow-worker
Standalone worker process for distributed DAG execution. Speaks `WorkerProtocol` over gRPC to the server control plane, pulls assigned tasks, executes the node payload locally, and streams events back with W3C `traceparent` continuity so worker spans stitch into the parent OTel trace. Node payloads dispatched today (P2.8, closed): `template`, `file`, `mock`, `llm`, `http`, `mcp`, `agent` (`agentflow-worker/src/lib.rs::execute_supported_node_payload`). The `agent` payload's tool wiring is still minimal — it runs against an empty `ToolRegistry` (no distributed tool-call support yet); that remains the one open gap in worker payload coverage.

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
- **RAG** — chunking, embeddings, Qdrant, retrieval, reranking; CLI `rag ops search|index|collections` (operator vector-store ops) + `rag eval`; eval harness with Recall@K / MRR / nDCG@K metrics + paired baseline comparison
- **Observability/reliability (Phase 1.5)** — timeout control, K8s-compatible health checks (`agentflow-server`'s `/health`/`/health/live` unconditional 200, `/health/ready` runs a real DB `SELECT 1` and returns 503 on failure, W5.3), checkpoint recovery, retry, advisory-only resource-limit warnings (`FlowExecutionConfig::resource_limits` → `WorkflowEvent::ResourceWarning`, W5.3 — no eviction/enforcement mechanism exists), structured logging, Prometheus metrics
- **Tracing** — `EventListener`, JSONL persistence (`FileTraceStorage`; SQLite/Postgres are DDL-only, unimplemented), `trace replay` TUI, OTel span model + W3C `traceparent` propagation (inbound on workflow start + outbound through LLM HTTP calls). First-party OTLP/HTTP+JSON exporter ships (W4.4, `otlp-http` feature); gRPC transport (+ TLS + auth) is **deferred** — operators wire their own `OtelSpanSink` for that.
- **OS-level sandbox** — macOS sandbox-exec / Linux seccomp+Landlock+cgroup v2 backends for shell/script tools, `security.os_sandbox` defaults `true` (S3.4 — a skill opts a tool *out*, not in); active backend name + `enforcement_level` (`enforcing` / `permissive` / `disabled`) is visible in `ToolCapabilityDecision` events and `agentflow doctor --format json` output
- **`code_exec` LLM code execution** (S4.2) — `ContainerBackend` (Apple `container` CLI / rootless Podman) runs LLM-generated Python in a mandatory, strongly-isolated per-call container/microVM, separate from the OS-sandbox tier above; zero network access until an egress allowlist proxy lands; `agentflow doctor` / `skill inspect --explain-permissions` report its status independently
- **Platform skeleton** — server gateway routes (`/v1/runs`, SSE, skills) + DB schema/repos + auth
- **Distributed worker foundation** — `agentflow-worker` runtime/binary, gRPC `WorkerProtocol`, server control-plane façade, stitched worker traces mapped to OTel spans; all seven node payload types dispatch (`template`/`file`/`mock`/`llm`/`http`/`mcp`/`agent`, P2.8 closed) — the `agent` payload's distributed tool-call wiring is the one remaining gap
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

**Last Updated**: 2026-08-13 (P-LLM2.4 fixed the Anthropic multimodal content-translation bug + base64-image mishandling across every OpenAI-compatible vendor and Google, then implemented Anthropic PDF/document input + Google video input per current official API docs; new `agentflow-tools-ai` crate gives `ReActAgent`/`DynamicWorkflowAgent`/Harness access to TTS/ASR/image/video generation as `Tool`s for the first time, wired into `agentflow-skills` and `agentflow workflow dynamic --allow-modalities`; DeepSeek audit found `.thinking()` calls against `deepseek-v4-*` were a silent no-op — DeepSeek's V4 API uses a nested `thinking: {type, reasoning_effort}` block, not OpenAI's flat `reasoning_effort` string, despite routing through the shared `OpenAIProvider` — now dispatched correctly by model name, with a new `ThinkingKind::DeepSeekReasoningEffort`; P-LLM2.5 fixed Google/Moonshot/StepFun's streaming `tool_call_deltas` — previously hardcoded empty, silently dropping tool calls requested mid-stream and breaking the ReAct loop, which streams unconditionally; P-LLM2.6 added `ModelConfig`/`ModelCapabilities` output-modality metadata (`outputs: Option<Vec<OutputType>>`, mirroring `accepts`) — found this wasn't hypothetical: `gemini-2.0-flash-*-image-generation` already return inline image parts in ordinary chat turns, undersold as `Text`-only by `primary_output()`; metadata only, no response parser constructs `ContentType::Image` yet; P-LLM2.7 made `RateLimitConfig` (9 vendors' RPM/TPM, previously parsed then discarded) real — `ModelRegistry` builds one `governor` token bucket per vendor, `LLMClient` waits on it before dispatching; `retry_transient` now honors a server `Retry-After` header exactly when present (new `LLMError::RateLimitedWithRetryAfter`, built only by the 6 chat-provider `execute`/`execute_streaming` paths that actually retry) and equal-jitters its own computed exponential backoff otherwise, closing a thundering-herd gap the old zero-jitter formula didn't address); P-LLM2.8 fixed a stale doc comment in `agentflow-llm/src/providers/modality/mod.rs` still claiming nodes called StepFun directly pre-P-LLM.3 — they now route through `crate::modality_dispatch` via `AgentFlow::{tts,asr,text2image_for,image2image,image_edit}`; P-LLM2.9 closed out the P-LLM2 backlog — audited `tokenizer.rs`'s `pick_encoding()` against all ~190 registry model names and found genuine drift (DashScope's `qvq-*` vision-reasoning and `codeqwen*` models silently missed the `qwen` prefix and fell to the generic heuristic instead of the family's usual cl100k_base approximation), fixed it, and made a deliberate, documented decision *not* to bundle a default `pricing.yml` for `agentflow-agents::eval::pricing::PricingTable` — `ReActConfig::pricing_table` is already intentionally empty-by-default ("inert unless the caller configures real prices"), and unlike token-count approximation, wrong bundled prices are actively harmful (mis-enforced `cost_limit_usd`) rather than honestly rough, so pricing stays caller-supplied; W5.8 (`docs/RFC_MCP_PROTOCOL_MODERNIZATION.md` Phases 2 + 4) closed `agentflow-mcp`'s Legacy-only ceiling — `McpClientPool` now clears its cached client on any transient error, not just timeout; new additive Modern-era (`2026-07-28`) protocol types (`protocol/modern.rs`); a new `StreamableHttpTransport` (client side, hand-rolled per-request-scoped SSE parsing, `reqwest` now a real dependency) implementing the existing `Transport` trait unchanged, confirming the RFC's own "no redesign needed" finding; and `MCPClient::connect()` now speaks Modern (`_meta`-carried per-request protocol version/capabilities, no `initialize` handshake, `UnsupportedProtocolVersionError` retry, MRTR `InputRequiredResult` detection) over `StreamableHttpTransport` specifically — era is chosen by transport type rather than a runtime wire probe, a deliberate scope narrowing from the RFC's full cross-era-probing design since this crate's `StdioTransport`/test fixtures can't be proven safe against an unsolicited `server/discover` probe without a real Modern stdio server to test against; zero behavior change to any Legacy code path, confirmed by the full pre-existing test suite passing unchanged. Phase 3 (Modern-era *server* support) stays open as W5.8's remaining scope) — 2026-08-12 (W4.2 cross-replica gateway state externalized (SSE broker / cancellation / approvals / admission all now cross-replica-safe via Postgres NOTIFY + DB-intent patterns, `replicaCount: 1` no longer a hard constraint); W4.3b wired `DistributedDagScheduler` into `POST /v1/runs` via opt-in `execution_mode: "distributed"`; W5.1 doc-code drift correction pass; W5.3 deleted `resource_manager`/`concurrency`/`state_monitor` (zero real callers, `state_monitor`'s LRU eviction unsafe for `Flow`'s state pool), wired `resource_limits` into `Flow` as an advisory-only warning, and wired `HealthChecker` into `agentflow-server`'s `/health/ready`)
**AgentFlow Version**: 0.2.0+ (targeting v0.3.0)
**Rust Edition**: 2024 (all workspace members)
**Composite Maturity Rating**: A (per `docs/archive/PROJECT_EVALUATION_2026-05-19.md`)

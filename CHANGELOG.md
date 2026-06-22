# Changelog

All notable changes to AgentFlow are documented in this file. Format
loosely follows [Keep a Changelog](https://keepachangelog.com/), and
the project tracks [Semantic Versioning](https://semver.org/) at the
workspace level (most crates pin to 0.2.x or 0.3.0-alpha as of this
file's first entry).

## [Unreleased]

_New entries go here. Will roll into the next tag (likely
`v1.0.0-rc.2` or `v1.0.0`)._

### Added

- **`agentflow harness chat` gains a `/clear` command (H.5.1).** Clears the
  current session's conversation memory **in place** — keeping the session id —
  and rebuilds the runtime so the next turn starts fresh (vs. `/new`, which
  rotates to a new id). It clears the `--model` path's persistent
  `SqliteMemory` for the session; in `--skill` mode it prints a note that a
  skill configuring its own persistent memory keeps it separately (use `/new`
  for a guaranteed reset). A not-yet-created store is treated as already empty.

- **`PlanExecuteAgent::run_as_flow` — plan + run on the graph engine end-to-end
  (P-A2.2 follow-up).** Building on `compile_plan_to_flow`, the new
  `run_as_flow(context, runner)` plans with the LLM, compiles the plan to a
  `Flow`, and executes it on the deterministic graph engine via an injected
  `FlowRunner` — inheriting retry / checkpoint / timeout / tracing / replay and
  the plan's parallelism — instead of the hand-rolled sequential loop. It returns
  an `AgentRunResult` with an `Observe → Plan → per-node ToolCall/ToolResult →
  FinalAnswer` trace built from the flow's state pool (a failed node stops with
  `AgentStopReason::Error`), and honours the same cancellation / timeout /
  token + step + tool-call budgets as the sequential path. `PlanExecuteError`
  gains a `Flow(#[from] AgentFlowError)` variant. The legacy sequential
  `run_with_context` path is unchanged.

- **Dynamic-workflow plans can include agent steps (P-A2.2 follow-up).**
  `WorkflowPlanStep` gains a `kind` field (`tool` default | `agent`).
  `compile_plan_to_flow` compiles a `kind = "agent"` step to an `AgentNode`
  wrapping a `ReActAgent` built from the step's `params` (`model` required,
  `persona` optional, `prompt` → the agent's `message`); the agent shares the
  plan's tool registry, so it inherits the same sandbox / approval governance.
  Dependency wiring now uses each step's real output key — `result` for a tool
  step, `response` for an agent step — so a step depending on an agent receives
  its answer. The `DynamicWorkflowAgent` planner prompt teaches the LLM the agent
  step shape, so `agentflow workflow dynamic` can author them. Validation
  rejects an agent step missing `model`/`prompt` and a tool step with no `tool`.

- **`agentflow harness run-flow <workflow.yaml>` CLI (P-A2.2 follow-up).** Runs a
  config workflow (DAG) under harness governance: builds the `Flow` from YAML,
  runs it via `HarnessRuntime::run_flow` + `CoreFlowRunner`, and persists the
  Harness envelope (`session_started` runtime=`flow` → per-node `step_started` →
  `stopped`) as JSONL like an agent session — so a deterministic workflow gets
  the same observable / replayable governance stream. Flags: `--input key=value`
  (repeatable), `--model`, `--profile`, `--output text|json|stream-json|json-envelope`,
  `--workspace`, `--run-dir`, `--timeout-ms`, `--session`, `--max-concurrency`.
  A non-completed run exits non-zero. `--runtime flow` now parses for
  `HarnessRuntimeKind::Flow`. (Config-built nodes embed their own tools, so the
  CLI delivers the envelope + node events; tool-call approval governance is the
  programmatic `run_flow` + wrapped-registry path. A server route is a follow-up.)

- **Node-level `step_started` events in a governed Flow run (P-A2.2 follow-up).**
  `HarnessRuntime::run_flow` now instruments the flow with an
  `agentflow-graph::EventListener` that forwards each node's start (by id) onto a
  channel, drained concurrently with the run (biased `select!`) so a
  `step_started` event (`step_type = "node:<id>"`) interleaves in real time with
  the tool-call / approval events between `session_started` and `stopped`. The
  Flow's execution is observed through the existing `Flow::with_event_listener`
  seam — no executor coupling.

- **Harness governs a `Flow` run — orthogonal governance MVP (P-A2.2).** The
  harness governance shell now wraps a deterministic `agentflow-graph::Flow`
  run, not just an agent loop. New `HarnessRuntime::for_flow()` +
  `HarnessRuntime::run_flow(flow, runner, inputs, options)` brackets a
  `FlowRunner`-driven execution with the Harness envelope (`session_started`
  with the new `HarnessRuntimeKind::Flow` … `stopped`, classifying
  completed / failed / timed-out). Governance reaches the Flow's tool calls at
  the registry seam: when the Flow's node registry is wrapped via
  `wrap_registry` with a `HookConfig` sharing the runtime's seq counter + sinks,
  every `tool_call_requested` / `approval_requested` / `tool_call_completed`
  interleaves on the same monotonic event stream — an `AutoDeny` provider blocks
  a mutating tool inside a node and fails the run. `agentflow-harness` gains a
  downward dep on `agentflow-graph` (runtime→contract; the executor stays out
  via the injected `FlowRunner`). MVP scope: node-level step events, a CLI/server
  surface, and a `HarnessRuntimeKind::Flow` parser are follow-ups.

### Changed

- **`rag search` / `index` / `collections` CLI demoted under an `ops` group
  (P-A4.1b; RFC §9).** RAG's agent-facing retrieval path is now the `rag_search`
  tool a Skill exposes (P-A4.1/P-A4.2), so the direct vector-store commands move
  to `agentflow rag ops {search,index,collections}` — operator affordances for
  inspecting/populating the store out-of-band. `agentflow rag eval` stays
  top-level (it is the retriever quality gate). **Breaking** for scripts calling
  the old `agentflow rag search|index|collections` paths; flags are unchanged.

### Added

- **`Capability` contract + Skill lowering (P-A4.3; RFC §2).** The second
  load-bearing kernel trait lands: a *capability* is a packaged ability (persona
  + tools + knowledge + config) that `lower()`s to *tools + context* at the
  runtime boundary.
  - **`agentflow-agent-spi`** gains the `Capability` trait + `Lowered { tools,
    context }` + `CapabilityError` (`#[non_exhaustive]`). `Lowered.context`
    reuses the existing `ContextItem` (priority + token estimate) so a lowered
    capability slots straight into the harness/runtime prompt-budgeting
    machinery; `Lowered::merge` is how capabilities compose by flatten. Distinct
    from the OS-sandbox `agentflow_tools::Capability` enum (a process permission)
    — the two never share a position.
  - **`agentflow-skills`** implements it: `SkillCapability` bundles a manifest +
    skill dir and `lower()`s to the Skill's tools (built-in + MCP + the P-A4.2
    `rag_search` tool, via `build_registry`) plus its persona as one `Critical`
    context fragment. New direct L0 dep `agentflow-skills → agentflow-agent-spi`
    (capability → contract); `check-arch` stays OK.

  A surface can now hold a heterogeneous `Vec<Box<dyn Capability>>`, lower each,
  and merge the results into one registry + context bundle before handing a
  runtime its inputs — the runtime never knows a Skill was involved. Full
  surface adoption (replacing the direct `SkillBuilder::build` path) is a future
  step; the contract + impl + composition primitive land here.

- **Tiered Skill knowledge — `[[knowledge]]` gains a `backend` field (P-A4.2;
  RFC §9).** A skill's knowledge entries now choose how their content reaches
  the agent:
  - `backend = "files"` (default) — inline the file into the system prompt, as
    before. Omitting `backend` preserves the pre-P-A4.2 behaviour, so existing
    skills are unaffected.
  - `backend = "rag"` — index the file into an in-memory BM25 index and expose a
    single `rag_search` tool over it (built on the P-A4.1 `KnowledgeBackend` /
    `RagSearchTool`); the agent retrieves relevant passages on demand instead of
    carrying the whole corpus in its prompt. No vector DB / network — the skill's
    bundled files are indexed locally.

  `SkillBuilder` routes each entry independently: `build_persona` inlines only
  the files-tier entries, while a new `register_knowledge_backends` step indexes
  the rag-tier entries and registers the shared `rag_search` tool. `references/`
  documents remain files-tier. Documented in `docs/SKILLS.md`.

- **RAG repositioned onto the capability axis — `KnowledgeBackend` SPI +
  `rag_search` tool (P-A4.1; RFC §9).** RAG is no longer a top-level mode; it is
  a knowledge-retrieval *capability* behind a Skill's `knowledge:` declaration.
  - **`agentflow-store-spi`** gains the `KnowledgeBackend` trait + `KnowledgeChunk`
    / `KnowledgeError` (`#[non_exhaustive]`) — the kernel contract for "given a
    query, return relevant passages", living alongside `MemoryStore` so
    `agentflow-skills` and `agentflow-rag` agree on the shape without `skills`
    depending on the `rag` implementation crate.
  - **`agentflow-rag`** implements the SPI two ways: `Bm25KnowledgeBackend` (an
    in-memory BM25 keyword index — no network/embeddings, for the bundled-files
    tier) and `VectorStoreKnowledgeBackend` (semantic retrieval over any
    `VectorStore` via a `RetrievalStrategy`, for large/dynamic corpora). It also
    exposes `RagSearchTool` — a registry-installable `rag_search` `Tool`
    (idempotent, read-only) wrapping any `Arc<dyn KnowledgeBackend>`, so an agent
    retrieves on demand instead of having the whole corpus inlined into its
    prompt. `agentflow-rag` now depends downward on `agentflow-store-spi` +
    `agentflow-tools` (both L0); `check-arch` stays OK.

  Deferred to a P-A4.1b follow-up: demoting the user-facing `rag search` /
  `rag index` CLI to ops subcommands (pure UX regrouping, separable from the
  architectural repositioning). Skill `knowledge:` `backend:` wiring is P-A4.2,
  which sits on this contract.

- **`agentflow-nodes` split into a tool tier + `agentflow-nodes-ai` capability
  tier (P-A0.5 / P-A4 nodes decomposition; `docs/RFC_NODES_DECOMPOSITION.md`).**
  The monolithic node library is divided so the tool-tier crate carries zero
  capability dependencies:
  - **`agentflow-nodes`** now ships only the tool-tier `AsyncNode`s
    (`template`, `file`, `http`, `batch`, `conditional`, `arxiv`, `markmap`)
    and depends on just the IR + `agentflow-tools`. Its `agentflow-llm` /
    `agentflow-mcp` / `agentflow-rag` deps (and the `llm` / `mcp` / `rag`
    features) are gone; defaults stay `["http", "file", "template"]`.
  - **`agentflow-nodes-ai`** (new) carries the capability-backed adapters
    (`llm`, `asr`, `tts`, `text_to_image`, `image_to_image`, `image_understand`,
    `image_edit`, plus `mcp` / `rag` behind matching features) and depends on
    `agentflow-nodes` for the shared `common` / `error` modules.

  The workflow YAML `type:` strings are unchanged: the dispatch table in
  `agentflow-config::executor::factory` now imports tool nodes from
  `agentflow-nodes` and capability nodes from `agentflow-nodes-ai`. Consumers
  updated accordingly — `agentflow-cli` forwards `mcp` / `rag` to
  `agentflow-config`, and `agentflow-worker` keeps the tool tier while pulling
  `agentflow-nodes-ai` (with `mcp`) only for the `llm` / `mcp` payloads it
  dispatches. With the capability deps removed from the tool tier, the
  `nodes → llm`, `nodes → mcp`, and `nodes → rag` latent edges in
  `cargo xtask check-arch` are resolved and pruned from `ARCH_LATENT_EDGES`.

- **`agents → core` edge burned — the P-A contract-kernel allowlist is now EMPTY
  (0 tracked architectural violations).** The last tracked runtime-isolation
  edge is gone: `agentflow-agents` no longer depends on the `agentflow-core`
  executor. A new `FlowRunner` trait in `agentflow-graph` abstracts "execute a
  `Flow`, return the state pool"; `agentflow-core::CoreFlowRunner` is the
  executor-backed implementation. The agents that run embedded flows
  (`WorkflowTool`, `DynamicWorkflowAgent`) now take an injected
  `Arc<dyn FlowRunner>` and depend on the graph IR + the runner contract +
  `agentflow-async-util` instead of the core executor; the surface (CLI /
  examples) injects a `CoreFlowRunner`. `agentflow-core` stays an `agents`
  dev-dependency for the in-crate execution tests. With this, every
  runtime/surface-isolation edge the P-A track set out to burn —
  `agents → core`, `harness → agents` (P-A2.1), `server → cli` (P-A2.4),
  `worker → server` (P-A2.3) — is paid down, and `cargo xtask check-arch`
  reports **0 tracked** with an empty `ARCH_ALLOWLIST`.

- **P-A3 type-hardening (partial: 3.5–3.7).** Three of the five P-A3 items
  delivered:
  - **P-A3.6** — `agentflow harness chat`'s `/model` and `/skill` switch now
    commit on success: a failed switch (bad model id, unloadable skill) keeps
    the prior model + skill instead of leaving a dirty half-switched state.
  - **P-A3.5** — the duplicated char-bounded `truncate` in the trace `tui` /
    `replay` views is consolidated into one audited `format::truncate_chars`
    (the single UTF-8-safe truncation path; counts/slices by `char`, never
    panics on a multi-byte boundary).
  - **P-A3.7** — the contract error enums (`AgentFlowError`,
    `AgentRuntimeError`, `HarnessError`, `SchedulerError`) are now
    `#[non_exhaustive]` so error variants can be added without a breaking change
    for external consumers (one `_` arm added). Deliberately *not* applied to
    the event / stop-reason enums (would force ~600 in-workspace matches to add
    `_`, trading away internal exhaustiveness checking). P-A3.4 (seq write-order)
    and P-A3.3 (session type-state) are deferred — the seq value is already
    atomic/correct, and the `LoopSession` trait object can't carry compile-time
    state.

- **`agentflow-worker-proto` — shared worker control-plane contract; `worker →
  server` edge burned** (P-A2.3). The worker protocol surface moved out of
  `agentflow-server` into a new `agentflow-worker-proto` crate: the
  `WorkerProtocol` trait + wire types (`WorkerTask` / `WorkerTaskResult` /
  `WorkerHeartbeat` / `WorkerId` / `WorkerCapabilities` / `ClaimHints` /
  `WorkerTraceEvent` / `WorkerTransport`), `SchedulerError`,
  `InMemoryWorkerProtocol`, `NodeExecutionPayload`, the gRPC client
  (`GrpcWorkerProtocol`) + the proto↔domain conversions + traceparent helpers,
  and the `worker.proto` codegen (`build.rs` + `tonic-build`, generating the
  `pb` messages). `agentflow-worker` now depends on this shared contract instead
  of the gateway crate; `agentflow-server` keeps the control plane
  (`WorkerControlPlane`, the distributed scheduler) and the gRPC *server*
  (`WorkerControlServer` + the `WorkerControl` trait), importing the contract +
  `pb` + conversions from `agentflow-worker-proto` and re-exporting them under
  the original `scheduler::*` paths (so the control plane, the server-side
  service, and all the gRPC round-trip tests are unchanged). `agentflow-server`
  stays a worker dev-dependency for the integration tests. With the last
  surface-isolation edge gone, `cargo xtask check-arch` now reports **1 tracked
  edge** — only `agents → core` (runtime-isolation, the final P-A repoint)
  remains. Also fixes a stale `build_report` doc comment flagged by
  `clippy::empty_line_after_doc_comments`.

- **`server → cli` dependency edge burned** (P-A2.4, step 2 — complete). The
  doctor diagnostics report builder (`build_report`, `DoctorProfile`,
  `DoctorReport` + the report model, `print_text_report`) moved from
  `agentflow-cli` into `agentflow_config::diagnostics`, so `agentflow-server`'s
  `/v1/diagnostics` route depends on the shared crate. The doctor command's
  `execute` (which couples to the CLI's JSON envelope) and the top-level
  `mcp.toml` probe (which reads the CLI's `McpConfigFile`) stay in the CLI;
  `build_report` now *receives* the top-level MCP probe result as a parameter
  (the server passes an empty one). With the last usage repointed,
  **`agentflow-server` no longer depends on `agentflow-cli` at all** — the
  `agentflow xtask check-arch` allowlist drops the `server → cli` entry (now 2
  tracked edges: `agents → core`, `worker → server`). The CLI re-exports the
  diagnostics surface under the original `doctor::*` paths, so the `doctor`
  command is unchanged.

- **`agentflow-config` — shared workflow-assembly crate** (P-A2.4, step 1). The
  YAML workflow config schema (`config::v2::{FlowDefinitionV2, NodeDefinitionV2}`,
  `config::schema`) and the executor that compiles it into an `agentflow-core`
  `Flow` (`executor::build_flow_from_yaml` + the node factories) moved out of
  `agentflow-cli` into a new shared crate, so `agentflow-server` assembles and
  schedules workflows by depending on the shared crate instead of the CLI binary
  crate. `agentflow-cli` re-exports both modules under their original
  `agentflow_cli::{config, executor}` paths (consumers unchanged); the server's
  `runs.rs` (`build_flow_from_yaml`) and `scheduler::distributed` (the V2 schema
  types) now import from `agentflow-config`. The crate mirrors the CLI's
  `plugin` / `rag` / `mcp` feature flags. This is the first of two steps toward
  burning the `server → cli` dependency edge; the remaining usage is the doctor
  diagnostics `build_report`, which moves in a follow-up (it needs a careful
  split because the doctor command's `execute` couples to the CLI's JSON
  envelope).

- **`race_with_limits` consolidation completed for the ReAct hot path**
  (P-A3.2 follow-up). The batch-dispatch racing sites — the concurrent
  `join_all` group and the serial per-call loop — now also delegate to
  `race_with_limits`, so the `ReActAgent` no longer hand-writes any
  timeout/cancellation `select!` matrix (all four sites: LLM call, single tool
  call, concurrent batch, serial batch). Four batch racing characterization
  tests (concurrent/serial × timeout/cancel) were added first and pass both
  before and after the repoint, proving it behaviour-preserving; the test
  `SleepingTool` gained an idempotency switch to drive both batch paths.

- **`async-util::race_with_limits` — shared timeout/cancellation racing
  combinator** (P-A3.2). A single `race_with_limits(fut, remaining, cancel) ->
  RaceOutcome::{Completed, TimedOut, Cancelled}` replaces the hand-written
  four-arm `(Option<Duration>, Option<CancelSignal>)` match + nested
  `tokio::select!` blocks that every call site copied. The `ReActAgent`'s
  LLM-call and tool-call sites now delegate to it, so the timeout- and
  cancel-handling branches (which the old matrix duplicated across the
  both/timeout-only/cancel-only arms) are written once per call site. Behaviour
  is unchanged — proven by the P-A3.1 racing characterization tests, which pass
  against the refactored code — plus six combinator unit tests. Re-exported as
  `agentflow_core::race_with_limits` / `RaceOutcome`. Scope note: the two
  single-call sites are repointed here; the batch-dispatch sites and the
  `agentflow-core` shutdown paths still carry their own `select!` blocks and are
  a follow-up (they need their own racing-path coverage first).

- **ReAct loop timeout/cancellation racing-path characterization tests**
  (P-A3.1). Pins the runtime behaviour when a wall-clock deadline or a
  cancellation token wins a race against an *in-flight* call — the four
  `select!` arms in `run_turn_llm_call` / `run_turn_tool_call` that P-A3.2 will
  consolidate into `async-util::race_with_limits`. Four deterministic tests
  (LLM-call timeout, LLM-call cancellation, tool-call timeout, tool-call
  cancellation) using a never-completing slow operation so outcomes don't depend
  on scheduler timing. Supporting change: the Mock LLM provider now honours an
  `AGENTFLOW_MOCK_DELAY_MS` env var, so env-driven tests (which build the mock
  through the model registry rather than the `with_delay` builder) can simulate
  a slow round-trip.
- **Harness governance contracts moved into the kernel** (P-A1.1 step 2/2). The
  Harness wire/protocol surface — `HarnessEvent` (+ all payload types),
  `ApprovalRequest` / `ApprovalDecision` / `ApprovalProvider` (+ risk / scope /
  outcome enums), `PreToolHook` / `PostToolHook`, the `HarnessEventSink` trait,
  `ContextProvider` (+ `HarnessContext` / `HarnessProfile` / `HarnessRuntimeKind`
  / `ContextItem` / `ContextPriority`), and the shared `HarnessError` — moved out
  of `agentflow-harness` into a new `agentflow-agent-spi::harness` module, so the
  operations crates can depend on the contract in the kernel (L0) rather than the
  `agentflow-harness` runtime (L3). Faithful strangler-fig: the harness `error` /
  `approval` / `context` / `hooks` / `event` modules became `pub use` re-export
  shims and `persistence` keeps the concrete sinks (`JsonlEventSink` /
  `StdoutEventSink` / `InMemoryEventSink` / `SinkChain`) while re-exporting the
  trait — so every consumer (server / cli) compiles unchanged. `agentflow-agent-spi`
  gained **no new dependencies**. Redaction (`params_summary` →
  `agentflow-tracing`) intentionally stays in `agentflow-harness` (the contract
  types hold already-redacted strings), so this step does not yet burn the
  `harness → tracing` edge. The `Capability` / `Lowered` traits (RFC §2) are split
  out to land with their consumer in P-A4.3.

- **Contract-kernel architecture track (`P-A`) — guardrails landed.** The
  contract-kernel design (`docs/RFC_CRATE_ARCHITECTURE.md`) is validated by a
  dependency-graph-grounded, per-edge architecture-lens evaluation
  (`docs/ARCHITECTURE_EVALUATION_2026-06-20.md`; verdict: direction confirmed,
  six refinements R1–R6). `cargo xtask check-arch` now runs in the Quality CI
  workflow as a required `release-gate` job (P-A0.3), and additionally reports
  the full **latent target-state edge map** — the 16-edge repoint checklist for
  the kernel migration, code-tracked with a self-maintaining staleness guard so
  it cannot rot (P-A0.4). No shipped-crate behavior change; this is internal
  architecture tooling + docs.

- **`agentflow-value` contract leaf crate** (P-A1.5, first kernel extraction).
  `FlowValue` + its serde conversions moved out of `agentflow-core` into a new
  zero-internal-dependency `agentflow-value` crate. `agentflow-core` re-exports
  it as `agentflow_core::value` / `agentflow_core::FlowValue`, so every existing
  consumer compiles unchanged. This is the universal kernel leaf and a
  prerequisite of the upcoming `agentflow-graph` split (evaluation R1).
- **`agentflow-graph` IR crate — leaf extraction** (P-A1.3 step 1/2). The pure
  IR leaf modules — `error` (`AgentFlowError`), `async_node` (`AsyncNode`),
  `node`, `expr` — moved out of `agentflow-core` into the new `agentflow-graph`
  crate (depends only on `agentflow-value`), beginning the IR ≠ executor split
  (RFC §5). `agentflow-core` re-exports them under their original paths; all
  consumers compile unchanged.
- **`Flow` IR moved to `agentflow-graph`; executor is now `FlowExt`** (P-A1.3
  step 2/2, completes the IR ≠ executor split). `Flow` / `GraphNode` / `NodeType`
  (+ pure builders and accessors) now live in `agentflow-graph`; the run / resume
  / scheduling logic stays in `agentflow-core` behind a new **`FlowExt`** trait
  (`use agentflow_core::FlowExt;` to run a flow) backed by an internal
  `FlowExecutor`. The `Flow` struct holds checkpoint *config* (data) rather than
  a live manager. `agentflow_core::{Flow, GraphNode, NodeType, FlowExt}` are
  re-exported, so the only caller-visible change is needing `FlowExt` in scope to
  call `flow.run()` / `with_checkpointing()` / etc. A runtime can now construct a
  `Flow` by depending on `agentflow-graph` alone — the dynamic-workflow
  prerequisite.

- **`agentflow-store-spi` storage-contract crate** (P-A1.2). The storage
  *contracts* — `MemoryStore`, `Message` / `Role` / `TokenCounter`, and the shared
  `MemoryError` — moved out of `agentflow-memory` into a new `agentflow-store-spi`
  crate. The concrete stores (`SessionMemory`, `SqliteMemory`, `SemanticMemory`, …)
  stay in `agentflow-memory`, which re-exports everything under its original
  `agentflow_memory::*` paths — no consumer changes. This gives `Message` a
  contract home so the upcoming `agentflow-agent-spi` can depend on it without
  depending on the `memory` implementation crate. (The `EmbeddingProvider`
  contract — evaluation R6 — is a follow-up pending error-surface unification.)

- **`agentflow-agent-spi` agent-runtime contract crate** (P-A1.1, runtime
  contracts). The `AgentRuntime` trait + the structured `AgentEvent` / `AgentStep`
  / `AgentContext` / `RuntimeLimits` / cancellation + event/memory hook contracts
  moved out of `agentflow-agents` into a new `agentflow-agent-spi` crate (depends
  on `store-spi` for `Message` + `tools`). The concrete runtimes (`ReActAgent`,
  `PlanExecuteAgent`, supervisors) stay in `agentflow-agents`, which re-exports
  everything under its original `agentflow_agents::runtime` path — no consumer
  changes. This lets `agentflow-harness` later govern a runtime via the contract
  instead of the `agents` impl crate (the P-A2.1 target). (The harness-side event
  / approval / hook contracts and the `Capability` / `Lowered` traits are a
  follow-up sub-step; `agent-spi → llm` for `LlmTraceContext` is transitional,
  pending a trace-context contract per evaluation R6.)

- **`agentflow-async-util` reliability crate** (P-A1.4, completes the kernel
  crates). The retry (`RetryPolicy`/`RetryContext`/`RetryStrategy`) and timeout
  combinators moved out of `agentflow-core` into a new `agentflow-async-util`
  crate, so the executor and the agent loop can share one implementation instead
  of duplicating it (the de-dup against `agentflow-agents` is P-A3.2).
  `agentflow-core` re-exports both under their original `agentflow_core::{retry,
  timeout}` paths — no consumer changes; `retry_executor` stays in core. With
  this, all five new kernel crates exist (`value`, `graph`, `store-spi`,
  `agent-spi`, `async-util`).

- **Dynamic-workflow vertical-slice spike** (P-A1.6) — the payoff of the
  contract-kernel migration. New `agentflow-agents/examples/dynamic_workflow_spike.rs`
  demonstrates a runtime *generating* a `Flow` from the `agentflow-graph` IR
  (shape decided at runtime, not compile time) and `agentflow-core` executing it
  via `FlowExt` — the two meeting only through the `graph` contract. Wired into
  the `examples-smoke` CI gate so it stays green. This proves the kernel can
  carry dynamic workflows; P-A4 productionizes it (`PlanExecuteAgent` emits a
  real `Flow`).

- **`harness` no longer depends on the `agents` impl crate** (P-A2.1, first
  architecture violation burned down). The turn-driven contracts (`TurnDrivenRuntime`
  / `LoopSession` / `TurnProgress`) moved from `agentflow-agents` into
  `agentflow-agent-spi`; `agentflow-harness` now depends on the `agent-spi`
  contract and the concrete runtime (`ReActAgent`) is injected (the smoke test
  keeps `agents` as a dev-dependency). `check-arch`'s tracked-violation allowlist
  shrank 4 -> 3.

- **Dynamic workflow productized — declarative plan -> `Flow` compiler** (P-A4.4).
  New `agentflow_agents::dynamic::compile_plan_to_flow` turns a `WorkflowPlan`
  (the JSON an LLM emits: steps of `{id, tool, params, depends_on}`) into an
  executable `Flow` of real tool calls — `depends_on` becomes graph dependencies,
  so independent steps run concurrently and dependents receive their inputs. This
  is the productized form of the P-A1.6 spike: the agent collapses its intent into
  one deterministic, replayable artifact instead of looping step-by-step. New
  `dynamic_workflow_plan` example (JSON plan -> parallel execution), wired into the
  examples-smoke CI gate; unit tests cover the diamond DAG + validation.

- **`DynamicWorkflowAgent` — LLM plans, the engine executes** (P-A4.4). An agent
  that makes one up-front LLM call to generate a `WorkflowPlan` (given a goal +
  the available tools), then compiles it to a `Flow` and runs it concurrently —
  closing the dynamic-workflow loop end to end. `plan()` returns the LLM-produced
  plan; `run()` plans + compiles + executes. Tested against a mock model that
  emits a parallel plan.

- **`agentflow workflow dynamic` — governed dynamic-workflow CLI surface**
  (P-A4.5). Exposes the dynamic-workflow paradigm on the CLI: an LLM authors a
  plan for `--goal`, which compiles to a `Flow` and executes concurrently. Because
  the plan is LLM-authored then executed, tool access is governed — the built-in
  tools (`FileTool` + `HttpTool`; shell is never registered) carry a restrictive
  `SandboxPolicy`, so file paths and HTTP domains must be granted explicitly via
  `--allow-path` / `--allow-domain`; `--dry-run` prints the plan without running
  any tool; and `--approve cli|auto-allow|auto-deny` routes every call through the
  Harness `wrap_registry` approval pipeline (the planner and compiler share the
  same wrapped `Arc<ToolRegistry>`, so governance is not bypassed). Unit tests
  cover the policy/approval/render helpers; integration tests assert dry-run does
  not execute, an ungranted path is sandbox-denied (non-zero exit), and a granted
  path writes successfully.

### Changed

- `RoadMap.md` reframed around the four execution paradigms (static DAG / native
  loop / harness / dynamic workflow) converging on the contract kernel, replacing
  the pre-kernel "two first-class paths" framing.

## [v1.0.0-rc.1] — 2026-05-21

The R1 → R4 reflection arc (see `docs/L1_L3_REFLECTION_R*.md`) drove
this set of changes. Most landed via a multi-session dogfooding loop:
build an application that exercises a platform surface, capture
findings as it goes, close the findings, repeat. **31 dogfooding
findings closed across 4 reflection cycles** (every
agentflow-internal item in the action queue), with no regressions
across the 200+ tests touched.

### Added

#### Security

- **`AgeEncryptedPreferenceStore` — encryption-at-rest for the
  preference memory layer** (P10.7.2). New
  `agentflow-memory::AgeEncryptedPreferenceStore<S: PreferenceStore>`
  wraps any preference store impl (defaults to `SqlitePreferenceStore`)
  and transparently age-encrypts every `value` payload on write +
  decrypts on read using a single-user X25519 identity. Keys (tenant
  / user / key string) stay plaintext on disk — only the value is
  opaque to anyone without the identity file. KMS decision: `age` only,
  no cloud KMS dependency (cloud / envelope re-keying / multi-user
  deferred to v2 per `docs/ROADMAP_v2.md` Theme B). On-disk shape is
  `"age:v1:<base64-age-ciphertext>"`; the marker prefix lets readers
  reject plaintext-bleed-through from a stale
  `SqlitePreferenceStore` writer. Identity-file helpers
  (`generate_identity_file` + `load_identity_file`) refuse to
  overwrite existing keys and chmod 0600 on Unix. 12 hermetic tests
  cover round-trips, version semantics through the wrapper,
  decrypt-with-wrong-identity rejection, plaintext-bleed-through
  rejection, and identity-file lifecycle.

- **Per-tool `os_sandbox` override on `[[tools]]` blocks** (P10.4.1).
  Skill manifests can now opt individual `shell` / `script` tools in
  or out of the OS-level sandbox independently of the manifest-level
  `[security] os_sandbox` default. The new `os_sandbox: Option<bool>`
  field on `ToolConfig` is fully optional; `None` falls back to the
  manifest-level value (so every pre-P10.4.1 manifest parses
  identically with unchanged behaviour). `Some(true)` opts the tool
  in even when the manifest default is off; `Some(false)` opts it
  out even when the manifest default is on. Only the two subprocess-
  spawning built-in tools honour the override; `file` / `http`
  ignore it. `agentflow skill inspect --explain-permissions` now
  prints a per-tool resolution table under `Sandbox profile`
  showing each sandboxable tool's resolved value + its source
  (`per-tool override` vs `inherited`). 6 new builder unit tests
  + 1 serde round-trip pin the resolution rules and schema.

#### CLI ops (cont.)

- **RAG eval `--chunk-size <N>` per-chunk latency dimension**
  (P10.6.3). `agentflow rag eval` was chunking-agnostic — corpus
  documents went straight into the retriever index. Now passing
  `--chunk-size N` re-chunks every corpus doc with a `FixedSizeChunker
  (N, overlap=0)` before indexing; retrieved chunk ids are remapped
  back to source doc ids (with dedupe within the top-K window) so
  `Recall@K`/`MRR`/`nDCG@K` stay comparable across chunked and
  un-chunked runs. The latency block reflects the chunked index, so
  capturing one baseline per chunk strategy spots chunking-side
  regressions. `EvalReport.chunk_size: Option<usize>` is additive
  (omitted from JSON when un-chunked, preserving pre-P10.6.3 baseline
  shape). When `--compare-baseline` is supplied and chunk sizes
  differ, the CLI prints a stderr warning but doesn't fail — cross-
  chunk comparison is still useful for "did the chunking change hurt
  recall?" investigations. 12 hermetic tests cover the new library
  surface (`chunk_dataset`, `evaluate_with_remapping`) plus the CLI
  flag. `docs/RAG_EVAL.md` gains a "Per-chunk-size latency profile"
  section with the three-baselines capture recipe.

- **`agentflow marketplace search --format text|json|json-envelope`**
  (P10.9.2). The `search` subcommand was text-only; now also speaks
  the canonical JSON shapes so operators can script against it. Bare
  `--format json` emits the structured payload (`registry`, `query`,
  `package_type_filter`, `manifest` block, `entries[]`,
  `matched_count`); `--format json-envelope` wraps the same body in
  the `agentflow.cli/1` envelope. Empty matches produce `entries: []`
  + `matched_count: 0` (never null) so consumers don't special-case
  the no-result path. Web UI marketplace tab from the original TODO
  is deferred (out of scope per the P10.17.1 debugger-focused UI
  commitment); a `P10.9.2-FU1` carry-forward tracks it until concrete
  demand surfaces.
- **`agentflow agent replay <current> --diff <baseline>`** (P10.8.1).
  New `agent` top-level subcommand namespace + `replay` file-to-file
  diff. Compares two ReAct `AgentEvent` JSONL streams along three
  operator-facing dimensions: tool-call order, terminal stop-reason,
  per-step LLM token usage. Step-kind / tool-name / stop-reason
  divergence fails the gate; token deltas are soft variances by
  default (`--strict-tokens` promotes them). Output formats
  `text` / `stream-json` / `json-envelope` (canonical
  `agentflow.cli/1` shape). Pure file-to-file — the user produces
  both JSONL files however they like (no `agent run` wrapper yet;
  follow-up territory once one exists). 14 pure unit tests cover
  the comparator + JSONL parser; 6 hermetic CLI tests cover the
  binary end-to-end. Separate namespace from `agentflow harness
  replay` because the wire shapes differ (`AgentEvent` is
  fine-grained per-step / per-LLM-call; `HarnessEvent` is the
  approval-gated session envelope).

#### Release engineering

- **GitHub Release workflow + multi-arch GHCR image push** (P10.0.4).
  New `.github/workflows/release.yml` fires on any `v*.*.*` tag push
  (or manual `workflow_dispatch` with `dry_run: false`) and runs three
  jobs: a 4-target matrix building `agentflow-<target>.tar.gz` for
  linux x86_64 + linux aarch64 + macOS Intel + macOS Apple Silicon;
  a `docker buildx` multi-arch (linux/amd64 + linux/arm64) build of
  the root `Dockerfile` pushed to `ghcr.io/<owner>/agentflow-server`
  with `:<tag>` always and `:latest` only on stable tags; and a
  publish step that aggregates tarballs + a combined `SHA256SUMS.txt`
  and creates the GitHub Release with auto-generated commit-list body.
  `prerelease` flag auto-derives from a `-` in the tag (RC / alpha /
  beta tags don't promote `latest`). Companion
  `scripts/release_dry_run/run.sh` rehearses the build legs locally
  without pushing; uses `cargo metadata` to resolve the target dir
  through the AgentFlow `~/.cargo/config.toml::build.target-dir`
  redirect. `docs/RELEASE_CHECKLIST.md` §10 documents the trigger,
  first-push prerequisites (GHCR public-visibility flip), and
  manual-dispatch rehearsal mode.

- **Production deployment dress-rehearsal reproducible via Apple
  container** (P10.0.1). New `scripts/production_dress_rehearsal/`
  ships a two-stage Containerfile (rust builder + both `agentflow`
  + `agentflow-server` binaries), a step-walking
  `inside_container.sh`, a host-side driver `run.sh`, and checked-in
  `last-run.{log,json}` fixtures. Walks all 6 steps of the
  `docs/RELEASE_NOTES_v1.0.0-rc.1.md::Production Deployment
  Checklist` plus the 4 acceptance gates inside a fresh
  `ubuntu:24.04` container. Canonical outcome on a clean apple-
  aarch64 box: **doctor exit code 0, status `ok`**, 5 of 6 steps
  PASS, step 6 / AG3 / AG4 SKIP (host-side, requires Docker +
  Postgres sidecar — documented host-side commands in the README).
  Discovered + filed two follow-ups: (a) `serve --check` requires a
  real Postgres connection despite a stale source comment claiming
  otherwise; (b) `agentflow serve` shells out to a separate
  `agentflow-server` binary not bundled with the doctor-smoke
  image. Paired with P10.0.5 in the pre-tag checklist.

- **Fresh-VM `agentflow doctor` smoke reproducible via Apple container**
  (P10.0.5). New `scripts/doctor_smoke/` directory ships a multi-stage
  `Containerfile` (rust:1-slim-bookworm builder → ubuntu:24.04 smoke
  image with the binary copied in), a driver `run.sh`, a checked-in
  `last-run.json` fixture, and a README. Drives Apple's `container`
  CLI by default; `DOCTOR_SMOKE_RUNTIME=docker` switches to Docker.
  Reproduces step 5 of `docs/RELEASE_NOTES_v1.0.0-rc.1.md`'s pre-cut
  checklist. The canonical fresh-VM outcome is `status: fail` /
  exit code 2 — production-profile treats missing `~/.agentflow/*`
  dirs as fail (vs. warning on dev/local), so a zero-state Ubuntu
  produces fail by design. The fixture + README document this
  expected outcome so future operators don't mis-interpret it as a
  binary crash.

#### Workspace tooling

- **`cargo xtask refresh-live-models`** (P10.3.4 / P10.18.1). New
  subcommand that pings each of the 9 live-test providers' `/models`
  endpoints (OpenAI / Anthropic / Google / Moonshot / StepFun / GLM /
  DashScope / DeepSeek / MiniMax), verifies the hard-coded text-model
  default in `agentflow-llm/tests/provider_consistency_live.rs` is
  still listed, and prints suggestions ranked by shared-prefix when a
  default goes missing. Loads `~/.agentflow/.env` locally (silently
  no-op on CI where keys come from the workflow's env block); treats
  existing-but-empty env vars as unset so an exported-but-empty key
  doesn't block a valid `.env` value. Validate-only by design — the
  operator copies suggested replacements into the test source rather
  than auto-editing (the defaults carry inline rationale comments
  that an automated rewrite would clobber). First real-world run
  surfaced two findings: `glm-4.5-flash` actually deprecated (rotate
  to `glm-4.5` / `glm-4.6`); `claude-haiku-4-5` is a rolling alias
  that doesn't appear in `/v1/models` but resolves in real API calls
  (Anthropic-specific false positive of the "is the id in the list"
  check — documented).

- **FlowValue + checkpoint hot-path benches in `agentflow-core`**
  (P10.1.1). New `agentflow-core/benches/hot_paths.rs` covers the two
  hot paths the existing scheduler bench misses: `serde_json::from_value
  ::<FlowValue>` over the P0.2 tagged-enum wire shape (5 variants ×
  sizes) and the full `CheckpointManager` save+load round-trip plus
  decode-only isolation (10 / 100 node state pools, 9 bench points
  total). Wired into the bench CI workflow + `apple-m2-max.json`
  baseline; `cargo xtask bench-gate --allow-missing` now compares 19
  rows (10 from P10.2.1 + 9 here) at the default 1.25× threshold. The
  TODO's "look for P3.3 envelope regressions" half found no target —
  P3.3 was the CLI envelope wave with zero touches to `agentflow-core::
  {value,checkpoint,flow}`. The new baseline locks in post-N8 numbers
  so any future regression in these paths trips the gate.

- **Per-node criterion benches in `agentflow-nodes`** (P10.2.1). New
  `agentflow-nodes/benches/node_latency.rs` covers the pure-compute
  built-ins — Tera template rendering (3 sizes), conditional
  dispatch (3 variants), and tokio fs read/write (2 payload sizes,
  10 bench points total). Wired into the existing `bench-gate` CI
  workflow + the `apple-m2-max.json` baseline so a template-render
  regression now trips the same gate `scheduler` / `provider_hop` /
  `retrieval` / `event_write` already feed. Nodes that depend on
  external services (LLM, HTTP, MCP, RAG, image, audio, etc.) are
  intentionally out of scope — their latency is dominated by
  round-trip, not AgentFlow code.

- **`cargo xtask test-gate`** (P10.19.2). New sibling to the existing
  `bench-gate` (criterion microbench gate). Runs
  `cargo test -p <crate> --all-targets --quiet` per workspace member,
  measures per-crate wall-clock, compares against a host-specific
  baseline JSON, fails on a configurable threshold (default **1.5×**,
  looser than bench-gate's 1.25× because `cargo test` is meaningfully
  noisier than criterion). Three modes — compare (default), `--update`
  (refresh the baseline), and `--input <path>` (pure-data comparison
  for CI two-stage flows). Baselines live at
  `benches/baselines/test-timings/<host>.json` with a README pinning
  schema + capture flow. 16 hermetic unit tests cover the pure
  comparator, the test-count parser, crate selection, and the
  argument validation paths. Not wired into CI yet — landing the
  xtask first lets contributors confirm the heuristic against real
  PRs locally, same staged rollout as bench-gate.

#### Observability

- **`agentflow_state_size_bytes{run_id}` Prometheus gauge wired
  end-to-end** (P10.14.2-FU6). The last deferred series from the
  "Memory & workflow state" Grafana panel now renders live: the
  dashboard matrix is 14/14 ✅. Surface lands in two pieces:
  - `agentflow-core`: new public `state_size` module with the
    `StateSizeObserver` trait, `estimated_state_pool_bytes`
    helper, and `FlowValue::estimated_size_bytes` method. `Flow`
    gains a `with_state_size_observer(Arc<dyn StateSizeObserver>)`
    builder + a private hook that fires after every node insert
    in both serial and concurrent execution paths.
  - `agentflow-server`: new public `live_state_registry` module
    with the process-local `LiveStateRegistry`. `AppState` carries
    one (cloned cheaply into every `RunContext`); the DAG
    executor attaches a per-run observer at submit time and
    deregisters on terminal transitions so gauge cardinality is
    bounded to currently-running runs. The `/metrics`
    `refresh_scrape_time_gauges` helper iterates the snapshot
    and emits one labelled gauge sample per active run.
  - 15 hermetic tests across 3 layers (`agentflow-core` lib +
    integration, `agentflow-server` lib + integration). No
    Postgres or real workflow needed at test time — the
    integration test seeds the registry directly through the
    same observer path the executor uses.

#### CLI ops

- **`agentflow backup --output <path>`** (P10.15.1). Orchestrates
  `pg_dump --format=custom` + `tar -czf` of the five filesystem
  state surfaces (`run_dir`, `trace_dir`, marketplace cache,
  skills, plugins) into a single bundle directory with a
  versioned `manifest.json`. Closes the operator loop that
  `docs/SERVER_BACKUP_RESTORE.md` documents — that doc described
  *which* state surfaces must be backed up; this command actually
  does it in one invocation instead of leaving the operator to
  run six commands by hand and reason about the order.
  - Flags: `--output` (required), `--database-url`, `--include`
    (repeatable; aliases like `runs` → `run_dir`, `database` →
    `db` accepted), `--dry-run`, `--force`,
    `--format text|json|json-envelope` (canonical `agentflow.cli/1`).
  - Manifest schema discriminator `agentflow.backup/1` is the
    wire-shape promise a future `agentflow restore --input <path>`
    will consume. Restore itself stays out of this TODO's scope.
  - Tool requirements (`pg_dump`, `tar`) are PATH-probed up front;
    "tool not found" surfaces as a `failed` step with a
    package-manager hint, not an unhelpful panic.
  - A missing source directory is `skipped` (not `failed`) — the
    common case where the operator only opted into a subset of
    state surfaces stays out of the failure path.
  - 12 hermetic unit tests in `commands::backup::tests`:
    include-name parsing + aliases, artifact-name layout
    discipline, URL password redaction (3 variants), dir-prep
    refuse/force/create/dry-run paths, end-to-end dry-run
    behavior (all 6 includes), explicit-include subset, and
    DB-step skip-without-`DATABASE_URL`. Postgres / tar are
    never invoked in the test suite, so this runs hermetically
    in CI.

#### LLM provider layer

- **Memory layer routed through real tokenizer in agent
  production paths** (P10.3.3-FU1). New
  `agentflow_memory::TokenCounter` trait + `HeuristicCounter`
  default + five `Message::*_with_counter` constructors
  preserve every existing `Message::new` callsite as the
  heuristic path while adding a parallel precise path for
  callers that know their target model id.
  `agentflow-agents::token_counter_adapter` bridges
  `agentflow_llm::TokenCounter` (BPE from P10.3.3) to the
  memory crate's local trait without creating a new
  cross-crate dependency. `ReActAgent` and
  `PlanExecuteAgent` gained a `message_counter` field
  rebuilt from `context.model` in `apply_context`; all 15
  production `Message::user/assistant/system/tool_result`
  callsites inside the two agents now route through
  `*_with_counter(&self.session_id, content,
  &*self.message_counter)`. Direct consequence:
  `apply_memory_prompt_budget` compacts against precise BPE
  counts for OpenAI family + cl100k_base-compatible vendors,
  ending the CJK over-estimation (3-5×) and code
  under-estimation the heuristic produced. 9 new hermetic
  tests; ~50 test-site callers stay on the heuristic
  intentionally (they're testing message-handling logic, not
  tokenization accuracy).

- **Provider-specific tokenizer trait (`TokenCounter`)** (P10.3.3
  foundation slice). New `agentflow_llm::tokenizer` module ships
  `TokenCounter` trait, `TiktokenCounter` (BPE via `tiktoken-rs`
  — cl100k_base / o200k_base / p50k_base / r50k_base),
  `HeuristicCounter` (preserves the workspace's existing
  `len / 4` fallback), and `counter_for_model(model_id)` +
  `count_tokens_for_model(model_id, text)` factories. Closes the
  precision gap for OpenAI-family pre-call token budgeting: the
  heuristic over-estimates CJK text by 3-5× and code by
  4× — the BPE counter is exact for `gpt-3.5-*`, `gpt-4*`,
  `gpt-4o*`, `o1*`, `o3*`, `gpt-5*`, and within ~5-15% for the
  OpenAI-compat vendors (Moonshot, DeepSeek, GLM, DashScope
  Qwen, MiniMax, StepFun) that ship documented BPE-compatible
  tokenizers. Anthropic / Google fall back to the heuristic;
  their post-call responses still report exact counts so cost
  tracking stays accurate. 13 hermetic unit tests cover BPE
  counts against known inputs, model-name routing for every
  documented family, case-insensitivity, and the error path.
  **Follow-up `P10.3.3-FU1`** is open to wire
  `count_tokens_for_model` into `agentflow-memory::Message::new`
  (the existing heuristic site) — the foundation landed first
  so the accuracy improvement is visible without rippling
  through 50+ test sites. Adds `tiktoken-rs = "0.6"` dep to
  `agentflow-llm`.

#### Database layer

- **DB read-replica support** (P10.15.2). `Database` gains
  optional `read_pool: Option<PgPool>` + helper
  `read_pool()` that falls back to the primary when no
  replica is configured. New constructors
  `Database::connect_with_replica` and
  `Database::connect_and_migrate_with_replica` take the
  primary URL + replica URL + per-pool connection caps;
  migrations always run against the primary so DDL never
  races the replica. Every Pg*Repo carries both `pool`
  (write) and `read_pool` (read); 12 `SELECT`-shaped sites
  in the repo layer route to `read_pool`, while every
  `INSERT...RETURNING` / `UPDATE` / `DELETE` stays on
  `pool`. New `Repositories::from_pools(write, read)` +
  `Repositories::from_database(&db)` constructors;
  `from_pool(pool)` stays as a backwards-compat shim.
  `agentflow serve` gains `--database-read-url <URL>`
  (default env `AGENTFLOW_DATABASE_READ_URL`) forwarded
  through to the server binary. Single-node deployments
  are unaffected — `read_pool: None` falls through to the
  primary and the existing test suite passes unchanged.
  Documented in `docs/DEPLOYMENT.md` "Read-replica routing
  (P10.15.2)" with the replication-lag caveat called out.
  6 hermetic unit tests (lazy pools — no live Postgres
  required).

#### Operator dashboards

- **Prometheus `/metrics` — process / state inspectors**
  (P10.14.2-FU5). Three new scrape-time gauges
  plug into the `refresh_scrape_time_gauges` helper
  established by FU4:
  `agentflow_health_status{component}` (emits `system=1`
  unconditionally and `database=1|0` from a `SELECT 1`
  probe), `agentflow_memory_usage_bytes` (reads
  `/proc/self/statm` on Linux, `0` fallback elsewhere),
  and `agentflow_workflow_runs_active{tenant}` (single
  `GROUP BY` query against the read pool). All inherit the
  fail-soft contract from FU4. 13 of 14 dashboard series
  are now live; the remaining `state_size_bytes{run_id}`
  is deferred to `P10.14.2-FU6` because it requires
  architectural access to live `Flow::context.state_pool`
  contents the server doesn't expose, and the available
  proxies (events table size, artifacts size) would
  mislead operators. 5 new tests (3 unit + 2 integration,
  including a contract-pin that
  `agentflow_health_status{component="system"} 1` renders
  on every scrape).

- **Prometheus `/metrics` — harness session gauges**
  (P10.14.2-FU4). New scrape-time pattern: the `/metrics`
  handler now runs `refresh_scrape_time_gauges(&state)`
  before rendering. Two gauges source from this path:
  `agentflow_harness_sessions_active{status}` (computed via
  `SELECT status, COUNT(*) FROM harness_sessions GROUP BY
  status` against the read pool — reuse of P10.15.2's
  optional replica) and `agentflow_harness_approvals_pending`
  (sourced from `PendingApprovalRegistry::pending_count()`,
  an in-process mutex read). All four known status buckets
  emit every scrape so a bucket dropping to 0 renders as 0
  instead of leaving a stale value. **Fail-soft contract:**
  a DB-query failure inside the refresh is logged and
  swallowed; the scrape still returns 200 and the remaining
  metrics render. Pinned by a dedicated integration test.
  4 new tests (2 unit + 2 integration including the
  unreachable-DB invariant). 10 of the 14 dashboard series
  are now live.

- **Prometheus `/metrics` — worker fleet gauges**
  (P10.14.2-FU3). `AuthenticatedControlPlane` now emits the
  two worker-fleet gauges from its three mutation sites:
  `admit()` sets `agentflow_workers_admitted` to the
  distinct-worker count after every successful admission;
  `claim_task` sets
  `agentflow_worker_tasks_inflight{worker_id}` to the
  post-increment value; `report_result` sets the same gauge
  after the decrement. Gauges are absolute (set-not-
  increment) so idempotent re-admissions don't double-count.
  New `metrics::observe_workers_admitted` /
  `metrics::observe_worker_tasks_inflight` helpers + matching
  constants. 2 new unit tests + 1 end-to-end integration test
  exercising the real admission code path against an
  in-memory protocol. 8 of the 14 dashboard series are now
  live (6 from FU1+FU2 + 2 from this slice).

- **Prometheus `/metrics` — cleanup sweep counters**
  (P10.14.2-FU2). `cleanup_expired` now fires
  `agentflow_cleanup_runs_deleted_total`,
  `agentflow_cleanup_events_deleted_total`, and
  `agentflow_cleanup_artifacts_deleted_total` at the end of
  every sweep via a new
  `metrics::observe_cleanup_sweep(dry_run, runs, events,
  artifacts)` helper. Dry-run sweeps skip the increment (the
  Grafana panel is about actual reaping, not previews). The
  six-series matrix in `dashboards/README.md` updates from 3
  ✅ to 6 ✅; the three retention bars in the Grafana
  overview now render real data. 1 new unit test + 1 new
  integration test exercising the `/metrics` route after a
  synthetic sweep observation.

- **Prometheus `/metrics` endpoint emission — slice 1**
  (P10.14.2-FU1). `agentflow-server` now exposes
  `GET /metrics` returning Prometheus text format. The
  endpoint bypasses auth (same convention as `/health`) so
  scrapers don't need a bearer token. Three workflow-event-
  derived series are live: `agentflow_workflow_completed_total
  {status}` (counter), `agentflow_workflow_duration_seconds`
  (histogram, buckets 0.1s … 10min), and
  `agentflow_nodes_failed_total{node_type}` (counter, labelled
  by `node_id` until a future event-payload extension splits
  node_type from node_id). The recorder is installed once
  during `serve::run` boot via a `OnceLock`-guarded
  `init_recorder()` so multi-`run()` callers don't panic.
  `WorkflowEventListener::on_event` fires the counter +
  histogram on each terminal event. Deps:
  `metrics = "0.23"` + `metrics-exporter-prometheus = "0.15"`
  added to `agentflow-server`. The four remaining series
  classes in the dashboard contract (retention sweep counters,
  worker fleet gauges, harness session gauges, scrape-time
  process inspectors) are deferred to follow-up TODOs
  `P10.14.2-FU2/FU3/FU4/FU5`; `dashboards/README.md`
  "Current emission status" carries the per-metric
  live/deferred matrix. 11 hermetic tests (5 unit + 6
  integration via `tests/metrics_endpoint.rs` exercising
  the actual Axum route, content-type, auth bypass, and
  every contracted metric name).

- **Checked-in Grafana dashboard template** (P10.14.2). New
  `dashboards/grafana/agentflow-overview.json` (9 panels:
  system health, active runs per tenant, workflow completions
  by status, p50/p95/p99 duration, node failures by node_type,
  worker fleet admitted + in-flight, memory + state size,
  retention sweep deletions, Harness Mode sessions +
  approvals) + `dashboards/README.md` operator playbook
  (import recipe, metric contract, conventions). The dashboard
  uses a `${DS_PROMETHEUS}` variable so it survives datasource
  renames during import, and tags with `agentflow / overview /
  operator` for discoverability. Imports cleanly into Grafana
  8+ (`schemaVersion: 38`). `docs/KUBERNETES_DEPLOYMENT.md`
  §Grafana Dashboard updated to link the JSON.
  - **Documented gap:** the in-core Prometheus metrics module
    was removed during the observability split, and
    `agentflow-server` doesn't expose `/metrics` today. The
    dashboard is forward-compatible — it will render the
    moment emission lands. Shipping the JSON now pins the
    operator-side metric-name contract so the emission code
    (tracked under follow-up `P10.14.2-FU1`) can be
    unit-tested against an external source of truth, and so
    operators have something to import on day one of
    `P10.14.2-FU1` closure.

#### Worker dispatch hints

- **gRPC wire-extension for capability + locality hints**
  (P10.16.2-FU1). Closes the follow-up opened during P10.16.2.
  `pb::WorkerTask` gained `node_type: string` (tag 6),
  `pb::ClaimTaskRequest` gained `accepted_node_types:
  repeated string` (tag 2) + `locality_run_id: string`
  (tag 3), and `pb::HeartbeatRequest` gained
  `accepted_node_types: repeated string` (tag 5). All four
  fields are wire-additive — pre-FU1 workers (which never set
  them) encode as empty values which the server decodes as "no
  hints / untagged task," preserving pre-P10.16.2 FIFO
  behavior exactly. Both `GrpcWorkerService` and
  `WorkerControlPlane`'s tonic adapter now route through
  `protocol.claim_task_with_hints`; `GrpcWorkerProtocol`
  (client side) gained an explicit `claim_task_with_hints`
  impl. `agentflow-worker::WorkerConfig` gained a
  `capabilities: WorkerCapabilities` knob +
  `with_capabilities` builder; `run_once` sends them on every
  heartbeat AND attaches them to claim hints, so distributed
  workers can declare which node types they accept and the
  queue scan skips work they can't run. 7 hermetic
  round-trip tests in `scheduler::grpc::hint_proto_tests`
  cover the wire-shape conversions, malformed locality UUID
  rejection, and the pre-FU1 backwards-compat invariants.
  `docs/DISTRIBUTED.md` "Wire-extension status" subsection
  marks FU1 closed with a wire-shape mapping table.

- **Capability + locality hints on worker claim** (P10.16.2
  foundation). `WorkerCapabilities { node_types }` advertises
  which task labels a worker accepts; `WorkerTask.node_type:
  Option<String>` tags tasks with their capability label;
  `ClaimHints { capabilities, locality_run_id }` is the optional
  per-claim payload. New trait method
  `WorkerProtocol::claim_task_with_hints(worker_id, hints)`
  with a default impl falling back to `claim_task` so the gRPC
  adapter compiles unchanged; `InMemoryWorkerProtocol` overrides
  to scan the queue in three passes (same-run + capability,
  capability anywhere, FIFO). The protocol also caches the
  most-recently-claimed `run_id` per worker so workers without
  an explicit locality hint still get warm-cache continuity.
  `WorkerHeartbeat` gains a `capabilities` field with a default
  empty value; `WorkerControlPlane::claim_task_with_hints` is
  the public entry point and still increments the run snapshot
  the same way bare `claim_task` does.
  - Wire-extension to gRPC stays as a tracked follow-up
    (`P10.16.2-FU1`); the trait surface is forward-compatible.
    Workers talking gRPC today effectively send "no hints" and
    get pre-P10.16.2 FIFO behavior — fully additive upgrade.
  - 9 hermetic unit tests cover capability default, restricted
    set with untagged-task fallback, capability filtering,
    locality preference, FIFO fallback when no locality match,
    cached-last-run bias, combined capability + locality, and
    the control-plane snapshot invariant.

#### Worker admission

- **Signed-JWT identity flavour for worker admission** (P10.16.1).
  New `agentflow-server::scheduler::jwt` module ships `JwtPolicy`
  (issuer / audience / key pool / leeway), HS256 + RS256
  `JwtVerificationKey`, and a strict claim validator
  (`iss`/`aud`/`sub`/`exp`/`nbf`). `WorkerAdmissionPolicy` gains
  `jwt: Option<JwtPolicy>` + `jwt_workers: HashSet<WorkerId>` so
  workers can opt into JWT auth alongside (or instead of) the
  existing PSK path. PSK takes precedence over JWT when a worker
  is misconfigured into both sets so there's no silent
  downgrade. The `aud` claim deserializer is tolerant of both
  the string and string-array forms per RFC §4.1.3. Key
  rotation works the same way as PSK: append a new
  `JwtVerificationKey` to the policy pool, flip the IdP, drop
  the old key. HS256 fits self-administered deployments; RS256
  fits the production path where an external IdP (Okta / Auth0
  / Vault / GCP Workload Identity) signs and the control plane
  only holds the public key.
  - **Wire-shape additive change:** `AdmissionError::InvalidCredential`
    gained a `reason: String` field so the verifier-specific
    failure mode (PSK rotation mismatch / JWT issuer mismatch /
    expired / etc.) reaches the operator-facing log line. The
    contract is `Experimental` per `docs/STABILITY.md` so this
    is in scope; no external matches exist in the workspace.
  - 14 hermetic unit tests in `scheduler::jwt::tests` cover
    happy path, every documented failure mode, leeway boundary,
    key rotation pool, and the multi-`aud` shape; 7 more tests
    in `scheduler::admission::tests::jwt_flavor` exercise the
    policy-layer routing (valid token admitted, missing
    credential, wrong subject, expired token,
    `jwt_workers`-without-`jwt`-policy as server config error,
    PSK-takes-precedence, anonymous workers still anonymous
    when JWT is configured). `now()` injection keeps the suite
    deterministic.
  - gRPC-metadata propagation of admission tokens is still
    deferred (separate TODO).
  - Adds `jsonwebtoken = "9.3"` dep to `agentflow-server`.

#### Server gateway

- **Per-run retention override on `POST /v1/runs`** (P10.14.1).
  The request body now accepts an optional `retention_overrides`
  object with `events_days` and `artifacts_days` fields. The
  cleanup sweep uses `max(global, override)` so a per-run
  override can only *extend* retention — it cannot shorten the
  tenant + profile default. Pinning a run's events or artifacts
  also pins the parent `runs` row itself: the cleanup SQL keys
  the run-row deletion on `GREATEST(global, events_override,
  artifacts_override)` so the `ON DELETE CASCADE` from `runs`
  doesn't yank the pinned children out from under the override.
  Negative overrides are rejected at the API layer with a clean
  `bad_request` error; `Some(0)` is accepted (caller convenience)
  and normalized to NULL in the DB so the audit story stays
  honest. New migration `0005_run_retention_overrides.sql` adds
  the two nullable columns to `runs`; existing rows default to
  NULL (no override) so the upgrade is a no-op for everyone
  who doesn't opt in. See `docs/DEPLOYMENT.md` "Per-run
  retention overrides" for the operator-facing snippet.
  Closes the P2.2-deferred per-run override item.

#### Workflow grammar

- **`type: shell` YAML workflow node** (F-A7-2 fully closed,
  `3c3ab02`). Wraps `agentflow_tools::ShellTool` with a
  `SandboxPolicy` built from YAML params. `allowed_commands` is a
  required schema field — workflows without an allowlist fail at
  parse time, no permissive-by-default arbitrary code execution.
  See `agentflow-cli/src/executor/shell.rs`.
- **`input_mapping` accepts `{{ item.* }}` lookups inside a map
  sub-flow** (F-A6-5, `54a2751`). Flat (`item.field`) and dotted
  (`item.foo.bar`) paths both supported. Encoded via the sentinel
  source-node id `"!item"`. Existing `{{ nodes.X.outputs.Y }}`
  lookups work unchanged.
- **Map node `max_concurrent: N` parameter** bounds simultaneously-
  running sub-flows via `tokio::sync::Semaphore` (F-A6-1,
  `a4e89e8`). Unbounded preserved as legacy default. `Some(0)` is
  rejected as a config error.
- **Map node `results_summary` sibling output** surfaces
  `{total, ok, err, err_indexes}` alongside `results` (F-A6-3,
  `fee8586`). Workflows can route on partial failure without
  walking nested JSON; `eprintln!` warning fires on any failure.
- **Template node auto-detects JSON output** when the rendered
  string starts with `[` or `{` (F-A6-7, `8b73298`). Parse failure
  falls back to String wrap (safe for prose). Explicit
  `output_format: "json"` preserved as strict mode.

#### CLI

- **Dense + hybrid RAG eval baselines checked in** (P10.6.2). The
  bundled `ci_offline` dataset now has three regression-gate
  baselines under `agentflow-rag/eval_baselines/ci_offline/`:
  `bm25.json` (offline, always gated on every PR), plus the new
  `dense.json` (`text-embedding-3-small`) and `hybrid.json` (RRF
  over BM25 + dense). CI gates against all three; the dense + hybrid
  steps self-skip via `if: ${{ secrets.OPENAI_API_KEY != '' }}` so
  forks without the secret stay green. A bug-fix in the
  `--compare-baseline` reader lets it accept BOTH the bare
  `EvalReport` shape (the bm25.json convention) AND the
  `{ dataset, baseline, candidate, ... }` envelope shape that
  `--output <path>` writes — pre-P10.6.2 operators had to
  hand-extract the `.baseline` field to round-trip their own
  `--output` files back through the regression gate.
- **Pluggable RAG eval retrievers** (P10.6.1):
  `agentflow-rag::eval::DenseEval` (in-memory cosine similarity over
  pre-embedded corpus + queries) and
  `agentflow-rag::eval::HybridEval` (Reciprocal Rank Fusion with
  configurable `k` and inner-k multiplier) join the existing
  `Bm25Eval` behind the `Retriever` trait. The CLI gains
  `--retriever {bm25,dense,hybrid}` plus `--embedding-model
  <name>` (defaults to `text-embedding-3-small`); dense and
  hybrid require `OPENAI_API_KEY` at run time and surface a
  single-line actionable error when it's missing. Eval-scale
  corpora (<100k docs) keep the full vector matrix in RAM — no
  Qdrant required for the eval harness. RRF tie-break is
  deterministic (score desc, then id asc) so paired sign-test
  comparisons across runs remain reproducible. 10 new unit
  tests in `eval::retrievers::tests` plus 1 hermetic CLI test
  (`build_dense_retriever_errors_without_openai_api_key`) cover
  the new code paths.
- **`agentflow skill inspect --explain-permissions` now runs MCP
  discovery by default** (P10.9.1). Pre-P10.9.1 it was opt-in via
  `--with-mcp-discovery` because spawning every declared MCP
  server is heavy. This release flips the default and adds a
  manifest-level JSON cache at
  `~/.agentflow/cache/skill_mcp_discovery.json` (24-hour TTL,
  keyed by a stable SHA-256 of the manifest's `mcp_servers`
  section — including `name`/`command`/`args`/`env`, excluding
  `timeout_secs`/`max_concurrent_calls` which don't affect tool
  advertisements). Cache hits return in microseconds; cache
  misses show an `indicatif` spinner while the servers are
  spawned. The summary line now identifies which path was
  taken (`cache hit` / `fresh discovery (cached for next run)` /
  `forced re-discovery` / `skipped`). New `--no-mcp-discovery`
  flag opts out entirely; `--refresh-mcp-cache` busts the cache
  on demand. The old `--with-mcp-discovery` flag is kept as a
  no-op + deprecation warning so existing scripts don't break.
  13 unit tests in `commands::skill::mcp_discovery_cache::tests`
  (hash stability across env iteration order / server ordering;
  hash distinguishes argv / command / env-value changes; hash
  ignores timeout; load/save round-trip; load returns empty on
  missing file / schema mismatch / malformed JSON; TTL fresh/
  stale/unknown) + 4 hermetic CLI tests covering the
  deprecation warning, baseline (no warning when not set),
  `--no-mcp-discovery` short-circuit (with an MCP-declaring
  skill whose server script doesn't exist, so a spurious spawn
  would fail loudly), and stray-flag-without-`--explain-permissions`
  note.
- **`docs/ROADMAP_v2.md`** consolidated post-v1.0 roadmap
  (P10.19.3). Single source of truth for "what comes after v1.0
  GA", consolidating signals previously scattered across
  `RoadMap.md` Later Tracks, `TODOs.md` v1.x entries, and
  `docs/archive/PROJECT_EVALUATION_2026-05-19.md` §7. Ten themes
  (LLM/provider expansion, memory/RAG, server platform, Web UI
  debugger-scope, distributed/worker, Harness H6, plugin
  runtime WASM, perf, ops tooling, docs/contributor experience)
  with backreferences to the canonical `TODOs.md` IDs. Explicit
  v2 non-goals carry the v1 non-goals forward + add
  operator-dashboard Web UI per P10.17.1. `RoadMap.md::Later
  Tracks` gains an inline pointer at the top of the section so
  future contributors land on the consolidated view first.
- **`agentflow mcp config list --format json-envelope`** support
  (P10.11.3). The audit of all `format: String` clap fields in
  `agentflow-cli/src/main.rs` found exactly one holdout that
  didn't accept `json-envelope`: this one. The legacy `--format
  json` bare-body shape is preserved for back-compat; the new
  envelope mode wraps the same `{source, servers}` payload in
  the canonical `agentflow.cli/1` wire schema. Hermetic CLI test
  added alongside the existing json test.
- **`cargo xtask check-changelog`** subcommand (P10.18.2). Fails
  when a non-trivial source change versus a base ref (default
  `origin/main`) didn't touch `CHANGELOG.md` AND no commit body
  in the branch range carries `chore(skip-changelog)`. Trivial
  paths (`docs/`, `*.md`, `Cargo.lock` / `package-lock.json`,
  `.github/workflows/`, `tests/`, `**/fixtures/`, `*.test.ts` /
  `*.test.rs`) are excluded — that classifier is the
  single-source-of-truth pass/fail boundary, pinned by a
  dedicated unit test. Not wired into `quality.yml` today;
  available for manual + local pre-commit use. New
  `check_changelog_tests` module covers the 4 outcomes
  (only-docs / changelog-touched / skip-marker / source-but-no-
  signal) with a git fixture per test.
- **Checkpoint schema documentation** (P10.1.2) —
  `docs/CHECKPOINT_SCHEMA.md` formally documents the
  `decode_checkpoint_flow_value` warn-vs-silent fallback
  asymmetry: tagged-but-corrupt values warn loudly so a writer
  regression is debuggable; genuinely untagged legacy values
  fall back silently because spamming the operator on every
  pre-0.2 resume would be noise. The doc names the tests that
  pin each branch + the operator-facing diagnostic surface
  (`tagged ... but failed to deserialize` substring) so the
  contract stays auditable. STABILITY.md cross-references it.
- **Playwright UI e2e wired into CI** (P10.17.4). The 6 specs in
  `agentflow-ui/e2e/` (across two files: `runs-new.spec.ts` +
  `harness-sessions.spec.ts`) now run on a new
  `.github/workflows/ui-e2e.yml` workflow — `workflow_dispatch`
  + nightly schedule at 10:30 UTC, **not** in
  `quality.yml::release-gate.needs`. The pattern mirrors
  `llm-live.yml`: manual + nightly catches regressions between
  releases without the build + browser-install + flakiness tax
  on every PR. `@playwright/test` promoted from optional dev
  dep to a real `devDependencies` entry; new `npm run e2e`
  script plus a `playwright.config.ts` (Chromium-only, JUnit
  XML + HTML report on CI, trace-on-first-retry). Full
  operator + CI runbook in `agentflow-ui/e2e/README.md`. The
  CI job spins up a Postgres 16 service container, builds the
  server (release), boots it in the background with a 30s
  readiness probe against `/ui`, runs the suite, and uploads
  the `playwright-report/` HTML report as an artifact on
  failure. `workflow_dispatch` accepts an optional
  `spec_filter` input that maps to `playwright test -g
  <pattern>`.
- **Server-side `?filter=` pre-filter for events history**
  (P10.17.3). `GET /v1/runs/{id}/events/history?filter=<expr>`
  accepts the same grammar as the client-side `eventFilter.ts`
  (kind=/kind!=/kind~ + step compares joined by AND). Long runs
  no longer need to ship every event over the wire just to be
  filtered client-side. New `agentflow-server::events_filter`
  module with 21 unit tests pinning every clause shape +
  case-insensitivity + the AND-inside-value non-split rule +
  every parse-error path; new self-skipping integration tests
  in `agentflow-server/tests/events_filter_route.rs` cover
  kind-contains + after_seq+filter compose + 400-on-bad-expr +
  empty-param no-op. UI side: run-console history fetch now
  passes the operator's saved filter expression on initial
  attach; 400 responses fall back to no-filter so a malformed
  expression still loads the timeline (the inline parse error
  from the client `compileFilter` is what the operator
  actually sees and fixes).
- **Web UI preferences sync to `/v1/preferences`** (P10.17.2).
  Selected localStorage values now round-trip through the
  server's tenant-scoped preferences API so an operator's
  settings roam across browsers. Pure-helper module
  (`agentflow-ui/src/preferences.ts`) lists the syncable keys —
  tenant ids, profile selections, harness runtime kind, per-run
  event-filter expressions — and explicitly excludes the API
  token, workflow YAML drafts, harness user_input prompts, and
  workspace_root paths (security / size / machine-specific).
  React hook (`usePreferenceSync.ts`) GETs once per
  `(apiToken, tenant)` pair, debounces PUTs at 500 ms via a
  last-write-wins queue, flushes on unmount. localStorage stays
  as a fast first-paint cache. End-to-end wired today for the
  run-console tenant; the other 6 keys are mapped in the helper
  with the same 3-line pattern available for replication —
  tracked as follow-up work inside the new
  `docs/WEB_UI.md § Durable preferences (P10.17.2)` table.
  28 PASS in `preferences.test.ts` (bun-driven; same node-tsx
  pattern as `eventFilter.test.ts`); `npx tsc --noEmit` clean.
- **`agentflow harness replay <session_id>`** subcommand
  (P10.10.2). Re-streams a persisted JSONL session log with
  time-paced output (sleeps between events based on their
  original `ts` deltas). Useful for debugging long-finished
  sessions where the *pacing* of events carries diagnostic value
  — e.g. spotting a tool call that fired right before a stall.
  `--speed 1x` (default) honours the original timing; `2x` /
  `0.5x` scale it; `inf` / `instant` skip all sleeps (== `resume`
  but routed through the per-event formatter). `--from-seq` /
  `--to-seq` clip the visible window; `--filter-kind` (repeatable)
  acts as an OR include-list over the `kind` discriminator.
  `--output {text, stream-json}` — `json` / `json-envelope` are
  rejected because replay is open-ended (mirrors the
  `workflow logs --follow` rejection). The 1-hour sleep cap
  prevents an overnight idle gap from hanging the replay (run
  with `--speed inf` if you really want the original timing).
- **`agentflow memory prune --layer {preference,entity_facts}
  --db <path> --older-than <duration>`** subcommand (P10.7.1)
  wires the existing trait surface
  (`PreferenceStore::prune_older_than`,
  `EntityFactStore::prune_invalidated`) to a CLI front-end so
  operators can drop stale memory rows from the command line.
  `--older-than` accepts `<integer><unit>` where unit ∈
  `{s, m, h, d, w, y}` — bare integers are rejected so a typo
  (`--older-than 30` instead of `--older-than 30d`) can't
  silently mean "30 seconds". `--format text` (coloured ✓ line
  with row count) or `--format json-envelope` (canonical
  `agentflow.cli/1` wrapper carrying `{layer, db, older_than,
  older_than_seconds, removed_rows}`). The `entity_facts` path
  is invalidation-bounded — active facts are never touched, even
  with `--older-than 0s`. Session + semantic layers are out of
  scope for this slice because they expose per-session clear
  rather than retention-based prune. Defaults `--db` to
  `~/.agentflow/memory.db` matching the agent-runtime
  convention. 6 unit tests + 5 hermetic integration tests cover
  the parser, layer routing, missing-db error, unsupported-layer
  rejection, and the end-to-end round-trip against a real
  on-disk SQLite file seeded via the public memory-crate API.
- **`agentflow workflow run --server` flag validation** (P10.11.4):
  closes the silent-drop class of bug for the local-only flag set
  by rejecting up front with a single-line actionable error when
  any of `--model`, `--execution-mode` (non-default),
  `--max-concurrency` (non-default), `--run-dir`, `--watch`,
  `--output`, `--input`, `--dry-run`, `--timeout` (non-default),
  or `--max-retries` (non-zero) is combined with `--server`. Two
  categories: **always-local** (filesystem + in-process flow:
  `--run-dir` / `--output` / `--watch` / `--dry-run`) each name a
  concrete server-side alternative (e.g. `--watch` points at
  `agentflow workflow logs <run_id> --follow`); **future API
  addition** (server-side execution knobs the wire format could
  accept but doesn't today: `--model` / `--execution-mode` /
  `--max-concurrency` / `--input` / `--timeout` / `--max-retries`)
  each name P10.11.4 so curious operators can find the follow-up
  work. Defaults pass through silently — the validator only fires
  when the operator explicitly overrode a flag. 13 unit tests + 11
  hermetic CLI tests cover every guard + the baseline-passes path.
- **`agentflow skill run --server <url>`** dispatches the skill
  to a remote `agentflow serve` instance via
  `POST /v1/skills/{name}:run` (P10.11.2). Mirrors the
  `workflow run --server` pattern: submits, polls
  `GET /v1/runs/{id}` until terminal status (succeeded / failed /
  cancelled), and pretty-prints the final row. The positional
  argument shifts semantics in server mode — it's the skill NAME
  resolved via the server's `AGENTFLOW_SKILLS_INDEX` catalog, not
  a local filesystem path. New flags: `--server <url>`,
  `--auth-token <token>`, `--tenant <id>`. Local-only flags
  (`--memory`, `--model`, `--session`, `--trace`) are rejected
  with a single-line actionable error when combined with
  `--server` because the wire contract doesn't accept per-request
  overrides today. `--output` accepts `json-envelope` in server
  mode (canonical `CliJsonEnvelope`); the local-mode `json`
  value is rejected to keep the wire-schema surface narrow.
  Hermetic axum mock-server integration tests cover the
  submission round-trip, envelope wrap, local-only flag
  rejection, and 404 propagation.
- **`agentflow workflow logs <run_id>`** subcommand consumes the
  server's persisted event log (P10.11.1). Without `--follow`,
  fetches the history snapshot as a single JSON array via
  `GET /v1/runs/{id}/events/history`. With `--follow` (`-f`),
  opens an SSE stream against `GET /v1/runs/{id}/events` and
  prints each event as it arrives until the server closes the
  connection. Supports `--after-seq <n>` for resuming reconnects,
  `--format text|json|json-envelope` (envelope incompatible with
  `--follow` — rejected with a clear error since envelopes are
  bounded and follow streams are not), `--server` / `--auth-token`
  / `--tenant` matching the other server-backed `workflow`
  subcommands. Hermetic round-trip tests via a tiny axum mock
  server (no Postgres required).
- **`agentflow harness run --approve {none,cli,auto-allow,auto-deny}`**
  wires `HookedTool` into the CLI Harness path (F-A2-11, `9d386b3`).
  Combined with `--profile production`, every NonIdempotent tool
  call surfaces an interactive operator prompt. Default `none`
  preserves legacy behaviour for back-compat.
- **`agentflow skill run --output {text,json}`** emits a single
  JSON object on stdout suitable for piping into jq / downstream
  tooling (F-A2-6, `9a96058`). Banners go to stderr in JSON mode;
  `--trace` inlines the runtime trace under the `trace` key.
- **`agentflow doctor` opens its Config section with a
  human-readable source label** ("`/path/to/models.yml` (overrides
  built-in)") instead of the bare Rust-debug enum (F-A7-4,
  `bdaff36`). JSON output gains `models_config_source_kind` as a
  stable snake_case enum.
- **`agentflow llm models --refresh-from-api`** live-queries each
  OpenAI-compatible provider's `/v1/models` endpoint and prints the
  delta vs the local registry (F-A7-6, `6290ca9`). Output groups
  per-provider: `new` (provider-side additions to add to
  `models.yml`), `only_local` (deprecated / typo / private
  deployment), `shared` (count). Read-only; respects `--provider`
  filter. Currently supports openai / moonshot / stepfun /
  dashscope. 5 new unit tests on URL construction + truncation.

#### Agent runtime

- **ReAct steering note on repeat tool calls** (F-A2-13, `d7651f7`).
  When the LLM returns the exact same `(tool, params)` two
  iterations in a row, the second tool result memory message gets
  an `[agentflow steering note (F-A2-13): ...]` nudging the model
  to advance. Advisory only — tool still runs. Trace-side
  `AgentStepKind::ToolResult` stays clean.

#### Skill manifest

- **SKILL.md frontmatter `model:` field now honoured** (F-AF-2,
  `100c267`). Was silently dropped because the field wasn't on
  `SkillMdFrontmatter`. Empty/whitespace strings collapse to
  `None` so a stray `model: ""` doesn't propagate.

#### Examples

- **`examples/applications/code-reviewer-write/`** (`83a9765`)
  end-to-end Harness Mode approval gate validation binary; uses
  `wrap_registry` + `CliApprovalProvider` to exercise the
  approval flow against real shell + file:write tools. Includes
  `--auto-approve` for CI smoke and `--prefetch-diff` workaround
  for moonshot-v1-128k's loop pathology.
- **`examples/applications/research-assistant/`** (`b492f6a`)
  L1 binary fetching arxiv papers, deduping via
  `SqliteEntityFactStore`, summarising via a single LLM call.
- **`examples/applications/doc-translator/`** — full A6 spec
  shipped in 4 iterations:
  - iter 1 (`141b993`): `map parallel + LLM` primitive validator,
    4 langs hardcoded.
  - iter 2 (`4b882eb`): real file I/O, 2 files × 4 langs.
  - iter 3 (`ec2c15d`): `file_list × lang_list` cross-product via
    Tera template.
  - iter 4 (`35c1d01`): real file discovery via the new
    `type: shell` node. Adding a markdown file to `input/` is a
    zero-line workflow change.

### Changed

- **Workspace package metadata centralised via `[workspace.package]`**
  (P10.0.2-FU1). New shared metadata table in the root `Cargo.toml`
  pins `edition` / `license` / `authors` / `repository` / `homepage`
  once for every publishable member; each member's `[package]` table
  opts in with `<field>.workspace = true`. Clears the
  `manifest has no documentation, homepage or repository` warning
  from `cargo publish --dry-run` across all 15 publishable crates.
  Two pre-existing `repository = "https://github.com/agentflow/agentflow"`
  values (an org that doesn't exist — 404) replaced with the inherited
  canonical URL `https://github.com/yuxuetr/agentflow`. A future repo
  rename is now a one-line change to the workspace block instead of
  a 15-file `sed`. `xtask` stays opted out (`publish = false`).
- **Workspace-internal `[dependencies]` now carry explicit
  `version = "X.Y"`** (P10.0.2). Every path-dep on a workspace
  crate was previously bare `{ path = "../..." }`, which
  `cargo publish` rejects with `all dependencies must have a
  version requirement specified when publishing`. Eleven Cargo.toml
  files were patched (the four leaf crates `agentflow-core` /
  `agentflow-tools` / `agentflow-rag` / `agentflow-db` had no
  internal deps). Workspace builds and test suites are unaffected
  — `path =` still wins at resolution time; the version is only
  consulted when the package gets published. `[dev-dependencies]`
  path-deps are left as-is (cargo strips them at publish).
- **`Cargo.lock` refreshed off the yanked `slab v0.4.10`** (P10.0.2).
  `cargo update -p slab` bumped to `v0.4.12`, removing the
  `package 'slab v0.4.10' in Cargo.lock is yanked in registry
  'crates-io'` warning from `cargo publish --dry-run`.
- **Default template node behaviour**: auto-detects JSON when
  rendered output starts with `[`/`{`. Workflows that already set
  `output_format: "json"` are unaffected; new workflows can omit
  the hint. Parse failure falls back to legacy String wrap (safe).
- **Validator behaviour for template nodes**: arbitrary
  `parameters` keys no longer false-warn (F-A6-6, `8b73298`).
  Template's whole point is to consume arbitrary Tera context.
  Typo-detection for other node types (closed ParamSpec) is
  unaffected.
- **`LLMConfig::validate()` is lenient on missing API keys**
  (P10.3.1). Previously, `AgentFlow::init()` against the bundled
  `default_models.yml` would fail-close for a fresh user with
  only `OPENAI_API_KEY` set, because the YAML references ~9
  providers and the strict validator required every key to be
  present. Now `validate()` emits an `eprintln!` warning naming
  each missing provider + the affected models, and returns
  `Ok(())` so init proceeds. The fail-fast moves to the lookup
  path: `ModelRegistry::get_provider("anthropic")` now returns
  `LLMError::MissingApiKey { provider: "anthropic" }` (actionable
  — names the env var to set) when that provider was skipped at
  init time, rather than the misleading `UnsupportedProvider`.
  **Migration**: callers that need the old fail-close semantics
  (e.g., `agentflow doctor --profile production` health checks)
  should use the new `LLMConfig::validate_strict()` method, which
  returns `Err(LLMError::MissingApiKey)` on the first missing
  configured key. Structural validation (unsupported vendor,
  out-of-range numeric fields) remains a hard error in both
  paths.

### Fixed

- **`HarnessProfile::Local` silent-allow footgun documented**
  (F-A2-12, `c552d3c`). Rustdoc on the enum variants + a footgun
  callout in `docs/HARNESS_MODE.md` make it clear that without
  `.with_profile(HarnessProfile::Production)`, the approval gate
  doesn't fire for NonIdempotent tools.
- **`agentflow workflow validate` no longer false-warns on map's
  `input_list` / `max_concurrent`** (F-A6-2, `a4e89e8`). ParamSpec
  list bumped.
- **`agentflow skill run` answer recovery from truncated JSON**
  (F-A2-1, `ec3e1a7`). Best-effort `answer` field extraction in
  `react/parser.rs` when `max_tokens` truncates the response
  mid-JSON. 6 new tests.
- **Doctor integration test patched** for the F-A7-4 output-format
  change (`8b73298`). Previously only the unit tests covered it.
- **94 text models bumped from `max_tokens: 4096` to `32768`**
  (F-A7-8, `42c3225`). 1M+ context models can sustain much
  longer outputs; the previous cap was too conservative.
- **`LLMError::MissingApiKey` renders an actionable one-liner**
  (F-AF-4, `6b89317`). Names the provider-specific env var (e.g.
  `MOONSHOT_API_KEY (or MOONSHOT_KEY)`), points at
  `~/.agentflow/.env`, suggests `agentflow config init` to
  generate the template, and references the docs. New
  `env_var_hint(provider)` helper has table coverage for 6
  providers with a generic fallback. 3 new unit tests.
- **A1.5 persona now re-measures LUFS before save** (F-EX-1,
  `b35f371`). Adds a step 6 audio_loudness call after
  normalize_lufs / fade so the final report uses the **实测**
  value rather than the target parameter (integrity issue
  caught in R1 dogfooding).

### Changed (stability)

- **`agentflow-mcp::server` promoted from Experimental to Beta**
  (P10.5.2). The closed method set is now pinned:
  `initialize` / `notifications/initialized` / `tools/list` /
  `tools/call`. New methods may be added in minor releases; the
  existing four stay wire-stable. New public surface includes
  `MCPServer::handle_request` (the single request → response
  entry point, now `pub` so non-stdio transports can drive it)
  and `STABLE_PROTOCOL_VERSION = "2024-11-05"` (bumping this is
  the explicit signal that the wire contract changed). Backed by
  6 fixture-driven compat tests + 2 invariant tests in
  `agentflow-mcp/tests/server_contracts.rs`. The fixture format
  pins required fields + exact values + error envelope shapes but
  tolerates additive fields, matching the Beta promise. See
  `docs/STABILITY.md` for the full contract and fixture-ownership
  row.

### Removed

- **`agentflow-viz` crate** (P10.13.1). Removed alongside the
  `/v1/runs/{id}/graph` REST endpoint, the `agentflow workflow
  graph` CLI subcommand, the `RunGraphResponse` shape, and the
  Mermaid `<pre>` block in the Web UI. An honest audit revealed
  the "DAG visualisation" surface was a button grid of node
  status badges plus the raw Mermaid markdown text in a code
  block — no SVG, no spatial layout, no edges. The data-plumbing
  cost (an entire workspace crate + a beta REST route + a CLI
  subcommand + the UI fetch path) was disproportionate to the
  rendering value. The UI's node-status grid is now derived
  entirely from event payloads, which was already the source of
  truth for execution state. A future RFC may revisit graphical
  DAG / agent topology rendering as an additive feature
  (e.g. mounting mermaid.js to render `agentflow workflow
  validate --output mermaid` as SVG client-side); see
  `docs/ROADMAP_v2.md` Theme D for the decision rationale.
  Stability impact: `/v1/runs/{id}/graph` was listed as Beta in
  `docs/STABILITY.md`; the row was deleted with a P10.13.1
  cross-reference so anyone hitting the old endpoint gets a
  pointer to the rationale.

- **`agentflow-mcp::client_old`** and the legacy `transport` module
  it depended on (P10.5.1). Both were `#[doc(hidden)]` and had no
  external callers in the workspace; deleting them removes ~330
  lines of dead code. The current `transport_new` module is renamed
  to `transport` so the post-cleanup name is internally consistent.
  A `#[deprecated]` `pub use transport as transport_new;` re-export
  preserves the old import path for any 3rd-party caller through
  the transition window — they get a deprecation warning instead of
  a hard break. A compat unit test pins the alias's type identity
  so the re-export can't silently degrade. agentflow-mcp is below
  the stability tier line per `docs/STABILITY.md`, so this rename
  is in scope.

- **6 dead `agentflow-llm/config/models/*.yml` files** (F-A7-3,
  `5578743`). The vendor_configs split was never read by the
  runtime registry. 4 misdirecting docs updated to point at the
  real source (`templates/default_models.yml`).

### Docs / conventions

- **LLM provider module promotion criteria 1-pager** (P10.3.2,
  `docs/LLM_PROVIDER_MODULE_PROMOTION.md`). Pins per-vendor
  promotion triggers for the four OpenAI-compat vendors
  (GLM, DashScope, DeepSeek, MiniMax) that currently share
  `OpenAIProvider`. Documents six concrete divergence signals
  (tool-call shape, multimodal shape, streaming-protocol,
  auth/endpoint topology, vendor-specific feature with no
  upstream OpenAI mapping, operator-side request) and ties
  each to an existing `cross_provider_*` consistency-test
  invariant where applicable, so "should I extract X?" is
  answerable in minutes against the empirical state of the
  nightly live suite rather than a re-derived analysis. As
  of 2026-05-20 no trigger has fired. Closes a *tracking*
  TODO; no Rust code shipped. Same posture as the WASM
  1-pager (P10.19.1) and the H6 criteria 1-pager (P10.10.1).
  `docs/ROADMAP_v2.md` Theme A updated to mark closed with
  a pointer to the criteria doc.

- **H6 promotion criteria 1-pager** (P10.10.1,
  `docs/H6_PROMOTION_CRITERIA.md`). Pins per-item promotion
  triggers for the 5 H6 (Phase Harness "advanced compatibility")
  items so the TODO list doesn't accidentally pull them en
  bloc when a single one gets demand. For each of
  slash-command expansion, TUI product shell, OpenHarness
  config import, plugin compatibility adapters, and provider
  subscription bridge: documents the demand signal that would
  tip the scale, the per-item RFC scope, the estimated effort,
  and the explicit cross-reference to the non-goal stance in
  `RoadMap.md` for the two items currently documented as
  non-goals (TUI, subscription bridge). Closes a *tracking*
  TODO — no code shipped; the value is preserving the per-item
  review discipline so a future P11.x opens a focused RFC
  instead of a five-item batch. Same posture as the WASM
  1-pager (P10.19.1). `docs/HARNESS_MODE.md` H6 row links to
  the criteria doc; `docs/ROADMAP_v2.md` Theme F flags
  non-goals explicitly.

- **WASM plugin runtime 1-pager** (P10.19.1,
  `docs/WASM_PLUGIN_EVALUATION.md`). The narrowed
  wasmtime-vs-wasmer-vs-extism comparison concludes that
  wasmtime + WIT + WASI 0.2 is the right runtime *if* we adopt
  WASM, and decides to **push the adoption itself to v2.0**.
  Subprocess JSON-RPC is mature and the 50-200 ms subprocess
  cold start is dominated by the first LLM call's TCP
  handshake in any realistic workflow — the ~6-8 person-week
  pre-GA investment doesn't fix any current friction. The
  `PluginRuntime::Wasm` enum variant stays in `manifest.rs` as
  a forward-compatible reservation; v2 wires the real host
  when at least one of the documented re-evaluation triggers
  fires (latency complaint, polyglot demand, distribution
  complaint, or peer-project precedent). Closes the
  P10.19.1 HIGH pre-GA item from the v1.0 RC backlog.

- **R1 → R4 reflection sequence** (`docs/L1_L3_REFLECTION_*.md`,
  `b3ee990` / `2d3d06d` / `edd9572`). R2 froze the L1↔L3
  selection rule + per-application matrix; R3 documented the
  R2-follow-up sweep + 6 emergent patterns; R4 captured the A6
  sweep + 6 more patterns. R4 §5 has the cumulative arc.
- **Examples conventions** (`examples/README.md`) gained two
  cross-cutting bullets: LLM-judgement output is
  non-deterministic — run multiple times, union the findings
  (F-A2-5, `0d921aa`); translation workflows must guard
  `source_lang != target_lang` before LLM dispatch (F-A6-4,
  `907b6e7`).
- **`docs/HARNESS_MODE.md`** got a footgun callout for the
  Local-vs-Production approval-gate asymmetry + an inline comment
  in the canonical snippet (F-A2-12, `c552d3c`).
- **`docs/AGENT_SDK.md` gained two reference sections**
  (`b35f371`): a `FlowValue` field reference table enumerating
  exact field names per variant (F-DOC-2; prevents `media_type`
  vs `mime_type` round-trips), and a "Loading
  `~/.agentflow/.env` from standalone binaries" canonical 6-line
  snippet (F-A7-7; standardises the pattern used by every
  standalone example binary).
- **`agentflow-llm/README.md` § Moonshot** documents the
  kimi-k2.6 `temperature: 1.0` constraint with the exact API 400
  error message (F-A7-5, `b35f371`), plus the org-level
  concurrency limit of 3 that motivates `max_concurrent: 3` in
  `map parallel` workflows.

### Known still-open

- **F-A6-8** (Tera library quirk, not an agentflow bug): Tera
  `loop.parent.*` introspection doesn't work in this Tera
  version. `set_global` accumulator is the documented workaround
  in `examples/applications/doc-translator/workflow-iter3.yml`.
- **A6 iter 5** (100+ file stress test): all platform capability
  axes are now in place; iter 5 is a quantity question rather
  than a capability one. Probably worth running once before any
  v0.3.0 cut.
- **Phonon-external items** (F-PH-1/2/3, plus F-DOC-3/4 docs that
  live phonon-side): not in the agentflow workspace; tracked
  separately.
- **All other agentflow-internal items from the R1 → R4 sweep
  are now closed** (F-DOC-2, F-A7-5, F-A7-6, F-A7-7, F-AF-4,
  F-EX-1 all landed; see Added / Fixed / Docs above).

---

## [0.2.0] and earlier

No structured changelog kept before this entry. For change history
prior to v0.3.0 prep, see `git log` (most commits follow
Conventional Commits) and `docs/archive/PROJECT_EVALUATION_2026-05-19.md`
for the most recent cumulative project evaluation (or
`docs/archive/PROJECT_EVALUATION_2026-05-14.md` for the prior one).

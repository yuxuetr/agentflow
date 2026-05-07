# Agent SDK

This guide is for developers who want to **extend** AgentFlow's agent runtime
— plug in a custom planning loop, a custom reflection strategy, a custom
memory-summary backend, a custom tool, or a custom memory store — without
forking the workspace.

If you only want to **run** built-in agents (ReAct, Plan-Execute, Multi-Agent
supervisors), read [`AGENT_RUNTIME.md`](./AGENT_RUNTIME.md) and
[`MULTI_AGENT.md`](./MULTI_AGENT.md) first.

## Goals

- 30 minutes from "I cloned the repo" to "my custom reflection strategy runs
  inside `ReActAgent`".
- Every extension point is a Rust trait with a stable contract and at least
  one runnable example.
- The SDK does not require external API access; every example uses the
  `mock` LLM provider or runs without a model at all.

## Five-minute quickstart

The fastest possible warm-up is a custom reflection strategy. The strategy
is a single trait with two methods, and the runtime accepts any
`Arc<dyn ReflectionStrategy>`.

```bash
# 1. Build and run the canonical example.
cargo run -p agentflow-agents --example custom_reflection
```

Expected output (truncated):

```
Answer: echo: sdk
Stop reason: FinalAnswer
Triggers observed by LoggingReflection: [Final]
--- Steps ---
   0: observe: Echo the word sdk.
   1: plan: call echo
   2: tool_call: echo
   3: tool_result: echo -> echo: sdk
   4: plan: echo returned the expected text
   5: final: echo: sdk
   6: reflect: [logging] final answer at step 6: echo: sdk
```

Read [`agentflow-agents/examples/custom_reflection.rs`](../agentflow-agents/examples/custom_reflection.rs)
end-to-end. The whole example is ~120 lines and exercises:

- The `ReflectionStrategy` trait (`name`, `reflect`).
- Wiring through `ReActAgent::with_reflection_strategy`.
- The mock LLM provider (no API key needed).
- Structured `AgentStep` output, including the inserted `Reflect` step.

## Extension points

| Trait | Crate | Purpose | Example |
| --- | --- | --- | --- |
| `AgentRuntime` | `agentflow-agents` | Replace the entire planning / execution loop | [`custom_runtime.rs`](../agentflow-agents/examples/custom_runtime.rs) |
| `ReflectionStrategy` | `agentflow-agents` | Inject reflection text into the step trace | [`custom_reflection.rs`](../agentflow-agents/examples/custom_reflection.rs) |
| `MemorySummaryBackend` | `agentflow-agents::react` | Compress prompt memory when it overflows the budget | [`custom_memory_summary.rs`](../agentflow-agents/examples/custom_memory_summary.rs) |
| `AgentMemoryHook` | `agentflow-agents` | Non-failing observer for memory reads / writes | (in tests, see `react/agent.rs`) |
| `Tool` | `agentflow-tools` | Add a new tool callable by any agent | [`agent_native_react.rs`](../agentflow-agents/examples/agent_native_react.rs) (`EchoTool`) |
| `MemoryStore` | `agentflow-memory` | Add a new conversation memory backend | `SessionMemory` / `SqliteMemory` impls |

Each trait below has the same anatomy: **what it does**, **the contract**,
**how to plug it in**, and **gotchas**.

### `AgentRuntime`

```rust
#[async_trait]
pub trait AgentRuntime: Send {
  async fn run(&mut self, context: AgentContext) -> Result<AgentRunResult, AgentRuntimeError>;
  fn runtime_name(&self) -> &'static str;
}
```

**What it does.** Owns one full agent invocation. The runtime consumes an
[`AgentContext`](../agentflow-agents/src/runtime.rs) (session id, input, model,
persona, limits, cancellation token) and returns a structured
[`AgentRunResult`](../agentflow-agents/src/runtime.rs) (answer, stop reason,
step trace, event stream).

**Contract.** Implementations MUST:

1. Honour `RuntimeLimits` (`max_steps`, `max_tool_calls`, `timeout_ms`,
   `token_budget`); whichever is hit first stops the loop with the
   matching `AgentStopReason`.
2. Honour `context.cancellation_token` if present (poll
   `AgentCancellationToken::is_cancelled` between steps; stop with
   `AgentStopReason::Cancelled`).
3. Emit `AgentStep`s with **monotonically increasing** indices, in
   chronological order, so trace replay and event listeners stay
   coherent.
4. Use `AgentRuntimeError::InvalidContext` for pre-flight validation
   problems and `AgentRuntimeError::ExecutionFailed` for anything raised
   *after* the loop starts. Both error variants short-circuit the run
   (no `AgentRunResult` is produced).

**Wiring.** Anywhere you would call a built-in runtime, call yours:

```rust
let mut runtime = MyRuntime::new(...);
let result = AgentRuntime::run(&mut runtime, AgentContext::new(...)).await?;
```

`AgentNode` (DAG embedding) and the multi-agent supervisors take any
`AgentRuntime`, so a custom runtime composes into hybrid workflows for free.

**Gotchas.**

- `AgentStepKind` is **closed** — you cannot add a new variant out-of-tree.
  If you need a step kind that does not exist (e.g. a domain-specific
  `Critique` step), open an issue first; reusing `Plan` / `Reflect` /
  `ToolResult` is usually the right answer.
- The runtime is responsible for writing user input and assistant
  responses to memory. If you skip that, multi-turn conversation breaks
  silently.
- Long-running steps should still poll cancellation; otherwise users see
  unresponsive runs even after `cancel()`.

See [`custom_runtime.rs`](../agentflow-agents/examples/custom_runtime.rs)
for the smallest viable shell — no LLM, no tools, just the
trait contract.

### `ReflectionStrategy`

```rust
#[async_trait]
pub trait ReflectionStrategy: Send + Sync {
  fn name(&self) -> &'static str;
  async fn reflect(
    &self,
    context: &ReflectionContext,
  ) -> Result<Option<Reflection>, ReflectionError>;
}
```

**What it does.** Inserts a `Reflect` step into the trace at well-defined
points (`Step` / `Failure` / `Final`) without changing the planning loop.

**Contract.**

- `name()` is stable, lowercase, snake_case (used in the persisted
  step record and trace UI).
- Filter on `context.trigger` first; return `Ok(None)` for triggers you
  do not handle. The runtime handles `None` by skipping the reflection
  step entirely.
- Treat your own failures as soft: prefer `Ok(None)` over `Err`. The
  runtime will continue without inserting a reflection if you return
  `Ok(None)`. Only return `Err(ReflectionError::Failed)` when the
  caller can act on it.
- Reflections run **inline** with the loop; don't call slow blocking
  APIs without an explicit timeout.

**Wiring.**

```rust
let agent = ReActAgent::new(config, memory, tools)
  .with_reflection_strategy(Arc::new(MyStrategy::new()));
```

Built-ins: `NoOpReflection`, `FailureReflection`, `FinalReflection` (in
`agentflow_agents::reflection`).

### `MemorySummaryBackend`

```rust
#[async_trait]
pub trait MemorySummaryBackend: Send + Sync {
  fn name(&self) -> &'static str;
  async fn summarize(&self, context: MemorySummaryContext) -> Result<Option<String>, ReActError>;
}
```

**What it does.** Compresses the older slice of a session's memory into a
single summary string when the prompt-memory token estimate exceeds the
configured `memory_prompt_token_budget`.

**Contract.**

- Triggered only when `ReActConfig::memory_prompt_token_budget` is set,
  `memory_summary_strategy != Disabled`, and the running token estimate
  exceeds the budget.
- `Ok(Some(summary))` injects `summary` as a synthetic system message
  ahead of the kept messages.
- `Ok(None)` skips the summary (the loop silently truncates).
- `Err(ReActError::MemorySummary { .. })` aborts the run; reserve it for
  genuine failures.
- Backends should stay deterministic where possible; LLM-backed
  backends should set a tight timeout to avoid stalling the loop.

**Wiring.**

```rust
let agent = ReActAgent::new(config, memory, tools)
  .with_memory_summary_backend(Arc::new(MyBackend::new()));
```

Built-ins: `RecentOnlyMemorySummary` (records what was dropped),
`CompactMemorySummary` (deterministic rule-based bullet list).

See [`custom_memory_summary.rs`](../agentflow-agents/examples/custom_memory_summary.rs)
for both direct invocation (drives the trait without a full run) and the
production wiring.

### `AgentMemoryHook`

```rust
pub trait AgentMemoryHook: Send + Sync {
  fn on_memory_read(&self, _context: &MemoryHookContext) {}
  fn on_memory_write(&self, _context: &MemoryHookContext) {}
}
```

**What it does.** Non-failing observer for memory operations. Use it to
record metrics, build secondary summaries, or fan out to a search index.
Hooks intentionally cannot fail; if your hook needs to surface errors,
log them instead.

**Wiring.**

```rust
let agent = ReActAgent::new(config, memory, tools)
  .with_memory_hook(Arc::new(MyHook::new()));
```

### `Tool`

```rust
#[async_trait]
pub trait Tool: Send + Sync {
  fn name(&self) -> &str;
  fn description(&self) -> &str;
  fn parameters_schema(&self) -> Value;
  fn metadata(&self) -> ToolMetadata { ... }
  fn idempotency(&self, params: &Value) -> ToolIdempotency { ... }
  fn requires_capabilities(&self) -> Vec<Capability> { ... }
  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError>;
}
```

**What it does.** Defines a callable tool surfaced to any agent through
`ToolRegistry`. The same trait powers built-in tools (`FileTool`,
`HttpTool`, `ShellTool`), MCP-bridged tools, and workflow-as-tool
(`WorkflowTool`).

**Contract.**

- `name()` must be globally unique inside one registry.
- `parameters_schema()` is JSON Schema; the LLM-facing prompt
  serialises it directly.
- `metadata()` declares `ToolSource`, `ToolPermissionSet`, and
  `ToolIdempotency`. Set permissions accurately — the policy layer and
  OS sandbox both consult them.
- `idempotency(params)` overrides metadata when replay safety depends
  on parameters (e.g. `http GET` vs. `http POST`).
- `execute()` should return `ToolOutput::error(...)` for tool-level
  failures (the agent loop handles it) and `Err(ToolError)` only for
  protocol-level failures (the run aborts).

See [`agent_native_react.rs`](../agentflow-agents/examples/agent_native_react.rs)
(`EchoTool`) and the built-in implementations under
`agentflow-tools/src/builtin/`.

### `MemoryStore`

```rust
#[async_trait]
pub trait MemoryStore: Send + Sync {
  async fn add_message(&mut self, message: Message) -> Result<(), MemoryError>;
  async fn get_history(&self, session_id: &str, limit: usize) -> Result<Vec<Message>, MemoryError>;
  async fn get_all(&self, session_id: &str) -> Result<Vec<Message>, MemoryError>;
  async fn search(&self, session_id: &str, query: &str, limit: usize) -> Result<Vec<Message>, MemoryError>;
  async fn clear_session(&mut self, session_id: &str) -> Result<(), MemoryError>;
  async fn session_token_count(&self, session_id: &str) -> Result<u32, MemoryError>;
  async fn to_prompt(&self, session_id: &str) -> Result<String, MemoryError> { ... }
}
```

**What it does.** Pluggable conversation memory backend. Built-ins:
`SessionMemory` (in-process, token-windowed), `SqliteMemory` (persistent),
`SemanticMemory` (similarity search via `agentflow-rag`).

**Contract.**

- `get_history(session_id, limit)` returns the most recent `limit`
  messages **oldest-first** (callers depend on chronological order).
- `add_message` is the **only** mutator; runtimes never reach in to
  rewrite history. If your backend supports compaction, do it
  transparently.
- `session_token_count` should be cheap; it is polled inside the
  `RuntimeLimits::token_budget` guard.
- All methods are `async`; use whatever runtime you like internally
  (Tokio is canonical).

## Closed enums and stability

These enums are deliberately closed; do not extend them out-of-tree:

- `AgentStepKind` (runtime step variants).
- `AgentStopReason` (run termination reasons).
- `ReflectionTrigger` (`Step` / `Failure` / `Final`).
- `ToolPermission`, `Capability`, `ToolSource` (security boundary).

If a closed enum is genuinely missing a variant for your use case, open
an issue describing the use case before forking. Keeping these closed
keeps trace replay, sandbox enforcement, and multi-agent supervisors
honest.

## Testing without an API key

Two patterns:

1. **No model**: implement `AgentRuntime` directly. See
   [`custom_runtime.rs`](../agentflow-agents/examples/custom_runtime.rs).
2. **Mock provider**: register the `mock` provider and pre-load it via
   `AGENTFLOW_MOCK_RESPONSES` (a JSON-encoded list of canned replies).
   See [`agent_native_react.rs`](../agentflow-agents/examples/agent_native_react.rs)
   and [`custom_reflection.rs`](../agentflow-agents/examples/custom_reflection.rs)
   for the standard wiring.

## Where to read next

- [`AGENT_RUNTIME.md`](./AGENT_RUNTIME.md) — runtime boundary, ReAct loop,
  hybrid composition with DAG.
- [`MULTI_AGENT.md`](./MULTI_AGENT.md) — Handoff / Blackboard / Debate
  supervisors and their step kinds.
- [`TOOL_PERMISSIONS.md`](./TOOL_PERMISSIONS.md) — three-way capability
  merge between tool, skill, and CLI flag.
- [`TRACING_DESIGN.md`](./TRACING_DESIGN.md) — how `AgentEvent`s are
  persisted and how OTel spans are produced.

## Documentation hygiene

`cargo doc -p agentflow-agents -p agentflow-tools -p agentflow-memory --no-deps`
must report **zero** warnings. The SDK extension surface is the contract
this guide describes; if rustdoc breaks, this guide is broken.

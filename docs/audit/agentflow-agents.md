# Audit: agentflow-agents

**Date**: 2026-05-24
**Auditor**: Claude (automated deep audit)
**Crate path**: agentflow-agents/
**Crate version**: 0.2.0
**Layer**: L3 (Agent / Orchestration)
**Stability tier**: beta (per CLAUDE.md, the AgentRuntime/HarnessEvent surface is beta as of P-H.5; agent step kinds are documented as a closed enum)

## Scope summary

`agentflow-agents` is the L3 agent-native runtime crate. It provides:

- The cross-runtime contract: `AgentRuntime` trait, `AgentContext`, `AgentRunResult`, `AgentStep` / `AgentStepKind` (closed enum), `AgentEvent` (closed enum), `AgentStopReason`, `RuntimeLimits`, `AgentCancellationToken`, `AgentMemoryHook`.
- Three concrete runtimes: `ReActAgent` (the dominant impl, 3,497 LOC), `PlanExecuteAgent`, and three supervisor runtimes (`HandoffSupervisor`, `BlackboardSupervisor`, `DebateSupervisor`).
- Reflection plumbing (`ReflectionStrategy`, `NoOpReflection`, `FailureReflection`, `FinalReflection`).
- Memory summarisation (`MemorySummaryBackend`, `RecentOnlyMemorySummary`, `CompactMemorySummary`).
- Hybrid composition: `AgentNode` (embed an agent in a DAG) + `AgentNodeResumeContract` (partial-resume metadata) + `AgentTool` / `WorkflowTool` (agents as tools and workflows as tools).
- An eval harness (`eval::EvalRunner`, `Assertion`, `Dataset`, `PricingTable`).
- Legacy app utilities (`common::{BatchProcessor, StepFunPDFParser, ...}`), still re-exported via `pub use common::*`.

The crate is the central bridge between L1 (`agentflow-core`), L2 (`agentflow-llm`, `agentflow-memory`, `agentflow-tools`, `agentflow-mcp`), and L4 (`agentflow-harness`, `agentflow-server`).

## Findings

### CRITICAL

None. The runtime correctness is solid: `unwrap()/expect()` usage is almost entirely confined to tests, the H3 ordering contract is implemented and pinned by tests, cancellation is honoured at every blocking await, and the contract enums are explicitly closed.

### MAJOR

- [M1] `expect("every prepared call must have an output by this point")` in the batch finaliser — `agentflow-agents/src/react/agent.rs:2073`
  **What**: The batch dispatcher's final pass over the `outputs: Vec<Option<...>>` vector uses `.expect(...)` instead of returning a structured error. The invariant is correct in current code (the concurrent group fills every concurrent index in 5a, the serial loop fills every serial index in 5b, and `concurrent_idxs ∪ serial_idxs == 0..n`), but it is enforced only by code shape — there's no debug_assert, and any future early-return that skips a serial index would panic the runtime.
  **Why it matters**: This is the only `.expect()` in non-test runtime code on the hot agent loop. CLAUDE.md project rules forbid `unwrap()/expect()` outside test/example code; per the rules a panic on a structural invariant should be `unreachable!("reason")` so reviewers see the intent, or restructured to use a dense `Vec<(ToolOutput, u64)>` keyed by build order rather than `Vec<Option<_>>`.
  **Fix**: Replace with `.unwrap_or_else(|| unreachable!("prepared call {} produced no output: concurrent_idxs={:?} serial_idxs={:?}", i, concurrent_idxs, serial_idxs))` so the panic message tells operators which partition lost the output, or refactor `outputs` to be filled in two `Vec<(usize, output, dur)>` and re-merged.

- [M2] Cancellation aborts the `select!`, not the in-flight tool task — `agentflow-agents/src/react/agent.rs:986-1052,1100-1126,1873-1928,1979-2050`
  **What**: Tool execution uses `tokio::select!` between the tool future and `token.cancelled()`. When cancellation fires, the runtime drops the tool future and returns an `AgentStopReason::Cancelled` result. In-process tools cooperate with this because dropping the future cancels it, but the contract is silent on tools that spawn detached `tokio::spawn` work, hit `Drop`-unsafe FFI, or wrap blocking syscalls — those continue running after cancellation.
  **Why it matters**: The CLAUDE.md doc claims "graceful shutdown of in-flight tool calls" (audit prompt question C) but in practice the agent only emits a `ToolCallCompleted { is_error: true, duration_ms: <elapsed> }` event and returns. There is no `Tool::cancel(&self)` hook, no per-tool cancellation token, and the harness layer can't surface a `cancelled` reason to the consumer.
  **Fix**: Either document the current behaviour (cooperative cancellation; tools that spawn detached work must honour their own cancellation) or add a per-call cancellation token to `Tool::execute` so heavy tools (shell, MCP, plugin subprocess) can abort cleanly. Same gap exists in `PlanExecuteAgent::execute_tool` (`plan_execute.rs:498-539`) and all three supervisors.

- [M3] `PlanExecuteAgent` does not enforce `RuntimeLimits.token_budget` — `agentflow-agents/src/plan_execute.rs:144-418`
  **What**: `PlanExecuteAgent::run_with_context` reads `context.limits.max_steps` (line 167), `max_tool_calls` (168), `timeout_ms` (169), and `cancellation_token` (170) — but never reads `token_budget`. `RuntimeLimits::react_defaults()` sets a 50k-token budget that callers expect to apply across both runtimes; in plan-execute it's silently dropped, so a runaway planner producing huge memory writes will be capped only by `max_steps`.
  **Why it matters**: `RuntimeLimits` is documented as a uniform contract (`runtime.rs:13-46`: "All four bounds are independent stop signals; whichever is hit first terminates the run"). The runtime trait promise is broken for plan-execute. The eval harness applies the limits to every runtime via `limits_from_case` (`eval/runner.rs:455-463`), so plan-execute cases in evals also skip the budget check.
  **Fix**: After every loop turn (or before each planner / tool call) compute `memory.session_token_count(&session_id)` and emit `AgentStopReason::TokenBudgetExceeded` when over budget, mirroring `react/agent.rs:619-642`. Add a regression test alongside the existing plan-execute tests.

- [M4] `Blackboard.write_internal` uses `.expect("blackboard version poisoned")` on a `Mutex` lock — `agentflow-agents/src/supervisor/blackboard.rs:97-105`
  **What**: A poisoned `next_version` mutex panics the writer instead of recovering. Adjacent `entries.write()` and `ops.lock()` calls (lines 112-122) use `if let Ok(...)` and silently drop the write on poisoning — so the contract is inconsistent within a single function: a poisoned version counter aborts the agent, but a poisoned entries map silently swallows the write.
  **Why it matters**: One reachable code path (a panicking writer in `BlackboardSchedule::Parallel`) poisons the version mutex; subsequent agents' writes panic. Per the project's Rust error-handling rules, `unwrap()/expect()` is forbidden in non-test code; `Mutex` poisoning specifically should use `into_inner()` recovery or `parking_lot::Mutex`.
  **Fix**: Either recover the lock (`self.next_version.lock().unwrap_or_else(|p| p.into_inner())`) consistently across all three locks, or switch to `parking_lot::Mutex` (no poisoning). The silent-drop branches on entries/ops should also at minimum emit a `tracing::error!`.

- [M5] LLM tool-call parameters dispatched without schema validation — `agentflow-agents/src/react/agent.rs:980,1863,1975` and `agentflow-agents/src/plan_execute.rs:317`
  **What**: When the LLM returns `tool_calls` (native) or a parsed `AgentResponse::Action`, the agent immediately calls `self.tools.execute(&tool, params)` with no JSON-schema validation against the tool's declared `parameters_schema()`. Each tool's `execute()` does its own ad-hoc validation (e.g. `HandoffTool::execute` checks for missing `to`/`message`), but there's no central guard.
  **Why it matters**: Tools written by third-party plugin authors may not validate exhaustively, leaving room for type-confusion bugs (e.g. an `i64` field arriving as a JSON string crashes a tool that does `.as_i64().unwrap()`). Per the audit prompt question C ("Tool call validation before dispatch (schema/allowlist)"), and per the closed-enum design philosophy of `AgentStepKind`, the runtime should fail fast with `ToolError::InvalidParams` before the wire crosses into tool code.
  **Fix**: Add a validation pass (e.g. `jsonschema::JSONSchema::compile(&tool.parameters_schema())` cached per tool) before `tools.execute(...)`. The policy / capability layer is already consulted at the same site, so the schema check fits there; emit `ToolCallCompleted { is_error: true }` with the validation error as content so the agent can self-correct.

### MINOR

- [m1] `common::BatchProcessor` uses `sem.acquire().await.unwrap()` — `agentflow-agents/src/common/batch_processor.rs:38,89`
  **What**: Both batch entry points unwrap the semaphore permit. `Semaphore::acquire` only errors when the semaphore is closed; the semaphore here is local to the call, so this is theoretically unreachable. Still, it's an `unwrap()` in non-test code and the project rule forbids that.
  **Fix**: Either `unwrap_or_else(|_| unreachable!("local semaphore cannot be closed"))` or drop the result via `let Ok(_permit) = sem.acquire().await else { return; };`.

- [m2] `common::StepFunPDFParser` unwraps `path.file_name()` and prints to stdout — `agentflow-agents/src/common/pdf_parser.rs:40,113-120,126`
  **What**: The PDF parser unwraps `path.file_name()` (would panic on a path ending in `..`), prints `println!` for progress instead of `tracing::info!`, and silently downgrades `content` / `token_count` to defaults via `unwrap_or("")` / `unwrap_or(0)`.
  **Fix**: Replace `path.file_name().unwrap()` with `.ok_or_else(|| "path has no filename")?`. Switch `println!` to `tracing::info!`. Surface the missing fields explicitly rather than substituting empty strings. This module is also a holdover from the old "agents/" applications layout (per `README.md`); consider moving the legacy `common::pdf_parser` / `common::output_formatter` / `common::file_utils` out of the library surface — they leak into `pub use common::*` from `lib.rs:22` and are unrelated to the L3 agent contract.

- [m3] `merge_resumed_result` and the equivalent in `supervisor/handoff.rs` / `supervisor/debate.rs` duplicate the `AgentEvent` enum match — `agentflow-agents/src/react/agent.rs:2330-2370`, `supervisor/handoff.rs:391-416`, `supervisor/debate.rs:370-395`
  **What**: Three near-identical `rewrite_event_step_index` / `merge_resumed_result` functions enumerate every variant of `AgentEvent` carrying a `step_index`. Adding a new variant requires touching three call sites; missing one causes step indices to silently disagree between supervisor and child runtime.
  **Fix**: Extract a single helper (e.g. `AgentEvent::step_index_mut(&mut self) -> Option<&mut usize>`) on the enum itself and use it from all three callers. The closed-enum design (`AgentStepKind` / `AgentEvent`) makes this a one-shot refactor; the regression risk grows every time a new step variant lands.

- [m4] `compact_memory_summary` truncates at byte boundary, not char boundary — `agentflow-agents/src/react/agent.rs:2378-2389`
  **What**: `content.truncate(160)` panics on multi-byte UTF-8 if byte 160 lands inside a character (e.g. Chinese or emoji content). The same risk exists in `info!(tool = %tool, "Observation: {}", &observation[..observation.len().min(200)])` on line 1145 and the batch path's line 2083.
  **Fix**: Use `content.char_indices().take_while(|(idx, _)| *idx < 160).last()` or `unicode_truncate` to clip safely. Pin a regression test using `let m = "汉字汉字汉字...".repeat(N);`.

- [m5] `AgentNode::execute` deserialises an untrusted `agent_result` input as `AgentRunResult` without size limits — `agentflow-agents/src/nodes/agent_node.rs:302-318`
  **What**: When a workflow resumes an `AgentNode`, the prior trace arrives as a `FlowValue::Json` value and is deserialised via `serde_json::from_value`. If the trace was tampered with (large `events` array, deeply nested JSON), the deserialiser will allocate without bounds and the agent then calls `restore_trace_memory` over the entire `steps` vector — replaying potentially expensive memory writes.
  **Fix**: Cap `steps.len()` and `events.len()` before deserialising, or validate against a known-size budget after parse. The harness HTTP entry-point on the server side has similar concerns — coordinate with the server crate to share a `MAX_AGENT_RESUME_BYTES` constant.

- [m6] `eval/dataset.rs::Dataset::load_from_dir` and `pricing.rs::PricingTable::load_from_yaml` parse user-supplied YAML/JSON without depth limits — verified by inspection; `eval/runner.rs:288` then drives the LLM with whatever it parsed.
  **Fix**: Document that eval datasets must be from a trusted source, or add `serde_yaml::Deserializer::from_str` with a depth limit. Low priority because eval is operator-facing.

- [m7] `RecentOnlyMemorySummary` / `CompactMemorySummary` produce summaries that include raw message content via `format!` — `react/agent.rs:99-104, 2378-2389`
  **What**: Memory summaries are injected back into the next LLM prompt as a `system` message. If the omitted messages contain PII / secrets (API keys, user PII pasted into the conversation), `CompactMemorySummary` includes verbatim 160-character slices of each. No redaction step.
  **Why it matters**: Audit prompt C asks about "PII leakage risk" in memory summary. Today the `agentflow-tracing` crate handles redaction at the trace layer (per CLAUDE.md), but the summary path bypasses tracing and feeds straight into a fresh LLM request. A leaked API key in a tool error gets re-sent to the model on every subsequent turn.
  **Fix**: Plumb the same redaction primitives (`agentflow_tracing::Redactor`) into the summary backends, or document explicitly that summaries are unredacted and callers should sanitise upstream.

- [m8] `agent_node.rs::AgentNode::execute` does not propagate `cancellation_token` from the surrounding workflow — `agentflow-agents/src/nodes/agent_node.rs:269-298`
  **What**: When `AgentNode` runs an embedded ReAct agent it constructs the `AgentContext` from the prior trace's session id and the `message` input, but never copies a workflow-level cancellation token in. The agent runs to completion regardless of the DAG's overall cancellation state.
  **Fix**: When `agentflow-core::Flow` exposes a per-run cancellation token, plumb it into `AgentContext::with_cancellation_token` here. Otherwise document that `AgentNode` is uncancellable from the DAG.

- [m9] `HandoffSignal` "most recent handoff request wins" silently overwrites prior requests — `agentflow-agents/src/supervisor/handoff.rs:63-71`
  **What**: If the active agent's LLM calls `handoff()` twice in one turn (e.g. via the H3 parallel batch path with two `handoff` calls in one batch), the second overwrites the first with no warning. The trace will show two `ToolCall { tool: "handoff" }` steps both succeeding, but only one transition happens.
  **Fix**: Detect and warn on multiple sets within one agent run; ideally reject the second with `ToolError::ExecutionFailed` so the agent sees the conflict.

- [m10] Deep `tokio::select!` ladders in ReAct (4 arms × 3 occurrences) are mechanically expanded — `agentflow-agents/src/react/agent.rs:666-755, 985-1135, 1869-1928, 1978-2053`
  **What**: The (timeout, cancel) combination is hand-expanded as a 4-arm `match` four separate times in the file. This is the source of ~700 of the 3,497 lines and makes audit harder: e.g. the timeout branches all hardcode `timeout_ms.unwrap_or_default()` (which would emit `Timeout { timeout_ms: 0 }` if the field were unexpectedly `None`).
  **Fix**: Extract a `select_with_limits(future, timeout, cancel)` helper returning `Result<T, RuntimeOutcome>` where `RuntimeOutcome` is one of `Timeout`, `Cancelled`. Mechanical, no behaviour change; collapses each call site to 3 lines.

- [m11] `traits/agent.rs::AgentApplication` is dead-code-adjacent — `agentflow-agents/src/traits/agent.rs`
  **What**: The trait set (`AgentApplication`, `FileAgent`, `BatchAgent`, `AgentConfig`) appears to be the legacy "agent applications" shape from the old `agents/` subdirectory, separate from `AgentRuntime`. It's re-exported via `pub use traits::*` (lib.rs:23) but no in-crate type implements it.
  **Fix**: Either delete (with grep confirming no downstream usage) or move to a `legacy` module clearly marked as not part of the L3 contract. Today it confuses the public API: a reader sees two unrelated "agent" trait families.

- [m12] Multi-agent recursion depth not bounded — supervisor implementations
  **What**: `HandoffSupervisor` bounds handoffs via `max_handoffs` (default 5). `DebateSupervisor` bounds rounds. But there is no global cap on supervisor-of-supervisor recursion: a `HandoffSupervisor` can register a sub-agent whose tool registry contains an `AgentTool` wrapping another `HandoffSupervisor`, leading to unbounded nesting. Each level multiplies token cost and step count.
  **Fix**: Add a `recursion_depth: usize` field to `AgentContext` (or propagate via `metadata`) and reject when it exceeds a configurable max. Pin a test that nests three levels and verifies the inner-most refusal.

### POSITIVE OBSERVATIONS

- **The H3 parallel-tool-call ordering contract is correctly implemented and pinned by tests.** `dispatch_native_tool_calls_batch` (`react/agent.rs:1701-2134`) splits the batch into `concurrent_idxs` (Idempotent) and `serial_idxs` (NonIdempotent / Unknown), emits `ToolPolicyDecision` / `ToolCapabilityDecision` / `ToolCallStarted` / `ToolCall` step rows in LLM-returned order *before* any execution begins (lines 1772-1837), runs the idempotent group via `futures::future::join_all` and the others serially in array order, then walks `prepared.iter().enumerate()` to emit completions and `ToolResult` step rows again in LLM order (lines 2070-2121). The CLAUDE.md claim is faithful to the code. Three tests pin the contract (`batch_path_runs_multiple_idempotent_tool_calls_in_order`, `batch_path_continues_when_one_tool_fails`, `batch_path_returns_cancelled_when_token_already_signalled`).

- **The `max_tool_calls` atomicity guard is correct**: the batch is rejected before any call runs if the batch would exceed the cap (`react/agent.rs:1720-1746`), so partial batches can't leak through.

- **AgentStepKind and AgentEvent are explicitly closed enums** with a comment forbidding local extension (`runtime.rs:286-291`). This is exactly what platform-stability tier "beta" promises.

- **Cancellation is checked at every blocking await** in `ReActAgent::run_with_context`: pre-LLM (lines 557, 647), inside the LLM call (`tokio::select!`), pre-tool-execute, inside tool-execute, and in the batch dispatcher. Pre-cancellation short-circuits before tool execution starts.

- **Zero TODO/FIXME/XXX/HACK markers** in non-test code (a single false positive in a doctest at `react/parser.rs:401` referencing `\uXXXX`).

- **Truncated-JSON recovery in `react/parser.rs`**: the `try_extract_answer_field` / `unescape_json_string_until_quote` path turns mid-string `max_tokens` truncation into a usable answer instead of dumping the JSON envelope on the user. This is exemplary defensive parsing with documented rationale (F-A2-1).

- **F-A2-13 anti-loop steering** (`react/agent.rs:527-540, 1186-1194`) detects when the LLM calls the same tool with identical params twice in a row and appends a steering note *only* to the LLM's working memory, not the trace. The trace stays faithful while the model gets nudged toward progress.

- **The `MemorySummaryBackend` and `ReflectionStrategy` traits** are tight, single-method, return `Result<Option<...>, ...>` (so backends can opt out without erroring), and have well-named built-in implementations.

- **Three supervisor implementations all implement `AgentRuntime`**, so they compose uniformly with `AgentNode` / `AgentTool` and reuse the closed-enum trace contract.

- **Resume contract (`AgentNodeResumeContract`) is comprehensive**: `from_result_with_tools` consults registry idempotency, params hints, and explicitly documents the precedence (`agent_node.rs:142-187`). Five integration tests in `tests/agent_node_resume_contract.rs` pin every branch.

- **Golden-fixture tests** (`tests/agent_runtime_golden.rs`, `tests/agent_trace_compat.rs`) round-trip the full `AgentRunResult` against on-disk JSON, catching any accidental shape change.

## Metrics

- Source files: 30 (under `src/`)
- Lines of code: 14,051 (includes tests embedded in `#[cfg(test)] mod tests` blocks)
- Largest single file: `src/react/agent.rs` at 3,497 LOC (~50% tests by line count)
- Agent runtimes implemented: 5 (`ReActAgent`, `PlanExecuteAgent`, `HandoffSupervisor`, `BlackboardSupervisor`, `DebateSupervisor`)
- Supervisors implemented: 3 (Handoff, Blackboard, Debate)
- Reflection strategies: 3 (`NoOpReflection`, `FailureReflection`, `FinalReflection`)
- Memory backends: 2 (`RecentOnlyMemorySummary`, `CompactMemorySummary`) plus the pluggable `MemorySummaryBackend` trait
- Test files: ~9 inline `#[cfg(test)] mod tests` in src + 5 integration files in `tests/` (`agent_node_resume_contract.rs`, `agent_runtime_golden.rs`, `agent_trace_compat.rs`, `prompt_assembly_benchmarks.rs`, `prompt_assembly_golden.rs`)
- `unwrap()/expect()` in non-test code: 7 confirmed (top: `react/agent.rs:2073 .expect("every prepared call must have an output by this point")`; `supervisor/blackboard.rs:102 .expect("blackboard version poisoned")`; `common/batch_processor.rs:38,89 .unwrap()` on `sem.acquire`; `common/pdf_parser.rs:40,126 path.file_name().unwrap()`)
- TODO/FIXME/XXX/HACK markers: 0 (one false positive in a doctest)
- Public items missing rustdoc: small. Most `pub` types in `runtime.rs`, `react/agent.rs`, `reflection.rs`, `supervisor/{handoff,blackboard,debate}.rs`, `nodes/agent_node.rs` carry `///` docs. The `traits/agent.rs` legacy traits (`AgentApplication` and friends) are documented but stale. `common/*` (legacy app utilities) carry sparse rustdoc.
- Sleep / real-LLM tests: zero `tokio::time::sleep` in tests, zero real-LLM calls in unit/integration tests (mock provider via `AGENTFLOW_MOCK_RESPONSES` env var); test serialisation via `crate::LLM_TEST_LOCK` (`lib.rs:63`).

## Recommendations (prioritized)

1. **Fix M1** (replace `.expect()` in batch finaliser with `unreachable!` or refactor) — purely cosmetic but it's the only `expect` on the hot agent loop and violates project rules.
2. **Fix M3** (plug `token_budget` into `PlanExecuteAgent`) — silently dropped contract; low effort, mirrors ReAct.
3. **Fix M4** (`Blackboard` mutex poisoning) — single-line `.unwrap_or_else(|p| p.into_inner())` or switch to `parking_lot`. Avoids panic-cascade on parallel-mode supervisor errors.
4. **Decide on M2** (cancellation semantics for in-flight tool calls) — either document the cooperative model explicitly in `AgentRuntime` rustdoc, or add a per-call cancellation token to `Tool::execute` (breaking change in `agentflow-tools`).
5. **Address M5** (LLM-emitted tool params not schema-validated) — implement a cached `jsonschema::JSONSchema` validator alongside the existing `evaluate_policy` / `evaluate_capabilities` gates. This is the only missing layer before tool dispatch.
6. **Refactor m3** (`AgentEvent::step_index_mut`) — one-shot dedup that defuses a class of future bugs as new step variants land.
7. **Refactor m10** (extract `select_with_limits` helper) — pure cleanup; deletes ~600 lines of structurally-identical match arms in `react/agent.rs`.
8. **Document or delete m11** (legacy `traits/agent.rs::AgentApplication`) — currently a public-API distraction.
9. **Add m12** (multi-agent recursion depth cap) — defensive guard; the contract already has `metadata: Value` which can carry the counter.
10. **Address m7** (PII leakage in memory summary) — share redactor primitives with `agentflow-tracing` so summaries pass through the same scrubber as on-disk traces.
11. **Move m2 / `common/*` legacy module** out of the public surface or into a `legacy` submodule clearly marked as not part of L3 stability.

End of report.

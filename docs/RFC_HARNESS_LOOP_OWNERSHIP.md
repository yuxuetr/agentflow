# RFC: Harness Loop Ownership + Context Engineering

- Status: **Draft** (design only; not promoted to `TODOs.md`)
- Author: (proposed)
- Created: 2026-06-11
- Related: `docs/HARNESS_MODE.md`, `docs/ROADMAP_v2.md` §F, `docs/STABILITY.md`,
  `docs/archive/PROJECT_EVALUATION_2026-06-06.md` §3.10
- Affected crates: `agentflow-harness` (primary), `agentflow-agents` (contract
  change), `agentflow-server` (wiring), `agentflow-llm` (tokenizer reuse)

## TL;DR

`agentflow-harness` today is a **governance + post-hoc translation shell** around
an agent loop it does not own. It nails the control/safety half of harness
engineering (approval, hooks, audit, redaction, fail-closed) but delegates the
two hardest pillars — **owning the loop** and **context engineering** — to the
inner `AgentRuntime`, or does not do them at all.

This RFC proposes a **three-phase** evolution to close that gap without throwing
away the "wrap any `AgentRuntime`" composability that is currently a strength:

- **Phase 0 — Context hygiene (no contract change).** Use the real tokenizer,
  fix the greedy trim, truncate `params_summary`. Immediate quality, zero blast
  radius.
- **Phase 1 — Live event seam (additive contract change).** Give the inner
  agent an optional live `AgentEventSink` so the harness *observes the loop in
  real time*. Deletes post-hoc translation, kills the split-brain seq epochs,
  and lets the harness finally emit the `MemorySummaryAdded` event it has always
  advertised but never produced.
- **Phase 2 — Turn-driven control (larger contract change).** Let the harness
  *drive the loop turn-by-turn* so it can own context assembly, compaction, and
  re-injection between turns — i.e. genuinely **take over context engineering**.

Phase 1 is the enabling step (observe before you steer). Phase 2 is the
end-state the title asks for. Each phase ships and delivers value on its own.

## 1. Background: where we are today

### 1.1 The harness does not own the loop

`HarnessRuntime::run` delegates the entire agentic loop to the inner runtime in a
single opaque `await`, then reconstructs structured events *after the fact*:

```
// agentflow-harness/src/runtime.rs:354
let inner_result = self.inner.run(agent_context).await?;
// :360
let translated = translate_inner_events(&inner_result, &session_id, &self.seq_counter);
```

`translate_inner_events` (`runtime.rs:469`) walks `result.steps` / `result.events`
*after* the loop has finished and emits `step_started` / `tool_call_requested` /
`tool_call_completed` in a batch.

Meanwhile the **governance** events (`approval_requested` / `approval_decided`)
fire **live**, during execution, from inside `HookedTool::execute`
(`hooks_runtime.rs:688`). Both paths share one `Arc<AtomicU64>` seq counter, so
`seq` is monotonic *by emission order* — but the two halves of a single tool call
now live in **different seq epochs**: the approval (live, low seq) and the
matching `tool_call_completed` (post-hoc, high seq) can be dozens of events
apart. For a `stream-json` / SSE consumer, the operator sees approvals stream
live and then a burst of "here is what happened" at the very end. The `step_index`
on the two paths even comes from two different counters
(`HookConfig::step_index_counter` vs the inner agent's `step.index`), so
correlating an approval to its step is not guaranteed.

The root cause is the contract: `AgentRuntime::run` is the only seam, and it is
**run-to-completion**:

```
// agentflow-agents/src/runtime.rs:658
pub trait AgentRuntime: Send {
  async fn run(&mut self, context: AgentContext) -> Result<AgentRunResult, AgentRuntimeError>;
  fn runtime_name(&self) -> &'static str;
}
```

`AgentContext` (`runtime.rs:59`) carries no event sink and no per-turn callback.
The harness is structurally blind to the loop while it runs.

### 1.2 Context engineering is assemble-once

- Context is collected **once**, before the loop, into a static persona string
  (`runtime.rs:301-304`), and never refreshed.
- Budget trimming is a **greedy single-knapsack**: sort by priority, drop whole
  items that do not fit (`trim_to_budget`, `runtime.rs:415`). An oversized
  `Critical` item is dropped silently while a small `Low` item may be admitted —
  a priority inversion. No per-item truncation, no summary of dropped items.
- Token cost is a `chars / 4` heuristic (`providers.rs:31`) even though
  `agentflow-llm` ships a real tokenizer
  (`agentflow_llm::tokenizer::counter_for_model`, `tokenizer.rs:209`).
- **No compaction.** The `MemorySummaryAdded` variant exists in the frozen
  envelope (`event.rs:87`) but the runtime **never emits it**. Long-horizon
  window management lives inside `ReActAgent` (`MemorySummaryBackend` +
  `apply_memory_prompt_budget`, `react/agent.rs:82`) and is invisible to the
  harness. The harness advertises a capability it does not provide.

### 1.3 What is already good (and must be preserved)

The approval / hook / audit machinery (`hooks_runtime.rs`) is A-grade and must
survive every phase **unchanged in behavior**:

- decision merge `Deny > RequireApproval > Allow`, production auto-escalation of
  `NonIdempotent` calls, scoped approval caching, `DenyAndStop` short-circuit,
  synthetic audit events for cached / gated decisions, redaction on every
  emission path.

This RFC changes *who drives the loop and assembles context*; it does **not**
touch the governance pipeline's semantics.

## 2. Goals / Non-goals

### Goals

1. The harness observes the loop **live**, on one coherent event clock (closes
   §1.1 split-brain).
2. The harness **owns the context window**: real token accounting, sane trim,
   and **mid-loop compaction** under its own policy (closes §1.2).
3. The harness emits the `MemorySummaryAdded` event it already promises.
4. Preserve the governance pipeline behavior bit-for-bit.
5. Preserve "wrap any `AgentRuntime`" composability: runtimes that do not opt
   into the new seams keep working through the existing `run()` path.

### Non-goals

- **Rewriting the agent loop inside the harness** (the "Option A" below). We do
  not duplicate ReAct / Plan-Execute / Supervisor logic.
- Breaking the frozen `HarnessEvent` wire shape. Phase 1 changes emission
  *timing/order*, not the schema (this is an improvement to the
  "seq monotonic, never gaps" promise, documented as a behavioral note).
- Mid-loop **user steering** (injecting new instructions while the agent runs).
  Phase 2 makes it *possible*; the steering UX itself is a follow-up RFC.
- Subprocess / WASM isolation for sub-agents (separate track, `ROADMAP_v2.md` §G).

## 3. Design options considered

| Option | Idea | Verdict |
|---|---|---|
| **A. Harness owns a brand-new loop** | Reimplement the agentic loop in `agentflow-harness` directly against `LLMClient` + `ToolRegistry` + `MemoryStore`. | **Rejected.** Duplicates `agentflow-agents`, discards ReAct/PlanExecute/Supervisors, max blast radius, destroys composability. |
| **B. Live event seam (observe)** | Keep `AgentRuntime::run`; add an optional `AgentEventSink` to `AgentContext` so the inner agent streams `AgentEvent`s as they happen. | **Accepted as Phase 1.** Additive, low-risk; fixes the epoch split and unblocks streaming. Observe-only — does not give the harness control. |
| **C. Turn-driven control (own)** | Add a turn-boundary contract so the harness calls the inner agent one turn at a time and interleaves its own context engineering between turns. | **Accepted as Phase 2.** The real "own the loop + own context." Bigger contract change; do it after B proves the seam. |

Phased B→C lets each step ship independently and de-risks the contract change.

## 4. Phase 0 — Context hygiene (no contract change)

Standalone, ships first, buys immediate quality.

1. **Real tokenizer.** Replace `estimate_tokens` (`providers.rs:33`) and the
   budget math in `trim_to_budget` with
   `agentflow_llm::tokenizer::counter_for_model(&ctx.model)`. Keep a heuristic
   fallback for unknown models (the tokenizer crate already provides
   `HeuristicCounter`).
2. **Trim that respects priority.** Rework `trim_to_budget` (`runtime.rs:415`):
   - admit by ascending priority;
   - when an item does not fit, **truncate-to-fit with a marker** instead of
     dropping outright, down to a per-item floor; only drop below the floor;
   - record `context_items_truncated` alongside `context_items_dropped` in
     `HarnessRunResult` so callers can see what happened.
3. **Honor the `truncated` contract.** `ToolCallRequestedPayload.params_summary`
   is documented as "redacted/**truncated**" (`event.rs:142`) but only redaction
   happens. Add a size cap with a truncation marker in
   `tool_call_requested_from_step` (`runtime.rs:531`) and the hook path
   (`hooks_runtime.rs:521`).

Acceptance: unit tests for tokenizer-backed budgeting; an oversized-Critical-item
test that asserts truncation-not-drop; a `params_summary` cap test.

## 5. Phase 1 — Live event seam (additive contract change)

### 5.1 New trait + additive `AgentContext` field

```rust
// agentflow-agents/src/runtime.rs
#[async_trait]
pub trait AgentEventSink: Send + Sync {
  /// Called for every AgentEvent at the moment it is produced, before the
  /// loop continues. Implementations must be cheap and non-blocking-ish;
  /// the runtime awaits this inline. Errors are swallowed (observability
  /// must never break execution).
  async fn emit(&self, event: &AgentEvent);
}

pub struct AgentContext {
  // ... existing fields ...
  /// Optional live event observer. `None` keeps today's behavior exactly
  /// (events are only accumulated into AgentRunResult). Additive +
  /// #[serde(skip)] so the wire shape and all existing callers are
  /// unchanged.
  #[serde(skip)]
  pub event_sink: Option<Arc<dyn AgentEventSink>>,
}
```

A builder `AgentContext::with_event_sink(Arc<dyn AgentEventSink>)` mirrors the
existing `with_cancellation_token` pattern.

### 5.2 `ReActAgent` emits live

Today the loop pushes to a local `events` vec (`react/agent.rs:763, 915, 941,
955, ...`). Introduce a single helper and route every push through it:

```rust
async fn record_event(sink: &Option<Arc<dyn AgentEventSink>>,
                      events: &mut Vec<AgentEvent>, ev: AgentEvent) {
  if let Some(sink) = sink { sink.emit(&ev).await; }
  events.push(ev);
}
```

Back-compat is exact: with `sink = None` the behavior is identical to today
(accumulate-only). `AgentRunResult` still carries the full `events` / `steps`
vecs for trace replay and for runtimes that have not adopted the seam.

Apply the same pattern to `PlanExecuteAgent` and the three supervisors as a
fast-follow; they keep working untouched until then because the field defaults to
`None`.

### 5.3 New `AgentEvent` for memory summaries

`ReActAgent` already produces a summary via `MemorySummaryBackend` when prompt
memory exceeds budget (`apply_memory_prompt_budget`). Emit it:

```rust
// agentflow-agents AgentEvent (additive variant)
MemorySummaryAdded { session_id, layer: String, summary: String, token_estimate: usize, timestamp }
```

### 5.4 Harness bridge replaces post-hoc translation

Add `HarnessAgentEventBridge` in `agentflow-harness`: an `AgentEventSink` that
maps each live `AgentEvent` → `HarnessEvent` using the *existing* translation
logic, dispatches through the `SinkChain` with the shared `seq_counter`, and
redacts `params_summary` exactly as `translate_inner_events` does today.

Then:

- `HarnessRuntime::run` builds the bridge, calls
  `agent_context.with_event_sink(bridge)`, runs the inner agent, and **deletes
  `translate_inner_events`** (`runtime.rs:469-529`).
- `step_started` / `tool_call_requested` / `tool_call_completed` now interleave
  **live** with `approval_requested` / `approval_decided` on the **same
  monotonic clock**. The split-brain epoch (§1.1) is gone.
- The dual `step_index` numbering dissolves: both governance and structure events
  now reference the live agent step stream. (The hook layer keeps its counter for
  the approval-cache key, but the audit-facing `step_index` is reconciled to the
  agent step.)
- `MemorySummaryAdded` finally flows end-to-end → maps to
  `HarnessEventBody::MemorySummaryAdded`.

### 5.5 Server wiring

`agentflow-server/src/harness_live.rs:404-422` already builds the runtime with a
shared seq counter and wraps the registry. It gains one line: the bridge is
constructed against the same `SinkChain` + `seq_counter` and threaded via
`HarnessRunOptions`. No route or DB change.

### 5.6 Stability impact

The `HarnessEvent` wire schema is unchanged. Emission **order/timing** changes:
event `seq` now reflects true logical order rather than "governance-then-batch."
This is a strict improvement to the documented "seq monotonic, never gaps"
promise, but it is observable, so it gets a `docs/STABILITY.md` migration note.
The `envelope_contract.rs` fixture test is extended to assert *logical*
ordering (an `approval_decided` for a call precedes that call's
`tool_call_completed`).

## 6. Phase 2 — Turn-driven control (the "own the loop" end-state)

Phase 1 makes the harness *see* the loop. Phase 2 makes the harness *drive* it,
which is the prerequisite for the harness owning context engineering.

### 6.1 Turn-boundary contract

```rust
// agentflow-agents
#[async_trait]
pub trait TurnDrivenRuntime: Send {
  /// Initialize a run and return a session the caller pumps one turn at a
  /// time. The inner agent owns per-turn LLM + tool mechanics; the caller
  /// owns what surrounds each turn (context window, stop conditions).
  async fn begin(&mut self, context: AgentContext)
    -> Result<Box<dyn LoopSession>, AgentRuntimeError>;
}

#[async_trait]
pub trait LoopSession: Send {
  /// Advance exactly one agent turn (one observe→plan→act cycle).
  async fn next_turn(&mut self) -> Result<TurnOutcome, AgentRuntimeError>;
  /// Mutable access to the run's memory so the driver can compact between
  /// turns. Returns the same MemoryStore the inner agent reads on the next
  /// turn, so a harness-applied summary is visible downstream.
  fn memory(&mut self) -> &mut dyn MemoryStore;
  fn finish(self: Box<Self>) -> AgentRunResult;
}

pub enum TurnOutcome {
  Continued,                       // produced a step, loop should continue
  FinalAnswer(String),
  Stopped(AgentStopReason),
}
```

`ReActAgent`'s monolithic `loop {}` (`react/agent.rs:558`) is refactored so one
iteration of the loop becomes `next_turn`. The existing `run_with_context`
becomes a thin driver over the turn API, so **`AgentRuntime::run` keeps working
unchanged** and runtimes that do not implement `TurnDrivenRuntime` (PlanExecute,
supervisors) stay on the old path. Blast radius is contained to `ReActAgent`'s
internal loop structure; its per-turn logic is unchanged.

### 6.2 Harness becomes the loop owner

```rust
// agentflow-harness, conceptual
let mut session = inner.begin(agent_context).await?;
loop {
  // HARNESS owns the context window now:
  self.compact_if_over_budget(session.memory()).await?;   // emits MemorySummaryAdded
  self.refresh_changed_context(&ctx, session.memory()).await?; // re-run providers whose inputs changed
  match session.next_turn().await? {
    TurnOutcome::FinalAnswer(a) => { answer = Some(a); break; }
    TurnOutcome::Stopped(r)     => { stop = r; break; }
    TurnOutcome::Continued      => {}
  }
  if let Some(r) = self.harness_stop_condition() { stop = r; break; } // harness-level limits
}
let inner_result = session.finish();
```

This is where the harness **takes over context engineering**:

- **Compaction it controls**: project the next-turn window with the real
  tokenizer; when over threshold, summarize the oldest turns via a
  harness-owned `MemorySummaryBackend`, write the summary back through
  `session.memory()`, and emit `MemorySummaryAdded`. This is the long-advertised
  capability, now actually owned by the harness rather than buried in the agent.
- **Context refresh**: re-run providers whose inputs changed (file mtimes, git
  HEAD) and inject deltas — the loop can react to a workspace that changed
  mid-run.
- **Harness-level stop conditions** layered on top of the agent's own
  (`RuntimeLimits`): a harness wall-clock budget, a context-growth ceiling, etc.

Governance is unaffected: tool calls still flow through the `HookedTool`-wrapped
registry inside `next_turn`.

### 6.3 Why not do 6.x in Phase 1

Mid-loop compaction and refresh require sitting *between* turns with mutable
access to memory. That is exactly what the turn boundary provides and what the
single opaque `run()` cannot. Hence the ordering: Phase 1 (observe) is necessary
but not sufficient; Phase 2 (drive) is the part that satisfies "take over context
engineering."

## 7. Mapping back to the four pillars

From the harness-engineering rubric used in the evaluation:

| Pillar | Today | After this RFC |
|---|---|---|
| 1. Loop ownership | C+ (post-hoc shell) | **A-** (Phase 2 drives turn-by-turn) |
| 2. Context engineering | C (assemble-once) | **A-** (Phase 0 accounting + Phase 2 compaction/refresh) |
| 3. Tool/action interface | A- | A- (unchanged) |
| 4. Control & safety | A | A (governance preserved bit-for-bit) |

## 8. Risks & mitigations

- **Contract creep in `agentflow-agents`.** Mitigation: Phase 1 is one additive
  `#[serde(skip)] Option` field + one trait; Phase 2's `TurnDrivenRuntime` is
  opt-in, with `run()` retained as a wrapper. No existing caller breaks.
- **Performance of inline `emit`.** One extra `await` per event. Sinks are
  already async and bounded; negligible vs an LLM round-trip. Bench with
  `bench-gate` if needed.
- **Compaction correctness** (summarizing away load-bearing context). Mitigation:
  never compact `Critical`-priority items or the active turn; summaries are
  additive system messages, not deletions, mirroring `MemorySummaryBackend`
  today.
- **Behavioral-order change** tripping a downstream consumer that relied on the
  batch-at-end ordering. Mitigation: `STABILITY.md` note + extend the fixture
  contract test; the new order is the *intended* one the schema always implied.

## 9. Rollout plan

1. **Phase 0** — one PR in `agentflow-harness` (+ `providers.rs`). No contract
   change. Ship immediately.
2. **Phase 1** — `agentflow-agents` additive seam + `ReActAgent` emit +
   `MemorySummaryAdded` variant; `agentflow-harness` bridge; delete
   `translate_inner_events`; `agentflow-server` one-line wiring. Fast-follow:
   PlanExecute / supervisors adopt `record_event`.
3. **Phase 2** — `TurnDrivenRuntime` / `LoopSession` traits; refactor
   `ReActAgent` loop to a turn driver; harness loop owner + compaction + refresh.
   Own PR series; promote to a `P11.x` segment in `TODOs.md` before starting.

## 10. Acceptance criteria

- Phase 0: tokenizer-backed budgeting; truncate-not-drop for oversized Critical;
  `params_summary` capped.
- Phase 1: `translate_inner_events` removed; a test asserts `approval_decided`
  precedes its `tool_call_completed` on the seq clock; `MemorySummaryAdded`
  emitted when budget forces a summary; `runtime_react_smoke` + `envelope_contract`
  green.
- Phase 2: a long synthetic session compacts mid-loop and keeps the projected
  window under budget while still producing a final answer; a context-refresh
  test observes a mid-run file change reflected in a later turn; governance
  tests in `hooks_runtime.rs` unchanged and green.

## 11. Open questions

1. Should `MemorySummaryAdded` carry a structured "what was summarized" range
   (step indices) for replay fidelity, or stay a plain summary string?
2. In Phase 2, do supervisors (handoff/blackboard/debate) get their own
   `TurnDrivenRuntime` impls, or stay on the opaque `run()` path indefinitely?
   (Recommendation: stay opaque until a concrete need; they are coarser-grained.)
3. Does context refresh re-run *all* providers each turn (simple, costs IO) or
   only those declaring a cheap `changed()` probe (faster, more trait surface)?

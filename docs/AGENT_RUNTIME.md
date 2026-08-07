# Agent Runtime

Agent Runtime is the agent-native execution boundary in AgentFlow. It sits next
to the existing DAG `Flow` runtime and reuses the same lower-level capabilities:
tools, skills, memory, model calls, and tracing.

## Runtime Boundary

`agentflow-core::Flow` remains the deterministic DAG runtime. It owns node
ordering, workflow state, retries, checkpoints, and node-level recovery.

`agentflow-agents::AgentRuntime` owns autonomous loop execution. It records
observations, plans, tool calls, tool results, reflections, final answers, and
agent stop reasons.

The boundary is:

- DAG `Flow` decides when an agent runs.
- `AgentRuntime` decides how an agent reaches an answer.
- `ToolRegistry` is shared by both DAG and agent-native execution.
- Skills build agent configuration, memory, and tools, but do not own the loop.
- MCP servers are adapted into `ToolRegistry`; agents call them as normal tools.
- Checkpointing belongs to `Flow`; agent step history is the serializable state
  a future `AgentNode` can checkpoint.

## Core Types

- `AgentContext`: per-run input, session, model, persona, limits, metadata,
  and an optional cancellation token.
- `AgentStep`: durable step history for observe, plan, tool call, tool result,
  reflection, verification, and final answer.
- `AgentEvent`: runtime event stream for run start/stop, tool calls,
  reflection, and verification.
- `AgentStopReason`: structured stop reason for final answer, stop condition,
  max steps, max tool calls, timeout, cancellation, token budget, cost budget
  (T1.1), loop detection, or error.
- `AgentRunResult`: final runtime output containing answer, stop reason, steps,
  and events.
- `AgentCancellationToken`: shared shutdown signal that can stop an active
  agent run through `AgentContext`.
- `ReflectionStrategy`: optional pluggable reflection hook.
- `VerificationStrategy`: optional pluggable gate on a candidate final answer;
  unlike `ReflectionStrategy` it can reject the answer and send the loop back
  around for another attempt.
- `AgentMemoryHook`: optional observer for memory reads, searches, and writes.

## ReAct Runtime

The existing `ReActAgent` now implements `AgentRuntime` while keeping the
legacy `run(&str) -> Result<String, ReActError>` API.

`run_with_context` returns `AgentRunResult` and records:

- observe step for user input.
- plan step for model thought.
- tool call and tool result steps.
- final answer step.
- optional reflection step.
- tool and reflection events.

Runtime guards cover max steps, max tool calls, global timeout, token budget,
stop conditions, and (T1.1) a USD cost budget (`RuntimeLimits::cost_limit_usd`
/ `ReActConfig::cost_limit_usd`, checked at the top of each turn against
cumulative spend estimated from `ReActConfig::pricing_table` —
`agentflow-agents::eval::pricing::PricingTable`, the same table the eval
harness uses. The table defaults to all-zero prices, so the guard is inert
until configured with real per-model rates). `PlanExecuteAgent` enforces the
same cost budget around its single planner call via the matching
`PlanExecuteConfig` fields.

Runs can be cancelled by passing
`AgentContext::with_cancellation_token(AgentCancellationToken::new())` and
calling `cancel()` from another task. `ReActAgent` checks the token at loop
boundaries and while awaiting LLM/tool futures, returning
`AgentStopReason::Cancelled` with a `RunStopped` event.

Reflection remains opt-in through `with_reflection_strategy(...)` and can be
disabled at runtime configuration level with
`ReActConfig::with_reflection_enabled(false)`. When disabled, no `Reflect` step
or `ReflectionAdded` event is recorded even if a strategy is attached.

Verification is opt-in through `with_verification_strategy(...)` and gates the
loop rather than just observing it: after a candidate final answer is recorded
(and reflected on, if a `ReflectionStrategy` is attached), the strategy's
verdict decides whether the run actually stops. `VerificationOutcome::Rejected
{ feedback }` records a `Verify` step (`approved: false`) and a
`VerificationCompleted` event, feeds `feedback` into memory as the next
observation (the same mechanism used for tool results), and loops the agent
for another attempt. Attempts are bounded by
`ReActConfig::with_max_verification_attempts(...)` (default `2`); exhausting
the bound force-accepts the candidate answer instead of erroring, so a
strategy that never approves cannot hang a run. Disabling verification with
`ReActConfig::with_verification_enabled(false)` skips the gate entirely, even
with a strategy attached — no `Verify` step or event is recorded.

**Structured output (V2.1).** `ReActConfig::with_output_schema(schema)`
requires the final answer to validate against a caller-supplied JSON Schema.
When set, `collect_tool_specs` additionally offers a synthetic `final_answer`
tool (`react::agent::FINAL_ANSWER_TOOL_NAME`) alongside the agent's real
tools, whose `input_schema` is the caller's schema — providers with native
tool calling (all six supported today) enforce the shape directly there
rather than relying on prompt-only constraint; calling it is recognised as
the agent's final answer, not a real tool dispatch. This is deliberately
*not* built on `LLMClient::json_schema(...)`/`response_format`: that
mechanism can't coexist with the agent's own `tools` on Anthropic/Google (see
`docs/LLM_PROVIDERS_MATRIX.md`), and even where it can, a top-level
`response_format` schema can't represent ReAct's inherent "either take an
action or give this answer" polymorphism — native tool calling already
solves exactly that.

A candidate answer that fails to parse as JSON or fails schema validation is
rejected: the validation errors are fed back into memory as the next
observation and the loop continues for another attempt, mirroring
verification's retry shape but tracked by its own budget
(`ReActConfig::with_max_schema_correction_attempts(...)`, default `2`,
independent of `max_verification_attempts`). Unlike verification rejection,
which force-accepts once its budget is exhausted, exhausting the schema
budget is a hard `ReActError::SchemaValidationFailed` — a schema is a
caller-declared hard contract, not an advisory critique, so returning
non-conformant output labelled "final" would silently break that contract.
Both the schema gate and verification reuse the existing
`AgentStepKind::Verify`/`AgentEvent::VerificationCompleted` shapes (no new
wire variants); the gate runs before verification, since there is no reason
to run a domain verification strategy against an answer that does not even
conform to the caller's declared shape.

**Token-level streaming (V2.2).** Every `ReActAgent` LLM call now streams
(`LLMClientBuilder::execute_streaming_collected`) rather than calling
`execute_full`, so a provider configured `requires_streaming: true` can
actually be used, and every chunk is forwarded live as
`AgentEvent::TokenDelta { session_id, step_index, delta, is_final,
timestamp }` through `self.live_sink`. `delta` is whatever text the
provider streamed for that chunk verbatim — for a ReAct turn this is a
fragment of the turn's JSON envelope (or a native tool-call's arguments),
not necessarily clean prose; extracting a human-readable partial answer
would need a genuine incremental JSON parser, which is out of scope here.
`TokenDelta` is deliberately *not* accumulated into `AgentRunResult.events`
(unlike every other `AgentEvent` variant) — a per-token event embedded in
every stored trace or checkpoint forever would bloat them for a signal
whose entire value is being observed live; only a `live_sink` attached via
`AgentContext::with_event_sink(...)` ever sees it.
`collect_streaming_response` (`agentflow-llm`) reconstructs the same
`LLMResponse` shape `execute_full` used to hand back (content
concatenation, `tool_calls` reassembled from `tool_call_deltas` grouped by
index, best-effort `stop_reason`/`usage`), so tool-call dispatch, JSON
parsing, and usage/cost accounting downstream of the call are unchanged.
`agentflow-harness`'s `HarnessAgentEventBridge` translates the live stream
into the `token_delta` `HarnessEvent` kind (see `docs/HARNESS_MODE.md`);
`agentflow harness chat` / `run --output text` and the Web UI render it as
an in-place typing indicator.

**Agent-loop-level persistent checkpoint (V2.4).** Distinct from the
DAG-level `Flow` checkpoint (`agentflow-core`), which treats an embedded
agent run as one opaque, all-or-nothing unit — and distinct from
`resume_with_context`'s post-hoc unresolved-tool-call replay, which
restores conversation memory and starts an entirely fresh loop. The
contract (`agentflow_agent_spi::checkpoint::AgentLoopCheckpoint` +
`AgentLoopCheckpointer`) captures a snapshot of in-flight loop progress —
recorded `steps`/`events`, step/iteration counters, tool-call history,
verification/schema-retry counters (ReAct), or the frozen plan +
step cursor (PlanExecute) — after every completed turn (ReAct) or plan
step (PlanExecute), when a checkpointer is attached via
`AgentContext::with_loop_checkpointer(...)`. Process-local fields
(cancellation token, turn-start clock, between-turn hook) and run
*configuration* (limits, model) are deliberately excluded and
reconstructed fresh from the resuming call's `AgentContext` — a resume
can legitimately be handed different limits than the original run.

`ReActAgent::resume_from_loop_checkpoint(context, checkpoint, answer)`
and `PlanExecuteAgent::resume_from_loop_checkpoint(context, checkpoint,
answer)` splice the restored state directly back into the turn loop /
execute loop instead of restarting: ReAct re-enters `run_one_turn` at
the checkpointed step (still needs the LLM for remaining turns);
PlanExecute needs no further LLM call at all — the plan was already
frozen into the checkpoint — and just resumes tool execution at
`plan_position`. (The `answer` parameter is V2.3 — see below; it is
`None` for every checkpoint that isn't paused on a question.)
`should_clear_checkpoint` (shared by both runtimes,
`agentflow-agents::checkpoint`) is an exhaustive match over
`AgentStopReason`: checkpoints clear on genuine completion
(`FinalAnswer`/`StopCondition`/`Error`) and survive every other stop
reason, including the case this feature targets — the process dies with
no stop reason produced at all, so the last-written checkpoint simply
sits on disk untouched.

`agentflow-agents::FileLoopCheckpointer` is the concrete file-based
implementation (atomic write-then-rename, one JSON file per session,
sibling to `agentflow_harness::default_session_dir`'s convention).
`agentflow-harness`'s `HarnessRunOptions::with_loop_checkpointer(...)`
forwards a checkpointer into the inner agent's context; `agentflow
harness run` attaches one by default whenever a run-dir is available.
`agentflow harness resume-loop <session_id>` (distinct from `resume`,
which only re-prints the persisted JSONL event log) rebuilds the agent
and calls `resume_from_loop_checkpoint` to genuinely continue execution
— a minimal CLI surface; full `HarnessRuntime`-level resume wiring
(live event stream, approval-gate re-wrapping) is `HarnessRuntime::
resume_from_interrupt`, part of V2.3 below.

**General HITL interrupt/resume (V2.3).** Built on top of the V2.4
checkpoint machinery above: the agent asks the user a question mid-run,
the loop pauses (`AgentStopReason::AwaitingInput { question }`), and
once the user replies the run resumes carrying their answer — distinct
from the approval mechanism (`agentflow-harness`'s hook pipeline), which
gates whether one specific tool call proceeds rather than pausing the
whole loop for an open-ended answer. `ReActAgent` exposes this as an
always-registered synthetic tool, `ASK_USER_TOOL_NAME = "ask_user"`
(mirrors `final_answer`'s interception mechanism — unconditionally
registered, unlike `final_answer`, since `ask_user` needs no
`output_schema` to be safe to expose). `PlanExecuteAgent` has no
mid-loop LLM re-entry point at all, so it treats the same name as a
reserved pseudo-tool intercepted inside a plan step, before real tool
dispatch, in both `run_plan_execute_loop` and
`resume_from_loop_checkpoint` (the step loop body is duplicated across
the two).

The checkpoint saved at the interception point is explicit, not
inherited from the ordinary per-turn/per-step save above — that save is
one full turn/step stale by the time `ask_user` fires and would not yet
carry the question — and sets the new `AgentLoopCheckpoint.
pending_question: Option<String>` field (`None` for every other stop
reason). `resume_from_loop_checkpoint`'s `answer: Option<String>`
parameter is validated in both directions: a checkpoint with
`pending_question: Some(_)` requires `answer: Some(_)` and vice versa,
mismatches are a hard `InvalidCheckpoint` error. ReAct writes the answer
back into memory as a user message before re-entering the turn loop;
PlanExecute treats it as the paused step's synthetic tool result,
pushed into `observations` exactly like a real tool's output would be,
and resumes at `plan_position + 1`.

`HarnessRuntime::resume_from_interrupt(options, checkpoint, answer)` is
the harness-layer entry point (does not re-run `HarnessRuntime::run`
wholesale — no fresh `session_started`, no context-provider
re-assembly — just reattaches the live event bridge, stamps
`interrupt_answered`, and dispatches through `AgentRuntime::
resume_from_loop_checkpoint`). `agentflow-db`'s `DbLoopCheckpointer`
is the server-side `AgentLoopCheckpointer`, attached to every
`LiveHarnessExecutor` session by default; `POST /v1/harness/sessions/
{id}/interrupt/answer` is the HTTP entry point. `agentflow harness
resume-loop` gained `--runtime react|plan_execute` (a checkpoint is
only resumable by the runtime kind that produced it) and `--answer
<text>` (falls back to an interactive stdin prompt when omitted and the
checkpoint is paused); `agentflow harness run`/`chat` handle
`AwaitingInput` inline (TTY-gated prompt for `run`, next-REPL-line for
`chat`). See `docs/HARNESS_MODE.md`'s "Interrupt protocol (V2.3)"
section for the wire-level contract.

`ReActAgent::query_memory(...)` and `query_session_memory(...)` expose the
runtime memory query boundary. The active `MemoryStore` owns retrieval behavior:
`SemanticMemory` performs vector search with keyword fallback, while simpler
stores can keep their existing keyword behavior.

`ReActAgent::with_memory_hook(...)` attaches a non-failing memory observer. The
hook is invoked when the loop writes user, assistant, or tool messages, when it
reads conversation history for an LLM call, and when explicit memory search is
used.

Prompt memory can be bounded with
`ReActConfig::with_memory_prompt_token_budget(...)`. When paired with
`MemorySummaryStrategy::RecentOnly` or `MemorySummaryStrategy::Compact`, older
messages are omitted or compacted into a deterministic summary while recent
messages remain available to the model. The default strategy is disabled to
preserve existing runtime behavior.

`ReActAgent::with_memory_summary_backend(...)` can replace the built-in
summary behavior with a custom `MemorySummaryBackend`. The backend receives the
omitted messages, kept messages, token budget, and omitted token estimate, so it
can implement rule-based summaries, LLM-generated summaries, or persistent
summary storage without changing the ReAct loop.

`agentflow skill run --trace` prints the structured `AgentRunResult` JSON for a
Skill execution, including tool calls to MCP-backed tools and the resulting
AgentRuntime steps/events.

`agentflow-agents/tests/agent_runtime_golden.rs` locks the serialized
`AgentRunResult` contract with a golden JSON fixture. The test runs a mock ReAct
loop through observe, plan, tool call, tool result, final answer, and reflection;
dynamic timestamps and tool durations are normalized before comparison.

Workflow tracing can now attach an event listener to `Flow`. The trace collector
captures workflow/node lifecycle events, node outputs, and nested
`agent_result` payloads. Agent step history and tool calls are stored under the
node's `agent_details`; tool names beginning with `mcp_` are marked as MCP tool
calls so one trace can connect workflow -> agent -> tool -> MCP.

## Flow Interop Direction

`AgentNode` wraps an agent runtime and maps workflow input into an agent run. It
writes `AgentRunResult.answer`, selected metadata, the full serialized
`agent_result`, and a stable `agent_resume` contract back to workflow state.
Checkpoints store runtime output and resume metadata, not the runtime
implementation itself.

Current `AgentNode` output includes `response`, `session_id`, `stop_reason`, and
`agent_result`, plus `agent_resume`, so DAG workflows can persist or inspect
agent step history and the recovery boundary. Flow checkpoints preserve these
outputs, including serialized agent steps, and restore original node output keys
instead of collapsing them into a generic result field.

`agent_resume` is an `AgentNodeResumeContract` with:

- `version`: contract version for future migrations.
- `runtime_name` and `session_id`: runtime identity and memory/session anchor.
- `resume_mode`: `completed_run`, `partial_run_supported`,
  `partial_run_unsupported`, or `restart_required`.
- `completed`: whether the stop reason is a successful terminal state.
- `step_count` and `last_step_index`: durable step boundary.
- `tool_calls`: recorded tool calls with params, result step, result error
  state, and replay policy.
- `completed_run_replay_safe`: completed runs can be reused from checkpoint
  outputs without calling the agent again.
- `partial_run_resume_supported`: `true` when all recorded tool calls have
  result observations and the runtime can continue without replaying them.
- `restart_requires_idempotent_tools`: `true` when an interrupted run had tool
  calls and must be restarted rather than reused.

The current contract is conservative: completed `AgentNode` runs are checkpoint
reusable, recorded tool observations use `reuse_recorded_result`, partial runs
with completed tool observations can continue from recovered memory, and
interrupted runs with unresolved tool calls require idempotent tools before a
full restart.

`AgentNode` accepts optional `agent_result` input for partial resume. When
present, it restores the prior `AgentRunResult` into the agent memory, refuses
traces with unresolved tool calls, and continues the ReAct loop without
replaying tool calls that already have `ToolResult` steps.

`WorkflowTool` should wrap a `Flow` as a `Tool`, exposing a JSON schema and
returning a `ToolOutput`. From an agent perspective, workflows are just tools.

Current `WorkflowTool` maps tool JSON parameters into workflow initial inputs
and serializes workflow node results back to a JSON `ToolOutput`. Node failures
are returned as `ToolOutput::error`, so the agent can continue reasoning with
the failed workflow observation.

This keeps the dependency direction stable:

`Flow -> AgentNode -> AgentRuntime -> ToolRegistry -> Tool/MCP/WorkflowTool`

See `agentflow-agents/examples/hybrid_workflow_agent.rs` for a runnable mock
example of this full path. It runs a parent DAG with `AgentNode`, calls a child
DAG through `WorkflowTool`, and prints the resulting agent steps/events.

## Plan-and-Execute Runtime

`PlanExecuteAgent` is the first Plan-and-Execute runtime prototype. It is
parallel to `ReActAgent` and implements the same `AgentRuntime` trait, so callers
receive the same `AgentRunResult`, `AgentStep`, `AgentEvent`, and
`AgentStopReason` contract.

The planner model returns strict JSON:

```json
{
  "plan": [
    {
      "id": "1",
      "description": "Echo the requested phrase",
      "tool": "echo",
      "params": {
        "text": "plan-execute"
      }
    }
  ],
  "final_answer": "optional answer when no tool is needed"
}
```

The runtime records an observe step, one plan step containing the planner's
ordered steps, tool call/result steps for executable items, and a final answer.
If `final_answer` is omitted, the prototype returns the joined tool
observations as the answer.

It reuses:

- `ToolRegistry` for all tool calls.
- `MemoryStore` for user, planner, tool, and final-answer messages.
- `AgentMemoryHook` for memory read/write observability.
- `AgentCancellationToken`, timeout, max steps, max tool call, token budget,
  and (T1.1) cost budget guards.

**Structured output (V2.1).** `PlanExecuteConfig::with_output_schema(schema)`
mirrors `ReActConfig`'s option, but the retry shape differs: `PlanExecuteAgent`
plans in a single LLM call per attempt, so there is no per-turn loop to hook a
mid-run retry into. A `final_answer` that fails schema validation instead
retries the *whole* plan-and-execute cycle (`run_with_context`/`run_as_flow`
keep their public names and stay byte-identical when `output_schema` is
`None`; their previous bodies became private `*_once` methods called by a
thin retry wrapper). The rejected answer and the validation errors become the
next attempt's `context.input`, so the retry's own prologue records them as a
fresh user turn — the model sees what went wrong through the same
session-scoped memory continuity `ReActAgent` gets from its live loop, just
implemented as independent attempts rather than an internal loop. Bounded by
`PlanExecuteConfig::with_max_schema_correction_attempts(...)` (default `2`);
exhausting it is a hard `PlanExecuteError::SchemaValidationFailed`, matching
`ReActAgent`'s no-force-accept contract for a caller-declared schema.

Run the mock example with:

```sh
cargo run -p agentflow-agents --example plan_execute_agent
```

## Extending the runtime

Want to write a custom `AgentRuntime`, `ReflectionStrategy`,
`VerificationStrategy`, `MemorySummaryBackend`, `Tool`, or `MemoryStore`? See
[`AGENT_SDK.md`](./AGENT_SDK.md) for the extension contract and runnable
examples (`custom_runtime`, `custom_reflection`, `custom_verification`,
`custom_memory_summary`).

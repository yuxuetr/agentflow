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
  reflection, and final answer.
- `AgentEvent`: runtime event stream for run start/stop, tool calls, and
  reflection.
- `AgentStopReason`: structured stop reason for final answer, stop condition,
  max steps, max tool calls, timeout, cancellation, token budget, or error.
- `AgentRunResult`: final runtime output containing answer, stop reason, steps,
  and events.
- `AgentCancellationToken`: shared shutdown signal that can stop an active
  agent run through `AgentContext`.
- `ReflectionStrategy`: optional pluggable reflection hook.
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
and stop conditions.

Runs can be cancelled by passing
`AgentContext::with_cancellation_token(AgentCancellationToken::new())` and
calling `cancel()` from another task. `ReActAgent` checks the token at loop
boundaries and while awaiting LLM/tool futures, returning
`AgentStopReason::Cancelled` with a `RunStopped` event.

Reflection remains opt-in through `with_reflection_strategy(...)` and can be
disabled at runtime configuration level with
`ReActConfig::with_reflection_enabled(false)`. When disabled, no `Reflect` step
or `ReflectionAdded` event is recorded even if a strategy is attached.

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
- `AgentCancellationToken`, timeout, max steps, and max tool call guards.

Run the mock example with:

```sh
cargo run -p agentflow-agents --example plan_execute_agent
```

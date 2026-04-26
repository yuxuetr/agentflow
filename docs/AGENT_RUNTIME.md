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

- `AgentContext`: per-run input, session, model, persona, limits, metadata.
- `AgentStep`: durable step history for observe, plan, tool call, tool result,
  reflection, and final answer.
- `AgentEvent`: runtime event stream for run start/stop, tool calls, and
  reflection.
- `AgentStopReason`: structured stop reason for final answer, stop condition,
  max steps, max tool calls, timeout, token budget, or error.
- `AgentRunResult`: final runtime output containing answer, stop reason, steps,
  and events.
- `ReflectionStrategy`: optional pluggable reflection hook.

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

`agentflow skill run --trace` prints the structured `AgentRunResult` JSON for a
Skill execution, including tool calls to MCP-backed tools and the resulting
AgentRuntime steps/events.

## Flow Interop Direction

`AgentNode` should wrap an `AgentRuntime` and map workflow input into
`AgentContext`. It should write `AgentRunResult.answer` and selected metadata
back to workflow state. Checkpoints should store `AgentRunResult.steps` or a
compact summary, not the runtime implementation itself.

Current `AgentNode` output includes `response`, `session_id`, `stop_reason`, and
`agent_result`, so DAG workflows can persist or inspect agent step history.

`WorkflowTool` should wrap a `Flow` as a `Tool`, exposing a JSON schema and
returning a `ToolOutput`. From an agent perspective, workflows are just tools.

Current `WorkflowTool` maps tool JSON parameters into workflow initial inputs
and serializes workflow node results back to a JSON `ToolOutput`. Node failures
are returned as `ToolOutput::error`, so the agent can continue reasoning with
the failed workflow observation.

This keeps the dependency direction stable:

`Flow -> AgentNode -> AgentRuntime -> ToolRegistry -> Tool/MCP/WorkflowTool`

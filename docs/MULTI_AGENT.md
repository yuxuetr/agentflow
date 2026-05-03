# Multi-Agent Collaboration

AgentFlow's `agentflow-agents` crate ships three multi-agent supervisors. Each
implements the `AgentRuntime` trait, so they compose with the rest of the
framework — embed them in `AgentNode`, drive them from a YAML workflow via the
`multi_agent` node type, or call them directly from Rust.

This document covers:

- [When to choose which pattern](#choosing-a-pattern)
- [Handoff supervisor](#handoff-supervisor)
- [Blackboard supervisor](#blackboard-supervisor)
- [Debate supervisor](#debate-supervisor)
- [YAML node reference](#yaml-multi_agent-node)
- [Trace shape and observability](#trace-shape)

## Choosing a pattern

| Pattern | Control flow | State sharing | Concurrency | Picks the answer | Use it when |
|---|---|---|---|---|---|
| **Handoff** | each agent decides who runs next via `handoff(...)` | none — each agent has its own session/memory | sequential | the *last* agent in the chain | the user request needs *routing*: triage → specialist → resolver |
| **Blackboard** | a static schedule (sequential or parallel) | shared key/value board | sequential or parallel | the value at `answer_from` (or none) | the agents need to *cooperate* on building a shared artefact |
| **Debate** | parallel proposals → optional revision rounds → judge | none between participants; judge sees all final proposals | parallel | the judge's answer | reliability matters and you want to *cross-check* with N independent answers |

Two reliable rules of thumb:

1. If the agents do *different* jobs and only one of them needs to talk to the
   user at a time → **Handoff**.
2. If all agents contribute to the same artefact and you need each one's
   intermediate result visible to the others → **Blackboard**.

Debate is the heaviest of the three (N parallel runs + a judge); reach for it
only when independent verification is worth the cost.

## Handoff supervisor

```rust
use std::sync::Arc;
use agentflow_agents::supervisor::HandoffSupervisorBuilder;
use agentflow_agents::react::{ReActAgent, ReActConfig};
use agentflow_memory::SessionMemory;
use agentflow_tools::ToolRegistry;

let mut supervisor = HandoffSupervisorBuilder::new()
  .add_agent("triage", "Front-desk router", |handoff| {
    let mut registry = ToolRegistry::new();
    registry.register(handoff);                 // <-- caller registers
    ReActAgent::new(
      ReActConfig::new("gpt-4o").with_persona("..."),
      Box::new(SessionMemory::default_window()),
      Arc::new(registry),
    )
  })
  .add_agent("billing", "Billing specialist", |handoff| {
    let mut registry = ToolRegistry::new();
    registry.register(handoff);
    ReActAgent::new(
      ReActConfig::new("gpt-4o").with_persona("..."),
      Box::new(SessionMemory::default_window()),
      Arc::new(registry),
    )
  })
  .initial_agent("triage")
  .max_handoffs(5)
  .build()
  .unwrap();
```

How handoff is detected: each registered participant gets the same
`HandoffTool` instance. When the LLM calls `handoff(to=X, message=...)`, the
tool stores the request in a shared `HandoffSignal`. After the active agent's
loop completes, the supervisor checks the signal:

- present → switch to `X`, use `message` as the next input, continue.
- absent → terminate with the agent's final answer.

`max_handoffs` caps the number of transitions so a runaway LLM cannot bounce
forever; on cap, the supervisor returns the most recent agent's answer with
`AgentStopReason::StopCondition { ... }`.

**Trace contributions** (per transition): one `AgentStepKind::Handoff { from,
to, message }` step + one `AgentEvent::HandoffOccurred` event.

## Blackboard supervisor

```rust
use std::sync::Arc;
use agentflow_agents::supervisor::{
  Blackboard, BlackboardReadTool, BlackboardSchedule, BlackboardStop,
  BlackboardSupervisorBuilder, BlackboardWriteTool,
};

let mut supervisor = BlackboardSupervisorBuilder::new()
  .add_agent("researcher", "Gathers facts", |bb| {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(BlackboardReadTool::new(bb.clone(), "researcher")));
    registry.register(Arc::new(BlackboardWriteTool::new(bb, "researcher")));
    ReActAgent::new(/* ... */)
  })
  .add_agent("writer", "Writes the report", |bb| {
    /* same wiring */
  })
  .schedule(BlackboardSchedule::Sequential(vec!["researcher".into(), "writer".into()]))
  .stop_when(BlackboardStop::KeySet("report".into()))
  .answer_from("report")
  .build()
  .unwrap();
```

Two tools every participant gets:

- `bb_read(key)` returns the JSON value previously written under `key`, or
  `null` if the key is unset.
- `bb_write(key, value)` writes any JSON value to the shared board. Empty
  keys are rejected.

**Schedules**:

- `Sequential([a, b, c])` — one at a time, in order. Later agents see prior
  writes.
- `Parallel([a, b, c])` — all run concurrently in one round. Writes are
  visible only after the round ends.

**Stop conditions**:

- `AllAgentsCompleted` — run the schedule once.
- `KeySet(key)` — terminate as soon as `key` has been written. (Sequential
  mode short-circuits between agents; parallel mode checks after the round.)

**Answer**: if `answer_from(key)` is set, the supervisor reads the final
value at that key as its answer. Otherwise the supervisor returns no answer
and `result.answer` is `None` — callers should use `result.steps` /
`supervisor.blackboard().snapshot()` to extract output instead.

**Trace contributions** (per blackboard op): the underlying `bb_read` /
`bb_write` invocations show up as the agent's standard `ToolCall` /
`ToolResult` steps. The supervisor *additionally* synthesises
`AgentStepKind::BlackboardOp { op, key, agent, value }` steps and
`AgentEvent::BlackboardWritten` events for a clean control-plane summary.

## Debate supervisor

```rust
use agentflow_agents::supervisor::DebateSupervisorBuilder;

let mut supervisor = DebateSupervisorBuilder::new()
  .add_participant("performance", agent_for_persona("performance reviewer"))
  .add_participant("readability", agent_for_persona("readability reviewer"))
  .judge(agent_for_persona("synthesising tech-lead"))
  .rounds(1)
  .build()
  .unwrap();
```

Lifecycle:

1. Round 1: every participant runs **concurrently** with the user input.
2. Round N+1 (when `rounds > 1`): each participant sees every round-N proposal
   appended to its prompt and is asked to revise.
3. Judge: receives the user request + every final-round proposal, produces
   one final answer.

A failed participant (e.g. an LLM error) is recorded as an empty proposal so
the debate continues; the judge still runs and is told the proposal is
missing.

**Trace contributions**:

- `AgentEvent::DebateRoundStarted { round, participants }` at the top of each
  round.
- `AgentStepKind::DebateProposal { round, agent, proposal }` per participant
  per round.
- `AgentStepKind::DebateVerdict { winner, rationale }` after the judge runs.
- `AgentEvent::DebateVerdictRendered` mirrors the verdict step.

`winner` is `None` because the built-in judge synthesises a merged answer
rather than picking a single proposal. (A future "majority-vote" judge can
populate it.)

## YAML `multi_agent` node

The CLI factory accepts `type: multi_agent` workflow nodes. Each participant
references a skill directory the same way `skill_agent` nodes do.

Required inputs:

- `message` — the user request (typically wired up via `input_mapping`).

Common outputs (same shape as `skill_agent`):

- `response` — the supervisor's final answer string.
- `session_id` — the supervisor's session id.
- `stop_reason` — JSON-serialised `AgentStopReason`.
- `agent_result` — full `AgentRunResult` (steps + events).

### Handoff

```yaml
- id: pipeline
  type: multi_agent
  input_mapping:
    message: "{{ inputs.user_message }}"
  parameters:
    mode: handoff
    initial_agent: triage
    max_handoffs: 5             # default = 5
    agents:
      - name: triage
        skill: ./skills/triage
      - name: billing
        skill: ./skills/billing
      - name: tech
        skill: ./skills/tech
```

The factory injects a single `HandoffTool` (with the participant names
baked in) into each skill's tool registry before construction.

### Blackboard

```yaml
- id: research_team
  type: multi_agent
  input_mapping:
    message: "{{ inputs.topic }}"
  parameters:
    mode: blackboard
    schedule:
      mode: sequential          # sequential | parallel
      agents: [researcher, writer]
    stop_when:
      type: key_set             # all_completed | key_set
      key: report
    answer_from: report
    agents:
      - name: researcher
        skill: ./skills/researcher
      - name: writer
        skill: ./skills/writer
```

Each agent receives `BlackboardReadTool` + `BlackboardWriteTool` injected
into its registry.

### Debate

```yaml
- id: code_review
  type: multi_agent
  input_mapping:
    message: "{{ inputs.code }}"
  parameters:
    mode: debate
    rounds: 1                   # default = 1
    participants:
      - name: performance
        skill: ./skills/perf-reviewer
      - name: readability
        skill: ./skills/readability-reviewer
    judge:
      name: tech-lead
      skill: ./skills/tech-lead
    judge_prompt: |             # optional; overrides the default
      You are a tech lead. Combine the reviewers' feedback into a single
      prioritised review comment.
```

Skills used as debate participants do **not** receive any extra tools; they
just need to be standard ReAct-capable agents.

## Trace shape

A successful multi-agent run produces an `AgentRunResult` with these step kinds
in addition to the usual `Observe / Plan / ToolCall / ToolResult / Reflect /
FinalAnswer`:

| Step kind | Producer | Meaning |
|---|---|---|
| `Handoff { from, to, message }` | HandoffSupervisor | one transition between agents |
| `BlackboardOp { op, key, agent, value }` | BlackboardSupervisor | one read or write against the shared board |
| `DebateProposal { round, agent, proposal }` | DebateSupervisor | one participant's proposal |
| `DebateVerdict { winner, rationale }` | DebateSupervisor | the judge's final answer |

Matching event variants (`HandoffOccurred`, `BlackboardWritten`,
`DebateRoundStarted`, `DebateVerdictRendered`) carry the same data on the
event bus and are serialised verbatim into trace JSONL files.

`agentflow trace replay <run_id>` and `agentflow trace tui` will render the
new step kinds; older trace logs that pre-date 0.4.0 simply do not contain
them and are unaffected.

## Cancellation

All three supervisors honour the `AgentCancellationToken` carried in the
`AgentContext`. When the token is cancelled the supervisor returns
`AgentStopReason::Cancelled` and stops dispatching further agents (in-flight
agents see the same token and exit at their next safe checkpoint).

## Migration from the legacy `Supervisor`

The original `agentflow_agents::supervisor::Supervisor` /
`SupervisorBuilder` API still works and is unchanged. It is best understood as
a degenerate "delegate" pattern (one orchestrator, sub-agents wrapped as tools)
and remains available for backward compatibility. New code should prefer
`HandoffSupervisor` for the same shape with explicit handoff tracking.

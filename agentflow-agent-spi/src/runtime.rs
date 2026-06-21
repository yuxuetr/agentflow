use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Notify;

use agentflow_store_spi::Message;
use agentflow_tools::{Capability, CapabilityDecisionEntry, SandboxStatus, ToolOutputPart};

/// Runtime limits shared by agent-native execution loops.
///
/// All four bounds are independent stop signals; whichever is hit first
/// terminates the run with the corresponding [`AgentStopReason`]. `None`
/// disables that bound.
///
/// `max_steps` counts every emitted [`AgentStep`] (observe / plan / tool
/// call / tool result / reflect / final answer). `token_budget` is checked
/// against the running estimated-token tally for the session memory and is
/// the primary defence against runaway prompt growth.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeLimits {
  /// Maximum total [`AgentStep`]s before the runtime stops.
  pub max_steps: Option<usize>,
  /// Maximum number of `ToolCall` steps before the runtime stops.
  pub max_tool_calls: Option<usize>,
  /// Wall-clock timeout in milliseconds.
  pub timeout_ms: Option<u64>,
  /// Approximate token budget for the conversation memory.
  pub token_budget: Option<u32>,
}

impl RuntimeLimits {
  /// Defaults considered safe for ReAct: 15 steps, 50 000-token budget,
  /// no wall-clock timeout, no per-tool-call cap.
  pub fn react_defaults() -> Self {
    Self {
      max_steps: Some(15),
      max_tool_calls: None,
      timeout_ms: None,
      token_budget: Some(50_000),
    }
  }
}

/// Per-run context passed into an agent runtime.
///
/// `AgentContext` carries everything a runtime needs to start a single
/// invocation: session identity, user input, model selection, optional
/// persona / skill identity, runtime bounds, and a cancellation token.
/// Construct it via [`AgentContext::new`] and the `with_*` builders.
///
/// The context is intentionally serializable so platform code (server,
/// trace replay) can persist and replay it; the cancellation token is
/// `#[serde(skip)]` because cancellation is a process-local signal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentContext {
  /// Stable identifier shared by all events and memory rows for one run.
  pub session_id: String,
  /// Top-level user input that kicks off the agent loop.
  pub input: String,
  /// LLM model identifier this run should use.
  pub model: String,
  /// Optional persona / system-prompt fragment.
  pub persona: Option<String>,
  /// Skill name when the agent was invoked through a `SKILL.md` package.
  pub skill_name: Option<String>,
  /// Limits applied to this run; see [`RuntimeLimits`].
  pub limits: RuntimeLimits,
  /// Free-form structured metadata attached to the run for observability.
  #[serde(default)]
  pub metadata: Value,
  /// Wall-clock start time set by the runtime.
  pub started_at: DateTime<Utc>,
  /// Optional cancellation token honored by the runtime.
  #[serde(skip)]
  pub cancellation_token: Option<AgentCancellationToken>,
  /// Optional W3C trace context. When set, the agent runtime propagates
  /// it to every `LLMClient` call so the outbound HTTP request carries a
  /// `traceparent` header and the OTel trace tree stays continuous across
  /// the LLM hop. The session id alone is not enough — OpenTelemetry
  /// requires a 16-byte trace id and 8-byte span id.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub trace_context: Option<agentflow_llm::LlmTraceContext>,
  /// Phase 1 (RFC_HARNESS_LOOP_OWNERSHIP): optional live event observer.
  /// When set, the runtime calls [`AgentEventSink::emit`] for each
  /// [`AgentEvent`] at the moment it is produced, in addition to
  /// accumulating it into [`AgentRunResult::events`]. `None` (the
  /// default) keeps behavior byte-identical to a runtime with no
  /// observer. `#[serde(skip)]` because a sink is a process-local handle
  /// (like `cancellation_token`).
  #[serde(skip)]
  pub event_sink: Option<EventSinkHandle>,
  /// Phase 2b (RFC_HARNESS_LOOP_OWNERSHIP): optional between-turn hook.
  /// When set, the runtime calls [`BetweenTurnHook::before_turn`] at the
  /// top of each turn so the loop's owner (the Harness) can compact or
  /// refresh context mid-run. `None` (default) is a no-op. `#[serde(skip)]`
  /// because a hook is a process-local handle.
  #[serde(skip)]
  pub between_turn_hook: Option<BetweenTurnHookHandle>,
}

/// Cloneable handle wrapping an [`AgentEventSink`] so [`AgentContext`] can
/// keep deriving `Debug` / `PartialEq` (a bare `Arc<dyn AgentEventSink>`
/// implements neither). Two handles compare equal iff they point at the
/// same sink (`Arc::ptr_eq`), mirroring how [`AgentCancellationToken`]
/// carries a manual `PartialEq`.
#[derive(Clone)]
pub struct EventSinkHandle(pub Arc<dyn AgentEventSink>);

impl std::fmt::Debug for EventSinkHandle {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("EventSinkHandle(<dyn AgentEventSink>)")
  }
}

impl PartialEq for EventSinkHandle {
  fn eq(&self, other: &Self) -> bool {
    Arc::ptr_eq(&self.0, &other.0)
  }
}

/// Live observer of an agent run's [`AgentEvent`] stream.
///
/// The runtime invokes [`emit`](AgentEventSink::emit) inline at each
/// event's production point — *before* the loop proceeds — so an
/// observer (e.g. the Harness event bridge) sees events on the same
/// logical clock as any governance side-effects that fire during tool
/// execution. Implementations must be cheap and must not panic;
/// observability must never alter or break execution.
#[async_trait]
pub trait AgentEventSink: Send + Sync {
  /// Called for every [`AgentEvent`] at the instant it is produced.
  async fn emit(&self, event: &AgentEvent);
}

/// Cloneable handle wrapping a [`BetweenTurnHook`] so [`AgentContext`]
/// keeps deriving `Debug` / `PartialEq`. Mirrors [`EventSinkHandle`].
#[derive(Clone)]
pub struct BetweenTurnHookHandle(pub Arc<dyn BetweenTurnHook>);

impl std::fmt::Debug for BetweenTurnHookHandle {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("BetweenTurnHookHandle(<dyn BetweenTurnHook>)")
  }
}

impl PartialEq for BetweenTurnHookHandle {
  fn eq(&self, other: &Self) -> bool {
    Arc::ptr_eq(&self.0, &other.0)
  }
}

/// Between-turn control point for an agent loop (RFC_HARNESS_LOOP_OWNERSHIP
/// Phase 2b — the turn-driven seam).
///
/// The runtime invokes [`before_turn`](BetweenTurnHook::before_turn) at
/// the top of each agent turn, *before* the LLM call, handing the hook
/// the run's [`MemoryStore`](agentflow_store_spi::MemoryStore). This is the
/// point at which an owner of the loop (the Harness) performs
/// context-window engineering between turns — most importantly memory
/// compaction — so a long-running loop's prompt stays bounded under a
/// policy the *caller* controls, rather than only the agent's built-in
/// `MemorySummaryBackend`.
///
/// The hook is best-effort and infallible by contract: it must not break
/// execution (mirrors [`AgentEventSink`]). `None` (the default) is a
/// no-op, so runs without a hook are byte-identical.
#[async_trait]
pub trait BetweenTurnHook: Send + Sync {
  /// Called before each turn's LLM call. `turn_index` is the 0-based
  /// turn. The hook may read and rewrite `memory` (all `&self` async
  /// methods) to compact or refresh the conversation.
  async fn before_turn(
    &self,
    turn_index: usize,
    session_id: &str,
    memory: &dyn agentflow_store_spi::MemoryStore,
  );
}

impl AgentContext {
  /// Build a minimal context with no persona, no skill, no limits, and
  /// `started_at = Utc::now()`.
  pub fn new(
    session_id: impl Into<String>,
    input: impl Into<String>,
    model: impl Into<String>,
  ) -> Self {
    Self {
      session_id: session_id.into(),
      input: input.into(),
      model: model.into(),
      persona: None,
      skill_name: None,
      limits: RuntimeLimits::default(),
      metadata: Value::Object(Default::default()),
      started_at: Utc::now(),
      cancellation_token: None,
      trace_context: None,
      event_sink: None,
      between_turn_hook: None,
    }
  }

  /// Attach a persona fragment that the runtime prepends to the system prompt.
  pub fn with_persona(mut self, persona: impl Into<String>) -> Self {
    self.persona = Some(persona.into());
    self
  }

  /// Tag the run with the originating skill name (set by the skill CLI).
  pub fn with_skill_name(mut self, skill_name: impl Into<String>) -> Self {
    self.skill_name = Some(skill_name.into());
    self
  }

  /// Replace the [`RuntimeLimits`] applied to this run.
  pub fn with_limits(mut self, limits: RuntimeLimits) -> Self {
    self.limits = limits;
    self
  }

  /// Attach a shared cancellation token; cancelling it stops the loop with
  /// [`AgentStopReason::Cancelled`].
  pub fn with_cancellation_token(mut self, token: AgentCancellationToken) -> Self {
    self.cancellation_token = Some(token);
    self
  }

  /// Attach a W3C trace context that the runtime propagates to LLM HTTP
  /// calls. Pass `None` to clear.
  pub fn with_trace_context(
    mut self,
    context: impl Into<Option<agentflow_llm::LlmTraceContext>>,
  ) -> Self {
    self.trace_context = context.into();
    self
  }

  /// Phase 1: attach a live [`AgentEventSink`]. The runtime emits each
  /// [`AgentEvent`] to it as the event is produced. Pass `None`-equivalent
  /// by simply not calling this; the default is no observer.
  pub fn with_event_sink(mut self, sink: Arc<dyn AgentEventSink>) -> Self {
    self.event_sink = Some(EventSinkHandle(sink));
    self
  }

  /// Phase 2b: attach a [`BetweenTurnHook`] invoked before each turn's
  /// LLM call, for caller-owned between-turn context engineering.
  pub fn with_between_turn_hook(mut self, hook: Arc<dyn BetweenTurnHook>) -> Self {
    self.between_turn_hook = Some(BetweenTurnHookHandle(hook));
    self
  }
}

/// Shared cancellation signal for long-running agent loops.
///
/// Cheap to clone (`Arc` internally). A typical caller gives one clone to
/// the runtime via [`AgentContext::with_cancellation_token`] and keeps a
/// second clone to call [`AgentCancellationToken::cancel`] from a UI button,
/// timeout watchdog, or supervisor.
///
/// # Cancellation propagation model (cooperative)
///
/// AgentFlow runtimes check this token at every blocking await: pre-LLM,
/// inside the LLM HTTP call, pre-tool-dispatch, inside the tool future, and
/// inside the batch dispatcher. When the token fires mid-await, the runtime
/// races it against the work via `tokio::select!` — the losing branch is
/// **dropped**. There is no `Tool::cancel()` hook and no `LLMClient::cancel()`
/// hook: cancellation is exclusively cooperative through future drop.
///
/// What this means in practice:
///
/// - In-process Tokio futures (`reqwest`, `tokio::time::sleep`,
///   `tokio::process::Child` with `kill_on_drop(true)`, async channel recv)
///   are cancelled cleanly: `Drop` guards in the future's captured state run,
///   pending syscalls are aborted by the Tokio reactor, and child processes
///   exit.
/// - Work the tool has detached from the dispatch future will continue:
///   anything `tokio::spawn`-ed and not awaited inside the tool future, work
///   running on `spawn_blocking` worker threads, FFI calls in flight, or
///   `std::process::Child` without `kill_on_drop`. Tool authors who fan out
///   detached work must wire their own cancellation signal — see
///   [`crate::AgentRuntime`] and `agentflow_tools::Tool::execute` rustdoc for
///   the contract.
///
/// The trade-off is intentional: a per-call `cancel()` hook on every
/// `Tool` impl would be a breaking API change across `agentflow-tools`,
/// `agentflow-mcp`, and every workspace plugin without buying much for the
/// common case where tools are short-lived HTTP / subprocess calls. The
/// cooperative model + cancellation-aware Drop covers File / Http / Shell /
/// Script / Mcp tools as currently shipped.
///
/// See `tool_future_drop_runs_when_token_is_cancelled` and
/// `detached_spawn_survives_cancellation_for_documentation` in this module
/// for executable pins of this contract (Q3.12.1).
#[derive(Debug, Clone, Default)]
pub struct AgentCancellationToken {
  cancelled: Arc<AtomicBool>,
  notify: Arc<Notify>,
}

impl AgentCancellationToken {
  /// Create a fresh, not-yet-cancelled token.
  pub fn new() -> Self {
    Self::default()
  }

  /// Mark the token as cancelled and wake all `cancelled()` waiters.
  pub fn cancel(&self) {
    self.cancelled.store(true, Ordering::SeqCst);
    self.notify.notify_waiters();
  }

  /// Return whether the token has been cancelled.
  pub fn is_cancelled(&self) -> bool {
    self.cancelled.load(Ordering::SeqCst)
  }

  /// Resolve once the token is cancelled. Useful inside `tokio::select!`.
  pub async fn cancelled(&self) {
    while !self.is_cancelled() {
      self.notify.notified().await;
    }
  }
}

impl PartialEq for AgentCancellationToken {
  fn eq(&self, other: &Self) -> bool {
    self.is_cancelled() == other.is_cancelled()
  }
}

impl Eq for AgentCancellationToken {}

/// Why an agent-native loop stopped.
//
// `Eq` is intentionally not derived: the `CostLimitExceeded` variant
// carries `f64` fields. Consumers that need bit-exact equality should
// compare on individual fields rather than the variant as a whole.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum AgentStopReason {
  FinalAnswer,
  StopCondition {
    condition: String,
  },
  MaxSteps {
    max_steps: usize,
  },
  MaxToolCalls {
    max_tool_calls: usize,
  },
  Timeout {
    timeout_ms: u64,
  },
  Cancelled {
    message: String,
  },
  TokenBudgetExceeded {
    used: u32,
    budget: u32,
  },
  /// Accumulated provider cost crossed the eval harness's
  /// `cost_limit_usd`. Only emitted by the eval runner today; the agent
  /// runtimes themselves do not enforce cost budgets yet.
  CostLimitExceeded {
    used_usd: f64,
    budget_usd: f64,
  },
  Error {
    message: String,
  },
}

impl AgentStopReason {
  /// Return `true` when the loop ended because the agent produced a
  /// terminal answer or matched a stop condition. All other variants
  /// (limits hit, cancelled, error) report `false`.
  pub fn is_success(&self) -> bool {
    matches!(self, Self::FinalAnswer | Self::StopCondition { .. })
  }
}

/// One durable step in an agent-native loop.
///
/// `AgentStep` is the persistence unit recorded by the runtime: each
/// observation, plan, tool call, tool result, reflection, and final answer
/// becomes one step. Steps are append-only and serialisable so that
/// trace replay can faithfully reconstruct a run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentStep {
  /// Monotonically increasing index inside one run.
  pub index: usize,
  /// Typed payload for this step.
  pub kind: AgentStepKind,
  /// Wall-clock time the step was emitted.
  pub timestamp: DateTime<Utc>,
  /// Duration of the underlying work, when measurable.
  pub duration_ms: Option<u64>,
}

impl AgentStep {
  /// Create a new step with a fresh timestamp and no duration set.
  pub fn new(index: usize, kind: AgentStepKind) -> Self {
    Self {
      index,
      kind,
      timestamp: Utc::now(),
      duration_ms: None,
    }
  }

  /// Attach a measured duration to a step.
  pub fn with_duration_ms(mut self, duration_ms: u64) -> Self {
    self.duration_ms = Some(duration_ms);
    self
  }
}

/// The kind of operation a [`AgentStepKind::BlackboardOp`] represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlackboardOpKind {
  Read,
  Write,
}

/// The typed content of an [`AgentStep`].
///
/// This enum is intentionally *closed*: custom agent runtimes should
/// reuse the existing variants rather than invent new ones, so that all
/// trace replay / event listeners / multi-agent supervisors work
/// uniformly. If a new step kind is genuinely missing, open an issue
/// rather than forking the enum locally.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentStepKind {
  /// User input recorded at the start of a turn.
  Observe {
    /// The raw text the runtime observed.
    input: String,
  },
  /// Planning thought produced before a tool call or answer.
  Plan {
    /// The model's reasoning text.
    thought: String,
  },
  /// A tool invocation requested by the agent.
  ToolCall {
    /// Registered tool name.
    tool: String,
    /// JSON parameters matching the tool's schema.
    params: Value,
  },
  /// The tool's output, structured or text.
  ToolResult {
    /// Tool name that produced this result.
    tool: String,
    /// String summary content (always populated).
    content: String,
    /// Whether the tool reported a failure.
    is_error: bool,
    /// Optional typed output parts (text / image / resource).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    parts: Vec<ToolOutputPart>,
  },
  /// A reflection produced by a `ReflectionStrategy` (agentflow-agents).
  Reflect {
    /// The reflection text.
    content: String,
  },
  /// Terminal answer returned to the caller.
  FinalAnswer {
    /// The user-visible answer.
    answer: String,
  },

  // ── Multi-agent collaboration steps (since 0.4.0) ──────────────────────────
  /// One agent transferred control to another inside a HandoffSupervisor.
  Handoff {
    from: String,
    to: String,
    message: String,
  },
  /// A read or write against a shared blackboard inside a BlackboardSupervisor.
  BlackboardOp {
    op: BlackboardOpKind,
    key: String,
    agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    value: Option<Value>,
  },
  /// A participant's proposal in one debate round.
  DebateProposal {
    round: usize,
    agent: String,
    proposal: String,
  },
  /// The judge's verdict that closes a debate.
  DebateVerdict {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    winner: Option<String>,
    rationale: String,
  },
}

/// Runtime events emitted while an agent-native loop is executing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AgentEvent {
  RunStarted {
    session_id: String,
    model: String,
    timestamp: DateTime<Utc>,
  },
  StepStarted {
    session_id: String,
    step_index: usize,
    step_type: String,
    timestamp: DateTime<Utc>,
  },
  StepCompleted {
    session_id: String,
    step: AgentStep,
  },
  ToolCallStarted {
    session_id: String,
    step_index: usize,
    tool: String,
    params: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    permissions: Vec<String>,
    timestamp: DateTime<Utc>,
  },
  ToolPolicyDecision {
    session_id: String,
    step_index: usize,
    tool: String,
    allowed: bool,
    matched_rule: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deny_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    permissions: Vec<String>,
    params_summary: Value,
    timestamp: DateTime<Utc>,
  },
  /// Outcome of the three-way capability merge for a tool invocation.
  ///
  /// Emitted alongside [`AgentEvent::ToolPolicyDecision`] but reflects a
  /// finer-grained model: capabilities map onto OS-level sandbox primitives
  /// (sandbox-exec / seccomp). The full per-layer trace lets operators see
  /// which step (skill / tool policy / CLI flag) restricted each capability.
  ToolCapabilityDecision {
    session_id: String,
    step_index: usize,
    tool: String,
    allowed: bool,
    required: Vec<Capability>,
    effective: Vec<Capability>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    denied: Vec<Capability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deny_reason: Option<String>,
    trace: Vec<CapabilityDecisionEntry>,
    /// Active sandbox status for tools that wrap a child process.
    ///
    /// `None` for in-process tools (HTTP, file, MCP) where no OS sandbox is
    /// engaged. Shell, script, and plugin invocations populate this with the
    /// backend name (`sandbox-exec` / `seccomp` / `noop`) and the
    /// enforcement level visible at the moment of decision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sandbox: Option<SandboxStatus>,
    timestamp: DateTime<Utc>,
  },
  ToolCallCompleted {
    session_id: String,
    step_index: usize,
    tool: String,
    is_error: bool,
    duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    permissions: Vec<String>,
    timestamp: DateTime<Utc>,
  },
  ReflectionAdded {
    session_id: String,
    step_index: usize,
    timestamp: DateTime<Utc>,
  },
  /// One LLM round-trip completed. Emitted by every runtime that calls
  /// an LLM provider; carries the per-call `TokenUsage` so downstream
  /// consumers (eval cost tracking, tracing, dashboards) can aggregate
  /// without re-instrumenting the agents themselves.
  ///
  /// `prompt_tokens` / `completion_tokens` / `total_tokens` are all
  /// optional because not every provider reports usage — providers that
  /// don't (or whose response was truncated mid-stream) leave them
  /// `None`. Aggregators must treat `None` as "unknown" rather than
  /// zero.
  LlmCallCompleted {
    session_id: String,
    step_index: usize,
    model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    prompt_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    completion_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    total_tokens: Option<u32>,
    duration_ms: u64,
    timestamp: DateTime<Utc>,
  },
  RunStopped {
    session_id: String,
    reason: AgentStopReason,
    timestamp: DateTime<Utc>,
  },
  /// Memory was compacted: an older slice of conversation was replaced by
  /// a generated summary injected as a synthetic system message. Emitted
  /// by runtimes that run a `MemorySummaryBackend` (agentflow-agents) when the
  /// prompt-memory budget is exceeded, so observers (the Harness
  /// `MemorySummaryAdded` envelope) can surface context compaction that
  /// would otherwise be invisible.
  MemorySummaryAdded {
    session_id: String,
    /// Memory layer that produced the summary (`session`, `semantic`, …).
    layer: String,
    /// The injected summary text.
    summary: String,
    /// Approximate token cost of the summary.
    token_estimate: usize,
    timestamp: DateTime<Utc>,
  },

  // ── Multi-agent collaboration events (since 0.4.0) ─────────────────────────
  /// Recorded each time a HandoffSupervisor switches the active agent.
  HandoffOccurred {
    session_id: String,
    step_index: usize,
    from: String,
    to: String,
    timestamp: DateTime<Utc>,
  },
  /// Recorded for every blackboard read or write inside a BlackboardSupervisor.
  BlackboardWritten {
    session_id: String,
    step_index: usize,
    op: BlackboardOpKind,
    agent: String,
    key: String,
    timestamp: DateTime<Utc>,
  },
  /// Recorded at the start of each debate round before participants run.
  DebateRoundStarted {
    session_id: String,
    round: usize,
    participants: Vec<String>,
    timestamp: DateTime<Utc>,
  },
  /// Recorded after the judge has produced the final verdict.
  DebateVerdictRendered {
    session_id: String,
    step_index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    winner: Option<String>,
    timestamp: DateTime<Utc>,
  },
}

/// Final result returned by an agent runtime.
///
/// Carries the terminal answer (if any), the stop reason, the full step
/// trace, and the captured event stream. Persistence layers usually store
/// `steps` (compact, replayable) and stream `events` (richer per-tick
/// detail) to separate sinks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRunResult {
  /// Run-scoped identifier matching [`AgentContext::session_id`].
  pub session_id: String,
  /// User-visible answer when the run produced one.
  pub answer: Option<String>,
  /// Why the loop stopped.
  pub stop_reason: AgentStopReason,
  /// Append-only step trace.
  #[serde(default)]
  pub steps: Vec<AgentStep>,
  /// Captured event stream (subset of [`AgentEvent`]s emitted during the run).
  #[serde(default)]
  pub events: Vec<AgentEvent>,
}

impl AgentRunResult {
  /// Convenience constructor for runtimes that produce a one-step terminal
  /// answer (no tool calls, no reflection).
  pub fn final_answer(session_id: impl Into<String>, answer: impl Into<String>) -> Self {
    let answer = answer.into();
    Self {
      session_id: session_id.into(),
      answer: Some(answer.clone()),
      stop_reason: AgentStopReason::FinalAnswer,
      steps: vec![AgentStep::new(0, AgentStepKind::FinalAnswer { answer })],
      events: Vec::new(),
    }
  }
}

/// Memory operation observed by an agent runtime hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryHookKind {
  /// `MemoryStore::get_history` (or equivalent) was invoked.
  ReadHistory,
  /// `MemoryStore::search` was invoked.
  Search,
  /// `MemoryStore::add_message` was invoked.
  Write,
}

/// Context passed to an [`AgentMemoryHook`].
#[derive(Debug, Clone)]
pub struct MemoryHookContext {
  /// Session id of the run that triggered the hook.
  pub session_id: String,
  /// Which memory operation was observed.
  pub kind: MemoryHookKind,
  /// Search query when `kind == Search`; `None` otherwise.
  pub query: Option<String>,
  /// Result limit when applicable.
  pub limit: Option<usize>,
  /// Messages read or written.
  pub messages: Vec<Message>,
}

/// Optional observer for memory reads and writes inside an agent loop.
///
/// Hooks are intentionally non-failing so memory observability cannot break the
/// main agent run. Implementations can record metrics, build summaries, or feed
/// another memory backend.
pub trait AgentMemoryHook: Send + Sync {
  /// Called after a successful memory read (history fetch or search).
  fn on_memory_read(&self, _context: &MemoryHookContext) {}

  /// Called after a successful memory write.
  fn on_memory_write(&self, _context: &MemoryHookContext) {}
}

/// Common boundary for agent-native runtimes.
///
/// An `AgentRuntime` consumes an [`AgentContext`] and produces an
/// [`AgentRunResult`]. Implementors are responsible for honouring
/// [`RuntimeLimits`], the cancellation token, and emitting [`AgentStep`]s
/// in chronological order. See `agentflow-agents/examples/custom_runtime.rs`
/// for the smallest viable shell.
///
/// # Cancellation contract (cooperative)
///
/// `AgentRuntime` implementations honour `context.cancellation_token` at
/// every blocking await — pre-LLM, inside the LLM HTTP call, pre-tool-
/// dispatch, inside the tool future, and inside the batch dispatcher. The
/// pattern is `tokio::select!` between the work future and
/// `token.cancelled()`, which means cancellation drops the work future
/// rather than calling any explicit `cancel()` hook.
///
/// The implication for downstream consumers and `Tool` authors is that
/// cancellation is **cooperative through Drop**: any work composed of
/// in-process Tokio futures (`reqwest`, `tokio::time::sleep`, channel recv,
/// `tokio::process::Child` with `kill_on_drop`) aborts cleanly, but any
/// work the tool has detached via `tokio::spawn` /
/// `tokio::task::spawn_blocking` / FFI / `std::process::Child` will continue
/// running past cancellation. See [`AgentCancellationToken`] and
/// `agentflow_tools::Tool::execute` rustdoc for the full contract, and the
/// `tool_future_drop_runs_when_token_is_cancelled` /
/// `detached_spawn_survives_cancellation_for_documentation` tests in this
/// module for executable pins (Q3.12.1).
#[async_trait]
pub trait AgentRuntime: Send {
  /// Execute one run for `context` and return the structured outcome.
  ///
  /// Implementations should map internal errors to
  /// [`AgentRuntimeError::ExecutionFailed`] and reserve
  /// [`AgentRuntimeError::InvalidContext`] for validation problems detected
  /// before the loop starts.
  async fn run(&mut self, context: AgentContext) -> Result<AgentRunResult, AgentRuntimeError>;

  /// Stable, machine-readable runtime identifier (e.g. `"react"`).
  fn runtime_name(&self) -> &'static str;
}

/// Errors raised before a runtime can return a structured stop reason.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AgentRuntimeError {
  /// The supplied [`AgentContext`] failed pre-flight validation.
  #[error("Invalid agent runtime context: {message}")]
  InvalidContext {
    /// Human-readable validation failure description.
    message: String,
  },

  /// The runtime aborted with an internal error rather than a structured
  /// [`AgentStopReason`].
  #[error("Agent runtime failed: {message}")]
  ExecutionFailed {
    /// Human-readable execution failure description.
    message: String,
  },
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn context_builder_sets_runtime_boundaries() {
    let token = AgentCancellationToken::new();
    let context = AgentContext::new("session-1", "hello", "mock-model")
      .with_persona("Be concise")
      .with_skill_name("demo")
      .with_limits(RuntimeLimits::react_defaults())
      .with_cancellation_token(token.clone());

    assert_eq!(context.session_id, "session-1");
    assert_eq!(context.input, "hello");
    assert_eq!(context.model, "mock-model");
    assert_eq!(context.persona.as_deref(), Some("Be concise"));
    assert_eq!(context.skill_name.as_deref(), Some("demo"));
    assert_eq!(context.limits.max_steps, Some(15));
    assert_eq!(context.cancellation_token, Some(token));
  }

  #[tokio::test]
  async fn cancellation_token_notifies_waiters() {
    let token = AgentCancellationToken::new();
    assert!(!token.is_cancelled());

    let waiter = {
      let token = token.clone();
      tokio::spawn(async move {
        token.cancelled().await;
      })
    };
    token.cancel();

    waiter.await.unwrap();
    assert!(token.is_cancelled());
  }

  #[test]
  fn stop_reason_marks_success_only_for_terminal_answers() {
    assert!(AgentStopReason::FinalAnswer.is_success());
    assert!(
      AgentStopReason::StopCondition {
        condition: "done".to_string(),
      }
      .is_success()
    );
    assert!(!AgentStopReason::MaxSteps { max_steps: 3 }.is_success());
    assert!(
      !AgentStopReason::Cancelled {
        message: "cancelled".to_string(),
      }
      .is_success()
    );
  }

  #[test]
  fn tool_result_step_preserves_typed_parts() {
    let step = AgentStep::new(
      2,
      AgentStepKind::ToolResult {
        tool: "mcp_demo_image".to_string(),
        content: "[image:image/png;4 bytes]".to_string(),
        is_error: false,
        parts: vec![ToolOutputPart::Image {
          data: "aW1n".to_string(),
          mime_type: "image/png".to_string(),
        }],
      },
    );

    let json = serde_json::to_value(&step).unwrap();
    assert_eq!(json["kind"]["type"], "tool_result");
    assert_eq!(json["kind"]["parts"][0]["type"], "image");
  }

  #[test]
  fn final_answer_result_contains_terminal_step() {
    let result = AgentRunResult::final_answer("session-1", "done");

    assert_eq!(result.answer.as_deref(), Some("done"));
    assert_eq!(result.stop_reason, AgentStopReason::FinalAnswer);
    assert_eq!(result.steps.len(), 1);
  }

  // ── Multi-agent step kinds (since 0.4.0) ───────────────────────────────────

  #[test]
  fn handoff_step_round_trips_through_serde() {
    let step = AgentStep::new(
      3,
      AgentStepKind::Handoff {
        from: "triage".to_string(),
        to: "billing".to_string(),
        message: "Please refund order #42.".to_string(),
      },
    );

    let json = serde_json::to_value(&step).unwrap();
    assert_eq!(json["kind"]["type"], "handoff");
    assert_eq!(json["kind"]["from"], "triage");
    assert_eq!(json["kind"]["to"], "billing");

    let decoded: AgentStep = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, step);
  }

  #[test]
  fn blackboard_op_step_round_trips_through_serde() {
    let step = AgentStep::new(
      1,
      AgentStepKind::BlackboardOp {
        op: BlackboardOpKind::Write,
        key: "research/findings".to_string(),
        agent: "researcher".to_string(),
        value: Some(serde_json::json!({"score": 0.92})),
      },
    );

    let json = serde_json::to_value(&step).unwrap();
    assert_eq!(json["kind"]["type"], "blackboard_op");
    assert_eq!(json["kind"]["op"], "write");
    assert_eq!(json["kind"]["value"]["score"], 0.92);

    let decoded: AgentStep = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, step);
  }

  #[test]
  fn blackboard_read_step_omits_null_value() {
    let step = AgentStep::new(
      1,
      AgentStepKind::BlackboardOp {
        op: BlackboardOpKind::Read,
        key: "topic".to_string(),
        agent: "writer".to_string(),
        value: None,
      },
    );

    let json = serde_json::to_value(&step).unwrap();
    assert!(
      json["kind"].get("value").is_none(),
      "None value field should be skipped by serde"
    );
    assert_eq!(json["kind"]["op"], "read");
  }

  #[test]
  fn debate_proposal_step_round_trips_through_serde() {
    let step = AgentStep::new(
      0,
      AgentStepKind::DebateProposal {
        round: 1,
        agent: "analyst-a".to_string(),
        proposal: "We should ship.".to_string(),
      },
    );

    let json = serde_json::to_value(&step).unwrap();
    assert_eq!(json["kind"]["type"], "debate_proposal");
    assert_eq!(json["kind"]["round"], 1);

    let decoded: AgentStep = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, step);
  }

  #[test]
  fn debate_verdict_without_winner_serializes_without_field() {
    let step = AgentStep::new(
      4,
      AgentStepKind::DebateVerdict {
        winner: None,
        rationale: "Synthesised both proposals.".to_string(),
      },
    );

    let json = serde_json::to_value(&step).unwrap();
    assert_eq!(json["kind"]["type"], "debate_verdict");
    assert!(json["kind"].get("winner").is_none());

    let decoded: AgentStep = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, step);
  }

  // ── Multi-agent events (since 0.4.0) ───────────────────────────────────────

  #[test]
  fn handoff_event_round_trips_through_serde() {
    let now = Utc::now();
    let event = AgentEvent::HandoffOccurred {
      session_id: "session-1".to_string(),
      step_index: 5,
      from: "triage".to_string(),
      to: "billing".to_string(),
      timestamp: now,
    };

    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["event"], "handoff_occurred");
    assert_eq!(json["from"], "triage");

    let decoded: AgentEvent = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, event);
  }

  #[test]
  fn debate_round_started_event_round_trips_through_serde() {
    let now = Utc::now();
    let event = AgentEvent::DebateRoundStarted {
      session_id: "debate-1".to_string(),
      round: 2,
      participants: vec!["alice".to_string(), "bob".to_string()],
      timestamp: now,
    };

    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["event"], "debate_round_started");
    assert_eq!(json["participants"][1], "bob");

    let decoded: AgentEvent = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, event);
  }

  #[test]
  fn debate_verdict_event_round_trips_through_serde() {
    let now = Utc::now();
    let event = AgentEvent::DebateVerdictRendered {
      session_id: "debate-1".to_string(),
      step_index: 7,
      winner: Some("alice".to_string()),
      timestamp: now,
    };

    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["event"], "debate_verdict_rendered");
    assert_eq!(json["winner"], "alice");

    let decoded: AgentEvent = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, event);
  }

  #[test]
  fn blackboard_event_round_trips_through_serde() {
    let now = Utc::now();
    let event = AgentEvent::BlackboardWritten {
      session_id: "bb-1".to_string(),
      step_index: 3,
      op: BlackboardOpKind::Write,
      agent: "researcher".to_string(),
      key: "facts".to_string(),
      timestamp: now,
    };

    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["event"], "blackboard_written");
    assert_eq!(json["op"], "write");

    let decoded: AgentEvent = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, event);
  }

  #[test]
  fn tool_capability_decision_event_round_trips_through_serde() {
    let effective = agentflow_tools::EffectiveCapabilities::resolve(
      "shell",
      &[Capability::Exec],
      Some(&[Capability::Exec]),
      None,
      None,
    );
    let now = Utc::now();
    let event = AgentEvent::ToolCapabilityDecision {
      session_id: "session-cap".to_string(),
      step_index: 2,
      tool: "shell".to_string(),
      allowed: effective.allowed,
      required: effective.required.clone(),
      effective: effective.effective.clone(),
      denied: effective.denied.clone(),
      deny_reason: effective.deny_reason.clone(),
      trace: effective.trace.clone(),
      sandbox: effective.sandbox.clone(),
      timestamp: now,
    };

    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["event"], "tool_capability_decision");
    assert_eq!(json["allowed"], true);
    assert_eq!(json["required"][0], "exec");
    assert_eq!(json["trace"][0]["source"], "tool_required");
    // No backend attached for this synthetic decision; the field must be
    // omitted from JSON (skip_serializing_if) so old consumers keep working.
    assert!(
      json.get("sandbox").is_none(),
      "absent sandbox status must be elided from JSON for forward compatibility"
    );

    let decoded: AgentEvent = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, event);
  }

  #[test]
  fn tool_capability_decision_includes_sandbox_when_present() {
    use agentflow_tools::{SandboxEnforcement, SandboxStatus};

    let effective = agentflow_tools::EffectiveCapabilities::resolve(
      "shell",
      &[Capability::Exec],
      Some(&[Capability::Exec]),
      None,
      None,
    )
    .with_sandbox(SandboxStatus::new(
      "sandbox-exec",
      SandboxEnforcement::Enforcing,
    ));
    let event = AgentEvent::ToolCapabilityDecision {
      session_id: "session-cap-sb".to_string(),
      step_index: 1,
      tool: "shell".to_string(),
      allowed: effective.allowed,
      required: effective.required.clone(),
      effective: effective.effective.clone(),
      denied: effective.denied.clone(),
      deny_reason: effective.deny_reason.clone(),
      trace: effective.trace.clone(),
      sandbox: effective.sandbox.clone(),
      timestamp: Utc::now(),
    };

    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["sandbox"]["backend"], "sandbox-exec");
    assert_eq!(json["sandbox"]["enforcement"], "enforcing");

    let decoded: AgentEvent = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, event);
  }

  #[test]
  fn tool_capability_decision_surfaces_noop_backend_in_trace() {
    use agentflow_tools::{SandboxEnforcement, SandboxStatus};

    // Regression: a no-op backend used to be silent. The visibility rule for
    // P1.6 is that it must appear in trace events as `disabled` so operators
    // can spot misconfigured shell/script tools.
    let effective = agentflow_tools::EffectiveCapabilities::resolve(
      "shell",
      &[Capability::Exec],
      None,
      None,
      None,
    )
    .with_sandbox(SandboxStatus::new("noop", SandboxEnforcement::Disabled));
    let event = AgentEvent::ToolCapabilityDecision {
      session_id: "session-cap-noop".to_string(),
      step_index: 0,
      tool: "shell".to_string(),
      allowed: effective.allowed,
      required: effective.required.clone(),
      effective: effective.effective.clone(),
      denied: effective.denied.clone(),
      deny_reason: effective.deny_reason.clone(),
      trace: effective.trace.clone(),
      sandbox: effective.sandbox.clone(),
      timestamp: Utc::now(),
    };

    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["sandbox"]["backend"], "noop");
    assert_eq!(json["sandbox"]["enforcement"], "disabled");
  }

  // ── Q3.12.1: cooperative cancellation contract pins ───────────────────────
  //
  // These two tests pin the documented behaviour described on
  // `AgentCancellationToken`, `AgentRuntime`, and `agentflow_tools::Tool::execute`:
  //
  // 1. `tool_future_drop_runs_when_token_is_cancelled` — the *positive* path:
  //    when a tool composes only Drop-cancellable state, racing it against the
  //    cancellation token via `tokio::select!` runs its Drop guard, proving
  //    the cooperative model works end-to-end.
  //
  // 2. `detached_spawn_survives_cancellation_for_documentation` — the
  //    *limitation* path: when a tool spawns work via `tokio::spawn` and
  //    returns immediately, that detached work continues to completion after
  //    the dispatch future is dropped on cancellation. Codifies what the
  //    rustdoc warns about so anyone changing the runtime semantics has a
  //    failing test rather than a silent contract drift.
  //
  // We assert on `tokio::select!` directly rather than spinning up a full
  // ReActAgent: the runtime sites all wrap the tool future in the same
  // select-vs-`token.cancelled()` shape (see `react/agent.rs:986-1051` and
  // `plan_execute.rs:526-538`), so this is the smallest reproduction that
  // pins the contract for all current and future runtimes.

  #[tokio::test]
  async fn tool_future_drop_runs_when_token_is_cancelled() {
    struct DropFlag(Arc<AtomicBool>);
    impl Drop for DropFlag {
      fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
      }
    }

    let dropped = Arc::new(AtomicBool::new(false));
    let dropped_in_future = dropped.clone();
    let token = AgentCancellationToken::new();

    let tool_future = async move {
      let _guard = DropFlag(dropped_in_future);
      // Cooperative await that the cancellation race can drop. In real
      // tools this would be `reqwest`, `tokio::time::sleep`, or
      // `tokio::process::Child::wait`.
      tokio::time::sleep(std::time::Duration::from_secs(60)).await;
      "completed-naturally"
    };

    // Schedule cancellation just after the future starts.
    let canceller = {
      let token = token.clone();
      tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        token.cancel();
      })
    };

    let outcome = tokio::select! {
      result = tool_future => Some(result),
      _ = token.cancelled() => None,
    };

    canceller.await.unwrap();
    assert!(outcome.is_none(), "select should resolve via cancellation");
    assert!(
      dropped.load(Ordering::SeqCst),
      "DropFlag must fire when the tool future is dropped by the cancellation \
       race — this is the cooperative cancellation contract documented on \
       AgentCancellationToken and Tool::execute"
    );
  }

  #[tokio::test]
  async fn detached_spawn_survives_cancellation_for_documentation() {
    // Documents the rustdoc warning: work the tool detaches via
    // `tokio::spawn` and does NOT await inside the dispatch future is not
    // cancelled when the runtime drops the tool future. This test does not
    // exist to applaud the behaviour — it exists so anyone who later wires
    // a `Tool::cancel()` hook or per-call token has a failing test that
    // forces them to update the rustdoc on `AgentCancellationToken`,
    // `AgentRuntime`, and `Tool::execute`.

    let detached_completed = Arc::new(AtomicBool::new(false));
    let detached_flag = detached_completed.clone();
    let token = AgentCancellationToken::new();

    let tool_future = async move {
      // Simulate a tool that fans out background work and returns its
      // own future without joining the detached task.
      tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        detached_flag.store(true, Ordering::SeqCst);
      });
      // The visible future itself parks for a long time.
      tokio::time::sleep(std::time::Duration::from_secs(60)).await;
      "completed-naturally"
    };

    let canceller = {
      let token = token.clone();
      tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        token.cancel();
      })
    };

    let outcome = tokio::select! {
      result = tool_future => Some(result),
      _ = token.cancelled() => None,
    };

    canceller.await.unwrap();
    assert!(outcome.is_none(), "select should resolve via cancellation");

    // Give the detached task time to complete on its own clock. If a future
    // runtime gains a way to abort detached work transitively, this assertion
    // is the trigger to update the documented contract.
    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
    assert!(
      detached_completed.load(Ordering::SeqCst),
      "detached tokio::spawn work must outlive cancellation under the current \
       cooperative model; flipping this expectation requires updating the \
       rustdoc on AgentCancellationToken, AgentRuntime, and Tool::execute"
    );
  }
}

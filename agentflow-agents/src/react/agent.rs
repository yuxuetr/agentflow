use std::sync::Arc;
use std::time::{Duration, Instant};

use agentflow_llm::{
  AgentFlow, LLMResponse, MultimodalMessage, ToolCallRequest, ToolSpec, prompt_fingerprint,
};
use agentflow_memory::{MemoryStore, Message, Role};
use agentflow_tools::{ToolIdempotency, ToolMetadata, ToolRegistry};
use async_trait::async_trait;
use chrono::Utc;
use serde_json::{Value, json};
use tracing::{debug, info, warn};

use crate::react::parser::AgentResponse;
use crate::reflection::{ReflectionContext, ReflectionStrategy};
use crate::runtime::{
  AgentCancellationToken, AgentContext, AgentEvent, AgentMemoryHook, AgentRunResult, AgentRuntime,
  AgentRuntimeError, AgentStep, AgentStepKind, AgentStopReason, MemoryHookContext, MemoryHookKind,
  RuntimeLimits,
};

/// Phase 1 (RFC_HARNESS_LOOP_OWNERSHIP): emit an `AgentEvent` to the
/// optional live sink (if any), then push it into the run's event
/// accumulator. The live emission is inline `.await` at the event's
/// production point so an observer (the Harness bridge) sees it on the
/// same logical clock as governance side-effects that fire during tool
/// execution. With `self.live_sink == None` this is exactly
/// `$events.push(ev)` — byte-identical to the pre-Phase-1 behaviour.
macro_rules! emit_and_push {
  ($sink:expr, $events:expr, $event:expr) => {{
    let ev = $event;
    if let Some(handle) = ($sink).as_ref() {
      handle.0.emit(&ev).await;
    }
    $events.push(ev);
  }};
}

/// Error type for ReAct agent operations
#[derive(Debug, thiserror::Error)]
pub enum ReActError {
  #[error("LLM error: {0}")]
  LlmError(#[from] agentflow_llm::LLMError),

  #[error("Memory error: {0}")]
  MemoryError(#[from] agentflow_memory::MemoryError),

  #[error("Tool error: {tool} — {message}")]
  ToolError { tool: String, message: String },

  #[error("Max iterations ({0}) reached without a final answer")]
  MaxIterationsReached(usize),

  #[error("Token budget exceeded: {used} / {budget}")]
  BudgetExceeded { used: u32, budget: u32 },

  #[error("Agent run cancelled: {reason}")]
  Cancelled { reason: String },

  #[error("Memory summary error: {message}")]
  MemorySummary { message: String },

  #[error("turn-driven session already finished")]
  SessionFinished,
}

/// Input passed to a pluggable memory summary backend.
///
/// The runtime hands the backend the messages it had to drop in order to
/// fit the prompt budget along with the messages that were kept. Backends
/// can use either or both to produce a single string summary that the
/// runtime then prepends to the prompt as a synthetic system message.
#[derive(Debug, Clone)]
pub struct MemorySummaryContext {
  /// Session id of the run requesting the summary.
  pub session_id: String,
  /// Configured prompt-memory budget in approximate tokens.
  pub budget_tokens: u32,
  /// Approximate token count of the dropped messages.
  pub omitted_tokens: u32,
  /// Messages that did not fit and need summarising.
  pub omitted_messages: Vec<Message>,
  /// Messages that were kept verbatim in the prompt.
  pub kept_messages: Vec<Message>,
}

/// Pluggable backend for summarising prompt memory that exceeds a budget.
///
/// A backend receives a [`MemorySummaryContext`] describing what was kept
/// vs. dropped and returns:
///
/// - `Ok(Some(summary))` to inject `summary` as a synthetic system message
///   ahead of the kept messages.
/// - `Ok(None)` to skip the summary entirely (the runtime will silently
///   continue with truncated history).
/// - `Err(ReActError::MemorySummary { .. })` to surface a real failure.
///
/// Backends can be deterministic (rule-based) or LLM-backed; both should
/// stay on the synchronous side of the ReAct loop, so heavy work belongs
/// behind a separate task with a tight timeout.
#[async_trait]
pub trait MemorySummaryBackend: Send + Sync {
  /// Stable backend name (e.g. `"recent_only"`, `"compact"`).
  fn name(&self) -> &'static str;

  /// Produce an optional summary string for the omitted slice of memory.
  async fn summarize(&self, context: MemorySummaryContext) -> Result<Option<String>, ReActError>;
}

/// Summary backend that only records how much history was omitted.
#[derive(Debug, Default, Clone)]
pub struct RecentOnlyMemorySummary;

#[async_trait]
impl MemorySummaryBackend for RecentOnlyMemorySummary {
  fn name(&self) -> &'static str {
    "recent_only"
  }

  async fn summarize(&self, context: MemorySummaryContext) -> Result<Option<String>, ReActError> {
    Ok(Some(format!(
      "[Memory Summary]\n{} older messages omitted to fit the prompt memory budget (approx {} tokens).",
      context.omitted_messages.len(),
      context.omitted_tokens
    )))
  }
}

/// Deterministic rule-based summary backend for older prompt memory.
#[derive(Debug, Default, Clone)]
pub struct CompactMemorySummary;

#[async_trait]
impl MemorySummaryBackend for CompactMemorySummary {
  fn name(&self) -> &'static str {
    "compact"
  }

  async fn summarize(&self, context: MemorySummaryContext) -> Result<Option<String>, ReActError> {
    Ok(Some(compact_memory_summary(
      &context.omitted_messages,
      context.omitted_tokens,
    )))
  }
}

/// Configuration for a [`ReActAgent`].
#[derive(Debug, Clone)]
pub struct ReActConfig {
  /// LLM model identifier (e.g. `"gpt-4o"`, `"claude-3-5-sonnet"`)
  pub model: String,

  /// Optional persona / task description prepended to the system prompt.
  pub persona: Option<String>,

  /// Maximum number of tool-call iterations before giving up.
  pub max_iterations: usize,

  /// Stop after the session accumulates more than this many estimated tokens.
  /// `None` disables the token budget guard.
  pub budget_tokens: Option<u32>,

  /// Terminate if any of these strings appear in the LLM response.
  pub stop_conditions: Vec<String>,

  /// Enable reflection strategy execution when a strategy is attached.
  pub reflection_enabled: bool,

  /// Optional token budget for memory included in each LLM prompt.
  pub memory_prompt_token_budget: Option<u32>,

  /// Strategy used when prompt memory exceeds `memory_prompt_token_budget`.
  pub memory_summary_strategy: MemorySummaryStrategy,
}

/// Strategy used to fit conversation memory into a prompt budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemorySummaryStrategy {
  /// Keep legacy behavior and include full memory.
  Disabled,
  /// Drop older messages and keep only the newest messages that fit.
  RecentOnly,
  /// Replace older messages with a deterministic compact summary.
  Compact,
}

impl Default for ReActConfig {
  fn default() -> Self {
    Self {
      model: "gpt-4o".to_string(),
      persona: None,
      max_iterations: 15,
      budget_tokens: Some(50_000),
      stop_conditions: vec![],
      reflection_enabled: true,
      memory_prompt_token_budget: None,
      memory_summary_strategy: MemorySummaryStrategy::Disabled,
    }
  }
}

impl ReActConfig {
  pub fn new(model: impl Into<String>) -> Self {
    Self {
      model: model.into(),
      ..Default::default()
    }
  }

  pub fn with_persona(mut self, persona: impl Into<String>) -> Self {
    self.persona = Some(persona.into());
    self
  }

  pub fn with_max_iterations(mut self, n: usize) -> Self {
    self.max_iterations = n;
    self
  }

  pub fn with_budget_tokens(mut self, tokens: u32) -> Self {
    self.budget_tokens = Some(tokens);
    self
  }

  pub fn with_stop_conditions(mut self, conditions: Vec<String>) -> Self {
    self.stop_conditions = conditions;
    self
  }

  pub fn with_reflection_enabled(mut self, enabled: bool) -> Self {
    self.reflection_enabled = enabled;
    self
  }

  pub fn with_memory_prompt_token_budget(mut self, tokens: u32) -> Self {
    self.memory_prompt_token_budget = Some(tokens);
    self
  }

  pub fn with_memory_summary_strategy(mut self, strategy: MemorySummaryStrategy) -> Self {
    self.memory_summary_strategy = strategy;
    self
  }
}

/// An autonomous ReAct (Reasoning + Acting) agent.
///
/// On each call to [`ReActAgent::run`], the agent:
/// 1. Stores the user message in memory.
/// 2. Iterates: builds a prompt from memory, calls the LLM, parses the response.
/// 3. If the LLM returns a tool call, executes it and appends the result to memory.
/// 4. If the LLM returns a final answer, stores it and returns.
///
/// ## Example
/// ```rust,no_run
/// use agentflow_agents::react::{ReActAgent, ReActConfig};
/// use agentflow_memory::SessionMemory;
/// use agentflow_tools::{ToolRegistry, SandboxPolicy};
/// use agentflow_tools::builtin::ShellTool;
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() {
///     agentflow_llm::AgentFlow::init().await.unwrap();
///
///     let policy = Arc::new(SandboxPolicy::default());
///     let mut registry = ToolRegistry::new();
///     registry.register(Arc::new(ShellTool::new(policy)));
///
///     let mut agent = ReActAgent::new(
///         ReActConfig::new("gpt-4o"),
///         Box::new(SessionMemory::default_window()),
///         Arc::new(registry),
///     );
///
///     let answer = agent.run("What is today's date?").await.unwrap();
///     println!("{}", answer);
/// }
/// ```
pub struct ReActAgent {
  config: ReActConfig,
  memory: Box<dyn MemoryStore>,
  tools: Arc<ToolRegistry>,
  reflection: Option<Arc<dyn ReflectionStrategy>>,
  memory_hook: Option<Arc<dyn AgentMemoryHook>>,
  memory_summary_backend: Option<Arc<dyn MemorySummaryBackend>>,
  /// Stable identifier for this agent's conversation session
  pub session_id: String,
  /// Token counter used for every `Message::*_with_counter` call
  /// in the run loop (P10.3.3-FU1). Initialised lazily in
  /// `apply_context` from `context.model` so the per-message
  /// `token_count` reflects the real tokenizer for the target
  /// provider — `apply_memory_prompt_budget` then enforces the
  /// budget against the same numbers the LLM will actually bill.
  /// Defaults to the heuristic until the first context arrives.
  message_counter: Box<dyn agentflow_memory::TokenCounter>,
  /// Phase 1 (RFC_HARNESS_LOOP_OWNERSHIP): optional live event observer
  /// captured from `AgentContext::event_sink` at the start of
  /// `run_with_context`. When set, the loop emits each `AgentEvent` to it
  /// as it is produced (in addition to accumulating it into the result),
  /// so the Harness bridge sees tool events on the same logical clock as
  /// the governance events that fire during tool execution. `None` keeps
  /// behavior byte-identical to a runtime with no observer.
  live_sink: Option<crate::runtime::EventSinkHandle>,
}

impl ReActAgent {
  pub fn new(config: ReActConfig, memory: Box<dyn MemoryStore>, tools: Arc<ToolRegistry>) -> Self {
    let session_id = uuid::Uuid::new_v4().to_string();
    // Build the initial counter from the configured model so
    // agents created without a context (e.g. construction-time
    // dogfooding tools) still produce sane counts. `apply_context`
    // updates this if the run's context overrides the model.
    let message_counter = crate::token_counter_adapter::build_message_counter(&config.model);
    Self {
      config,
      memory,
      tools,
      reflection: None,
      memory_hook: None,
      memory_summary_backend: None,
      session_id,
      message_counter,
      live_sink: None,
    }
  }

  /// Continue an existing session by reusing a known `session_id`.
  pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
    self.session_id = session_id.into();
    self
  }

  /// Read-only view of the agent's tool registry.
  ///
  /// Useful for callers that want to introspect which tools are admitted —
  /// for example, the eval harness asserting that admission filters were
  /// applied, or `agentflow skill inspect` rendering the resolved set.
  pub fn tools(&self) -> &Arc<ToolRegistry> {
    &self.tools
  }

  /// Replace the agent's tool registry (builder-style setter).
  ///
  /// Used by callers that need to wrap or transform the registry after
  /// the agent has been constructed — for example, the
  /// `agentflow harness run` CLI uses this to install
  /// `agentflow_harness::wrap_registry`'s approval-gate pipeline
  /// around tools that came from `SkillBuilder::build`, without having
  /// to duplicate the manifest/persona/memory wiring.
  ///
  /// The provided `Arc` becomes the canonical registry for the rest of
  /// the agent's lifetime; subsequent `tools()` calls return this new
  /// `Arc`.
  pub fn with_tools(mut self, tools: Arc<ToolRegistry>) -> Self {
    self.tools = tools;
    self
  }

  /// Attach a reflection strategy to the runtime trace.
  pub fn with_reflection_strategy(mut self, strategy: Arc<dyn ReflectionStrategy>) -> Self {
    self.reflection = Some(strategy);
    self
  }

  /// Attach a memory hook that observes loop reads, searches, and writes.
  pub fn with_memory_hook(mut self, hook: Arc<dyn AgentMemoryHook>) -> Self {
    self.memory_hook = Some(hook);
    self
  }

  /// Attach a custom memory summary backend used when prompt memory exceeds budget.
  pub fn with_memory_summary_backend(mut self, backend: Arc<dyn MemorySummaryBackend>) -> Self {
    self.memory_summary_backend = Some(backend);
    self
  }

  /// Build the LLM message list without calling a model.
  ///
  /// This is useful for prompt debugging and prompt assembly benchmarks.
  pub async fn preview_llm_messages(&self) -> Result<Vec<MultimodalMessage>, ReActError> {
    let system_prompt = self.build_system_prompt();
    self.build_llm_messages(&system_prompt).await
  }

  /// Run the agent on a new user message and return the final answer.
  pub async fn run(&mut self, user_input: &str) -> Result<String, ReActError> {
    let result = self
      .run_with_context(self.context_for_input(user_input))
      .await?;
    Self::answer_from_result(result)
  }

  /// Run the agent on a new user message and return structured runtime output.
  pub async fn run_with_trace(&mut self, user_input: &str) -> Result<AgentRunResult, ReActError> {
    self
      .run_with_context(self.context_for_input(user_input))
      .await
  }

  /// Resume a partial run from a previously serialized runtime trace.
  ///
  /// This first-stage resume support restores durable observations into memory
  /// and continues the loop from a fresh prompt. It deliberately refuses traces
  /// that end with an unresolved tool call, because resuming those would require
  /// re-running a tool whose side effects are unknown.
  pub async fn resume_with_context(
    &mut self,
    context: AgentContext,
    mut prior: AgentRunResult,
  ) -> Result<AgentRunResult, ReActError> {
    if prior.stop_reason.is_success() {
      return Ok(prior);
    }
    self
      .replay_resume_safe_unresolved_tool_calls(&mut prior)
      .await?;
    if has_unresolved_tool_call(&prior) {
      return Err(ReActError::ToolError {
        tool: "runtime".to_string(),
        message: "cannot resume trace with unresolved non-idempotent or unknown tool call"
          .to_string(),
      });
    }

    self.apply_context(&context);
    self.restore_trace_memory(&prior).await?;

    let continuation = if context.input.trim().is_empty() {
      "Continue from the recovered tool observations and produce the next action or final answer."
        .to_string()
    } else {
      format!(
        "{}\n\nContinue from the recovered tool observations. Do not repeat tool calls whose results are already present unless new information is required.",
        context.input
      )
    };
    let resumed = self
      .run_with_context(AgentContext {
        input: continuation,
        ..context
      })
      .await?;

    Ok(merge_resumed_result(prior, resumed))
  }

  async fn replay_resume_safe_unresolved_tool_calls(
    &self,
    prior: &mut AgentRunResult,
  ) -> Result<(), ReActError> {
    let unresolved_calls: Vec<(usize, String, Value)> = prior
      .steps
      .iter()
      .filter_map(|step| {
        let AgentStepKind::ToolCall { tool, params } = &step.kind else {
          return None;
        };
        let has_result = prior.steps.iter().any(|candidate| {
          matches!(
            &candidate.kind,
            AgentStepKind::ToolResult {
              tool: result_tool,
              ..
            } if result_tool == tool && candidate.index > step.index
          )
        });
        if has_result {
          None
        } else {
          Some((step.index, tool.clone(), params.clone()))
        }
      })
      .collect();

    let mut next_index = prior.steps.iter().map(|step| step.index).max().unwrap_or(0) + 1;

    for (_step_index, tool, params) in unresolved_calls {
      if !is_resume_safe_tool_call(&params) {
        continue;
      }
      let execute_params = strip_agentflow_metadata(params);
      let output = match self.tools.execute(&tool, execute_params).await {
        Ok(output) => output,
        Err(error) => agentflow_tools::ToolOutput::error(error.to_string()),
      };
      prior.steps.push(AgentStep::new(
        next_index,
        AgentStepKind::ToolResult {
          tool,
          content: output.content,
          is_error: output.is_error,
          parts: output.parts,
        },
      ));
      next_index += 1;
    }

    Ok(())
  }

  /// Query memory for this agent's current session.
  ///
  /// The backing [`MemoryStore`] decides retrieval behavior. With
  /// `agentflow_memory::SemanticMemory`, this performs semantic vector search
  /// with keyword fallback; simpler stores may use keyword matching.
  pub async fn query_memory(&self, query: &str, limit: usize) -> Result<Vec<Message>, ReActError> {
    self
      .query_session_memory(&self.session_id, query, limit)
      .await
  }

  /// Query memory for an explicit session id.
  pub async fn query_session_memory(
    &self,
    session_id: &str,
    query: &str,
    limit: usize,
  ) -> Result<Vec<Message>, ReActError> {
    self.search_memory(session_id, query, limit).await
  }

  /// Run the agent and return structured runtime steps and events.
  pub async fn run_with_context(
    &mut self,
    context: AgentContext,
  ) -> Result<AgentRunResult, ReActError> {
    let mut st = self.init_run(&context).await?;
    loop {
      match self.run_one_turn(&mut st).await? {
        TurnStep::Continue => {}
        TurnStep::Stop(result) => return Ok(result),
      }
    }
  }

  /// Set up a run — apply context, capture the live sink, store the user
  /// message, build the system prompt — and return the initial
  /// [`LoopState`]. Shared by [`Self::run_with_context`] (the
  /// batteries-included driver) and [`Self::begin_turn_driven`] (the
  /// caller-owned turn-driven driver). RFC_HARNESS_LOOP_OWNERSHIP §6.
  ///
  /// F-A2-13 anti-loop steering: `last_tool_call` starts `None`; a repeat
  /// single-tool call with identical params later gets a steering note
  /// (see `dispatch_single_tool_call`).
  async fn init_run(&mut self, context: &AgentContext) -> Result<LoopState, ReActError> {
    self.apply_context(context);
    // Phase 1: capture the optional live event observer for this run.
    self.live_sink = context.event_sink.clone();
    info!(
        session = %self.session_id,
        model = %self.config.model,
        "ReActAgent starting"
    );

    self
      .add_memory_message(Message::user_with_counter(
        &self.session_id,
        &context.input,
        &*self.message_counter,
      ))
      .await?;

    let system_prompt = self.build_system_prompt();

    Ok(LoopState {
      steps: vec![AgentStep::new(
        0,
        AgentStepKind::Observe {
          input: context.input.clone(),
        },
      )],
      events: vec![AgentEvent::RunStarted {
        session_id: self.session_id.clone(),
        model: self.config.model.clone(),
        timestamp: context.started_at,
      }],
      step_index: 1,
      iteration: 0,
      tool_calls: 0,
      last_tool_call: None,
      max_iterations: context
        .limits
        .max_steps
        .unwrap_or(self.config.max_iterations),
      max_tool_calls: context.limits.max_tool_calls,
      timeout_ms: context.limits.timeout_ms,
      budget_tokens: context.limits.token_budget.or(self.config.budget_tokens),
      cancellation_token: context.cancellation_token.clone(),
      run_started_at: Instant::now(),
      system_prompt,
      trace_context: context.trace_context.clone(),
      between_turn_hook: context.between_turn_hook.clone(),
    })
  }

  /// Begin a **turn-driven** run: set up the session and hand back a
  /// [`ReActLoopSession`] the caller pumps one turn at a time via
  /// [`ReActLoopSession::next_turn`], performing its own context
  /// engineering (e.g. memory compaction) between turns. This is the
  /// loop-ownership seam of RFC_HARNESS_LOOP_OWNERSHIP §6 — the same per
  /// turn machinery as [`Self::run_with_context`], but the caller owns
  /// the loop.
  pub async fn begin_turn_driven(
    &mut self,
    context: AgentContext,
  ) -> Result<ReActLoopSession<'_>, ReActError> {
    let state = self.init_run(&context).await?;
    Ok(ReActLoopSession {
      agent: self,
      state,
      finished: false,
    })
  }

  /// Borrow the run's conversation memory (used by a turn-driven driver
  /// to compact/inspect context between turns).
  pub fn memory_ref(&self) -> &dyn MemoryStore {
    &*self.memory
  }

  /// Execute exactly one turn of the ReAct loop against `st`, returning
  /// `TurnStep::Continue` to advance or `TurnStep::Stop` with the
  /// terminal result. This is the loop body lifted whole out of
  /// `run_with_context` (RFC_HARNESS_LOOP_OWNERSHIP §6 step 5); the
  /// `run_with_context` loop is now just `loop { match run_one_turn … }`.
  /// Callable in isolation, it is the seam a `LoopSession` (step 6) drives.
  async fn run_one_turn(&mut self, st: &mut LoopState) -> Result<TurnStep, ReActError> {
    // Top-of-turn limit guards (cancel / timeout / max-steps / budget).
    if let Some(result) = self
      .check_turn_limits(
        &mut st.steps,
        &mut st.events,
        &mut st.step_index,
        st.iteration,
        st.run_started_at,
        st.timeout_ms,
        st.max_iterations,
        st.budget_tokens,
        &st.cancellation_token,
      )
      .await?
    {
      return Ok(TurnStep::Stop(result));
    }

    // LLM call (the Phase 2b between-turn hook fires inside).
    let (llm_response, raw_response) = match self
      .run_turn_llm_call(
        &mut st.steps,
        &mut st.events,
        &mut st.step_index,
        st.iteration,
        &st.system_prompt,
        st.trace_context.clone(),
        &st.between_turn_hook,
        st.run_started_at,
        st.timeout_ms,
        &st.cancellation_token,
      )
      .await?
    {
      LlmTurnOutcome::Proceed {
        llm_response,
        raw_response,
      } => (llm_response, raw_response),
      LlmTurnOutcome::Stop(result) => return Ok(TurnStep::Stop(result)),
    };

    // Stop conditions.
    if let Some(result) = self
      .check_stop_conditions(
        &mut st.steps,
        &mut st.events,
        &mut st.step_index,
        &raw_response,
      )
      .await?
    {
      return Ok(TurnStep::Stop(result));
    }

    // Multi-call batch path (P-H.3): >=2 native tool calls in one
    // response dispatch as a batch (concurrent for idempotent, serial
    // otherwise) in LLM-returned order.
    if llm_response.tool_calls.len() >= 2 {
      match self
        .dispatch_native_tool_calls_batch(
          &llm_response.tool_calls,
          &raw_response,
          &mut st.steps,
          &mut st.events,
          &mut st.step_index,
          &mut st.tool_calls,
          st.max_tool_calls,
          st.run_started_at,
          st.timeout_ms,
          st.cancellation_token.as_ref(),
        )
        .await?
      {
        BatchOutcome::Continue => {
          st.iteration += 1;
          return Ok(TurnStep::Continue);
        }
        BatchOutcome::Stop(result) => return Ok(TurnStep::Stop(*result)),
      }
    }

    // Parse response: prefer native tool_calls when present.
    let parsed = if let Some(call) = llm_response.tool_calls.first() {
      native_tool_call_to_agent_response(call)
    } else {
      AgentResponse::parse(&raw_response)
    };

    // Store the assistant turn.
    self
      .add_memory_message(Message::assistant_with_counter(
        &self.session_id,
        &raw_response,
        &*self.message_counter,
      ))
      .await?;

    match parsed {
      AgentResponse::Action {
        thought,
        tool,
        params,
      } => match self
        .dispatch_single_tool_call(
          thought,
          tool,
          params,
          &mut st.steps,
          &mut st.events,
          &mut st.step_index,
          &mut st.tool_calls,
          &mut st.last_tool_call,
          st.iteration,
          st.max_tool_calls,
          st.run_started_at,
          st.timeout_ms,
          &st.cancellation_token,
        )
        .await?
      {
        TurnStep::Continue => {
          st.iteration += 1;
          Ok(TurnStep::Continue)
        }
        TurnStep::Stop(result) => Ok(TurnStep::Stop(result)),
      },

      AgentResponse::Answer { thought, answer } => {
        // Q5.2: `thought` routinely contains user input verbatim —
        // fingerprint + length only at INFO; full text at TRACE.
        info!(
          thought_len = thought.len(),
          thought_sha = %prompt_fingerprint(&thought),
          "Final answer reached"
        );
        tracing::trace!(thought = %thought, "Final answer thought body");
        if !thought.trim().is_empty() {
          st.steps.push(AgentStep::new(
            st.step_index,
            AgentStepKind::Plan { thought },
          ));
          st.step_index += 1;
        }
        st.steps.push(AgentStep::new(
          st.step_index,
          AgentStepKind::FinalAnswer {
            answer: answer.clone(),
          },
        ));
        st.step_index += 1;
        self
          .record_reflection(
            ReflectionContext::final_answer(&self.session_id, st.step_index, &answer),
            &mut st.step_index,
            &mut st.steps,
            &mut st.events,
          )
          .await?;
        Ok(TurnStep::Stop(Self::stopped_result(
          &self.session_id,
          Some(answer),
          AgentStopReason::FinalAnswer,
          std::mem::take(&mut st.steps),
          std::mem::take(&mut st.events),
        )))
      }

      AgentResponse::Malformed(text) => {
        warn!("LLM returned non-JSON text; treating as final answer");
        st.steps.push(AgentStep::new(
          st.step_index,
          AgentStepKind::FinalAnswer {
            answer: text.clone(),
          },
        ));
        st.step_index += 1;
        self
          .record_reflection(
            ReflectionContext::final_answer(&self.session_id, st.step_index, &text),
            &mut st.step_index,
            &mut st.steps,
            &mut st.events,
          )
          .await?;
        Ok(TurnStep::Stop(Self::stopped_result(
          &self.session_id,
          Some(text),
          AgentStopReason::FinalAnswer,
          std::mem::take(&mut st.steps),
          std::mem::take(&mut st.events),
        )))
      }
    }
  }

  fn context_for_input(&self, user_input: &str) -> AgentContext {
    let mut context = AgentContext::new(&self.session_id, user_input, &self.config.model)
      .with_limits(RuntimeLimits {
        max_steps: Some(self.config.max_iterations),
        max_tool_calls: None,
        timeout_ms: None,
        token_budget: self.config.budget_tokens,
      });
    if let Some(persona) = &self.config.persona {
      context = context.with_persona(persona.clone());
    }
    context
  }

  fn apply_context(&mut self, context: &AgentContext) {
    self.session_id = context.session_id.clone();
    if !context.model.trim().is_empty() {
      self.config.model = context.model.clone();
      // Rebuild the per-message tokenizer when the model changes
      // so the precision claims in `apply_memory_prompt_budget`
      // match the model the run actually targets (P10.3.3-FU1).
      self.message_counter = crate::token_counter_adapter::build_message_counter(&context.model);
    }
    if let Some(persona) = &context.persona {
      self.config.persona = Some(persona.clone());
    }
  }

  fn stopped_result(
    session_id: &str,
    answer: Option<String>,
    reason: AgentStopReason,
    steps: Vec<AgentStep>,
    mut events: Vec<AgentEvent>,
  ) -> AgentRunResult {
    events.push(AgentEvent::RunStopped {
      session_id: session_id.to_string(),
      reason: reason.clone(),
      timestamp: Utc::now(),
    });
    AgentRunResult {
      session_id: session_id.to_string(),
      answer,
      stop_reason: reason,
      steps,
      events,
    }
  }

  fn cancelled_result(
    session_id: &str,
    reason: impl Into<String>,
    steps: Vec<AgentStep>,
    events: Vec<AgentEvent>,
  ) -> AgentRunResult {
    Self::stopped_result(
      session_id,
      None,
      AgentStopReason::Cancelled {
        message: reason.into(),
      },
      steps,
      events,
    )
  }

  /// Run one turn's LLM call: between-turn hook, prompt assembly, the
  /// model round-trip (with timeout/cancellation racing), and the
  /// `LlmCallCompleted` event. Returns the parsed-ready response, or a
  /// terminal result when the turn must stop (cancel / timeout).
  ///
  /// Turn-driven extraction (RFC_HARNESS_LOOP_OWNERSHIP §6, series step
  /// 2): pure relocation out of the `run_with_context` loop; `steps` /
  /// `events` are consumed (via `mem::take`) only on the stop paths.
  #[allow(clippy::too_many_arguments)]
  async fn run_turn_llm_call(
    &self,
    steps: &mut Vec<AgentStep>,
    events: &mut Vec<AgentEvent>,
    step_index: &mut usize,
    iteration: usize,
    system_prompt: &str,
    trace_context: Option<agentflow_llm::LlmTraceContext>,
    between_turn_hook: &Option<crate::runtime::BetweenTurnHookHandle>,
    run_started_at: Instant,
    timeout_ms: Option<u64>,
    cancellation_token: &Option<AgentCancellationToken>,
  ) -> Result<LlmTurnOutcome, ReActError> {
    // Phase 2b between-turn control point.
    if let Some(hook) = between_turn_hook {
      hook
        .0
        .before_turn(iteration, &self.session_id, &*self.memory)
        .await;
    }
    let messages = self.build_llm_messages(system_prompt).await?;

    if is_cancelled(cancellation_token) {
      return Ok(LlmTurnOutcome::Stop(Self::cancelled_result(
        &self.session_id,
        "cancellation token signalled",
        std::mem::take(steps),
        std::mem::take(events),
      )));
    }

    debug!(iteration, "Calling LLM");
    let tool_specs = self.collect_tool_specs();
    let mut builder = AgentFlow::model(&self.config.model)
      .multimodal_messages(messages)
      .trace_context(trace_context);
    if !tool_specs.is_empty() {
      builder = builder.tools(tool_specs);
    }
    let llm_call_started = std::time::Instant::now();
    let llm_call = builder.execute_full();
    let llm_response: LLMResponse = match (
      remaining_timeout(run_started_at, timeout_ms),
      cancellation_token.clone(),
    ) {
      (Some(remaining), Some(token)) => {
        tokio::select! {
          result = tokio::time::timeout(remaining, llm_call) => match result {
            Ok(result) => result?,
            Err(_) => {
              self
                .record_reflection(
                  ReflectionContext::failure(
                    &self.session_id,
                    *step_index,
                    format!(
                      "runtime timed out after {}ms",
                      timeout_ms.unwrap_or_default()
                    ),
                  ),
                  step_index,
                  steps,
                  events,
                )
                .await?;
              return Ok(LlmTurnOutcome::Stop(Self::stopped_result(
                &self.session_id,
                None,
                AgentStopReason::Timeout {
                  timeout_ms: timeout_ms.unwrap_or_default(),
                },
                std::mem::take(steps),
                std::mem::take(events),
              )));
            }
          },
          _ = token.cancelled() => {
            return Ok(LlmTurnOutcome::Stop(Self::cancelled_result(
              &self.session_id,
              "cancellation token signalled",
              std::mem::take(steps),
              std::mem::take(events),
            )));
          }
        }
      }
      (Some(remaining), None) => match tokio::time::timeout(remaining, llm_call).await {
        Ok(result) => result?,
        Err(_) => {
          self
            .record_reflection(
              ReflectionContext::failure(
                &self.session_id,
                *step_index,
                format!(
                  "runtime timed out after {}ms",
                  timeout_ms.unwrap_or_default()
                ),
              ),
              step_index,
              steps,
              events,
            )
            .await?;
          return Ok(LlmTurnOutcome::Stop(Self::stopped_result(
            &self.session_id,
            None,
            AgentStopReason::Timeout {
              timeout_ms: timeout_ms.unwrap_or_default(),
            },
            std::mem::take(steps),
            std::mem::take(events),
          )));
        }
      },
      (None, Some(token)) => {
        tokio::select! {
          result = llm_call => result?,
          _ = token.cancelled() => {
            return Ok(LlmTurnOutcome::Stop(Self::cancelled_result(
              &self.session_id,
              "cancellation token signalled",
              std::mem::take(steps),
              std::mem::take(events),
            )));
          }
        }
      }
      (None, None) => llm_call.await?,
    };

    let usage = llm_response.usage.as_ref();
    events.push(AgentEvent::LlmCallCompleted {
      session_id: self.session_id.clone(),
      step_index: *step_index,
      model: self.config.model.clone(),
      prompt_tokens: usage.and_then(|u| u.prompt_tokens),
      completion_tokens: usage.and_then(|u| u.completion_tokens),
      total_tokens: usage.and_then(|u| u.total_tokens),
      duration_ms: llm_call_started.elapsed().as_millis() as u64,
      timestamp: chrono::Utc::now(),
    });

    let raw_response = llm_response.content.clone();
    debug!(
      response_len = raw_response.len(),
      response_sha = %prompt_fingerprint(&raw_response),
      "LLM responded"
    );
    tracing::trace!(response = %raw_response, "LLM response body");

    Ok(LlmTurnOutcome::Proceed {
      llm_response,
      raw_response,
    })
  }

  /// After the LLM call, stop the run if the response contains any
  /// configured stop string. Returns `Some(result)` to stop, `None` to
  /// continue to the parse/dispatch phase.
  ///
  /// Turn-driven extraction (RFC_HARNESS_LOOP_OWNERSHIP §6, series step
  /// 3): pure relocation; `steps`/`events` are consumed via `mem::take`
  /// only on the stop path.
  async fn check_stop_conditions(
    &mut self,
    steps: &mut Vec<AgentStep>,
    events: &mut Vec<AgentEvent>,
    step_index: &mut usize,
    raw_response: &str,
  ) -> Result<Option<AgentRunResult>, ReActError> {
    let Some(condition) = self
      .config
      .stop_conditions
      .iter()
      .find(|cond| raw_response.contains(cond.as_str()))
      .cloned()
    else {
      return Ok(None);
    };
    info!("Stop condition matched: '{}'", condition);
    self
      .add_memory_message(Message::assistant_with_counter(
        &self.session_id,
        raw_response,
        &*self.message_counter,
      ))
      .await?;
    self
      .record_reflection(
        ReflectionContext::final_answer(&self.session_id, *step_index, raw_response),
        step_index,
        steps,
        events,
      )
      .await?;
    Ok(Some(Self::stopped_result(
      &self.session_id,
      Some(raw_response.to_string()),
      AgentStopReason::StopCondition { condition },
      std::mem::take(steps),
      std::mem::take(events),
    )))
  }

  /// Execute one tool call under the run's timeout/cancellation limits,
  /// racing the tool future against the deadline and the cancellation
  /// token. Returns `Output(tool_output)` on completion (success or a
  /// tool-level error, which is surfaced as an error `ToolOutput`), or
  /// `Stop(result)` when the run must terminate (timeout / cancellation).
  ///
  /// Turn-driven extraction (RFC_HARNESS_LOOP_OWNERSHIP §6, series step
  /// 3b): the gnarly tool-execute `select!` block lifted out of the
  /// `Action` arm, mirroring `run_turn_llm_call`. The future is created
  /// inside so the borrow of `self.tools` does not outlive this call.
  /// `steps`/`events` are consumed (via `mem::take`) only on stop paths.
  #[allow(clippy::too_many_arguments)]
  async fn execute_tool_with_limits(
    &self,
    tool: &str,
    params: serde_json::Value,
    tool_step_index: usize,
    tool_source: &Option<String>,
    tool_permissions: &[String],
    started_at: Instant,
    steps: &mut Vec<AgentStep>,
    events: &mut Vec<AgentEvent>,
    step_index: &mut usize,
    run_started_at: Instant,
    timeout_ms: Option<u64>,
    cancellation_token: &Option<AgentCancellationToken>,
  ) -> Result<ToolExecOutcome, ReActError> {
    let tool_call = self.tools.execute(tool, params);
    let tool_output = match (
      remaining_timeout(run_started_at, timeout_ms),
      cancellation_token.clone(),
    ) {
      (Some(remaining), Some(token)) => {
        tokio::select! {
          result = tokio::time::timeout(remaining, tool_call) => match result {
            Ok(result) => match result {
              Ok(out) => out,
              Err(e) => {
                warn!(tool = %tool, error = %e, "Tool execution failed");
                agentflow_tools::ToolOutput::error(e.to_string())
              }
            },
            Err(_) => {
              let duration_ms = started_at.elapsed().as_millis() as u64;
              emit_and_push!(self.live_sink, events, AgentEvent::ToolCallCompleted {
                session_id: self.session_id.clone(),
                step_index: tool_step_index,
                tool: tool.to_string(),
                is_error: true,
                duration_ms,
                source: tool_source.clone(),
                permissions: tool_permissions.to_vec(),
                timestamp: Utc::now(),
              });
              self
                .record_reflection(
                  ReflectionContext::failure(
                    &self.session_id,
                    *step_index,
                    format!(
                      "runtime timed out after {}ms",
                      timeout_ms.unwrap_or_default()
                    ),
                  ),
                  step_index,
                  steps,
                  events,
                )
                .await?;
              return Ok(ToolExecOutcome::Stop(Self::stopped_result(
                &self.session_id,
                None,
                AgentStopReason::Timeout {
                  timeout_ms: timeout_ms.unwrap_or_default(),
                },
                std::mem::take(steps),
                std::mem::take(events),
              )));
            }
          },
          _ = token.cancelled() => {
            emit_and_push!(self.live_sink, events, AgentEvent::ToolCallCompleted {
              session_id: self.session_id.clone(),
              step_index: tool_step_index,
              tool: tool.to_string(),
              is_error: true,
              duration_ms: started_at.elapsed().as_millis() as u64,
              source: tool_source.clone(),
              permissions: tool_permissions.to_vec(),
              timestamp: Utc::now(),
            });
            return Ok(ToolExecOutcome::Stop(Self::cancelled_result(
              &self.session_id,
              "cancellation token signalled",
              std::mem::take(steps),
              std::mem::take(events),
            )));
          }
        }
      }
      (Some(remaining), None) => match tokio::time::timeout(remaining, tool_call).await {
        Ok(result) => match result {
          Ok(out) => out,
          Err(e) => {
            warn!(tool = %tool, error = %e, "Tool execution failed");
            agentflow_tools::ToolOutput::error(e.to_string())
          }
        },
        Err(_) => {
          let duration_ms = started_at.elapsed().as_millis() as u64;
          emit_and_push!(
            self.live_sink,
            events,
            AgentEvent::ToolCallCompleted {
              session_id: self.session_id.clone(),
              step_index: tool_step_index,
              tool: tool.to_string(),
              is_error: true,
              duration_ms,
              source: tool_source.clone(),
              permissions: tool_permissions.to_vec(),
              timestamp: Utc::now(),
            }
          );
          self
            .record_reflection(
              ReflectionContext::failure(
                &self.session_id,
                *step_index,
                format!(
                  "runtime timed out after {}ms",
                  timeout_ms.unwrap_or_default()
                ),
              ),
              step_index,
              steps,
              events,
            )
            .await?;
          return Ok(ToolExecOutcome::Stop(Self::stopped_result(
            &self.session_id,
            None,
            AgentStopReason::Timeout {
              timeout_ms: timeout_ms.unwrap_or_default(),
            },
            std::mem::take(steps),
            std::mem::take(events),
          )));
        }
      },
      (None, Some(token)) => {
        tokio::select! {
          result = tool_call => match result {
            Ok(out) => out,
            Err(e) => {
              warn!(tool = %tool, error = %e, "Tool execution failed");
              agentflow_tools::ToolOutput::error(e.to_string())
            }
          },
          _ = token.cancelled() => {
            emit_and_push!(self.live_sink, events, AgentEvent::ToolCallCompleted {
              session_id: self.session_id.clone(),
              step_index: tool_step_index,
              tool: tool.to_string(),
              is_error: true,
              duration_ms: started_at.elapsed().as_millis() as u64,
              source: tool_source.clone(),
              permissions: tool_permissions.to_vec(),
              timestamp: Utc::now(),
            });
            return Ok(ToolExecOutcome::Stop(Self::cancelled_result(
              &self.session_id,
              "cancellation token signalled",
              std::mem::take(steps),
              std::mem::take(events),
            )));
          }
        }
      }
      (None, None) => match tool_call.await {
        Ok(out) => out,
        Err(e) => {
          warn!(tool = %tool, error = %e, "Tool execution failed");
          agentflow_tools::ToolOutput::error(e.to_string())
        }
      },
    };
    Ok(ToolExecOutcome::Output(tool_output))
  }

  /// Process one `AgentResponse::Action`: the max-tool-call guard, the
  /// plan step, tool policy/capability events, the tool execution (via
  /// [`Self::execute_tool_with_limits`]), the result step + observation,
  /// the F-A2-13 repeat-call steering note, and the memory write.
  /// Returns `TurnStep::Continue` to advance to the next turn, or
  /// `TurnStep::Stop` on a terminal condition (max tool calls / cancel /
  /// timeout).
  ///
  /// Turn-driven extraction (RFC_HARNESS_LOOP_OWNERSHIP §6, series step
  /// 3c): the `Action` arm body lifted whole out of the loop. Pure
  /// relocation; `steps`/`events` are consumed via `mem::take` only on
  /// stop paths, and the loop now owns the `iteration += 1` increment.
  #[allow(clippy::too_many_arguments)]
  async fn dispatch_single_tool_call(
    &mut self,
    thought: String,
    tool: String,
    params: serde_json::Value,
    steps: &mut Vec<AgentStep>,
    events: &mut Vec<AgentEvent>,
    step_index: &mut usize,
    tool_calls: &mut usize,
    last_tool_call: &mut Option<(String, serde_json::Value)>,
    iteration: usize,
    max_tool_calls: Option<usize>,
    run_started_at: Instant,
    timeout_ms: Option<u64>,
    cancellation_token: &Option<AgentCancellationToken>,
  ) -> Result<TurnStep, ReActError> {
    info!(iteration, tool = %tool, thought = %thought, "Tool call");
    // F-A2-13: detect the (tool, params) == previous call shape BEFORE
    // we touch `params` (it gets moved into `self.tools.execute` later).
    let is_repeat_tool_call = matches!(
      &*last_tool_call,
      Some((prev_tool, prev_params))
        if prev_tool == &tool && prev_params == &params
    );
    if is_repeat_tool_call {
      warn!(
        iteration,
        tool = %tool,
        "Repeat tool call detected (identical params as prior iteration); appending steering note (F-A2-13)"
      );
    }
    if let Some(max_tool_calls) = max_tool_calls
      && *tool_calls >= max_tool_calls
    {
      self
        .record_reflection(
          ReflectionContext::failure(
            &self.session_id,
            *step_index,
            format!("max tool calls ({}) reached", max_tool_calls),
          ),
          step_index,
          steps,
          events,
        )
        .await?;
      return Ok(TurnStep::Stop(Self::stopped_result(
        &self.session_id,
        None,
        AgentStopReason::MaxToolCalls { max_tool_calls },
        std::mem::take(steps),
        std::mem::take(events),
      )));
    }

    if !thought.trim().is_empty() {
      steps.push(AgentStep::new(
        *step_index,
        AgentStepKind::Plan {
          thought: thought.clone(),
        },
      ));
      *step_index += 1;
    }

    if is_cancelled(cancellation_token) {
      return Ok(TurnStep::Stop(Self::cancelled_result(
        &self.session_id,
        "cancellation token signalled",
        std::mem::take(steps),
        std::mem::take(events),
      )));
    }

    let tool_step_index = *step_index;
    let metadata = self.tools.tool_metadata(&tool);
    let (tool_source, tool_permissions) = tool_event_metadata(metadata.as_ref());
    let trace_params =
      annotate_tool_params_for_resume(params.clone(), self.tools.tool_idempotency(&tool, &params));
    if let Ok(decision) = self.tools.evaluate_policy(&tool, &params) {
      events.push(AgentEvent::ToolPolicyDecision {
        session_id: self.session_id.clone(),
        step_index: tool_step_index,
        tool: tool.clone(),
        allowed: decision.allowed,
        matched_rule: decision.matched_rule,
        deny_reason: decision.deny_reason,
        source: decision.source,
        permissions: decision.permissions,
        params_summary: decision.params_summary,
        timestamp: Utc::now(),
      });
    }
    if let Ok(effective) = self.tools.evaluate_capabilities(&tool) {
      events.push(AgentEvent::ToolCapabilityDecision {
        session_id: self.session_id.clone(),
        step_index: tool_step_index,
        tool: tool.clone(),
        allowed: effective.allowed,
        required: effective.required,
        effective: effective.effective,
        denied: effective.denied,
        deny_reason: effective.deny_reason,
        trace: effective.trace,
        sandbox: effective.sandbox,
        timestamp: Utc::now(),
      });
    }
    emit_and_push!(
      self.live_sink,
      events,
      AgentEvent::ToolCallStarted {
        session_id: self.session_id.clone(),
        step_index: tool_step_index,
        tool: tool.clone(),
        params: trace_params.clone(),
        source: tool_source.clone(),
        permissions: tool_permissions.clone(),
        timestamp: Utc::now(),
      }
    );
    steps.push(AgentStep::new(
      tool_step_index,
      AgentStepKind::ToolCall {
        tool: tool.clone(),
        params: trace_params,
      },
    ));
    *step_index += 1;

    let started_at = std::time::Instant::now();
    // F-A2-13: snapshot now so we can compare on the next iteration even
    // after `params` moves into `execute`.
    let params_snapshot = params.clone();
    let tool_output = match self
      .execute_tool_with_limits(
        &tool,
        params,
        tool_step_index,
        &tool_source,
        &tool_permissions,
        started_at,
        steps,
        events,
        step_index,
        run_started_at,
        timeout_ms,
        cancellation_token,
      )
      .await?
    {
      ToolExecOutcome::Output(output) => output,
      ToolExecOutcome::Stop(result) => return Ok(TurnStep::Stop(result)),
    };
    *tool_calls += 1;
    let duration_ms = started_at.elapsed().as_millis() as u64;

    let observation = if tool_output.is_error {
      format!("[ERROR] {}", tool_output.content)
    } else {
      tool_output.content.clone()
    };

    info!(tool = %tool, "Observation: {}", &observation[..observation.len().min(200)]);
    steps.push(AgentStep::new(
      *step_index,
      AgentStepKind::ToolResult {
        tool: tool.clone(),
        content: tool_output.content.clone(),
        is_error: tool_output.is_error,
        parts: tool_output.parts.clone(),
      },
    ));
    emit_and_push!(
      self.live_sink,
      events,
      AgentEvent::ToolCallCompleted {
        session_id: self.session_id.clone(),
        step_index: tool_step_index,
        tool: tool.clone(),
        is_error: tool_output.is_error,
        duration_ms,
        source: tool_source.clone(),
        permissions: tool_permissions.clone(),
        timestamp: Utc::now(),
      }
    );
    *step_index += 1;
    if tool_output.is_error {
      self
        .record_reflection(
          ReflectionContext::failure(&self.session_id, *step_index, &observation),
          step_index,
          steps,
          events,
        )
        .await?;
    }

    // F-A2-13: when this iteration is a repeat of the prior, append a
    // steering note ONLY to the memory the model sees on its next turn.
    let observation_for_memory = if is_repeat_tool_call {
      format!(
        "{observation}\n\n\
         [agentflow steering note (F-A2-13): this is your 2nd consecutive call to tool `{tool}` with identical parameters. The observation above is unchanged from the prior call — calling `{tool}` again with these params will not yield new information. To make progress, choose one of: (a) draw conclusions from the observation and emit a final answer, (b) call a different tool, or (c) call `{tool}` with materially different parameters.]"
      )
    } else {
      observation.clone()
    };

    self
      .add_memory_message(Message::tool_result_with_counter(
        &self.session_id,
        &tool,
        &observation_for_memory,
        &*self.message_counter,
      ))
      .await?;

    // Track the call so the next iteration's check can run.
    *last_tool_call = Some((tool.clone(), params_snapshot));

    Ok(TurnStep::Continue)
  }

  /// Top-of-turn limit guards (cancel / timeout / max-steps / token
  /// budget). Returns `Some(result)` when the run must stop this turn,
  /// `None` to proceed with the LLM call.
  ///
  /// Turn-driven extraction (RFC_HARNESS_LOOP_OWNERSHIP §6, series step
  /// 1): pulling the guards out of the monolithic `run_with_context`
  /// loop is the first move toward a resumable `LoopSession`. Behaviour
  /// is identical — this is a pure relocation; `steps`/`events` are only
  /// consumed (via `mem::take`) on the stop paths, where the caller
  /// returns immediately.
  #[allow(clippy::too_many_arguments)]
  async fn check_turn_limits(
    &self,
    steps: &mut Vec<AgentStep>,
    events: &mut Vec<AgentEvent>,
    step_index: &mut usize,
    iteration: usize,
    run_started_at: Instant,
    timeout_ms: Option<u64>,
    max_iterations: usize,
    budget_tokens: Option<u32>,
    cancellation_token: &Option<AgentCancellationToken>,
  ) -> Result<Option<AgentRunResult>, ReActError> {
    if is_cancelled(cancellation_token) {
      return Ok(Some(Self::cancelled_result(
        &self.session_id,
        "cancellation token signalled",
        std::mem::take(steps),
        std::mem::take(events),
      )));
    }

    if timed_out(run_started_at, timeout_ms) {
      self
        .record_reflection(
          ReflectionContext::failure(
            &self.session_id,
            *step_index,
            format!(
              "runtime timed out after {}ms",
              timeout_ms.unwrap_or_default()
            ),
          ),
          step_index,
          steps,
          events,
        )
        .await?;
      return Ok(Some(Self::stopped_result(
        &self.session_id,
        None,
        AgentStopReason::Timeout {
          timeout_ms: timeout_ms.unwrap_or_default(),
        },
        std::mem::take(steps),
        std::mem::take(events),
      )));
    }

    if iteration >= max_iterations {
      self
        .record_reflection(
          ReflectionContext::failure(
            &self.session_id,
            *step_index,
            format!("max steps ({}) reached", max_iterations),
          ),
          step_index,
          steps,
          events,
        )
        .await?;
      return Ok(Some(Self::stopped_result(
        &self.session_id,
        None,
        AgentStopReason::MaxSteps {
          max_steps: max_iterations,
        },
        std::mem::take(steps),
        std::mem::take(events),
      )));
    }

    if let Some(budget) = budget_tokens {
      let used = self.memory.session_token_count(&self.session_id).await?;
      if used > budget {
        self
          .record_reflection(
            ReflectionContext::failure(
              &self.session_id,
              *step_index,
              format!("token budget exceeded: {} / {}", used, budget),
            ),
            step_index,
            steps,
            events,
          )
          .await?;
        return Ok(Some(Self::stopped_result(
          &self.session_id,
          None,
          AgentStopReason::TokenBudgetExceeded { used, budget },
          std::mem::take(steps),
          std::mem::take(events),
        )));
      }
    }

    Ok(None)
  }

  async fn record_reflection(
    &self,
    context: ReflectionContext,
    step_index: &mut usize,
    steps: &mut Vec<AgentStep>,
    events: &mut Vec<AgentEvent>,
  ) -> Result<(), ReActError> {
    if !self.config.reflection_enabled {
      return Ok(());
    }
    let Some(strategy) = &self.reflection else {
      return Ok(());
    };
    let reflection = strategy
      .reflect(&context)
      .await
      .map_err(|err| ReActError::ToolError {
        tool: "reflection".to_string(),
        message: err.to_string(),
      })?;
    let Some(reflection) = reflection else {
      return Ok(());
    };

    let current_step = *step_index;
    steps.push(AgentStep::new(
      current_step,
      AgentStepKind::Reflect {
        content: reflection.content,
      },
    ));
    events.push(AgentEvent::ReflectionAdded {
      session_id: self.session_id.clone(),
      step_index: current_step,
      timestamp: reflection.timestamp,
    });
    *step_index += 1;
    Ok(())
  }

  async fn add_memory_message(&mut self, message: Message) -> Result<(), ReActError> {
    self.memory.add_message(message.clone()).await?;
    self.notify_memory_write(message);
    Ok(())
  }

  /// Build a `Vec<ToolSpec>` from the registered tools so it can be passed to
  /// the LLM as a native `tools` field. Returns an empty vector when no
  /// tools are registered, in which case the LLM call is unchanged.
  fn collect_tool_specs(&self) -> Vec<ToolSpec> {
    self
      .tools
      .list()
      .into_iter()
      .map(|tool| ToolSpec::new(tool.name(), tool.description(), tool.parameters_schema()))
      .collect()
  }

  async fn restore_trace_memory(&mut self, prior: &AgentRunResult) -> Result<(), ReActError> {
    self.memory.clear_session(&self.session_id).await?;
    for step in &prior.steps {
      match &step.kind {
        AgentStepKind::Observe { input } => {
          self
            .add_memory_message(Message::user_with_counter(
              &self.session_id,
              input,
              &*self.message_counter,
            ))
            .await?;
        }
        AgentStepKind::Plan { thought } => {
          self
            .add_memory_message(Message::assistant_with_counter(
              &self.session_id,
              thought,
              &*self.message_counter,
            ))
            .await?;
        }
        AgentStepKind::ToolCall { .. } => {}
        AgentStepKind::ToolResult {
          tool,
          content,
          is_error,
          ..
        } => {
          let observation = if *is_error {
            format!("[ERROR] {}", content)
          } else {
            content.clone()
          };
          self
            .add_memory_message(Message::tool_result_with_counter(
              &self.session_id,
              tool,
              observation,
              &*self.message_counter,
            ))
            .await?;
        }
        AgentStepKind::Reflect { content } => {
          self
            .add_memory_message(Message::assistant_with_counter(
              &self.session_id,
              content,
              &*self.message_counter,
            ))
            .await?;
        }
        AgentStepKind::FinalAnswer { answer } => {
          self
            .add_memory_message(Message::assistant_with_counter(
              &self.session_id,
              answer,
              &*self.message_counter,
            ))
            .await?;
        }
        AgentStepKind::Handoff { .. }
        | AgentStepKind::BlackboardOp { .. }
        | AgentStepKind::DebateProposal { .. }
        | AgentStepKind::DebateVerdict { .. } => {
          // Multi-agent supervisor steps are not part of this ReActAgent's own
          // conversation, so they are dropped when restoring its memory.
        }
      }
    }
    Ok(())
  }

  async fn read_memory_history(&self, session_id: &str) -> Result<Vec<Message>, ReActError> {
    let messages = self.memory.get_all(session_id).await?;
    self.notify_memory_read(
      session_id,
      MemoryHookKind::ReadHistory,
      None,
      None,
      messages.clone(),
    );
    Ok(messages)
  }

  async fn search_memory(
    &self,
    session_id: &str,
    query: &str,
    limit: usize,
  ) -> Result<Vec<Message>, ReActError> {
    let messages = self.memory.search(session_id, query, limit).await?;
    self.notify_memory_read(
      session_id,
      MemoryHookKind::Search,
      Some(query.to_string()),
      Some(limit),
      messages.clone(),
    );
    Ok(messages)
  }

  fn notify_memory_read(
    &self,
    session_id: &str,
    kind: MemoryHookKind,
    query: Option<String>,
    limit: Option<usize>,
    messages: Vec<Message>,
  ) {
    if let Some(hook) = &self.memory_hook {
      hook.on_memory_read(&MemoryHookContext {
        session_id: session_id.to_string(),
        kind,
        query,
        limit,
        messages,
      });
    }
  }

  fn notify_memory_write(&self, message: Message) {
    if let Some(hook) = &self.memory_hook {
      hook.on_memory_write(&MemoryHookContext {
        session_id: message.session_id.clone(),
        kind: MemoryHookKind::Write,
        query: None,
        limit: None,
        messages: vec![message],
      });
    }
  }

  fn answer_from_result(result: AgentRunResult) -> Result<String, ReActError> {
    match result.stop_reason {
      AgentStopReason::FinalAnswer | AgentStopReason::StopCondition { .. } => {
        Ok(result.answer.unwrap_or_default())
      }
      AgentStopReason::MaxSteps { max_steps } => Err(ReActError::MaxIterationsReached(max_steps)),
      AgentStopReason::TokenBudgetExceeded { used, budget } => {
        Err(ReActError::BudgetExceeded { used, budget })
      }
      AgentStopReason::MaxToolCalls { max_tool_calls } => Err(ReActError::ToolError {
        tool: "runtime".to_string(),
        message: format!("max tool calls ({}) reached", max_tool_calls),
      }),
      AgentStopReason::Timeout { timeout_ms } => Err(ReActError::ToolError {
        tool: "runtime".to_string(),
        message: format!("timeout after {}ms", timeout_ms),
      }),
      AgentStopReason::Cancelled { message } => Err(ReActError::Cancelled { reason: message }),
      AgentStopReason::CostLimitExceeded {
        used_usd,
        budget_usd,
      } => Err(ReActError::ToolError {
        tool: "runtime".to_string(),
        message: format!(
          "cost limit exceeded: ${:.4} (budget ${:.4})",
          used_usd, budget_usd
        ),
      }),
      AgentStopReason::Error { message } => Err(ReActError::ToolError {
        tool: "runtime".to_string(),
        message,
      }),
    }
  }

  /// Build the system prompt injected at the start of every LLM call.
  fn build_system_prompt(&self) -> String {
    let persona = self
      .config
      .persona
      .as_deref()
      .unwrap_or("You are a helpful autonomous AI assistant.");

    let tools_desc = self.tools.prompt_tools_description();
    let has_tools = !tools_desc.is_empty();

    let tools_section = if has_tools {
      format!(
        "\n\n## Available Tools\n{}\n\n\
                To call a tool, respond ONLY with this JSON:\n\
                {{\"thought\": \"<your reasoning>\", \"action\": {{\"tool\": \"<tool_name>\", \"params\": {{<parameters>}}}}}}\n",
        tools_desc
      )
    } else {
      String::new()
    };

    format!(
      "{}{}\n\
            To give a final answer, respond ONLY with this JSON:\n\
            {{\"thought\": \"<your final reasoning>\", \"answer\": \"<your answer>\"}}\n\n\
            Respond ONLY with valid JSON matching one of the formats above. \
            No additional text, no markdown, no explanation outside the JSON.",
      persona, tools_section
    )
  }

  /// Assemble the full message list to send to the LLM.
  async fn build_llm_messages(
    &self,
    system_prompt: &str,
  ) -> Result<Vec<MultimodalMessage>, ReActError> {
    let history = self.read_memory_history(&self.session_id).await?;
    let (memory_summary, history) = self.apply_memory_prompt_budget(history).await?;

    // Phase 2b (RFC_HARNESS_LOOP_OWNERSHIP): the agent compacts prompt
    // memory every turn when it exceeds budget. Surface that *mid-run*
    // compaction live so the Harness bridge turns it into a
    // `MemorySummaryAdded` envelope — previously this between-turn
    // context engineering was invisible. Live-only (the summary is a
    // transient prompt artifact, not a recorded step); `None` live_sink
    // is a no-op, so non-harness runs are unaffected.
    if let Some(summary) = &memory_summary
      && let Some(handle) = &self.live_sink
    {
      let token_estimate =
        agentflow_llm::tokenizer::count_tokens_for_model(&self.config.model, summary) as usize;
      handle
        .0
        .emit(&AgentEvent::MemorySummaryAdded {
          session_id: self.session_id.clone(),
          layer: "session".to_string(),
          summary: summary.clone(),
          token_estimate,
          timestamp: Utc::now(),
        })
        .await;
    }

    let mut messages = Vec::with_capacity(history.len() + 1);

    // Always start with the system prompt
    messages.push(MultimodalMessage::system().add_text(system_prompt).build());
    if let Some(summary) = memory_summary {
      messages.push(MultimodalMessage::system().add_text(summary).build());
    }

    // Map memory roles to LLM message roles
    for msg in &history {
      let llm_msg = match msg.role {
        Role::System => continue, // Skip — we inject our own system prompt
        Role::User => MultimodalMessage::user().add_text(&msg.content).build(),
        Role::Assistant => MultimodalMessage::assistant()
          .add_text(&msg.content)
          .build(),
        Role::Tool => {
          // Represent tool results as user messages with a clear prefix
          let tool_name = msg.tool_name.as_deref().unwrap_or("tool");
          let content = format!("[Tool Result: {}]\n{}", tool_name, msg.content);
          MultimodalMessage::user().add_text(&content).build()
        }
      };
      messages.push(llm_msg);
    }

    Ok(messages)
  }

  async fn apply_memory_prompt_budget(
    &self,
    history: Vec<Message>,
  ) -> Result<(Option<String>, Vec<Message>), ReActError> {
    let Some(budget) = self.config.memory_prompt_token_budget else {
      return Ok((None, history));
    };
    if self.config.memory_summary_strategy == MemorySummaryStrategy::Disabled {
      return Ok((None, history));
    }

    let total_tokens: u32 = history.iter().map(|msg| msg.token_count).sum();
    if total_tokens <= budget {
      return Ok((None, history));
    }

    let mut kept_reversed = Vec::new();
    let mut kept_tokens = 0u32;
    for message in history.iter().rev() {
      if !kept_reversed.is_empty() && kept_tokens.saturating_add(message.token_count) > budget {
        break;
      }
      kept_tokens = kept_tokens.saturating_add(message.token_count);
      kept_reversed.push(message.clone());
    }
    kept_reversed.reverse();

    let omitted_count = history.len().saturating_sub(kept_reversed.len());
    let omitted_tokens = total_tokens.saturating_sub(kept_tokens);
    let omitted_messages = history[..omitted_count].to_vec();
    let context = MemorySummaryContext {
      session_id: self.session_id.clone(),
      budget_tokens: budget,
      omitted_tokens,
      omitted_messages,
      kept_messages: kept_reversed.clone(),
    };

    let summary = match &self.memory_summary_backend {
      Some(backend) => backend.summarize(context).await?,
      None => match self.config.memory_summary_strategy {
        MemorySummaryStrategy::Disabled => None,
        MemorySummaryStrategy::RecentOnly => RecentOnlyMemorySummary.summarize(context).await?,
        MemorySummaryStrategy::Compact => CompactMemorySummary.summarize(context).await?,
      },
    };

    Ok((summary, kept_reversed))
  }

  /// Clear the current session's memory.
  pub async fn reset(&mut self) -> Result<(), ReActError> {
    self.memory.clear_session(&self.session_id).await?;
    self.session_id = uuid::Uuid::new_v4().to_string();
    Ok(())
  }

  /// Estimated tokens used in the current session.
  pub async fn token_count(&self) -> Result<u32, ReActError> {
    Ok(self.memory.session_token_count(&self.session_id).await?)
  }

  /// Dispatch a batch of native tool calls (`>=2`) produced by one
  /// LLM turn (P-H.3). Idempotent calls run concurrently;
  /// `NonIdempotent` / `Unknown` calls run serially, in array order.
  /// `ToolCallStarted` events fire in the LLM-returned array order
  /// before any execution begins; `ToolCallCompleted` and the
  /// `ToolResult` step rows also follow that order so trace replay
  /// remains deterministic across runs.
  #[allow(clippy::too_many_arguments)]
  async fn dispatch_native_tool_calls_batch(
    &mut self,
    tool_calls: &[ToolCallRequest],
    raw_response: &str,
    steps: &mut Vec<AgentStep>,
    events: &mut Vec<AgentEvent>,
    step_index: &mut usize,
    tool_calls_counter: &mut usize,
    max_tool_calls: Option<usize>,
    run_started_at: Instant,
    timeout_ms: Option<u64>,
    cancellation_token: Option<&AgentCancellationToken>,
  ) -> Result<BatchOutcome, ReActError> {
    let n = tool_calls.len();
    debug_assert!(n >= 2, "batch path expects >=2 native tool calls");

    // 1. Max-tool-calls precondition: refuse to start a batch that
    //    would put the counter over the limit. We treat the whole
    //    batch atomically so the agent never sees a partial trace.
    if let Some(max) = max_tool_calls
      && *tool_calls_counter + n > max
    {
      self
        .record_reflection(
          ReflectionContext::failure(
            &self.session_id,
            *step_index,
            format!(
              "batch of {n} tool calls would exceed max_tool_calls={max}; refusing to dispatch"
            ),
          ),
          step_index,
          steps,
          events,
        )
        .await?;
      return Ok(BatchOutcome::Stop(Box::new(Self::stopped_result(
        &self.session_id,
        None,
        AgentStopReason::MaxToolCalls {
          max_tool_calls: max,
        },
        std::mem::take(steps),
        std::mem::take(events),
      ))));
    }

    // 2. Cancellation precheck.
    if is_cancelled(&cancellation_token.cloned()) {
      return Ok(BatchOutcome::Stop(Box::new(Self::cancelled_result(
        &self.session_id,
        "cancellation token signalled",
        std::mem::take(steps),
        std::mem::take(events),
      ))));
    }

    // 3. Persist the assistant turn that triggered this batch.
    self
      .add_memory_message(Message::assistant_with_counter(
        &self.session_id,
        raw_response,
        &*self.message_counter,
      ))
      .await?;

    // 4. Pre-assign step indexes and emit `ToolPolicyDecision`,
    //    `ToolCapabilityDecision`, and `ToolCallStarted` for every
    //    call before dispatching anything. The trace is therefore
    //    deterministic regardless of completion order.
    let mut prepared: Vec<PreparedToolCall> = Vec::with_capacity(n);
    for call in tool_calls.iter() {
      let metadata = self.tools.tool_metadata(&call.name);
      let idempotency = self
        .tools
        .tool_idempotency(&call.name, &call.arguments)
        .unwrap_or(ToolIdempotency::Unknown);
      let (source, permissions) = tool_event_metadata(metadata.as_ref());
      let trace_params = annotate_tool_params_for_resume(call.arguments.clone(), Some(idempotency));
      let call_step_idx = *step_index;
      *step_index += 1;

      if let Ok(decision) = self.tools.evaluate_policy(&call.name, &call.arguments) {
        events.push(AgentEvent::ToolPolicyDecision {
          session_id: self.session_id.clone(),
          step_index: call_step_idx,
          tool: call.name.clone(),
          allowed: decision.allowed,
          matched_rule: decision.matched_rule,
          deny_reason: decision.deny_reason,
          source: decision.source,
          permissions: decision.permissions,
          params_summary: decision.params_summary,
          timestamp: Utc::now(),
        });
      }
      if let Ok(effective) = self.tools.evaluate_capabilities(&call.name) {
        events.push(AgentEvent::ToolCapabilityDecision {
          session_id: self.session_id.clone(),
          step_index: call_step_idx,
          tool: call.name.clone(),
          allowed: effective.allowed,
          required: effective.required,
          effective: effective.effective,
          denied: effective.denied,
          deny_reason: effective.deny_reason,
          trace: effective.trace,
          sandbox: effective.sandbox,
          timestamp: Utc::now(),
        });
      }
      emit_and_push!(
        self.live_sink,
        events,
        AgentEvent::ToolCallStarted {
          session_id: self.session_id.clone(),
          step_index: call_step_idx,
          tool: call.name.clone(),
          params: trace_params.clone(),
          source: source.clone(),
          permissions: permissions.clone(),
          timestamp: Utc::now(),
        }
      );
      steps.push(AgentStep::new(
        call_step_idx,
        AgentStepKind::ToolCall {
          tool: call.name.clone(),
          params: trace_params,
        },
      ));

      prepared.push(PreparedToolCall {
        tool: call.name.clone(),
        params: call.arguments.clone(),
        call_step_idx,
        idempotency,
        source,
        permissions,
      });
    }

    // 5. Partition by idempotency. Idempotent → concurrent group.
    //    Non-idempotent / Unknown → serial group, evaluated in LLM
    //    order. The harness `HookedTool` wrapper is responsible for
    //    approval gating; the agent only worries about safety
    //    relative to repeating the call.
    let concurrent_idxs: Vec<usize> = (0..n)
      .filter(|&i| matches!(prepared[i].idempotency, ToolIdempotency::Idempotent))
      .collect();
    let serial_idxs: Vec<usize> = (0..n)
      .filter(|&i| !matches!(prepared[i].idempotency, ToolIdempotency::Idempotent))
      .collect();

    let mut outputs: Vec<Option<(agentflow_tools::ToolOutput, u64)>> =
      (0..n).map(|_| None).collect();

    // 5a. Concurrent group.
    if !concurrent_idxs.is_empty() {
      let mut futs = Vec::with_capacity(concurrent_idxs.len());
      for &i in &concurrent_idxs {
        let tools = self.tools.clone();
        let tool = prepared[i].tool.clone();
        let params = prepared[i].params.clone();
        let started = Instant::now();
        futs.push(async move {
          let result = tools.execute(&tool, params).await;
          (i, result, started.elapsed().as_millis() as u64)
        });
      }
      let batch_fut = futures::future::join_all(futs);

      let timeout = remaining_timeout(run_started_at, timeout_ms);
      let cancel = cancellation_token.cloned();
      let result_set = match (timeout, cancel) {
        (Some(remaining), Some(token)) => {
          tokio::select! {
            done = tokio::time::timeout(remaining, batch_fut) => match done {
              Ok(results) => Some(results),
              Err(_) => {
                self.emit_batch_timeout(&prepared, &concurrent_idxs, events).await;
                return Ok(BatchOutcome::Stop(Box::new(Self::stopped_result(
                  &self.session_id,
                  None,
                  AgentStopReason::Timeout { timeout_ms: timeout_ms.unwrap_or_default() },
                  std::mem::take(steps),
                  std::mem::take(events),
                ))));
              }
            },
            _ = token.cancelled() => {
              self.emit_batch_cancelled(&prepared, &concurrent_idxs, events).await;
              return Ok(BatchOutcome::Stop(Box::new(Self::cancelled_result(
                &self.session_id,
                "cancellation token signalled",
                std::mem::take(steps),
                std::mem::take(events),
              ))));
            }
          }
        }
        (Some(remaining), None) => match tokio::time::timeout(remaining, batch_fut).await {
          Ok(results) => Some(results),
          Err(_) => {
            self
              .emit_batch_timeout(&prepared, &concurrent_idxs, events)
              .await;
            return Ok(BatchOutcome::Stop(Box::new(Self::stopped_result(
              &self.session_id,
              None,
              AgentStopReason::Timeout {
                timeout_ms: timeout_ms.unwrap_or_default(),
              },
              std::mem::take(steps),
              std::mem::take(events),
            ))));
          }
        },
        (None, Some(token)) => {
          tokio::select! {
            results = batch_fut => Some(results),
            _ = token.cancelled() => {
              self.emit_batch_cancelled(&prepared, &concurrent_idxs, events).await;
              return Ok(BatchOutcome::Stop(Box::new(Self::cancelled_result(
                &self.session_id,
                "cancellation token signalled",
                std::mem::take(steps),
                std::mem::take(events),
              ))));
            }
          }
        }
        (None, None) => Some(batch_fut.await),
      };

      if let Some(results) = result_set {
        for (i, result, dur) in results {
          let output = match result {
            Ok(out) => out,
            Err(e) => {
              warn!(tool = %prepared[i].tool, error = %e, "tool execution failed");
              agentflow_tools::ToolOutput::error(e.to_string())
            }
          };
          outputs[i] = Some((output, dur));
        }
      }
    }

    // 5b. Serial group. Each call is independently subject to
    //     cancellation + timeout.
    for &i in &serial_idxs {
      if is_cancelled(&cancellation_token.cloned()) {
        // Skip remaining calls; emit completion events for the rest
        // so the trace stays balanced.
        for &j in serial_idxs.iter().skip_while(|&&j| j != i) {
          if outputs[j].is_none() {
            emit_and_push!(
              self.live_sink,
              events,
              AgentEvent::ToolCallCompleted {
                session_id: self.session_id.clone(),
                step_index: prepared[j].call_step_idx,
                tool: prepared[j].tool.clone(),
                is_error: true,
                duration_ms: 0,
                source: prepared[j].source.clone(),
                permissions: prepared[j].permissions.clone(),
                timestamp: Utc::now(),
              }
            );
          }
        }
        return Ok(BatchOutcome::Stop(Box::new(Self::cancelled_result(
          &self.session_id,
          "cancellation token signalled",
          std::mem::take(steps),
          std::mem::take(events),
        ))));
      }
      let started = Instant::now();
      let tools = self.tools.clone();
      let tool = prepared[i].tool.clone();
      let params = prepared[i].params.clone();
      let call_fut = async move { tools.execute(&tool, params).await };
      let timeout = remaining_timeout(run_started_at, timeout_ms);
      let cancel = cancellation_token.cloned();
      let result = match (timeout, cancel) {
        (Some(remaining), Some(token)) => {
          tokio::select! {
            done = tokio::time::timeout(remaining, call_fut) => match done {
              Ok(r) => Some(r),
              Err(_) => {
                emit_and_push!(self.live_sink, events, AgentEvent::ToolCallCompleted {
                  session_id: self.session_id.clone(),
                  step_index: prepared[i].call_step_idx,
                  tool: prepared[i].tool.clone(),
                  is_error: true,
                  duration_ms: started.elapsed().as_millis() as u64,
                  source: prepared[i].source.clone(),
                  permissions: prepared[i].permissions.clone(),
                  timestamp: Utc::now(),
                });
                return Ok(BatchOutcome::Stop(Box::new(Self::stopped_result(
                  &self.session_id,
                  None,
                  AgentStopReason::Timeout {
                    timeout_ms: timeout_ms.unwrap_or_default(),
                  },
                  std::mem::take(steps),
                  std::mem::take(events),
                ))));
              }
            },
            _ = token.cancelled() => {
              return Ok(BatchOutcome::Stop(Box::new(Self::cancelled_result(
                &self.session_id,
                "cancellation token signalled",
                std::mem::take(steps),
                std::mem::take(events),
              ))));
            }
          }
        }
        (Some(remaining), None) => match tokio::time::timeout(remaining, call_fut).await {
          Ok(r) => Some(r),
          Err(_) => {
            emit_and_push!(
              self.live_sink,
              events,
              AgentEvent::ToolCallCompleted {
                session_id: self.session_id.clone(),
                step_index: prepared[i].call_step_idx,
                tool: prepared[i].tool.clone(),
                is_error: true,
                duration_ms: started.elapsed().as_millis() as u64,
                source: prepared[i].source.clone(),
                permissions: prepared[i].permissions.clone(),
                timestamp: Utc::now(),
              }
            );
            return Ok(BatchOutcome::Stop(Box::new(Self::stopped_result(
              &self.session_id,
              None,
              AgentStopReason::Timeout {
                timeout_ms: timeout_ms.unwrap_or_default(),
              },
              std::mem::take(steps),
              std::mem::take(events),
            ))));
          }
        },
        (None, Some(token)) => {
          tokio::select! {
            r = call_fut => Some(r),
            _ = token.cancelled() => {
              return Ok(BatchOutcome::Stop(Box::new(Self::cancelled_result(
                &self.session_id,
                "cancellation token signalled",
                std::mem::take(steps),
                std::mem::take(events),
              ))));
            }
          }
        }
        (None, None) => Some(call_fut.await),
      };
      let output = match result {
        Some(Ok(out)) => out,
        Some(Err(e)) => {
          warn!(tool = %prepared[i].tool, error = %e, "tool execution failed");
          agentflow_tools::ToolOutput::error(e.to_string())
        }
        None => unreachable!("result must be Some when we did not early-return"),
      };
      outputs[i] = Some((output, started.elapsed().as_millis() as u64));
    }

    // 6. Emit completions + push ToolResult steps in LLM order;
    //    append tool results to memory. Reflection is recorded for
    //    the batch once if any call errored, so the next LLM turn
    //    sees a single reflective summary rather than n reflections.
    let mut error_summary = String::new();
    for (i, prep) in prepared.iter().enumerate() {
      // Q2.9.1: previous code `expect`ed every prepared call to have
      // an output set by the earlier loop. The invariant should
      // hold (the previous loop fills `outputs[i]` for every i in
      // 0..prepared.len()) but a panic here would crash the entire
      // ReAct runtime mid-batch. Fall back to a synthetic error
      // output + warning so the rest of the batch still completes
      // and the operator sees the inconsistency in the trace.
      let (output, duration_ms) = match outputs[i].take() {
        Some(pair) => pair,
        None => {
          warn!(
            tool = %prep.tool,
            index = i,
            "internal invariant violation: prepared call has no recorded output; emitting synthetic error"
          );
          (
            agentflow_tools::ToolOutput::error(
              "internal invariant violation: tool call has no output recorded".to_string(),
            ),
            0,
          )
        }
      };
      let observation = if output.is_error {
        format!("[ERROR] {}", output.content)
      } else {
        output.content.clone()
      };
      info!(
        tool = %prep.tool,
        "Batch observation [{}]: {}",
        i,
        &observation[..observation.len().min(200)]
      );
      let result_step_idx = *step_index;
      *step_index += 1;
      steps.push(AgentStep::new(
        result_step_idx,
        AgentStepKind::ToolResult {
          tool: prep.tool.clone(),
          content: output.content.clone(),
          is_error: output.is_error,
          parts: output.parts.clone(),
        },
      ));
      emit_and_push!(
        self.live_sink,
        events,
        AgentEvent::ToolCallCompleted {
          session_id: self.session_id.clone(),
          step_index: prep.call_step_idx,
          tool: prep.tool.clone(),
          is_error: output.is_error,
          duration_ms,
          source: prep.source.clone(),
          permissions: prep.permissions.clone(),
          timestamp: Utc::now(),
        }
      );
      if output.is_error {
        if !error_summary.is_empty() {
          error_summary.push_str("; ");
        }
        error_summary.push_str(&format!("{}: {}", prep.tool, observation));
      }
      *tool_calls_counter += 1;
      self
        .add_memory_message(Message::tool_result_with_counter(
          &self.session_id,
          &prep.tool,
          &observation,
          &*self.message_counter,
        ))
        .await?;
    }
    if !error_summary.is_empty() {
      self
        .record_reflection(
          ReflectionContext::failure(&self.session_id, *step_index, error_summary),
          step_index,
          steps,
          events,
        )
        .await?;
    }

    Ok(BatchOutcome::Continue)
  }

  async fn emit_batch_timeout(
    &self,
    prepared: &[PreparedToolCall],
    idxs: &[usize],
    events: &mut Vec<AgentEvent>,
  ) {
    for &i in idxs {
      emit_and_push!(
        self.live_sink,
        events,
        AgentEvent::ToolCallCompleted {
          session_id: self.session_id.clone(),
          step_index: prepared[i].call_step_idx,
          tool: prepared[i].tool.clone(),
          is_error: true,
          duration_ms: 0,
          source: prepared[i].source.clone(),
          permissions: prepared[i].permissions.clone(),
          timestamp: Utc::now(),
        }
      );
    }
  }

  async fn emit_batch_cancelled(
    &self,
    prepared: &[PreparedToolCall],
    idxs: &[usize],
    events: &mut Vec<AgentEvent>,
  ) {
    for &i in idxs {
      emit_and_push!(
        self.live_sink,
        events,
        AgentEvent::ToolCallCompleted {
          session_id: self.session_id.clone(),
          step_index: prepared[i].call_step_idx,
          tool: prepared[i].tool.clone(),
          is_error: true,
          duration_ms: 0,
          source: prepared[i].source.clone(),
          permissions: prepared[i].permissions.clone(),
          timestamp: Utc::now(),
        }
      );
    }
  }
}

/// Outcome of `dispatch_native_tool_calls_batch`. `Stop` boxes the
/// full `AgentRunResult` (large struct; boxing keeps the enum
/// variants similarly sized).
enum BatchOutcome {
  Continue,
  Stop(Box<AgentRunResult>),
}

/// Outcome of one turn's LLM call (RFC_HARNESS_LOOP_OWNERSHIP §6, series
/// step 2). `Proceed` carries the response for the parse + dispatch
/// phase; `Stop` carries a terminal result (cancel / timeout).
enum LlmTurnOutcome {
  Proceed {
    llm_response: LLMResponse,
    raw_response: String,
  },
  Stop(AgentRunResult),
}

/// Outcome of a single-tool execution under timeout/cancellation limits
/// (RFC_HARNESS_LOOP_OWNERSHIP §6, series step 3b). `Output` carries the
/// tool result for the rest of the `Action` arm; `Stop` carries a
/// terminal result (timeout / cancellation).
enum ToolExecOutcome {
  Output(agentflow_tools::ToolOutput),
  Stop(AgentRunResult),
}

/// Outcome of processing one turn (RFC_HARNESS_LOOP_OWNERSHIP §6).
/// `Continue` means advance to the next turn; `Stop` carries the terminal
/// result. This is the shape [`ReActAgent::run_one_turn`] returns.
enum TurnStep {
  Continue,
  Stop(AgentRunResult),
}

/// Mutable + per-run state threaded through [`ReActAgent::run_one_turn`]
/// (RFC_HARNESS_LOOP_OWNERSHIP §6, steps 4–5). Bundling the loop's locals
/// into one struct is what makes a single turn callable in isolation —
/// the basis for a future `LoopSession`. The first six fields mutate
/// across turns; the rest are per-run configuration fixed at start.
struct LoopState {
  steps: Vec<AgentStep>,
  events: Vec<AgentEvent>,
  step_index: usize,
  iteration: usize,
  tool_calls: usize,
  last_tool_call: Option<(String, serde_json::Value)>,
  max_iterations: usize,
  max_tool_calls: Option<usize>,
  timeout_ms: Option<u64>,
  budget_tokens: Option<u32>,
  cancellation_token: Option<AgentCancellationToken>,
  run_started_at: Instant,
  system_prompt: String,
  trace_context: Option<agentflow_llm::LlmTraceContext>,
  between_turn_hook: Option<crate::runtime::BetweenTurnHookHandle>,
}

/// Outcome of one driven turn (RFC_HARNESS_LOOP_OWNERSHIP §6 step 6).
#[derive(Debug)]
pub enum TurnProgress {
  /// The agent advanced; call [`ReActLoopSession::next_turn`] again.
  Continued,
  /// The agent reached a terminal state; the run result is attached.
  Finished(AgentRunResult),
}

/// A **turn-driven** ReAct session (RFC_HARNESS_LOOP_OWNERSHIP §6 step 6).
///
/// Obtain one from [`ReActAgent::begin_turn_driven`], then drive it a
/// turn at a time with [`Self::next_turn`] until it returns
/// [`TurnProgress::Finished`]. Between turns the caller (typically the
/// Harness) owns the loop: it can inspect or rewrite the conversation
/// through [`Self::memory`] to compact / refresh context under its own
/// policy. [`ReActAgent::run_with_context`] is the equivalent
/// batteries-included driver that pumps every turn itself.
pub struct ReActLoopSession<'a> {
  agent: &'a mut ReActAgent,
  state: LoopState,
  finished: bool,
}

impl ReActLoopSession<'_> {
  /// Advance exactly one turn (one observe → plan → act cycle). Returns
  /// [`TurnProgress::Finished`] once the agent reaches a terminal state;
  /// calling again afterwards is a [`ReActError::SessionFinished`].
  pub async fn next_turn(&mut self) -> Result<TurnProgress, ReActError> {
    if self.finished {
      return Err(ReActError::SessionFinished);
    }
    match self.agent.run_one_turn(&mut self.state).await? {
      TurnStep::Continue => Ok(TurnProgress::Continued),
      TurnStep::Stop(result) => {
        self.finished = true;
        Ok(TurnProgress::Finished(result))
      }
    }
  }

  /// The run's conversation memory — read or rewrite it between turns to
  /// perform caller-owned context engineering.
  pub fn memory(&self) -> &dyn MemoryStore {
    self.agent.memory_ref()
  }

  /// 0-based index of the turn `next_turn` will run next.
  pub fn turn_index(&self) -> usize {
    self.state.iteration
  }

  /// Whether the session has reached a terminal state.
  pub fn is_finished(&self) -> bool {
    self.finished
  }
}

/// Internal staging record for one tool call in a multi-call batch.
struct PreparedToolCall {
  tool: String,
  params: Value,
  call_step_idx: usize,
  idempotency: ToolIdempotency,
  source: Option<String>,
  permissions: Vec<String>,
}

#[async_trait]
impl AgentRuntime for ReActAgent {
  async fn run(&mut self, context: AgentContext) -> Result<AgentRunResult, AgentRuntimeError> {
    self
      .run_with_context(context)
      .await
      .map_err(|err| AgentRuntimeError::ExecutionFailed {
        message: err.to_string(),
      })
  }

  fn runtime_name(&self) -> &'static str {
    "react"
  }
}

fn timed_out(started_at: Instant, timeout_ms: Option<u64>) -> bool {
  timeout_ms
    .map(Duration::from_millis)
    .is_some_and(|timeout| started_at.elapsed() >= timeout)
}

fn remaining_timeout(started_at: Instant, timeout_ms: Option<u64>) -> Option<Duration> {
  timeout_ms
    .map(Duration::from_millis)
    .map(|timeout| timeout.saturating_sub(started_at.elapsed()))
}

fn is_cancelled(token: &Option<AgentCancellationToken>) -> bool {
  token
    .as_ref()
    .is_some_and(AgentCancellationToken::is_cancelled)
}

/// Convert a provider-emitted native tool call into the existing
/// `AgentResponse::Action` shape so the rest of the ReAct loop is unchanged.
///
/// `thought` is left empty: native tool calls don't carry a separate
/// reasoning field. Tool result correlation still works because the tool
/// name + arguments are preserved verbatim.
fn native_tool_call_to_agent_response(call: &ToolCallRequest) -> AgentResponse {
  AgentResponse::Action {
    thought: String::new(),
    tool: call.name.clone(),
    params: call.arguments.clone(),
  }
}

fn has_unresolved_tool_call(result: &AgentRunResult) -> bool {
  result.steps.iter().any(|step| {
    let AgentStepKind::ToolCall { tool, .. } = &step.kind else {
      return false;
    };
    !result.steps.iter().any(|candidate| {
      matches!(
        &candidate.kind,
        AgentStepKind::ToolResult {
          tool: result_tool,
          ..
        } if result_tool == tool && candidate.index > step.index
      )
    })
  })
}

fn tool_event_metadata(metadata: Option<&ToolMetadata>) -> (Option<String>, Vec<String>) {
  match metadata {
    Some(metadata) => (
      Some(metadata.source.as_str().to_string()),
      metadata
        .permissions
        .permissions
        .iter()
        .map(|permission| permission.as_str().to_string())
        .collect(),
    ),
    None => (None, Vec::new()),
  }
}

fn annotate_tool_params_for_resume(
  mut params: Value,
  idempotency: Option<ToolIdempotency>,
) -> Value {
  let Some(idempotency) = idempotency else {
    return params;
  };
  let side_effect_class = match idempotency {
    ToolIdempotency::Idempotent => "idempotent",
    ToolIdempotency::NonIdempotent => "mutating",
    ToolIdempotency::Unknown => return params,
  };

  let Value::Object(map) = &mut params else {
    return json!({
      "value": params,
      "_agentflow": {
        "side_effect_class": side_effect_class
      }
    });
  };

  let mut metadata = map.remove("_agentflow").unwrap_or_else(|| json!({}));
  if !metadata.is_object() {
    metadata = json!({});
  }
  if let Value::Object(metadata_map) = &mut metadata {
    metadata_map
      .entry("side_effect_class".to_string())
      .or_insert_with(|| json!(side_effect_class));
  }
  map.insert("_agentflow".to_string(), metadata);
  params
}

fn is_resume_safe_tool_call(params: &Value) -> bool {
  matches!(
    params
      .get("_agentflow")
      .and_then(|metadata| metadata.get("side_effect_class"))
      .or_else(|| params.get("side_effect_class"))
      .and_then(Value::as_str),
    Some("read_only") | Some("idempotent")
  )
}

fn strip_agentflow_metadata(mut params: Value) -> Value {
  let Value::Object(map) = &mut params else {
    return params;
  };
  map.remove("_agentflow");
  map.remove("side_effect_class");
  params
}

fn merge_resumed_result(mut prior: AgentRunResult, mut resumed: AgentRunResult) -> AgentRunResult {
  let step_offset = prior
    .steps
    .iter()
    .map(|step| step.index)
    .max()
    .map(|idx| idx + 1)
    .unwrap_or(0);
  for mut step in resumed.steps {
    step.index += step_offset;
    prior.steps.push(step);
  }

  let event_offset = step_offset;
  for event in &mut resumed.events {
    match event {
      AgentEvent::StepStarted { step_index, .. }
      | AgentEvent::ToolCallStarted { step_index, .. }
      | AgentEvent::ToolPolicyDecision { step_index, .. }
      | AgentEvent::ToolCapabilityDecision { step_index, .. }
      | AgentEvent::ToolCallCompleted { step_index, .. }
      | AgentEvent::LlmCallCompleted { step_index, .. }
      | AgentEvent::ReflectionAdded { step_index, .. }
      | AgentEvent::HandoffOccurred { step_index, .. }
      | AgentEvent::BlackboardWritten { step_index, .. }
      | AgentEvent::DebateVerdictRendered { step_index, .. } => {
        *step_index += event_offset;
      }
      AgentEvent::StepCompleted { step, .. } => {
        step.index += event_offset;
      }
      AgentEvent::RunStarted { .. }
      | AgentEvent::RunStopped { .. }
      | AgentEvent::MemorySummaryAdded { .. }
      | AgentEvent::DebateRoundStarted { .. } => {}
    }
  }
  prior.events.extend(resumed.events);
  prior.answer = resumed.answer;
  prior.stop_reason = resumed.stop_reason;
  prior
}

fn compact_memory_summary(omitted: &[Message], omitted_tokens: u32) -> String {
  let mut lines = vec![format!(
    "[Memory Summary]\n{} older messages compacted (approx {} tokens):",
    omitted.len(),
    omitted_tokens
  )];
  for message in omitted.iter().take(8) {
    let mut content = message.content.replace('\n', " ");
    if content.len() > 160 {
      content.truncate(160);
      content.push_str("...");
    }
    lines.push(format!("- {}: {}", message.role, content));
  }
  if omitted.len() > 8 {
    lines.push(format!("- ... {} more messages", omitted.len() - 8));
  }
  lines.join("\n")
}

#[cfg(test)]
mod tests {
  use super::*;
  use agentflow_memory::SessionMemory;
  use agentflow_tools::{Tool, ToolError, ToolOutput};
  use async_trait::async_trait;
  use serde_json::{Value, json};
  use std::fs;
  use std::sync::atomic::{AtomicUsize, Ordering};
  use std::sync::{Arc, Mutex};

  struct EchoTool;

  struct CountingTool {
    calls: Arc<AtomicUsize>,
  }

  #[derive(Default)]
  struct RecordingMemoryHook {
    events: Mutex<Vec<MemoryHookContext>>,
  }

  impl AgentMemoryHook for RecordingMemoryHook {
    fn on_memory_read(&self, context: &MemoryHookContext) {
      self.events.lock().unwrap().push(context.clone());
    }

    fn on_memory_write(&self, context: &MemoryHookContext) {
      self.events.lock().unwrap().push(context.clone());
    }
  }

  #[derive(Default)]
  struct RecordingSummaryBackend {
    contexts: Mutex<Vec<MemorySummaryContext>>,
  }

  #[async_trait]
  impl MemorySummaryBackend for RecordingSummaryBackend {
    fn name(&self) -> &'static str {
      "recording"
    }

    async fn summarize(&self, context: MemorySummaryContext) -> Result<Option<String>, ReActError> {
      self.contexts.lock().unwrap().push(context.clone());
      Ok(Some(format!(
        "[Custom Summary] omitted={} kept={}",
        context.omitted_messages.len(),
        context.kept_messages.len()
      )))
    }
  }

  #[async_trait]
  impl Tool for EchoTool {
    fn name(&self) -> &str {
      "echo"
    }

    fn description(&self) -> &str {
      "Echo test input"
    }

    fn parameters_schema(&self) -> Value {
      json!({
        "type": "object",
        "properties": {
          "text": {"type": "string"}
        },
        "required": ["text"]
      })
    }

    async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
      Ok(ToolOutput::success(format!(
        "echo: {}",
        params["text"].as_str().unwrap_or_default()
      )))
    }
  }

  #[async_trait]
  impl Tool for CountingTool {
    fn name(&self) -> &str {
      "counting_echo"
    }

    fn description(&self) -> &str {
      "Echo input and count executions"
    }

    fn parameters_schema(&self) -> Value {
      json!({
        "type": "object",
        "properties": {
          "text": {"type": "string"}
        },
        "required": ["text"]
      })
    }

    fn idempotency(&self, _params: &Value) -> ToolIdempotency {
      ToolIdempotency::Idempotent
    }

    async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
      self.calls.fetch_add(1, Ordering::SeqCst);
      Ok(ToolOutput::success(format!(
        "echo: {}",
        params["text"].as_str().unwrap_or_default()
      )))
    }
  }

  async fn init_mock_model(model: &str) {
    let config_path = std::env::temp_dir().join(format!(
      "agentflow-agents-mock-{}.yml",
      uuid::Uuid::new_v4()
    ));
    fs::write(
      &config_path,
      format!(
        r#"
models:
  {model}:
    vendor: mock
    type: text
    model_id: {model}
providers:
  mock:
    api_key_env: MOCK_API_KEY
"#
      ),
    )
    .unwrap();

    AgentFlow::init_with_config(config_path.to_str().unwrap())
      .await
      .unwrap();
  }

  #[tokio::test]
  async fn run_with_context_records_steps_events_and_reflection_with_mock_llm() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-runtime-{}", uuid::Uuid::new_v4());
    // SAFETY: LLM_TEST_LOCK serializes mutation of process-wide mock env vars.
    unsafe {
      std::env::set_var(
        "AGENTFLOW_MOCK_RESPONSES",
        serde_json::to_string(&vec![
          r#"{"thought":"use tool","action":{"tool":"echo","params":{"text":"hi"}}}"#,
          r#"{"thought":"done","answer":"final: echo: hi"}"#,
        ])
        .unwrap(),
      );
    }
    init_mock_model(&model).await;

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(EchoTool));
    let memory_hook = Arc::new(RecordingMemoryHook::default());
    let mut agent = ReActAgent::new(
      ReActConfig::new(&model).with_max_iterations(4),
      Box::new(SessionMemory::default_window()),
      Arc::new(registry),
    )
    .with_memory_hook(memory_hook.clone())
    .with_reflection_strategy(Arc::new(crate::reflection::FinalReflection));

    let result = agent
      .run_with_context(AgentContext::new("session-1", "say hi", &model))
      .await
      .unwrap();

    assert_eq!(result.answer.as_deref(), Some("final: echo: hi"));
    assert_eq!(result.stop_reason, AgentStopReason::FinalAnswer);
    assert!(
      result
        .steps
        .iter()
        .any(|step| matches!(step.kind, AgentStepKind::ToolCall { .. }))
    );
    assert!(
      result
        .steps
        .iter()
        .any(|step| matches!(step.kind, AgentStepKind::ToolResult { .. }))
    );
    assert!(
      result
        .steps
        .iter()
        .any(|step| matches!(step.kind, AgentStepKind::Reflect { .. }))
    );
    assert!(
      result
        .events
        .iter()
        .any(|event| matches!(event, AgentEvent::ToolCallCompleted { .. }))
    );
    assert!(
      result
        .events
        .iter()
        .any(|event| matches!(event, AgentEvent::ReflectionAdded { .. }))
    );

    let memory_events = memory_hook.events.lock().unwrap();
    let read_sizes: Vec<usize> = memory_events
      .iter()
      .filter(|event| event.kind == MemoryHookKind::ReadHistory)
      .map(|event| event.messages.len())
      .collect();
    assert_eq!(read_sizes, vec![1, 3]);
    assert_eq!(
      memory_events
        .iter()
        .filter(|event| event.kind == MemoryHookKind::Write)
        .count(),
      4
    );
  }

  #[tokio::test]
  async fn run_with_context_consumes_native_tool_calls_when_available() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-native-tool-{}", uuid::Uuid::new_v4());
    // SAFETY: LLM_TEST_LOCK serializes mutation of process-wide mock env vars.
    //
    // Drives the ReAct loop through the native tool-calling path: iteration
    // 0 emits a tool call (via AGENTFLOW_MOCK_TOOL_CALLS), iteration 1 emits
    // an empty batch and a JSON answer (via AGENTFLOW_MOCK_RESPONSES). The
    // first response would be malformed JSON for the prompt parser, so a
    // successful tool call here proves the native path was actually taken.
    unsafe {
      std::env::set_var(
        "AGENTFLOW_MOCK_TOOL_CALLS",
        serde_json::to_string(&vec![
          vec![serde_json::json!({
            "id": "call_0",
            "name": "echo",
            "arguments": {"text": "hi"}
          })],
          Vec::<serde_json::Value>::new(),
        ])
        .unwrap(),
      );
      std::env::set_var(
        "AGENTFLOW_MOCK_RESPONSES",
        serde_json::to_string(&vec![
          // Iteration 0 content is irrelevant; tool_calls drive the loop.
          "(unused — native tool call)",
          r#"{"thought":"done","answer":"final: echo: hi"}"#,
        ])
        .unwrap(),
      );
    }
    init_mock_model(&model).await;

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(EchoTool));
    let mut agent = ReActAgent::new(
      ReActConfig::new(&model).with_max_iterations(4),
      Box::new(SessionMemory::default_window()),
      Arc::new(registry),
    )
    .with_reflection_strategy(Arc::new(crate::reflection::FinalReflection));

    let result = agent
      .run_with_context(AgentContext::new("session-native-tool", "say hi", &model))
      .await
      .unwrap();

    assert_eq!(result.answer.as_deref(), Some("final: echo: hi"));
    assert_eq!(result.stop_reason, AgentStopReason::FinalAnswer);
    let tool_call_count = result
      .steps
      .iter()
      .filter(|step| matches!(step.kind, AgentStepKind::ToolCall { .. }))
      .count();
    assert_eq!(tool_call_count, 1, "expected exactly one ToolCall step");

    // SAFETY: cleanup of the dedicated mock env vars after the test read.
    unsafe {
      std::env::remove_var("AGENTFLOW_MOCK_TOOL_CALLS");
      std::env::remove_var("AGENTFLOW_MOCK_RESPONSES");
    }
  }

  /// Phase 2b: the between-turn hook fires once at the top of every turn,
  /// before that turn's LLM call, with the 0-based turn index — the
  /// control point a loop owner uses for between-turn context engineering.
  #[tokio::test]
  async fn between_turn_hook_fires_before_each_turn() {
    use crate::runtime::BetweenTurnHook;

    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-turn-hook-{}", uuid::Uuid::new_v4());
    // Two turns: iteration 0 emits a tool call; iteration 1 the answer.
    unsafe {
      std::env::set_var(
        "AGENTFLOW_MOCK_TOOL_CALLS",
        serde_json::to_string(&vec![
          vec![serde_json::json!({"id":"call_0","name":"echo","arguments":{"text":"hi"}})],
          Vec::<serde_json::Value>::new(),
        ])
        .unwrap(),
      );
      std::env::set_var(
        "AGENTFLOW_MOCK_RESPONSES",
        serde_json::to_string(&vec![
          "(unused — native tool call)",
          r#"{"thought":"done","answer":"final"}"#,
        ])
        .unwrap(),
      );
    }
    init_mock_model(&model).await;

    struct CountingHook {
      seen: Arc<std::sync::Mutex<Vec<usize>>>,
    }
    #[async_trait]
    impl BetweenTurnHook for CountingHook {
      async fn before_turn(&self, turn_index: usize, _session_id: &str, _memory: &dyn MemoryStore) {
        self.seen.lock().unwrap().push(turn_index);
      }
    }

    let seen = Arc::new(std::sync::Mutex::new(Vec::<usize>::new()));
    let hook = Arc::new(CountingHook { seen: seen.clone() });

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(EchoTool));
    let mut agent = ReActAgent::new(
      ReActConfig::new(&model).with_max_iterations(4),
      Box::new(SessionMemory::default_window()),
      Arc::new(registry),
    )
    .with_reflection_strategy(Arc::new(crate::reflection::FinalReflection));

    let result = agent
      .run_with_context(
        AgentContext::new("turn-hook-session", "say hi", &model).with_between_turn_hook(hook),
      )
      .await
      .unwrap();
    assert_eq!(result.stop_reason, AgentStopReason::FinalAnswer);

    unsafe {
      std::env::remove_var("AGENTFLOW_MOCK_TOOL_CALLS");
      std::env::remove_var("AGENTFLOW_MOCK_RESPONSES");
    }

    assert_eq!(
      *seen.lock().unwrap(),
      vec![0, 1],
      "hook must fire before each turn with the 0-based turn index"
    );
  }

  /// RFC §6 step 6: the turn-driven session pumps one turn at a time —
  /// `Continued` while the agent works, `Finished(result)` at the end —
  /// exposes memory between turns, and rejects use after completion.
  #[tokio::test]
  async fn turn_driven_session_advances_then_finishes() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-turn-driven-{}", uuid::Uuid::new_v4());
    // Turn 0: a tool call; turn 1: the final answer.
    unsafe {
      std::env::set_var(
        "AGENTFLOW_MOCK_TOOL_CALLS",
        serde_json::to_string(&vec![
          vec![serde_json::json!({"id":"call_0","name":"echo","arguments":{"text":"hi"}})],
          Vec::<serde_json::Value>::new(),
        ])
        .unwrap(),
      );
      std::env::set_var(
        "AGENTFLOW_MOCK_RESPONSES",
        serde_json::to_string(&vec![
          "(unused — native tool call)",
          r#"{"thought":"done","answer":"final: ok"}"#,
        ])
        .unwrap(),
      );
    }
    init_mock_model(&model).await;

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(EchoTool));
    let mut agent = ReActAgent::new(
      ReActConfig::new(&model).with_max_iterations(4),
      Box::new(SessionMemory::default_window()),
      Arc::new(registry),
    )
    .with_reflection_strategy(Arc::new(crate::reflection::FinalReflection));

    let mut session = agent
      .begin_turn_driven(AgentContext::new("turn-driven-session", "say hi", &model))
      .await
      .unwrap();
    assert_eq!(session.turn_index(), 0);

    // Turn 0: tool call → Continued; memory is observable between turns.
    assert!(matches!(
      session.next_turn().await.unwrap(),
      TurnProgress::Continued
    ));
    assert!(!session.is_finished());
    assert_eq!(session.turn_index(), 1);
    let history = session
      .memory()
      .get_all("turn-driven-session")
      .await
      .unwrap();
    assert!(!history.is_empty(), "memory accessible mid-run");

    // Turn 1: final answer → Finished.
    let result = match session.next_turn().await.unwrap() {
      TurnProgress::Finished(r) => r,
      TurnProgress::Continued => panic!("expected Finished on the second turn"),
    };
    assert_eq!(result.answer.as_deref(), Some("final: ok"));
    assert_eq!(result.stop_reason, AgentStopReason::FinalAnswer);
    assert!(session.is_finished());

    // Using the session after it finished is an error.
    assert!(matches!(
      session.next_turn().await,
      Err(ReActError::SessionFinished)
    ));

    unsafe {
      std::env::remove_var("AGENTFLOW_MOCK_TOOL_CALLS");
      std::env::remove_var("AGENTFLOW_MOCK_RESPONSES");
    }
  }

  #[tokio::test]
  async fn batch_path_runs_multiple_idempotent_tool_calls_in_order() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-batch-{}", uuid::Uuid::new_v4());
    // SAFETY: LLM_TEST_LOCK serializes mutation of process-wide mock env vars.
    //
    // Iteration 0 emits three native tool calls in one turn; iteration 1
    // emits an empty batch and the final answer.
    unsafe {
      std::env::set_var(
        "AGENTFLOW_MOCK_TOOL_CALLS",
        serde_json::to_string(&vec![
          vec![
            serde_json::json!({"id": "call_a", "name": "counting_echo", "arguments": {"text": "a"}}),
            serde_json::json!({"id": "call_b", "name": "counting_echo", "arguments": {"text": "b"}}),
            serde_json::json!({"id": "call_c", "name": "counting_echo", "arguments": {"text": "c"}}),
          ],
          Vec::<serde_json::Value>::new(),
        ])
        .unwrap(),
      );
      std::env::set_var(
        "AGENTFLOW_MOCK_RESPONSES",
        serde_json::to_string(&vec![
          "(unused — native batch)",
          r#"{"thought":"done","answer":"batch complete"}"#,
        ])
        .unwrap(),
      );
    }
    init_mock_model(&model).await;

    let calls = Arc::new(AtomicUsize::new(0));
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(CountingTool {
      calls: calls.clone(),
    }));
    let mut agent = ReActAgent::new(
      ReActConfig::new(&model).with_max_iterations(4),
      Box::new(SessionMemory::default_window()),
      Arc::new(registry),
    );
    let result = agent
      .run_with_context(AgentContext::new("session-batch", "go", &model))
      .await
      .unwrap();

    assert_eq!(result.answer.as_deref(), Some("batch complete"));
    assert_eq!(result.stop_reason, AgentStopReason::FinalAnswer);
    // All three idempotent calls executed.
    assert_eq!(
      calls.load(Ordering::SeqCst),
      3,
      "all three calls in the batch should run"
    );

    // ToolCallStarted events must appear in LLM-returned (a, b, c) order.
    let started: Vec<String> = result
      .events
      .iter()
      .filter_map(|event| match event {
        AgentEvent::ToolCallStarted { params, .. } => {
          params["text"].as_str().map(|s| s.to_string())
        }
        _ => None,
      })
      .collect();
    assert_eq!(
      started,
      vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );

    // ToolCall steps must also be in LLM order.
    let step_order: Vec<String> = result
      .steps
      .iter()
      .filter_map(|step| match &step.kind {
        AgentStepKind::ToolCall { params, .. } => params["text"].as_str().map(|s| s.to_string()),
        _ => None,
      })
      .collect();
    assert_eq!(
      step_order,
      vec!["a".to_string(), "b".to_string(), "c".to_string()]
    );

    // ToolCallCompleted matches LLM order via step_index.
    let started_indexes: Vec<usize> = result
      .events
      .iter()
      .filter_map(|event| match event {
        AgentEvent::ToolCallStarted { step_index, .. } => Some(*step_index),
        _ => None,
      })
      .collect();
    let completed_indexes: Vec<usize> = result
      .events
      .iter()
      .filter_map(|event| match event {
        AgentEvent::ToolCallCompleted { step_index, .. } => Some(*step_index),
        _ => None,
      })
      .collect();
    assert_eq!(started_indexes, completed_indexes);

    // SAFETY: cleanup the dedicated mock env vars after the test read.
    unsafe {
      std::env::remove_var("AGENTFLOW_MOCK_TOOL_CALLS");
      std::env::remove_var("AGENTFLOW_MOCK_RESPONSES");
    }
  }

  #[tokio::test]
  async fn batch_path_continues_when_one_tool_fails() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-batch-partial-{}", uuid::Uuid::new_v4());
    unsafe {
      std::env::set_var(
        "AGENTFLOW_MOCK_TOOL_CALLS",
        serde_json::to_string(&vec![
          vec![
            serde_json::json!({"id": "ok1", "name": "counting_echo", "arguments": {"text": "ok"}}),
            serde_json::json!({"id": "boom", "name": "exploding", "arguments": {}}),
            serde_json::json!({"id": "ok2", "name": "counting_echo", "arguments": {"text": "ok2"}}),
          ],
          Vec::<serde_json::Value>::new(),
        ])
        .unwrap(),
      );
      std::env::set_var(
        "AGENTFLOW_MOCK_RESPONSES",
        serde_json::to_string(&vec![
          "(unused)",
          r#"{"thought":"done","answer":"partial done"}"#,
        ])
        .unwrap(),
      );
    }
    init_mock_model(&model).await;

    struct Exploding;
    #[async_trait]
    impl Tool for Exploding {
      fn name(&self) -> &str {
        "exploding"
      }
      fn description(&self) -> &str {
        "always errors"
      }
      fn parameters_schema(&self) -> Value {
        json!({"type": "object"})
      }
      fn idempotency(&self, _params: &Value) -> ToolIdempotency {
        ToolIdempotency::Idempotent
      }
      async fn execute(&self, _params: Value) -> Result<ToolOutput, ToolError> {
        Err(ToolError::ExecutionFailed {
          message: "exploded".into(),
        })
      }
    }

    let calls = Arc::new(AtomicUsize::new(0));
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(CountingTool {
      calls: calls.clone(),
    }));
    registry.register(Arc::new(Exploding));
    let mut agent = ReActAgent::new(
      ReActConfig::new(&model).with_max_iterations(4),
      Box::new(SessionMemory::default_window()),
      Arc::new(registry),
    );

    let result = agent
      .run_with_context(AgentContext::new("session-partial", "go", &model))
      .await
      .unwrap();

    assert_eq!(result.answer.as_deref(), Some("partial done"));
    assert_eq!(result.stop_reason, AgentStopReason::FinalAnswer);
    // Both ok calls still ran despite the middle one erroring.
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    // Verify one ToolCallCompleted is_error=true and two are false.
    let (errors, successes) = result.events.iter().fold((0, 0), |(e, s), event| {
      if let AgentEvent::ToolCallCompleted { is_error, .. } = event {
        if *is_error { (e + 1, s) } else { (e, s + 1) }
      } else {
        (e, s)
      }
    });
    assert_eq!(errors, 1);
    assert_eq!(successes, 2);

    // Step trace has three ToolResult entries.
    let result_steps = result
      .steps
      .iter()
      .filter(|step| matches!(step.kind, AgentStepKind::ToolResult { .. }))
      .count();
    assert_eq!(result_steps, 3);

    unsafe {
      std::env::remove_var("AGENTFLOW_MOCK_TOOL_CALLS");
      std::env::remove_var("AGENTFLOW_MOCK_RESPONSES");
    }
  }

  #[tokio::test]
  async fn batch_path_returns_cancelled_when_token_already_signalled() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-batch-cancel-{}", uuid::Uuid::new_v4());
    unsafe {
      std::env::set_var(
        "AGENTFLOW_MOCK_TOOL_CALLS",
        serde_json::to_string(&vec![vec![
          serde_json::json!({"id": "c1", "name": "counting_echo", "arguments": {"text": "a"}}),
          serde_json::json!({"id": "c2", "name": "counting_echo", "arguments": {"text": "b"}}),
        ]])
        .unwrap(),
      );
      std::env::set_var(
        "AGENTFLOW_MOCK_RESPONSES",
        serde_json::to_string(&vec!["(unused)"]).unwrap(),
      );
    }
    init_mock_model(&model).await;

    let token = AgentCancellationToken::new();
    token.cancel(); // pre-cancelled

    let calls = Arc::new(AtomicUsize::new(0));
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(CountingTool {
      calls: calls.clone(),
    }));
    let mut agent = ReActAgent::new(
      ReActConfig::new(&model).with_max_iterations(4),
      Box::new(SessionMemory::default_window()),
      Arc::new(registry),
    );
    let result = agent
      .run_with_context(
        AgentContext::new("session-cancel", "go", &model).with_cancellation_token(token),
      )
      .await
      .unwrap();
    assert!(
      matches!(result.stop_reason, AgentStopReason::Cancelled { .. }),
      "expected Cancelled, got {:?}",
      result.stop_reason
    );

    unsafe {
      std::env::remove_var("AGENTFLOW_MOCK_TOOL_CALLS");
      std::env::remove_var("AGENTFLOW_MOCK_RESPONSES");
    }
  }

  #[tokio::test]
  async fn batch_path_blocks_when_max_tool_calls_would_be_exceeded() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-batch-max-{}", uuid::Uuid::new_v4());
    unsafe {
      std::env::set_var(
        "AGENTFLOW_MOCK_TOOL_CALLS",
        serde_json::to_string(&vec![vec![
          serde_json::json!({"id": "c1", "name": "counting_echo", "arguments": {"text": "a"}}),
          serde_json::json!({"id": "c2", "name": "counting_echo", "arguments": {"text": "b"}}),
          serde_json::json!({"id": "c3", "name": "counting_echo", "arguments": {"text": "c"}}),
        ]])
        .unwrap(),
      );
      std::env::set_var(
        "AGENTFLOW_MOCK_RESPONSES",
        serde_json::to_string(&vec!["(unused)"]).unwrap(),
      );
    }
    init_mock_model(&model).await;

    let calls = Arc::new(AtomicUsize::new(0));
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(CountingTool {
      calls: calls.clone(),
    }));
    let mut agent = ReActAgent::new(
      ReActConfig::new(&model).with_max_iterations(4),
      Box::new(SessionMemory::default_window()),
      Arc::new(registry),
    );
    let limits = RuntimeLimits {
      max_tool_calls: Some(2),
      ..Default::default()
    };
    let result = agent
      .run_with_context(AgentContext::new("session-max", "go", &model).with_limits(limits))
      .await
      .unwrap();
    assert!(
      matches!(
        result.stop_reason,
        AgentStopReason::MaxToolCalls { max_tool_calls: 2 }
      ),
      "expected MaxToolCalls, got {:?}",
      result.stop_reason
    );
    assert_eq!(
      calls.load(Ordering::SeqCst),
      0,
      "batch must reject atomically; no inner tool runs"
    );

    unsafe {
      std::env::remove_var("AGENTFLOW_MOCK_TOOL_CALLS");
      std::env::remove_var("AGENTFLOW_MOCK_RESPONSES");
    }
  }

  #[test]
  fn native_tool_call_to_agent_response_preserves_name_and_args() {
    let call = ToolCallRequest {
      id: "call_0".into(),
      name: "echo".into(),
      arguments: serde_json::json!({"text": "hi"}),
    };
    match native_tool_call_to_agent_response(&call) {
      AgentResponse::Action {
        thought,
        tool,
        params,
      } => {
        assert!(thought.is_empty());
        assert_eq!(tool, "echo");
        assert_eq!(params["text"], "hi");
      }
      other => panic!("expected Action, got {:?}", other),
    }
  }

  #[test]
  fn tool_params_annotation_maps_idempotency_to_resume_metadata() {
    let params = annotate_tool_params_for_resume(
      json!({"url": "https://example.test"}),
      Some(ToolIdempotency::Idempotent),
    );

    assert_eq!(
      params["_agentflow"]["side_effect_class"],
      json!("idempotent")
    );
  }

  #[tokio::test]
  async fn resume_with_context_reuses_recorded_tool_result_without_replay() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-resume-runtime-{}", uuid::Uuid::new_v4());
    // SAFETY: LLM_TEST_LOCK serializes mutation of process-wide mock env vars.
    unsafe {
      std::env::remove_var("AGENTFLOW_MOCK_RESPONSES");
      std::env::set_var(
        "AGENTFLOW_MOCK_RESPONSE",
        r#"{"thought":"use recovered observation","answer":"final: echo: hi"}"#,
      );
    }
    init_mock_model(&model).await;

    let calls = Arc::new(AtomicUsize::new(0));
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(CountingTool {
      calls: calls.clone(),
    }));
    let mut agent = ReActAgent::new(
      ReActConfig::new(&model).with_max_iterations(4),
      Box::new(SessionMemory::default_window()),
      Arc::new(registry),
    );

    let prior = AgentRunResult {
      session_id: "resume-session".to_string(),
      answer: None,
      stop_reason: AgentStopReason::Cancelled {
        message: "shutdown".to_string(),
      },
      steps: vec![
        AgentStep::new(
          0,
          AgentStepKind::Observe {
            input: "say hi".to_string(),
          },
        ),
        AgentStep::new(
          1,
          AgentStepKind::ToolCall {
            tool: "counting_echo".to_string(),
            params: json!({"text": "hi"}),
          },
        ),
        AgentStep::new(
          2,
          AgentStepKind::ToolResult {
            tool: "counting_echo".to_string(),
            content: "echo: hi".to_string(),
            is_error: false,
            parts: vec![],
          },
        ),
      ],
      events: vec![],
    };

    let result = agent
      .resume_with_context(
        AgentContext::new("resume-session", "finish the task", &model),
        prior,
      )
      .await
      .unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(result.answer.as_deref(), Some("final: echo: hi"));
    assert_eq!(result.stop_reason, AgentStopReason::FinalAnswer);
    assert!(result.steps.len() > 3);
  }

  #[tokio::test]
  async fn resume_with_context_replays_unresolved_idempotent_tool_call() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-resume-replay-{}", uuid::Uuid::new_v4());
    // SAFETY: LLM_TEST_LOCK serializes mutation of process-wide mock env vars.
    unsafe {
      std::env::remove_var("AGENTFLOW_MOCK_RESPONSES");
      std::env::set_var(
        "AGENTFLOW_MOCK_RESPONSE",
        r#"{"thought":"use recovered replay","answer":"final: echo: hi"}"#,
      );
    }
    init_mock_model(&model).await;

    let calls = Arc::new(AtomicUsize::new(0));
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(CountingTool {
      calls: calls.clone(),
    }));
    let mut agent = ReActAgent::new(
      ReActConfig::new(&model).with_max_iterations(2),
      Box::new(SessionMemory::default_window()),
      Arc::new(registry),
    );

    let prior = AgentRunResult {
      session_id: "resume-replay-session".to_string(),
      answer: None,
      stop_reason: AgentStopReason::Cancelled {
        message: "shutdown".to_string(),
      },
      steps: vec![AgentStep::new(
        1,
        AgentStepKind::ToolCall {
          tool: "counting_echo".to_string(),
          params: json!({
            "text": "hi",
            "_agentflow": {
              "side_effect_class": "idempotent"
            }
          }),
        },
      )],
      events: vec![],
    };

    let result = agent
      .resume_with_context(
        AgentContext::new("resume-replay-session", "finish", &model),
        prior,
      )
      .await
      .unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(result.steps.iter().any(|step| {
      matches!(
        &step.kind,
        AgentStepKind::ToolResult { tool, .. } if tool == "counting_echo"
      )
    }));
    assert_eq!(result.answer.as_deref(), Some("final: echo: hi"));
  }

  #[tokio::test]
  async fn record_reflection_can_be_disabled_even_with_strategy() {
    let agent = ReActAgent::new(
      ReActConfig::new("mock-runtime")
        .with_max_iterations(4)
        .with_reflection_enabled(false),
      Box::new(SessionMemory::default_window()),
      Arc::new(ToolRegistry::new()),
    )
    .with_session_id("session-no-reflection")
    .with_reflection_strategy(Arc::new(crate::reflection::FinalReflection));
    let mut step_index = 1;
    let mut steps = vec![];
    let mut events = vec![];

    agent
      .record_reflection(
        crate::reflection::ReflectionContext::final_answer(
          "session-no-reflection",
          step_index,
          "done",
        ),
        &mut step_index,
        &mut steps,
        &mut events,
      )
      .await
      .unwrap();

    assert_eq!(step_index, 1);
    assert!(steps.is_empty());
    assert!(events.is_empty());
  }

  #[tokio::test]
  async fn query_memory_uses_backing_memory_search_for_current_session() {
    let memory = SessionMemory::default_window();
    memory
      .add_message(Message::user(
        "memory-session",
        "Remember that semantic search belongs to runtime memory.",
      ))
      .await
      .unwrap();
    memory
      .add_message(Message::assistant("other-session", "semantic but isolated"))
      .await
      .unwrap();

    let agent = ReActAgent::new(
      ReActConfig::new("mock-runtime"),
      Box::new(memory),
      Arc::new(ToolRegistry::new()),
    )
    .with_session_id("memory-session");

    let hits = agent.query_memory("semantic search", 5).await.unwrap();

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].session_id, "memory-session");
    assert!(hits[0].content.contains("semantic search"));
  }

  #[tokio::test]
  async fn memory_hook_observes_loop_reads_searches_and_writes() {
    let hook = Arc::new(RecordingMemoryHook::default());
    let mut agent = ReActAgent::new(
      ReActConfig::new("mock-runtime"),
      Box::new(SessionMemory::default_window()),
      Arc::new(ToolRegistry::new()),
    )
    .with_session_id("hook-session")
    .with_memory_hook(hook.clone());

    agent
      .add_memory_message(Message::user("hook-session", "semantic hook memory"))
      .await
      .unwrap();
    let _messages = agent
      .build_llm_messages(&agent.build_system_prompt())
      .await
      .unwrap();
    let hits = agent.query_memory("semantic", 3).await.unwrap();

    assert_eq!(hits.len(), 1);
    let events = hook.events.lock().unwrap();
    assert!(
      events
        .iter()
        .any(|event| event.kind == MemoryHookKind::Write && event.messages.len() == 1)
    );
    assert!(
      events
        .iter()
        .any(|event| event.kind == MemoryHookKind::ReadHistory && event.messages.len() == 1)
    );
    assert!(events.iter().any(|event| {
      event.kind == MemoryHookKind::Search
        && event.query.as_deref() == Some("semantic")
        && event.limit == Some(3)
        && event.messages.len() == 1
    }));
  }

  #[tokio::test]
  async fn run_with_context_returns_cancelled_when_token_already_signalled() {
    let token = AgentCancellationToken::new();
    token.cancel();
    let mut agent = ReActAgent::new(
      ReActConfig::new("mock-runtime"),
      Box::new(SessionMemory::default_window()),
      Arc::new(ToolRegistry::new()),
    );

    let result = agent
      .run_with_context(
        AgentContext::new("cancel-session", "do work", "mock-runtime")
          .with_cancellation_token(token),
      )
      .await
      .unwrap();

    assert_eq!(
      result.stop_reason,
      AgentStopReason::Cancelled {
        message: "cancellation token signalled".to_string(),
      }
    );
    assert!(result.answer.is_none());
    assert_eq!(result.steps.len(), 1);
    assert!(matches!(
      result.events.last(),
      Some(AgentEvent::RunStopped {
        reason: AgentStopReason::Cancelled { .. },
        ..
      })
    ));
  }

  #[test]
  fn compact_memory_summary_formats_older_messages() {
    let mut older = Message::user("budget-session", "older context about project goals");
    older.token_count = 10;

    let summary = compact_memory_summary(&[older], 10);

    assert!(summary.contains("1 older messages compacted"));
    assert!(summary.contains("older context about project goals"));
  }

  #[tokio::test]
  async fn memory_prompt_budget_compacts_older_messages() {
    let agent = ReActAgent::new(
      ReActConfig::new("mock-runtime")
        .with_memory_prompt_token_budget(8)
        .with_memory_summary_strategy(MemorySummaryStrategy::Compact),
      Box::new(SessionMemory::default_window()),
      Arc::new(ToolRegistry::new()),
    );
    let mut older = Message::user("budget-session", "older context about project goals");
    older.token_count = 10;
    let mut recent = Message::assistant("budget-session", "recent answer");
    recent.token_count = 4;

    let (summary, kept) = agent
      .apply_memory_prompt_budget(vec![older, recent.clone()])
      .await
      .unwrap();

    let summary = summary.unwrap();
    assert!(summary.contains("1 older messages compacted"));
    assert!(summary.contains("older context about project goals"));
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].content, recent.content);
  }

  /// Phase 2b: when the agent compacts prompt memory mid-run, it emits a
  /// `MemorySummaryAdded` event to the live sink so the Harness bridge can
  /// surface the between-turn context engineering. Pre-2b this compaction
  /// was invisible.
  #[tokio::test]
  async fn build_llm_messages_emits_memory_summary_added_when_compacting() {
    use crate::runtime::{AgentEventSink, EventSinkHandle};
    use std::sync::Mutex as StdMutex;

    struct RecordingSink {
      events: Arc<StdMutex<Vec<AgentEvent>>>,
    }
    #[async_trait]
    impl AgentEventSink for RecordingSink {
      async fn emit(&self, event: &AgentEvent) {
        self.events.lock().unwrap().push(event.clone());
      }
    }

    let recorded = Arc::new(StdMutex::new(Vec::new()));
    let sink = Arc::new(RecordingSink {
      events: recorded.clone(),
    });
    let mut agent = ReActAgent::new(
      ReActConfig::new("mock-runtime")
        .with_memory_prompt_token_budget(8)
        .with_memory_summary_strategy(MemorySummaryStrategy::Compact),
      Box::new(SessionMemory::default_window()),
      Arc::new(ToolRegistry::new()),
    )
    .with_session_id("emit-session");
    agent.live_sink = Some(EventSinkHandle(sink as Arc<dyn AgentEventSink>));

    // Populate memory over the 8-token budget so compaction fires.
    let mut older = Message::user("emit-session", "older context about project goals");
    older.token_count = 10;
    let mut recent = Message::assistant("emit-session", "recent answer");
    recent.token_count = 4;
    agent.add_memory_message(older).await.unwrap();
    agent.add_memory_message(recent).await.unwrap();

    let _ = agent.build_llm_messages("system prompt").await.unwrap();

    let events = recorded.lock().unwrap();
    assert!(
      events
        .iter()
        .any(|e| matches!(e, AgentEvent::MemorySummaryAdded { .. })),
      "mid-run compaction must emit MemorySummaryAdded; got {events:?}"
    );
  }

  #[tokio::test]
  async fn memory_prompt_budget_uses_custom_summary_backend() {
    let backend = Arc::new(RecordingSummaryBackend::default());
    let agent = ReActAgent::new(
      ReActConfig::new("mock-runtime")
        .with_memory_prompt_token_budget(8)
        .with_memory_summary_strategy(MemorySummaryStrategy::Compact),
      Box::new(SessionMemory::default_window()),
      Arc::new(ToolRegistry::new()),
    )
    .with_session_id("summary-session")
    .with_memory_summary_backend(backend.clone());
    let mut older = Message::user("summary-session", "older context");
    older.token_count = 10;
    let mut recent = Message::assistant("summary-session", "recent answer");
    recent.token_count = 4;

    let (summary, kept) = agent
      .apply_memory_prompt_budget(vec![older.clone(), recent.clone()])
      .await
      .unwrap();

    assert_eq!(
      summary.as_deref(),
      Some("[Custom Summary] omitted=1 kept=1")
    );
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].content, recent.content);
    let contexts = backend.contexts.lock().unwrap();
    assert_eq!(contexts.len(), 1);
    assert_eq!(contexts[0].session_id, "summary-session");
    assert_eq!(contexts[0].budget_tokens, 8);
    assert_eq!(contexts[0].omitted_tokens, 10);
    assert_eq!(contexts[0].omitted_messages[0].content, older.content);
  }

  /// F-A2-13: When the LLM returns the same `(tool, params)` two
  /// iterations in a row, the second tool result that lands in the
  /// agent's working memory MUST carry a steering note nudging the
  /// model to advance instead of looping. The tool itself still
  /// runs both times (the steering is advisory, not a hard block),
  /// and the trace-side `AgentStepKind::ToolResult` step keeps the
  /// raw observation unchanged so replay/audit stay faithful.
  #[tokio::test]
  async fn repeat_tool_call_appends_steering_note_to_memory() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-repeat-{}", uuid::Uuid::new_v4());

    // Same action twice, then a final answer. Identical params is
    // the trigger — F-A2-13 must detect it on iteration 2. Env var
    // MUST be set BEFORE init_mock_model so the mock provider reads
    // the queue at registration time.
    unsafe {
      std::env::set_var(
        "AGENTFLOW_MOCK_RESPONSES",
        serde_json::to_string(&vec![
          r#"{"thought":"first try","action":{"tool":"counting_echo","params":{"text":"hi"}}}"#,
          r#"{"thought":"again","action":{"tool":"counting_echo","params":{"text":"hi"}}}"#,
          r#"{"thought":"done","answer":"OK"}"#,
        ])
        .unwrap(),
      );
    }
    init_mock_model(&model).await;

    let calls = Arc::new(AtomicUsize::new(0));
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(CountingTool {
      calls: calls.clone(),
    }));

    let memory_hook = Arc::new(RecordingMemoryHook::default());
    let mut agent = ReActAgent::new(
      ReActConfig::new(&model).with_max_iterations(4),
      Box::new(SessionMemory::default_window()),
      Arc::new(registry),
    )
    .with_memory_hook(memory_hook.clone());

    let result = agent
      .run_with_context(AgentContext::new("session-repeat", "go", &model))
      .await
      .unwrap();

    assert_eq!(result.answer.as_deref(), Some("OK"));
    assert_eq!(result.stop_reason, AgentStopReason::FinalAnswer);
    // Steering is advisory — the tool MUST run both times so a
    // legitimate retry (e.g. polling) isn't broken by F-A2-13.
    assert_eq!(
      calls.load(Ordering::SeqCst),
      2,
      "tool should run both times; steering is a nudge, not a block"
    );

    // Inspect every memory write event the hook saw. The
    // tool-result messages (role=Tool) are the ones that matter
    // for steering; their content is what the model sees on its
    // next turn.
    let events = memory_hook.events.lock().unwrap().clone();
    let tool_result_messages: Vec<Message> = events
      .iter()
      .filter(|c| matches!(c.kind, MemoryHookKind::Write))
      .flat_map(|c| c.messages.iter().cloned())
      .filter(|m| matches!(m.role, Role::Tool))
      .collect();

    assert_eq!(
      tool_result_messages.len(),
      2,
      "expected exactly 2 tool results in memory, got {}",
      tool_result_messages.len()
    );
    assert!(
      !tool_result_messages[0].content.contains("steering note"),
      "first call must NOT carry the steering note: {}",
      tool_result_messages[0].content
    );
    assert!(
      tool_result_messages[1].content.contains("F-A2-13"),
      "second call MUST carry the F-A2-13 steering note: {}",
      tool_result_messages[1].content
    );
    assert!(
      tool_result_messages[1].content.contains("counting_echo"),
      "steering note must name the looping tool: {}",
      tool_result_messages[1].content
    );

    // ToolResult steps (the trace surface) carry the raw
    // observation unchanged — F-A2-13 only touches the memory
    // copy, not the trace.
    let tool_result_steps: Vec<&AgentStepKind> = result
      .steps
      .iter()
      .map(|s| &s.kind)
      .filter(|k| matches!(k, AgentStepKind::ToolResult { .. }))
      .collect();
    assert_eq!(tool_result_steps.len(), 2);
    for step in tool_result_steps {
      if let AgentStepKind::ToolResult { content, .. } = step {
        assert!(
          !content.contains("steering note"),
          "trace-side ToolResult must stay clean of F-A2-13 nudges: {content}"
        );
      }
    }

    unsafe {
      std::env::remove_var("AGENTFLOW_MOCK_RESPONSES");
    }
  }
}

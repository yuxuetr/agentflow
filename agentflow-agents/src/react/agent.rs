use std::sync::Arc;
use std::time::{Duration, Instant};

use agentflow_llm::{AgentFlow, MultimodalMessage};
use agentflow_memory::{MemoryStore, Message, Role};
use agentflow_tools::ToolRegistry;
use async_trait::async_trait;
use chrono::Utc;
use tracing::{debug, info, warn};

use crate::react::parser::AgentResponse;
use crate::reflection::{ReflectionContext, ReflectionStrategy};
use crate::runtime::{
  AgentCancellationToken, AgentContext, AgentEvent, AgentMemoryHook, AgentRunResult, AgentRuntime,
  AgentRuntimeError, AgentStep, AgentStepKind, AgentStopReason, MemoryHookContext, MemoryHookKind,
  RuntimeLimits,
};

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
}

/// Input passed to a pluggable memory summary backend.
#[derive(Debug, Clone)]
pub struct MemorySummaryContext {
  pub session_id: String,
  pub budget_tokens: u32,
  pub omitted_tokens: u32,
  pub omitted_messages: Vec<Message>,
  pub kept_messages: Vec<Message>,
}

/// Pluggable backend for summarizing prompt memory that exceeds a budget.
#[async_trait]
pub trait MemorySummaryBackend: Send + Sync {
  fn name(&self) -> &'static str;

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
/// On each call to [`run`], the agent:
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
}

impl ReActAgent {
  pub fn new(config: ReActConfig, memory: Box<dyn MemoryStore>, tools: Arc<ToolRegistry>) -> Self {
    let session_id = uuid::Uuid::new_v4().to_string();
    Self {
      config,
      memory,
      tools,
      reflection: None,
      memory_hook: None,
      memory_summary_backend: None,
      session_id,
    }
  }

  /// Continue an existing session by reusing a known `session_id`.
  pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
    self.session_id = session_id.into();
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
    prior: AgentRunResult,
  ) -> Result<AgentRunResult, ReActError> {
    if prior.stop_reason.is_success() {
      return Ok(prior);
    }
    if has_unresolved_tool_call(&prior) {
      return Err(ReActError::ToolError {
        tool: "runtime".to_string(),
        message: "cannot resume trace with unresolved tool call; restart requires idempotent tools"
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
    self.apply_context(&context);
    info!(
        session = %self.session_id,
        model = %self.config.model,
        "ReActAgent starting"
    );

    let mut steps = vec![AgentStep::new(
      0,
      AgentStepKind::Observe {
        input: context.input.clone(),
      },
    )];
    let mut events = vec![AgentEvent::RunStarted {
      session_id: self.session_id.clone(),
      model: self.config.model.clone(),
      timestamp: context.started_at,
    }];
    let mut step_index = 1;
    let max_iterations = context
      .limits
      .max_steps
      .unwrap_or(self.config.max_iterations);
    let max_tool_calls = context.limits.max_tool_calls;
    let timeout_ms = context.limits.timeout_ms;
    let budget_tokens = context.limits.token_budget.or(self.config.budget_tokens);
    let cancellation_token = context.cancellation_token.clone();
    let run_started_at = Instant::now();
    let mut tool_calls = 0usize;

    // 1. Store user message
    self
      .add_memory_message(Message::user(&self.session_id, &context.input))
      .await?;

    // 2. Inject system prompt if this is the first user message
    // (We prepend it to the conversation each time we call the LLM)
    let system_prompt = self.build_system_prompt();

    let mut iteration = 0;

    loop {
      if is_cancelled(&cancellation_token) {
        return Ok(Self::cancelled_result(
          &self.session_id,
          "cancellation token signalled",
          steps,
          events,
        ));
      }

      if timed_out(run_started_at, timeout_ms) {
        self
          .record_reflection(
            ReflectionContext::failure(
              &self.session_id,
              step_index,
              format!(
                "runtime timed out after {}ms",
                timeout_ms.unwrap_or_default()
              ),
            ),
            &mut step_index,
            &mut steps,
            &mut events,
          )
          .await?;
        return Ok(Self::stopped_result(
          &self.session_id,
          None,
          AgentStopReason::Timeout {
            timeout_ms: timeout_ms.unwrap_or_default(),
          },
          steps,
          events,
        ));
      }

      // --- Guard: max iterations ---
      if iteration >= max_iterations {
        self
          .record_reflection(
            ReflectionContext::failure(
              &self.session_id,
              step_index,
              format!("max steps ({}) reached", max_iterations),
            ),
            &mut step_index,
            &mut steps,
            &mut events,
          )
          .await?;
        return Ok(Self::stopped_result(
          &self.session_id,
          None,
          AgentStopReason::MaxSteps {
            max_steps: max_iterations,
          },
          steps,
          events,
        ));
      }

      // --- Guard: token budget ---
      if let Some(budget) = budget_tokens {
        let used = self.memory.session_token_count(&self.session_id).await?;
        if used > budget {
          self
            .record_reflection(
              ReflectionContext::failure(
                &self.session_id,
                step_index,
                format!("token budget exceeded: {} / {}", used, budget),
              ),
              &mut step_index,
              &mut steps,
              &mut events,
            )
            .await?;
          return Ok(Self::stopped_result(
            &self.session_id,
            None,
            AgentStopReason::TokenBudgetExceeded { used, budget },
            steps,
            events,
          ));
        }
      }

      // --- Build LLM messages from memory ---
      let messages = self.build_llm_messages(&system_prompt).await?;

      if is_cancelled(&cancellation_token) {
        return Ok(Self::cancelled_result(
          &self.session_id,
          "cancellation token signalled",
          steps,
          events,
        ));
      }

      // --- Call LLM ---
      debug!(iteration, "Calling LLM");
      let llm_call = AgentFlow::model(&self.config.model)
        .multimodal_messages(messages)
        .execute();
      let raw_response = match (
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
                      step_index,
                      format!(
                        "runtime timed out after {}ms",
                        timeout_ms.unwrap_or_default()
                      ),
                    ),
                    &mut step_index,
                    &mut steps,
                    &mut events,
                  )
                  .await?;
                return Ok(Self::stopped_result(
                  &self.session_id,
                  None,
                  AgentStopReason::Timeout {
                    timeout_ms: timeout_ms.unwrap_or_default(),
                  },
                  steps,
                  events,
                ));
              }
            },
            _ = token.cancelled() => {
              return Ok(Self::cancelled_result(
                &self.session_id,
                "cancellation token signalled",
                steps,
                events,
              ));
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
                  step_index,
                  format!(
                    "runtime timed out after {}ms",
                    timeout_ms.unwrap_or_default()
                  ),
                ),
                &mut step_index,
                &mut steps,
                &mut events,
              )
              .await?;
            return Ok(Self::stopped_result(
              &self.session_id,
              None,
              AgentStopReason::Timeout {
                timeout_ms: timeout_ms.unwrap_or_default(),
              },
              steps,
              events,
            ));
          }
        },
        (None, Some(token)) => {
          tokio::select! {
            result = llm_call => result?,
            _ = token.cancelled() => {
              return Ok(Self::cancelled_result(
                &self.session_id,
                "cancellation token signalled",
                steps,
                events,
              ));
            }
          }
        }
        (None, None) => llm_call.await?,
      };

      debug!(response = %raw_response, "LLM responded");

      // --- Check stop conditions ---
      if let Some(condition) = self
        .config
        .stop_conditions
        .iter()
        .find(|cond| raw_response.contains(cond.as_str()))
        .cloned()
      {
        info!("Stop condition matched: '{}'", condition);
        self
          .add_memory_message(Message::assistant(&self.session_id, &raw_response))
          .await?;
        self
          .record_reflection(
            ReflectionContext::final_answer(&self.session_id, step_index, &raw_response),
            &mut step_index,
            &mut steps,
            &mut events,
          )
          .await?;
        return Ok(Self::stopped_result(
          &self.session_id,
          Some(raw_response),
          AgentStopReason::StopCondition { condition },
          steps,
          events,
        ));
      }

      // --- Parse response ---
      let parsed = AgentResponse::parse(&raw_response);

      // Store the assistant turn
      self
        .add_memory_message(Message::assistant(&self.session_id, &raw_response))
        .await?;

      match parsed {
        AgentResponse::Action {
          thought,
          tool,
          params,
        } => {
          info!(iteration, tool = %tool, thought = %thought, "Tool call");
          if let Some(max_tool_calls) = max_tool_calls {
            if tool_calls >= max_tool_calls {
              self
                .record_reflection(
                  ReflectionContext::failure(
                    &self.session_id,
                    step_index,
                    format!("max tool calls ({}) reached", max_tool_calls),
                  ),
                  &mut step_index,
                  &mut steps,
                  &mut events,
                )
                .await?;
              return Ok(Self::stopped_result(
                &self.session_id,
                None,
                AgentStopReason::MaxToolCalls { max_tool_calls },
                steps,
                events,
              ));
            }
          }

          if !thought.trim().is_empty() {
            steps.push(AgentStep::new(
              step_index,
              AgentStepKind::Plan {
                thought: thought.clone(),
              },
            ));
            step_index += 1;
          }

          if is_cancelled(&cancellation_token) {
            return Ok(Self::cancelled_result(
              &self.session_id,
              "cancellation token signalled",
              steps,
              events,
            ));
          }

          let tool_step_index = step_index;
          events.push(AgentEvent::ToolCallStarted {
            session_id: self.session_id.clone(),
            step_index: tool_step_index,
            tool: tool.clone(),
            params: params.clone(),
            timestamp: Utc::now(),
          });
          steps.push(AgentStep::new(
            tool_step_index,
            AgentStepKind::ToolCall {
              tool: tool.clone(),
              params: params.clone(),
            },
          ));
          step_index += 1;

          let started_at = std::time::Instant::now();
          let tool_call = self.tools.execute(&tool, params);
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
                    events.push(AgentEvent::ToolCallCompleted {
                      session_id: self.session_id.clone(),
                      step_index: tool_step_index,
                      tool: tool.clone(),
                      is_error: true,
                      duration_ms,
                      timestamp: Utc::now(),
                    });
                    self
                      .record_reflection(
                        ReflectionContext::failure(
                          &self.session_id,
                          step_index,
                          format!(
                            "runtime timed out after {}ms",
                            timeout_ms.unwrap_or_default()
                          ),
                        ),
                        &mut step_index,
                        &mut steps,
                        &mut events,
                      )
                      .await?;
                    return Ok(Self::stopped_result(
                      &self.session_id,
                      None,
                      AgentStopReason::Timeout {
                        timeout_ms: timeout_ms.unwrap_or_default(),
                      },
                      steps,
                      events,
                    ));
                  }
                },
                _ = token.cancelled() => {
                  events.push(AgentEvent::ToolCallCompleted {
                    session_id: self.session_id.clone(),
                    step_index: tool_step_index,
                    tool: tool.clone(),
                    is_error: true,
                    duration_ms: started_at.elapsed().as_millis() as u64,
                    timestamp: Utc::now(),
                  });
                  return Ok(Self::cancelled_result(
                    &self.session_id,
                    "cancellation token signalled",
                    steps,
                    events,
                  ));
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
                events.push(AgentEvent::ToolCallCompleted {
                  session_id: self.session_id.clone(),
                  step_index: tool_step_index,
                  tool: tool.clone(),
                  is_error: true,
                  duration_ms,
                  timestamp: Utc::now(),
                });
                self
                  .record_reflection(
                    ReflectionContext::failure(
                      &self.session_id,
                      step_index,
                      format!(
                        "runtime timed out after {}ms",
                        timeout_ms.unwrap_or_default()
                      ),
                    ),
                    &mut step_index,
                    &mut steps,
                    &mut events,
                  )
                  .await?;
                return Ok(Self::stopped_result(
                  &self.session_id,
                  None,
                  AgentStopReason::Timeout {
                    timeout_ms: timeout_ms.unwrap_or_default(),
                  },
                  steps,
                  events,
                ));
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
                  events.push(AgentEvent::ToolCallCompleted {
                    session_id: self.session_id.clone(),
                    step_index: tool_step_index,
                    tool: tool.clone(),
                    is_error: true,
                    duration_ms: started_at.elapsed().as_millis() as u64,
                    timestamp: Utc::now(),
                  });
                  return Ok(Self::cancelled_result(
                    &self.session_id,
                    "cancellation token signalled",
                    steps,
                    events,
                  ));
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
          tool_calls += 1;
          let duration_ms = started_at.elapsed().as_millis() as u64;

          let observation = if tool_output.is_error {
            format!("[ERROR] {}", tool_output.content)
          } else {
            tool_output.content.clone()
          };

          info!(tool = %tool, "Observation: {}", &observation[..observation.len().min(200)]);
          steps.push(AgentStep::new(
            step_index,
            AgentStepKind::ToolResult {
              tool: tool.clone(),
              content: tool_output.content.clone(),
              is_error: tool_output.is_error,
              parts: tool_output.parts.clone(),
            },
          ));
          events.push(AgentEvent::ToolCallCompleted {
            session_id: self.session_id.clone(),
            step_index: tool_step_index,
            tool: tool.clone(),
            is_error: tool_output.is_error,
            duration_ms,
            timestamp: Utc::now(),
          });
          step_index += 1;
          if tool_output.is_error {
            self
              .record_reflection(
                ReflectionContext::failure(&self.session_id, step_index, &observation),
                &mut step_index,
                &mut steps,
                &mut events,
              )
              .await?;
          }

          self
            .add_memory_message(Message::tool_result(&self.session_id, &tool, &observation))
            .await?;

          iteration += 1;
        }

        AgentResponse::Answer { thought, answer } => {
          info!(thought = %thought, "Final answer reached");
          if !thought.trim().is_empty() {
            steps.push(AgentStep::new(step_index, AgentStepKind::Plan { thought }));
            step_index += 1;
          }
          steps.push(AgentStep::new(
            step_index,
            AgentStepKind::FinalAnswer {
              answer: answer.clone(),
            },
          ));
          step_index += 1;
          self
            .record_reflection(
              ReflectionContext::final_answer(&self.session_id, step_index, &answer),
              &mut step_index,
              &mut steps,
              &mut events,
            )
            .await?;
          return Ok(Self::stopped_result(
            &self.session_id,
            Some(answer),
            AgentStopReason::FinalAnswer,
            steps,
            events,
          ));
        }

        AgentResponse::Malformed(text) => {
          // Treat unstructured text as a final answer
          warn!("LLM returned non-JSON text; treating as final answer");
          steps.push(AgentStep::new(
            step_index,
            AgentStepKind::FinalAnswer {
              answer: text.clone(),
            },
          ));
          step_index += 1;
          self
            .record_reflection(
              ReflectionContext::final_answer(&self.session_id, step_index, &text),
              &mut step_index,
              &mut steps,
              &mut events,
            )
            .await?;
          return Ok(Self::stopped_result(
            &self.session_id,
            Some(text),
            AgentStopReason::FinalAnswer,
            steps,
            events,
          ));
        }
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

  async fn restore_trace_memory(&mut self, prior: &AgentRunResult) -> Result<(), ReActError> {
    self.memory.clear_session(&self.session_id).await?;
    for step in &prior.steps {
      match &step.kind {
        AgentStepKind::Observe { input } => {
          self
            .add_memory_message(Message::user(&self.session_id, input))
            .await?;
        }
        AgentStepKind::Plan { thought } => {
          self
            .add_memory_message(Message::assistant(&self.session_id, thought))
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
            .add_memory_message(Message::tool_result(&self.session_id, tool, observation))
            .await?;
        }
        AgentStepKind::Reflect { content } => {
          self
            .add_memory_message(Message::assistant(&self.session_id, content))
            .await?;
        }
        AgentStepKind::FinalAnswer { answer } => {
          self
            .add_memory_message(Message::assistant(&self.session_id, answer))
            .await?;
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
      | AgentEvent::ToolCallCompleted { step_index, .. }
      | AgentEvent::ReflectionAdded { step_index, .. } => {
        *step_index += event_offset;
      }
      AgentEvent::StepCompleted { step, .. } => {
        step.index += event_offset;
      }
      AgentEvent::RunStarted { .. } | AgentEvent::RunStopped { .. } => {}
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
  use serde_json::{json, Value};
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
    std::env::set_var(
      "AGENTFLOW_MOCK_RESPONSES",
      serde_json::to_string(&vec![
        r#"{"thought":"use tool","action":{"tool":"echo","params":{"text":"hi"}}}"#,
        r#"{"thought":"done","answer":"final: echo: hi"}"#,
      ])
      .unwrap(),
    );
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
    assert!(result
      .steps
      .iter()
      .any(|step| matches!(step.kind, AgentStepKind::ToolCall { .. })));
    assert!(result
      .steps
      .iter()
      .any(|step| matches!(step.kind, AgentStepKind::ToolResult { .. })));
    assert!(result
      .steps
      .iter()
      .any(|step| matches!(step.kind, AgentStepKind::Reflect { .. })));
    assert!(result
      .events
      .iter()
      .any(|event| matches!(event, AgentEvent::ToolCallCompleted { .. })));
    assert!(result
      .events
      .iter()
      .any(|event| matches!(event, AgentEvent::ReflectionAdded { .. })));

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
  async fn resume_with_context_reuses_recorded_tool_result_without_replay() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-resume-runtime-{}", uuid::Uuid::new_v4());
    std::env::remove_var("AGENTFLOW_MOCK_RESPONSES");
    std::env::set_var(
      "AGENTFLOW_MOCK_RESPONSE",
      r#"{"thought":"use recovered observation","answer":"final: echo: hi"}"#,
    );
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
    let mut memory = SessionMemory::default_window();
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
    assert!(events
      .iter()
      .any(|event| event.kind == MemoryHookKind::Write && event.messages.len() == 1));
    assert!(events
      .iter()
      .any(|event| event.kind == MemoryHookKind::ReadHistory && event.messages.len() == 1));
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
}

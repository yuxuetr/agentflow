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
  AgentContext, AgentEvent, AgentRunResult, AgentRuntime, AgentRuntimeError, AgentStep,
  AgentStepKind, AgentStopReason, RuntimeLimits,
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
}

impl Default for ReActConfig {
  fn default() -> Self {
    Self {
      model: "gpt-4o".to_string(),
      persona: None,
      max_iterations: 15,
      budget_tokens: Some(50_000),
      stop_conditions: vec![],
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

  /// Run the agent on a new user message and return the final answer.
  pub async fn run(&mut self, user_input: &str) -> Result<String, ReActError> {
    let result = self
      .run_with_context(self.context_for_input(user_input))
      .await?;
    Self::answer_from_result(result)
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
    let run_started_at = Instant::now();
    let mut tool_calls = 0usize;

    // 1. Store user message
    self
      .memory
      .add_message(Message::user(&self.session_id, &context.input))
      .await?;

    // 2. Inject system prompt if this is the first user message
    // (We prepend it to the conversation each time we call the LLM)
    let system_prompt = self.build_system_prompt();

    let mut iteration = 0;

    loop {
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

      // --- Call LLM ---
      debug!(iteration, "Calling LLM");
      let llm_call = AgentFlow::model(&self.config.model)
        .multimodal_messages(messages)
        .execute();
      let raw_response = match remaining_timeout(run_started_at, timeout_ms) {
        Some(remaining) => match tokio::time::timeout(remaining, llm_call).await {
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
        None => llm_call.await?,
      };

      debug!(response = %raw_response, "LLM responded");

      // --- Check stop conditions ---
      for cond in &self.config.stop_conditions {
        if raw_response.contains(cond.as_str()) {
          info!("Stop condition matched: '{}'", cond);
          self
            .memory
            .add_message(Message::assistant(&self.session_id, &raw_response))
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
            AgentStopReason::StopCondition {
              condition: cond.clone(),
            },
            steps,
            events,
          ));
        }
      }

      // --- Parse response ---
      let parsed = AgentResponse::parse(&raw_response);

      // Store the assistant turn
      self
        .memory
        .add_message(Message::assistant(&self.session_id, &raw_response))
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
          let tool_output = match remaining_timeout(run_started_at, timeout_ms) {
            Some(remaining) => match tokio::time::timeout(remaining, tool_call).await {
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
            None => match tool_call.await {
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
            .memory
            .add_message(Message::tool_result(&self.session_id, &tool, &observation))
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

  async fn record_reflection(
    &self,
    context: ReflectionContext,
    step_index: &mut usize,
    steps: &mut Vec<AgentStep>,
    events: &mut Vec<AgentEvent>,
  ) -> Result<(), ReActError> {
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
    let history = self.memory.get_all(&self.session_id).await?;

    let mut messages = Vec::with_capacity(history.len() + 1);

    // Always start with the system prompt
    messages.push(MultimodalMessage::system().add_text(system_prompt).build());

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

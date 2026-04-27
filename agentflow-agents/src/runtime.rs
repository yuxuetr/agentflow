use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Notify;

use agentflow_memory::Message;
use agentflow_tools::ToolOutputPart;

/// Runtime limits shared by agent-native execution loops.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeLimits {
  pub max_steps: Option<usize>,
  pub max_tool_calls: Option<usize>,
  pub timeout_ms: Option<u64>,
  pub token_budget: Option<u32>,
}

impl RuntimeLimits {
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentContext {
  pub session_id: String,
  pub input: String,
  pub model: String,
  pub persona: Option<String>,
  pub skill_name: Option<String>,
  pub limits: RuntimeLimits,
  #[serde(default)]
  pub metadata: Value,
  pub started_at: DateTime<Utc>,
  #[serde(skip)]
  pub cancellation_token: Option<AgentCancellationToken>,
}

impl AgentContext {
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
    }
  }

  pub fn with_persona(mut self, persona: impl Into<String>) -> Self {
    self.persona = Some(persona.into());
    self
  }

  pub fn with_skill_name(mut self, skill_name: impl Into<String>) -> Self {
    self.skill_name = Some(skill_name.into());
    self
  }

  pub fn with_limits(mut self, limits: RuntimeLimits) -> Self {
    self.limits = limits;
    self
  }

  pub fn with_cancellation_token(mut self, token: AgentCancellationToken) -> Self {
    self.cancellation_token = Some(token);
    self
  }
}

/// Shared cancellation signal for long-running agent loops.
#[derive(Debug, Clone, Default)]
pub struct AgentCancellationToken {
  cancelled: Arc<AtomicBool>,
  notify: Arc<Notify>,
}

impl AgentCancellationToken {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn cancel(&self) {
    self.cancelled.store(true, Ordering::SeqCst);
    self.notify.notify_waiters();
  }

  pub fn is_cancelled(&self) -> bool {
    self.cancelled.load(Ordering::SeqCst)
  }

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum AgentStopReason {
  FinalAnswer,
  StopCondition { condition: String },
  MaxSteps { max_steps: usize },
  MaxToolCalls { max_tool_calls: usize },
  Timeout { timeout_ms: u64 },
  Cancelled { message: String },
  TokenBudgetExceeded { used: u32, budget: u32 },
  Error { message: String },
}

impl AgentStopReason {
  pub fn is_success(&self) -> bool {
    matches!(self, Self::FinalAnswer | Self::StopCondition { .. })
  }
}

/// One durable step in an agent-native loop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentStep {
  pub index: usize,
  pub kind: AgentStepKind,
  pub timestamp: DateTime<Utc>,
  pub duration_ms: Option<u64>,
}

impl AgentStep {
  pub fn new(index: usize, kind: AgentStepKind) -> Self {
    Self {
      index,
      kind,
      timestamp: Utc::now(),
      duration_ms: None,
    }
  }

  pub fn with_duration_ms(mut self, duration_ms: u64) -> Self {
    self.duration_ms = Some(duration_ms);
    self
  }
}

/// The typed content of an [`AgentStep`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentStepKind {
  Observe {
    input: String,
  },
  Plan {
    thought: String,
  },
  ToolCall {
    tool: String,
    params: Value,
  },
  ToolResult {
    tool: String,
    content: String,
    is_error: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    parts: Vec<ToolOutputPart>,
  },
  Reflect {
    content: String,
  },
  FinalAnswer {
    answer: String,
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
    timestamp: DateTime<Utc>,
  },
  ToolCallCompleted {
    session_id: String,
    step_index: usize,
    tool: String,
    is_error: bool,
    duration_ms: u64,
    timestamp: DateTime<Utc>,
  },
  ReflectionAdded {
    session_id: String,
    step_index: usize,
    timestamp: DateTime<Utc>,
  },
  RunStopped {
    session_id: String,
    reason: AgentStopReason,
    timestamp: DateTime<Utc>,
  },
}

/// Final result returned by an agent runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRunResult {
  pub session_id: String,
  pub answer: Option<String>,
  pub stop_reason: AgentStopReason,
  #[serde(default)]
  pub steps: Vec<AgentStep>,
  #[serde(default)]
  pub events: Vec<AgentEvent>,
}

impl AgentRunResult {
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
  ReadHistory,
  Search,
  Write,
}

/// Context passed to an [`AgentMemoryHook`].
#[derive(Debug, Clone)]
pub struct MemoryHookContext {
  pub session_id: String,
  pub kind: MemoryHookKind,
  pub query: Option<String>,
  pub limit: Option<usize>,
  pub messages: Vec<Message>,
}

/// Optional observer for memory reads and writes inside an agent loop.
///
/// Hooks are intentionally non-failing so memory observability cannot break the
/// main agent run. Implementations can record metrics, build summaries, or feed
/// another memory backend.
pub trait AgentMemoryHook: Send + Sync {
  fn on_memory_read(&self, _context: &MemoryHookContext) {}

  fn on_memory_write(&self, _context: &MemoryHookContext) {}
}

/// Common boundary for agent-native runtimes.
#[async_trait]
pub trait AgentRuntime: Send {
  async fn run(&mut self, context: AgentContext) -> Result<AgentRunResult, AgentRuntimeError>;

  fn runtime_name(&self) -> &'static str;
}

/// Errors raised before a runtime can return a structured stop reason.
#[derive(Debug, thiserror::Error)]
pub enum AgentRuntimeError {
  #[error("Invalid agent runtime context: {message}")]
  InvalidContext { message: String },

  #[error("Agent runtime failed: {message}")]
  ExecutionFailed { message: String },
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
    assert!(AgentStopReason::StopCondition {
      condition: "done".to_string(),
    }
    .is_success());
    assert!(!AgentStopReason::MaxSteps { max_steps: 3 }.is_success());
    assert!(!AgentStopReason::Cancelled {
      message: "cancelled".to_string(),
    }
    .is_success());
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
}

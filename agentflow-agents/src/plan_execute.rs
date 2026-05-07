use std::sync::Arc;
use std::time::{Duration, Instant};

use agentflow_llm::{AgentFlow, LLMResponse, MultimodalMessage, ToolCallRequest, ToolSpec};
use agentflow_memory::{MemoryStore, Message};
use agentflow_tools::{ToolMetadata, ToolRegistry};
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, info, warn};

use crate::runtime::{
  AgentCancellationToken, AgentContext, AgentEvent, AgentMemoryHook, AgentRunResult, AgentRuntime,
  AgentRuntimeError, AgentStep, AgentStepKind, AgentStopReason, MemoryHookContext, MemoryHookKind,
};

/// Error type for Plan-and-Execute agent operations.
#[derive(Debug, thiserror::Error)]
pub enum PlanExecuteError {
  #[error("LLM error: {0}")]
  LlmError(#[from] agentflow_llm::LLMError),

  #[error("Memory error: {0}")]
  MemoryError(#[from] agentflow_memory::MemoryError),

  #[error("Plan parse error: {message}")]
  PlanParse { message: String },

  #[error("Agent run cancelled: {reason}")]
  Cancelled { reason: String },

  #[error("Agent run timed out after {timeout_ms}ms")]
  Timeout { timeout_ms: u64 },
}

/// Configuration for a [`PlanExecuteAgent`].
#[derive(Debug, Clone)]
pub struct PlanExecuteConfig {
  pub model: String,
  pub persona: Option<String>,
  pub max_steps: usize,
}

impl Default for PlanExecuteConfig {
  fn default() -> Self {
    Self {
      model: "gpt-4o".to_string(),
      persona: None,
      max_steps: 8,
    }
  }
}

impl PlanExecuteConfig {
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

  pub fn with_max_steps(mut self, max_steps: usize) -> Self {
    self.max_steps = max_steps;
    self
  }
}

/// One step produced by the planner model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanExecuteStep {
  pub id: String,
  pub description: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub tool: Option<String>,
  #[serde(default)]
  pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct PlanExecutePlan {
  #[serde(default)]
  plan: Vec<PlanExecuteStep>,
  #[serde(default)]
  final_answer: Option<String>,
}

/// A minimal Plan-and-Execute runtime.
///
/// The planner model must return JSON shaped like:
///
/// ```json
/// {
///   "plan": [
///     {"id":"1","description":"Look up data","tool":"search","params":{"q":"..."}}
///   ],
///   "final_answer": "optional answer when no tool is needed"
/// }
/// ```
pub struct PlanExecuteAgent {
  config: PlanExecuteConfig,
  memory: Box<dyn MemoryStore>,
  tools: Arc<ToolRegistry>,
  memory_hook: Option<Arc<dyn AgentMemoryHook>>,
  pub session_id: String,
}

impl PlanExecuteAgent {
  pub fn new(
    config: PlanExecuteConfig,
    memory: Box<dyn MemoryStore>,
    tools: Arc<ToolRegistry>,
  ) -> Self {
    Self {
      config,
      memory,
      tools,
      memory_hook: None,
      session_id: uuid::Uuid::new_v4().to_string(),
    }
  }

  pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
    self.session_id = session_id.into();
    self
  }

  pub fn with_memory_hook(mut self, hook: Arc<dyn AgentMemoryHook>) -> Self {
    self.memory_hook = Some(hook);
    self
  }

  pub async fn run_with_context(
    &mut self,
    context: AgentContext,
  ) -> Result<AgentRunResult, PlanExecuteError> {
    self.apply_context(&context);
    info!(
      session = %self.session_id,
      model = %self.config.model,
      "PlanExecuteAgent starting"
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
    let mut step_index = 1usize;
    let max_steps = context.limits.max_steps.unwrap_or(self.config.max_steps);
    let max_tool_calls = context.limits.max_tool_calls;
    let timeout_ms = context.limits.timeout_ms;
    let cancellation_token = context.cancellation_token.clone();
    let run_started_at = Instant::now();

    self
      .add_memory_message(Message::user(&self.session_id, &context.input))
      .await?;

    if is_cancelled(&cancellation_token) {
      return Ok(self.cancelled_result("cancellation token signalled", steps, events));
    }

    let history = self.read_memory_history(20).await?;
    let planner_response = self
      .call_planner(
        &context.input,
        &history,
        run_started_at,
        timeout_ms,
        cancellation_token.clone(),
        context.trace_context.clone(),
      )
      .await;
    let planner_response = match planner_response {
      Ok(response) => response,
      Err(PlanExecuteError::Timeout { timeout_ms }) => {
        return Ok(self.timeout_result(Some(timeout_ms), steps, events));
      }
      Err(PlanExecuteError::Cancelled { reason }) => {
        return Ok(self.cancelled_result(reason, steps, events));
      }
      Err(err) => return Err(err),
    };
    let planner_text = planner_response.content.clone();
    debug!(response = %planner_text, "PlanExecute planner responded");
    self
      .add_memory_message(Message::assistant(&self.session_id, &planner_text))
      .await?;

    let plan = if !planner_response.tool_calls.is_empty() {
      // Native tool calls drive the plan directly: each call becomes one
      // sequential plan step. Falls back to JSON parsing only when the
      // model emits no tool calls (legacy prompt protocol).
      plan_from_tool_calls(&planner_response.tool_calls)
    } else {
      parse_plan(&planner_text)?
    };
    if plan.plan.len() > max_steps {
      return Ok(self.stopped_result(None, AgentStopReason::MaxSteps { max_steps }, steps, events));
    }

    if !plan.plan.is_empty() {
      let thought = plan
        .plan
        .iter()
        .map(|step| format!("{}. {}", step.id, step.description))
        .collect::<Vec<_>>()
        .join("\n");
      steps.push(AgentStep::new(step_index, AgentStepKind::Plan { thought }));
      step_index += 1;
    }

    let mut observations = Vec::new();
    let mut tool_calls = 0usize;
    for planned_step in plan.plan {
      if is_cancelled(&cancellation_token) {
        return Ok(self.cancelled_result("cancellation token signalled", steps, events));
      }
      if timed_out(run_started_at, timeout_ms) {
        return Ok(self.timeout_result(timeout_ms, steps, events));
      }

      let Some(tool) = planned_step.tool else {
        observations.push(planned_step.description);
        continue;
      };

      if let Some(max_tool_calls) = max_tool_calls
        && tool_calls >= max_tool_calls
      {
        return Ok(self.stopped_result(
          None,
          AgentStopReason::MaxToolCalls { max_tool_calls },
          steps,
          events,
        ));
      }

      let params = planned_step.params;
      let tool_step_index = step_index;
      let metadata = self.tools.tool_metadata(&tool);
      let (tool_source, tool_permissions) = tool_event_metadata(metadata.as_ref());
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
          timestamp: Utc::now(),
        });
      }
      events.push(AgentEvent::ToolCallStarted {
        session_id: self.session_id.clone(),
        step_index: tool_step_index,
        tool: tool.clone(),
        params: params.clone(),
        source: tool_source.clone(),
        permissions: tool_permissions.clone(),
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

      let started_at = Instant::now();
      let output = match self
        .execute_tool(
          &tool,
          params,
          run_started_at,
          timeout_ms,
          cancellation_token.clone(),
        )
        .await
      {
        Ok(output) => output,
        Err(PlanExecuteError::Cancelled { reason }) => {
          events.push(AgentEvent::ToolCallCompleted {
            session_id: self.session_id.clone(),
            step_index: tool_step_index,
            tool: tool.clone(),
            is_error: true,
            duration_ms: started_at.elapsed().as_millis() as u64,
            source: tool_source.clone(),
            permissions: tool_permissions.clone(),
            timestamp: Utc::now(),
          });
          return Ok(self.cancelled_result(reason, steps, events));
        }
        Err(PlanExecuteError::Timeout { timeout_ms }) => {
          events.push(AgentEvent::ToolCallCompleted {
            session_id: self.session_id.clone(),
            step_index: tool_step_index,
            tool: tool.clone(),
            is_error: true,
            duration_ms: started_at.elapsed().as_millis() as u64,
            source: tool_source.clone(),
            permissions: tool_permissions.clone(),
            timestamp: Utc::now(),
          });
          return Ok(self.timeout_result(Some(timeout_ms), steps, events));
        }
        Err(err) => {
          warn!(tool = %tool, error = %err, "PlanExecute tool execution failed");
          agentflow_tools::ToolOutput::error(err.to_string())
        }
      };
      let duration_ms = started_at.elapsed().as_millis() as u64;
      events.push(AgentEvent::ToolCallCompleted {
        session_id: self.session_id.clone(),
        step_index: tool_step_index,
        tool: tool.clone(),
        is_error: output.is_error,
        duration_ms,
        source: tool_source.clone(),
        permissions: tool_permissions.clone(),
        timestamp: Utc::now(),
      });
      steps.push(
        AgentStep::new(
          step_index,
          AgentStepKind::ToolResult {
            tool: tool.clone(),
            content: output.content.clone(),
            is_error: output.is_error,
            parts: output.parts.clone(),
          },
        )
        .with_duration_ms(duration_ms),
      );
      step_index += 1;
      tool_calls += 1;

      self
        .add_memory_message(Message::tool_result(
          &self.session_id,
          &tool,
          &output.content,
        ))
        .await?;
      observations.push(output.content);
    }

    let answer = plan.final_answer.unwrap_or_else(|| {
      if observations.is_empty() {
        "Plan completed with no tool observations.".to_string()
      } else {
        observations.join("\n")
      }
    });
    self
      .add_memory_message(Message::assistant(&self.session_id, &answer))
      .await?;
    steps.push(AgentStep::new(
      step_index,
      AgentStepKind::FinalAnswer {
        answer: answer.clone(),
      },
    ));

    Ok(self.stopped_result(Some(answer), AgentStopReason::FinalAnswer, steps, events))
  }

  async fn call_planner(
    &self,
    input: &str,
    history: &[Message],
    run_started_at: Instant,
    timeout_ms: Option<u64>,
    cancellation_token: Option<AgentCancellationToken>,
    trace_context: Option<agentflow_llm::LlmTraceContext>,
  ) -> Result<LLMResponse, PlanExecuteError> {
    let mut user_prompt = String::new();
    if !history.is_empty() {
      user_prompt.push_str("Conversation history:\n");
      user_prompt.push_str(
        &history
          .iter()
          .map(Message::to_prompt_line)
          .collect::<Vec<_>>()
          .join("\n"),
      );
      user_prompt.push_str("\n\nCurrent task:\n");
    }
    user_prompt.push_str(input);

    let messages = vec![
      MultimodalMessage::text("system", self.system_prompt()),
      MultimodalMessage::text("user", user_prompt),
    ];
    let tool_specs = self.collect_tool_specs();
    let mut builder = AgentFlow::model(&self.config.model)
      .multimodal_messages(messages)
      .trace_context(trace_context);
    if !tool_specs.is_empty() {
      builder = builder.tools(tool_specs);
    }
    let llm_call = builder.execute_full();

    match (
      remaining_timeout(run_started_at, timeout_ms),
      cancellation_token,
    ) {
      (Some(remaining), Some(token)) => {
        tokio::select! {
          result = tokio::time::timeout(remaining, llm_call) => match result {
            Ok(result) => Ok(result?),
            Err(_) => Err(PlanExecuteError::Timeout {
              timeout_ms: timeout_ms.unwrap_or_default(),
            }),
          },
          _ = token.cancelled() => Err(PlanExecuteError::Cancelled {
            reason: "cancellation token signalled".to_string(),
          }),
        }
      }
      (Some(remaining), None) => match tokio::time::timeout(remaining, llm_call).await {
        Ok(result) => Ok(result?),
        Err(_) => Err(PlanExecuteError::Timeout {
          timeout_ms: timeout_ms.unwrap_or_default(),
        }),
      },
      (None, Some(token)) => {
        tokio::select! {
          result = llm_call => Ok(result?),
          _ = token.cancelled() => Err(PlanExecuteError::Cancelled {
            reason: "cancellation token signalled".to_string(),
          }),
        }
      }
      (None, None) => Ok(llm_call.await?),
    }
  }

  async fn execute_tool(
    &self,
    tool: &str,
    params: Value,
    run_started_at: Instant,
    timeout_ms: Option<u64>,
    cancellation_token: Option<AgentCancellationToken>,
  ) -> Result<agentflow_tools::ToolOutput, PlanExecuteError> {
    let tool_call = self.tools.execute(tool, params);
    match (
      remaining_timeout(run_started_at, timeout_ms),
      cancellation_token,
    ) {
      (Some(remaining), Some(token)) => {
        tokio::select! {
          result = tokio::time::timeout(remaining, tool_call) => match result {
            Ok(result) => Ok(result.unwrap_or_else(|err| agentflow_tools::ToolOutput::error(err.to_string()))),
            Err(_) => Err(PlanExecuteError::Timeout {
              timeout_ms: timeout_ms.unwrap_or_default(),
            }),
          },
          _ = token.cancelled() => Err(PlanExecuteError::Cancelled {
            reason: "cancellation token signalled".to_string(),
          }),
        }
      }
      (Some(remaining), None) => match tokio::time::timeout(remaining, tool_call).await {
        Ok(result) => {
          Ok(result.unwrap_or_else(|err| agentflow_tools::ToolOutput::error(err.to_string())))
        }
        Err(_) => Err(PlanExecuteError::Timeout {
          timeout_ms: timeout_ms.unwrap_or_default(),
        }),
      },
      (None, Some(token)) => {
        tokio::select! {
          result = tool_call => Ok(result.unwrap_or_else(|err| agentflow_tools::ToolOutput::error(err.to_string()))),
          _ = token.cancelled() => Err(PlanExecuteError::Cancelled {
            reason: "cancellation token signalled".to_string(),
          }),
        }
      }
      (None, None) => Ok(
        tool_call
          .await
          .unwrap_or_else(|err| agentflow_tools::ToolOutput::error(err.to_string())),
      ),
    }
  }

  fn apply_context(&mut self, context: &AgentContext) {
    self.session_id = context.session_id.clone();
    self.config.model = context.model.clone();
    if let Some(persona) = &context.persona {
      self.config.persona = Some(persona.clone());
    }
  }

  fn system_prompt(&self) -> String {
    let mut prompt = String::from(
      "You are a Plan-and-Execute agent. Return only JSON with keys `plan` and optional `final_answer`. Each plan item must include `id`, `description`, optional `tool`, and optional `params`. Use only available tools.\n\nAvailable tools:\n",
    );
    prompt.push_str(&self.tools.prompt_tools_description());
    if let Some(persona) = &self.config.persona {
      prompt.push_str("\n\nPersona:\n");
      prompt.push_str(persona);
    }
    prompt
  }

  /// Build a `Vec<ToolSpec>` from the registered tools so it can be passed
  /// to the planner LLM as a native `tools` field. Returns an empty vector
  /// when no tools are registered, leaving the LLM call unchanged.
  fn collect_tool_specs(&self) -> Vec<ToolSpec> {
    self
      .tools
      .list()
      .into_iter()
      .map(|tool| ToolSpec::new(tool.name(), tool.description(), tool.parameters_schema()))
      .collect()
  }

  async fn add_memory_message(&mut self, message: Message) -> Result<(), PlanExecuteError> {
    let context = MemoryHookContext {
      session_id: self.session_id.clone(),
      kind: MemoryHookKind::Write,
      query: None,
      limit: None,
      messages: vec![message.clone()],
    };
    self.memory.add_message(message).await?;
    if let Some(hook) = &self.memory_hook {
      hook.on_memory_write(&context);
    }
    Ok(())
  }

  async fn read_memory_history(&self, limit: usize) -> Result<Vec<Message>, PlanExecuteError> {
    let messages = self.memory.get_history(&self.session_id, limit).await?;
    if let Some(hook) = &self.memory_hook {
      hook.on_memory_read(&MemoryHookContext {
        session_id: self.session_id.clone(),
        kind: MemoryHookKind::ReadHistory,
        query: None,
        limit: Some(limit),
        messages: messages.clone(),
      });
    }
    Ok(messages)
  }

  fn stopped_result(
    &self,
    answer: Option<String>,
    stop_reason: AgentStopReason,
    steps: Vec<AgentStep>,
    mut events: Vec<AgentEvent>,
  ) -> AgentRunResult {
    events.push(AgentEvent::RunStopped {
      session_id: self.session_id.clone(),
      reason: stop_reason.clone(),
      timestamp: Utc::now(),
    });
    AgentRunResult {
      session_id: self.session_id.clone(),
      answer,
      stop_reason,
      steps,
      events,
    }
  }

  fn cancelled_result(
    &self,
    reason: impl Into<String>,
    steps: Vec<AgentStep>,
    events: Vec<AgentEvent>,
  ) -> AgentRunResult {
    self.stopped_result(
      None,
      AgentStopReason::Cancelled {
        message: reason.into(),
      },
      steps,
      events,
    )
  }

  fn timeout_result(
    &self,
    timeout_ms: Option<u64>,
    steps: Vec<AgentStep>,
    events: Vec<AgentEvent>,
  ) -> AgentRunResult {
    self.stopped_result(
      None,
      AgentStopReason::Timeout {
        timeout_ms: timeout_ms.unwrap_or_default(),
      },
      steps,
      events,
    )
  }
}

#[async_trait]
impl AgentRuntime for PlanExecuteAgent {
  async fn run(&mut self, context: AgentContext) -> Result<AgentRunResult, AgentRuntimeError> {
    self
      .run_with_context(context)
      .await
      .map_err(|err| AgentRuntimeError::ExecutionFailed {
        message: err.to_string(),
      })
  }

  fn runtime_name(&self) -> &'static str {
    "plan_execute"
  }
}

/// Convert a list of native tool calls into a `PlanExecutePlan`. Each call
/// becomes a sequential step with empty `description` and the tool's
/// arguments as `params`. Used when the planner emits provider-native tool
/// calls instead of a JSON plan envelope.
fn plan_from_tool_calls(calls: &[ToolCallRequest]) -> PlanExecutePlan {
  let plan = calls
    .iter()
    .enumerate()
    .map(|(idx, call)| PlanExecuteStep {
      id: format!("{}", idx + 1),
      description: String::new(),
      tool: Some(call.name.clone()),
      params: call.arguments.clone(),
    })
    .collect();
  PlanExecutePlan {
    plan,
    final_answer: None,
  }
}

fn parse_plan(raw: &str) -> Result<PlanExecutePlan, PlanExecuteError> {
  serde_json::from_str(raw).map_err(|err| PlanExecuteError::PlanParse {
    message: err.to_string(),
  })
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

#[cfg(test)]
mod tests {
  use super::*;
  use agentflow_memory::SessionMemory;
  use agentflow_tools::{Tool, ToolError, ToolOutput};
  use serde_json::json;

  struct EchoTool;

  #[async_trait]
  impl Tool for EchoTool {
    fn name(&self) -> &str {
      "echo"
    }

    fn description(&self) -> &str {
      "Echo text"
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

  #[test]
  fn parse_plan_accepts_tool_steps() {
    let plan = parse_plan(
      r#"{"plan":[{"id":"1","description":"echo it","tool":"echo","params":{"text":"hi"}}]}"#,
    )
    .unwrap();

    assert_eq!(plan.plan.len(), 1);
    assert_eq!(plan.plan[0].tool.as_deref(), Some("echo"));
  }

  #[tokio::test]
  async fn run_executes_planned_tool_and_returns_trace() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    init_mock_model(
      "mock-plan-execute-test",
      r#"{"plan":[{"id":"1","description":"echo input","tool":"echo","params":{"text":"hi"}}]}"#,
    )
    .await;

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(EchoTool));
    let mut agent = PlanExecuteAgent::new(
      PlanExecuteConfig::new("mock-plan-execute-test"),
      Box::new(SessionMemory::default_window()),
      Arc::new(registry),
    );

    let result = agent
      .run_with_context(AgentContext::new(
        "plan-execute-session",
        "say hi",
        "mock-plan-execute-test",
      ))
      .await
      .unwrap();

    assert_eq!(result.answer.as_deref(), Some("echo: hi"));
    assert_eq!(result.stop_reason, AgentStopReason::FinalAnswer);
    assert!(
      result
        .steps
        .iter()
        .any(|step| matches!(step.kind, AgentStepKind::ToolCall { .. }))
    );
    assert!(
      result
        .events
        .iter()
        .any(|event| matches!(event, AgentEvent::ToolCallCompleted { .. }))
    );
  }

  #[tokio::test]
  async fn run_consumes_native_tool_calls_when_available() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = "mock-plan-execute-native";
    // SAFETY: LLM_TEST_LOCK serializes mutation of process-wide mock env vars.
    //
    // Drives Plan-Execute through the native tool-calling path. The text
    // content is unparseable JSON, so a successful run proves the plan
    // came from `tool_calls` rather than `parse_plan`.
    unsafe {
      std::env::set_var(
        "AGENTFLOW_MOCK_TOOL_CALLS",
        serde_json::to_string(&vec![vec![serde_json::json!({
          "id": "call_0",
          "name": "echo",
          "arguments": {"text": "hi"}
        })]])
        .unwrap(),
      );
    }
    init_mock_model(model, "(unused — native tool call)").await;

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(EchoTool));
    let mut agent = PlanExecuteAgent::new(
      PlanExecuteConfig::new(model),
      Box::new(SessionMemory::default_window()),
      Arc::new(registry),
    );

    let result = agent
      .run_with_context(AgentContext::new("plan-execute-native", "say hi", model))
      .await
      .unwrap();

    assert_eq!(result.answer.as_deref(), Some("echo: hi"));
    assert_eq!(result.stop_reason, AgentStopReason::FinalAnswer);
    assert!(
      result
        .steps
        .iter()
        .any(|step| matches!(step.kind, AgentStepKind::ToolCall { .. }))
    );

    // SAFETY: cleanup of the dedicated mock env var after the test read.
    unsafe {
      std::env::remove_var("AGENTFLOW_MOCK_TOOL_CALLS");
    }
  }

  #[test]
  fn plan_from_tool_calls_maps_each_call_to_a_step() {
    let calls = vec![
      ToolCallRequest {
        id: "call_0".into(),
        name: "echo".into(),
        arguments: serde_json::json!({"text": "a"}),
      },
      ToolCallRequest {
        id: "call_1".into(),
        name: "shell".into(),
        arguments: serde_json::json!({"command": "ls"}),
      },
    ];
    let plan = plan_from_tool_calls(&calls);
    assert_eq!(plan.plan.len(), 2);
    assert_eq!(plan.plan[0].tool.as_deref(), Some("echo"));
    assert_eq!(plan.plan[0].params["text"], "a");
    assert_eq!(plan.plan[1].tool.as_deref(), Some("shell"));
    assert!(plan.final_answer.is_none());
  }

  #[tokio::test]
  async fn run_returns_cancelled_when_token_already_signalled() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    init_mock_model(
      "mock-plan-execute-test",
      r#"{"plan":[{"id":"1","description":"echo input","tool":"echo","params":{"text":"hi"}}]}"#,
    )
    .await;

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(EchoTool));
    let mut agent = PlanExecuteAgent::new(
      PlanExecuteConfig::new("mock-plan-execute-test"),
      Box::new(SessionMemory::default_window()),
      Arc::new(registry),
    );
    let token = AgentCancellationToken::new();
    token.cancel();

    let result = agent
      .run_with_context(
        AgentContext::new("plan-cancelled", "say hi", "mock-plan-execute-test")
          .with_cancellation_token(token),
      )
      .await
      .unwrap();

    assert!(matches!(
      result.stop_reason,
      AgentStopReason::Cancelled { .. }
    ));
  }

  async fn init_mock_model(model: &str, response: &str) {
    // SAFETY: tests serialize LLM config/env mutation with LLM_TEST_LOCK.
    unsafe {
      std::env::set_var("AGENTFLOW_MOCK_RESPONSE", response);
    }

    let config_path = std::env::temp_dir().join(format!(
      "agentflow-plan-execute-{}.yml",
      uuid::Uuid::new_v4()
    ));
    std::fs::write(
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
}

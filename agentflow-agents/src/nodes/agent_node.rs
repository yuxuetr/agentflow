//! `AgentNode` — wraps a [`ReActAgent`] as an [`AsyncNode`] so that
//! autonomous agents can be embedded directly in a DAG workflow.
//!
//! # Input keys
//! | Key       | Type                 | Required |
//! |-----------|----------------------|----------|
//! | `message` | `FlowValue::Json(String)` | yes  |
//!
//! # Output keys
//! | Key          | Type                    |
//! |--------------|-------------------------|
//! | `response`   | `FlowValue::Json(String)` |
//! | `session_id` | `FlowValue::Json(String)` |
//! | `stop_reason` | `FlowValue::Json(Object)` |
//! | `agent_result` | `FlowValue::Json(Object)` |
//! | `agent_resume` | `FlowValue::Json(Object)` |

use std::collections::HashMap;
use std::sync::Arc;

use agentflow_core::{
  async_node::{AsyncNodeInputs, AsyncNodeResult},
  error::AgentFlowError,
  value::FlowValue,
  AsyncNode,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::react::agent::ReActAgent;
use crate::runtime::{AgentContext, AgentRunResult, AgentStepKind, AgentStopReason};

const AGENT_RESUME_CONTRACT_VERSION: u32 = 1;

/// How an [`AgentNode`] output can be used during workflow resume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentNodeResumeMode {
  /// The run reached a successful terminal state and can be reused from
  /// checkpointed outputs without executing the agent again.
  CompletedRun,
  /// The runtime emitted durable steps, but this node cannot safely continue a
  /// partial agent loop yet.
  PartialRunUnsupported,
  /// The runtime emitted durable steps and can continue from recorded
  /// observations without replaying completed tool calls.
  PartialRunSupported,
  /// The node must start a new agent run. Any tool calls must be safe to repeat.
  RestartRequired,
}

/// Replay policy for a tool call recorded in an agent runtime trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentNodeToolReplayPolicy {
  /// A result was recorded; resume should reuse that observation instead of
  /// calling the tool again.
  ReuseRecordedResult,
  /// No result was recorded; restarting requires the tool to be idempotent.
  RequiresIdempotentRetry,
}

/// Tool call information extracted from the runtime trace for resume review.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentNodeToolResumeRecord {
  pub step_index: usize,
  pub tool: String,
  pub params: Value,
  pub result_step_index: Option<usize>,
  pub result_is_error: Option<bool>,
  pub replay_policy: AgentNodeToolReplayPolicy,
}

/// Stable resume contract emitted by [`AgentNode`].
///
/// The contract is intentionally explicit about what is and is not resumable.
/// Current workflow checkpointing already skips a completed `AgentNode`; this
/// structure makes the embedded agent state inspectable and defines the future
/// boundary for partial agent-loop resume.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentNodeResumeContract {
  pub version: u32,
  pub node_name: String,
  pub runtime_name: String,
  pub session_id: String,
  pub resume_mode: AgentNodeResumeMode,
  pub completed: bool,
  pub stop_reason: AgentStopReason,
  pub step_count: usize,
  pub last_step_index: Option<usize>,
  pub tool_calls: Vec<AgentNodeToolResumeRecord>,
  pub completed_run_replay_safe: bool,
  pub partial_run_resume_supported: bool,
  pub restart_requires_idempotent_tools: bool,
}

impl AgentNodeResumeContract {
  pub fn from_result(
    node_name: impl Into<String>,
    runtime_name: impl Into<String>,
    result: &AgentRunResult,
  ) -> Self {
    let completed = result.stop_reason.is_success();
    let partial_run_resume_supported =
      !completed && !result.steps.is_empty() && !has_unresolved_tool_call(result);
    let resume_mode = if completed {
      AgentNodeResumeMode::CompletedRun
    } else if partial_run_resume_supported {
      AgentNodeResumeMode::PartialRunSupported
    } else if result.steps.is_empty() {
      AgentNodeResumeMode::RestartRequired
    } else {
      AgentNodeResumeMode::PartialRunUnsupported
    };

    Self {
      version: AGENT_RESUME_CONTRACT_VERSION,
      node_name: node_name.into(),
      runtime_name: runtime_name.into(),
      session_id: result.session_id.clone(),
      resume_mode,
      completed,
      stop_reason: result.stop_reason.clone(),
      step_count: result.steps.len(),
      last_step_index: result.steps.last().map(|step| step.index),
      tool_calls: extract_tool_resume_records(result),
      completed_run_replay_safe: completed,
      partial_run_resume_supported,
      restart_requires_idempotent_tools: !completed && has_unresolved_tool_call(result),
    }
  }
}

// ── Public struct ─────────────────────────────────────────────────────────────

/// An [`AsyncNode`] that delegates execution to a [`ReActAgent`].
///
/// The inner agent is wrapped in `Arc<Mutex<…>>` so that:
/// - `AgentNode` satisfies the `&self` signature of `AsyncNode::execute`.
/// - The same inner agent can optionally be shared with an [`AgentTool`](crate::tools::AgentTool).
///
/// # Example
/// ```rust,no_run
/// use agentflow_agents::nodes::AgentNode;
/// use agentflow_agents::react::{ReActAgent, ReActConfig};
/// use agentflow_memory::SessionMemory;
/// use agentflow_tools::ToolRegistry;
/// use std::sync::Arc;
///
/// let agent = ReActAgent::new(
///     ReActConfig::new("gpt-4o"),
///     Box::new(SessionMemory::default_window()),
///     Arc::new(ToolRegistry::new()),
/// );
/// let node = AgentNode::from_agent("my_agent", agent);
/// ```
pub struct AgentNode {
  /// Logical name for this node (appears in workflow logs).
  pub name: String,
  agent: Arc<Mutex<ReActAgent>>,
}

impl AgentNode {
  /// Construct from an existing [`ReActAgent`].
  pub fn from_agent(name: impl Into<String>, agent: ReActAgent) -> Self {
    Self {
      name: name.into(),
      agent: Arc::new(Mutex::new(agent)),
    }
  }

  /// Return a cloned handle to the inner agent lock so it can be shared
  /// with an [`AgentTool`](crate::tools::AgentTool).
  pub fn agent_handle(&self) -> Arc<Mutex<ReActAgent>> {
    self.agent.clone()
  }
}

// ── AsyncNode implementation ──────────────────────────────────────────────────

#[async_trait]
impl AsyncNode for AgentNode {
  /// Execute the agent on the `"message"` input and return `"response"`.
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    // ── Extract "message" ─────────────────────────────────────────────
    let message = match inputs.get("message") {
      Some(FlowValue::Json(v)) => match v.as_str() {
        Some(s) => s.to_string(),
        None => {
          return Err(AgentFlowError::NodeInputError {
            message: format!("AgentNode '{}': 'message' must be a JSON string", self.name),
          });
        }
      },
      Some(other) => {
        return Err(AgentFlowError::NodeInputError {
          message: format!(
            "AgentNode '{}': 'message' must be FlowValue::Json(string), got {:?}",
            self.name, other
          ),
        });
      }
      None => {
        return Err(AgentFlowError::NodeInputError {
          message: format!(
            "AgentNode '{}': required input 'message' is missing",
            self.name
          ),
        });
      }
    };

    // ── Run agent ────────────────────────────────────────────────────
    let mut agent = self.agent.lock().await;
    let prior_result = parse_prior_agent_result(inputs)?;
    let result = if let Some(prior) = prior_result {
      let context = AgentContext::new(&prior.session_id, &message, "");
      agent
        .resume_with_context(context, prior)
        .await
        .map_err(|e| AgentFlowError::NodeExecutionFailed {
          message: format!("AgentNode '{}': {}", self.name, e),
        })?
    } else {
      agent
        .run_with_trace(&message)
        .await
        .map_err(|e| AgentFlowError::NodeExecutionFailed {
          message: format!("AgentNode '{}': {}", self.name, e),
        })?
    };
    if !result.stop_reason.is_success() {
      let partial_outputs = build_outputs(&self.name, &result)?;
      return Err(AgentFlowError::NodePartialExecutionFailed {
        message: format!(
          "AgentNode '{}': agent stopped before final answer: {:?}",
          self.name, result.stop_reason
        ),
        partial_outputs,
      });
    }
    build_outputs(&self.name, &result)
  }
}

fn parse_prior_agent_result(
  inputs: &AsyncNodeInputs,
) -> Result<Option<AgentRunResult>, AgentFlowError> {
  let Some(value) = inputs.get("agent_result") else {
    return Ok(None);
  };
  let FlowValue::Json(value) = value else {
    return Err(AgentFlowError::NodeInputError {
      message: "'agent_result' must be FlowValue::Json(object)".to_string(),
    });
  };
  serde_json::from_value(value.clone())
    .map(Some)
    .map_err(|e| AgentFlowError::NodeInputError {
      message: format!("failed to deserialize 'agent_result': {}", e),
    })
}

fn build_outputs(node_name: &str, result: &AgentRunResult) -> AsyncNodeResult {
  let response = result.answer.clone().unwrap_or_default();
  let stop_reason =
    serde_json::to_value(&result.stop_reason).map_err(|e| AgentFlowError::NodeExecutionFailed {
      message: format!(
        "AgentNode '{}': failed to serialize stop reason: {}",
        node_name, e
      ),
    })?;
  let agent_result =
    serde_json::to_value(result).map_err(|e| AgentFlowError::NodeExecutionFailed {
      message: format!(
        "AgentNode '{}': failed to serialize runtime result: {}",
        node_name, e
      ),
    })?;
  let agent_resume = serde_json::to_value(AgentNodeResumeContract::from_result(
    node_name, "react", result,
  ))
  .map_err(|e| AgentFlowError::NodeExecutionFailed {
    message: format!(
      "AgentNode '{}': failed to serialize resume contract: {}",
      node_name, e
    ),
  })?;

  let mut outputs = HashMap::new();
  outputs.insert("response".to_string(), FlowValue::Json(json!(response)));
  outputs.insert(
    "session_id".to_string(),
    FlowValue::Json(json!(result.session_id)),
  );
  outputs.insert("stop_reason".to_string(), FlowValue::Json(stop_reason));
  outputs.insert("agent_result".to_string(), FlowValue::Json(agent_result));
  outputs.insert("agent_resume".to_string(), FlowValue::Json(agent_resume));
  Ok(outputs)
}

fn extract_tool_resume_records(result: &AgentRunResult) -> Vec<AgentNodeToolResumeRecord> {
  let mut records = Vec::new();
  for step in &result.steps {
    let AgentStepKind::ToolCall { tool, params } = &step.kind else {
      continue;
    };
    let result_step = result.steps.iter().find(|candidate| {
      matches!(
        &candidate.kind,
        AgentStepKind::ToolResult {
          tool: result_tool,
          ..
        } if result_tool == tool && candidate.index > step.index
      )
    });
    let result_is_error = result_step.and_then(|candidate| {
      if let AgentStepKind::ToolResult { is_error, .. } = candidate.kind {
        Some(is_error)
      } else {
        None
      }
    });

    records.push(AgentNodeToolResumeRecord {
      step_index: step.index,
      tool: tool.clone(),
      params: params.clone(),
      result_step_index: result_step.map(|step| step.index),
      result_is_error,
      replay_policy: if result_step.is_some() {
        AgentNodeToolReplayPolicy::ReuseRecordedResult
      } else {
        AgentNodeToolReplayPolicy::RequiresIdempotentRetry
      },
    });
  }
  records
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
  use super::*;
  use agentflow_core::value::FlowValue;
  use agentflow_memory::SessionMemory;
  use agentflow_tools::ToolRegistry;
  use serde_json::json;

  use crate::react::{ReActAgent, ReActConfig};
  use crate::runtime::{AgentRunResult, AgentStep, AgentStepKind, AgentStopReason};

  fn make_agent() -> ReActAgent {
    ReActAgent::new(
      ReActConfig::new("gpt-4o"),
      Box::new(SessionMemory::default_window()),
      Arc::new(ToolRegistry::new()),
    )
  }

  // ── Construction ──────────────────────────────────────────────────────────

  #[test]
  fn from_agent_sets_name() {
    let node = AgentNode::from_agent("my-node", make_agent());
    assert_eq!(node.name, "my-node");
  }

  #[test]
  fn agent_handle_returns_same_arc() {
    let node = AgentNode::from_agent("shared", make_agent());
    let h1 = node.agent_handle();
    let h2 = node.agent_handle();
    // Both Arc pointers point to the same allocation
    assert!(Arc::ptr_eq(&h1, &h2));
  }

  // ── execute() input validation ────────────────────────────────────────────

  #[tokio::test]
  async fn execute_missing_message_returns_error() {
    let node = AgentNode::from_agent("test", make_agent());
    let inputs = HashMap::new(); // empty
    let err = node.execute(&inputs).await.unwrap_err();
    match err {
      AgentFlowError::NodeInputError { message } => {
        assert!(
          message.contains("'message'"),
          "error should mention 'message', got: {message}"
        );
      }
      other => panic!("expected NodeInputError, got {:?}", other),
    }
  }

  #[tokio::test]
  async fn execute_non_string_message_returns_error() {
    let node = AgentNode::from_agent("test", make_agent());
    let mut inputs = HashMap::new();
    inputs.insert("message".to_string(), FlowValue::Json(json!(42)));
    let err = node.execute(&inputs).await.unwrap_err();
    assert!(
      matches!(err, AgentFlowError::NodeInputError { .. }),
      "expected NodeInputError"
    );
  }

  #[tokio::test]
  async fn execute_file_value_returns_error() {
    let node = AgentNode::from_agent("test", make_agent());
    let mut inputs = HashMap::new();
    inputs.insert(
      "message".to_string(),
      FlowValue::File {
        path: std::path::PathBuf::from("/tmp/x"),
        mime_type: None,
      },
    );
    let err = node.execute(&inputs).await.unwrap_err();
    assert!(
      matches!(err, AgentFlowError::NodeInputError { .. }),
      "expected NodeInputError for File value"
    );
  }

  // ── execute() output shape (no LLM — we can only test the error path) ─────
  //
  // Real LLM calls are not made in unit tests.  The integration path is
  // exercised by the workflow integration tests in agentflow-cli.

  #[tokio::test]
  async fn execute_propagates_agent_error_as_execution_error() {
    // An agent with an empty model name will fail when it tries to call the
    // LLM.  We just verify the error variant is correct.
    let agent = ReActAgent::new(
      ReActConfig::new(""), // empty model → LLM call will fail
      Box::new(SessionMemory::default_window()),
      Arc::new(ToolRegistry::new()),
    );
    let node = AgentNode::from_agent("failing", agent);
    let mut inputs = HashMap::new();
    inputs.insert("message".to_string(), FlowValue::Json(json!("hello")));
    let result = node.execute(&inputs).await;
    // We expect either NodeExecutionFailed (LLM failure) or some other error
    // from the LLM stack — either way it must be Err.
    assert!(result.is_err(), "expected error when model name is empty");
  }

  #[test]
  fn resume_contract_marks_completed_run_as_checkpoint_reusable() {
    let result = AgentRunResult {
      session_id: "session-1".to_string(),
      answer: Some("done".to_string()),
      stop_reason: AgentStopReason::FinalAnswer,
      steps: vec![
        AgentStep::new(
          0,
          AgentStepKind::Observe {
            input: "hello".to_string(),
          },
        ),
        AgentStep::new(
          1,
          AgentStepKind::ToolCall {
            tool: "echo".to_string(),
            params: json!({"text": "hi"}),
          },
        ),
        AgentStep::new(
          2,
          AgentStepKind::ToolResult {
            tool: "echo".to_string(),
            content: "echo: hi".to_string(),
            is_error: false,
            parts: vec![],
          },
        ),
        AgentStep::new(
          3,
          AgentStepKind::FinalAnswer {
            answer: "done".to_string(),
          },
        ),
      ],
      events: vec![],
    };

    let contract = AgentNodeResumeContract::from_result("agent", "react", &result);

    assert_eq!(contract.version, 1);
    assert_eq!(contract.resume_mode, AgentNodeResumeMode::CompletedRun);
    assert!(contract.completed_run_replay_safe);
    assert!(!contract.partial_run_resume_supported);
    assert_eq!(contract.tool_calls.len(), 1);
    assert_eq!(
      contract.tool_calls[0].replay_policy,
      AgentNodeToolReplayPolicy::ReuseRecordedResult
    );
  }

  #[test]
  fn resume_contract_requires_idempotent_tools_for_partial_restart() {
    let result = AgentRunResult {
      session_id: "session-1".to_string(),
      answer: None,
      stop_reason: AgentStopReason::Cancelled {
        message: "shutdown".to_string(),
      },
      steps: vec![AgentStep::new(
        1,
        AgentStepKind::ToolCall {
          tool: "write_file".to_string(),
          params: json!({"path": "/tmp/out"}),
        },
      )],
      events: vec![],
    };

    let contract = AgentNodeResumeContract::from_result("agent", "react", &result);

    assert_eq!(
      contract.resume_mode,
      AgentNodeResumeMode::PartialRunUnsupported
    );
    assert!(contract.restart_requires_idempotent_tools);
    assert_eq!(
      contract.tool_calls[0].replay_policy,
      AgentNodeToolReplayPolicy::RequiresIdempotentRetry
    );
  }

  #[test]
  fn resume_contract_supports_partial_resume_after_recorded_tool_result() {
    let result = AgentRunResult {
      session_id: "session-1".to_string(),
      answer: None,
      stop_reason: AgentStopReason::Cancelled {
        message: "shutdown".to_string(),
      },
      steps: vec![
        AgentStep::new(
          1,
          AgentStepKind::ToolCall {
            tool: "echo".to_string(),
            params: json!({"text": "hi"}),
          },
        ),
        AgentStep::new(
          2,
          AgentStepKind::ToolResult {
            tool: "echo".to_string(),
            content: "echo: hi".to_string(),
            is_error: false,
            parts: vec![],
          },
        ),
      ],
      events: vec![],
    };

    let contract = AgentNodeResumeContract::from_result("agent", "react", &result);

    assert_eq!(
      contract.resume_mode,
      AgentNodeResumeMode::PartialRunSupported
    );
    assert!(contract.partial_run_resume_supported);
    assert!(!contract.restart_requires_idempotent_tools);
    assert_eq!(
      contract.tool_calls[0].replay_policy,
      AgentNodeToolReplayPolicy::ReuseRecordedResult
    );
  }
}

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

use std::collections::HashMap;
use std::sync::Arc;

use agentflow_core::{
  async_node::{AsyncNodeInputs, AsyncNodeResult},
  error::AgentFlowError,
  value::FlowValue,
  AsyncNode,
};
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::Mutex;

use crate::react::agent::ReActAgent;

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
    let session_id = agent.session_id.clone();

    let result =
      agent
        .run_with_trace(&message)
        .await
        .map_err(|e| AgentFlowError::NodeExecutionFailed {
          message: format!("AgentNode '{}': {}", self.name, e),
        })?;
    if !result.stop_reason.is_success() {
      return Err(AgentFlowError::NodeExecutionFailed {
        message: format!(
          "AgentNode '{}': agent stopped before final answer: {:?}",
          self.name, result.stop_reason
        ),
      });
    }
    let response = result.answer.clone().unwrap_or_default();
    let stop_reason = serde_json::to_value(&result.stop_reason).map_err(|e| {
      AgentFlowError::NodeExecutionFailed {
        message: format!(
          "AgentNode '{}': failed to serialize stop reason: {}",
          self.name, e
        ),
      }
    })?;
    let agent_result =
      serde_json::to_value(&result).map_err(|e| AgentFlowError::NodeExecutionFailed {
        message: format!(
          "AgentNode '{}': failed to serialize runtime result: {}",
          self.name, e
        ),
      })?;

    // ── Build outputs ─────────────────────────────────────────────────
    let mut outputs = HashMap::new();
    outputs.insert("response".to_string(), FlowValue::Json(json!(response)));
    outputs.insert("session_id".to_string(), FlowValue::Json(json!(session_id)));
    outputs.insert("stop_reason".to_string(), FlowValue::Json(stop_reason));
    outputs.insert("agent_result".to_string(), FlowValue::Json(agent_result));
    Ok(outputs)
  }
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
}

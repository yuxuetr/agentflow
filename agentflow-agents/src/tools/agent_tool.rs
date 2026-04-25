//! `AgentTool` — wraps a [`ReActAgent`] as a [`Tool`] so that one agent can
//! delegate sub-tasks to another agent via the normal tool-calling mechanism.
//!
//! # Parameters schema (what the LLM passes)
//! ```json
//! { "message": "<task description for the sub-agent>" }
//! ```
//!
//! # Usage
//! Register `AgentTool` in a parent agent's [`ToolRegistry`] just like any
//! other tool.  The parent LLM will call it by name with a `message` argument,
//! and the sub-agent's final answer is returned as the tool output.

use std::sync::Arc;

use agentflow_tools::{Tool, ToolError, ToolOutput};
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::react::agent::ReActAgent;

// ── Public struct ─────────────────────────────────────────────────────────────

/// A [`Tool`] that delegates execution to a [`ReActAgent`].
///
/// ```rust,no_run
/// use agentflow_agents::tools::AgentTool;
/// use agentflow_agents::react::{ReActAgent, ReActConfig};
/// use agentflow_memory::SessionMemory;
/// use agentflow_tools::{ToolRegistry, SandboxPolicy};
/// use std::sync::Arc;
///
/// let sub = ReActAgent::new(
///     ReActConfig::new("gpt-4o"),
///     Box::new(SessionMemory::default_window()),
///     Arc::new(ToolRegistry::new()),
/// );
///
/// let mut registry = ToolRegistry::new();
/// registry.register(Arc::new(AgentTool::new("researcher", "Search for facts", sub)));
/// ```
pub struct AgentTool {
  tool_name: String,
  tool_description: String,
  agent: Arc<Mutex<ReActAgent>>,
}

impl AgentTool {
  /// Create a new `AgentTool`.
  ///
  /// * `name` — the tool name the LLM uses to invoke this agent.
  /// * `description` — human-readable description shown in the system prompt.
  /// * `agent` — the [`ReActAgent`] that handles the delegated task.
  pub fn new(name: impl Into<String>, description: impl Into<String>, agent: ReActAgent) -> Self {
    Self {
      tool_name: name.into(),
      tool_description: description.into(),
      agent: Arc::new(Mutex::new(agent)),
    }
  }

  /// Construct from a shared `Arc<Mutex<ReActAgent>>` — useful when the same
  /// agent instance is also used as an [`AgentNode`](crate::nodes::AgentNode).
  pub fn from_shared(
    name: impl Into<String>,
    description: impl Into<String>,
    agent: Arc<Mutex<ReActAgent>>,
  ) -> Self {
    Self {
      tool_name: name.into(),
      tool_description: description.into(),
      agent,
    }
  }

  /// Return a cloned handle to the inner agent lock.
  pub fn agent_handle(&self) -> Arc<Mutex<ReActAgent>> {
    self.agent.clone()
  }
}

// ── Tool implementation ───────────────────────────────────────────────────────

#[async_trait]
impl Tool for AgentTool {
  fn name(&self) -> &str {
    &self.tool_name
  }

  fn description(&self) -> &str {
    &self.tool_description
  }

  fn parameters_schema(&self) -> Value {
    json!({
        "type": "object",
        "properties": {
            "message": {
                "type": "string",
                "description": "The task or question to send to the sub-agent."
            }
        },
        "required": ["message"]
    })
  }

  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
    let message = params["message"]
      .as_str()
      .ok_or_else(|| ToolError::InvalidParams {
        message: format!(
          "AgentTool '{}': 'message' parameter must be a string",
          self.tool_name
        ),
      })?
      .to_string();

    let mut agent = self.agent.lock().await;
    match agent.run(&message).await {
      Ok(answer) => Ok(ToolOutput::success(answer)),
      Err(e) => Ok(ToolOutput::error(e.to_string())),
    }
  }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
  use super::*;
  use agentflow_memory::SessionMemory;
  use agentflow_tools::ToolRegistry;

  use crate::react::{ReActAgent, ReActConfig};

  fn make_agent_tool(name: &str, desc: &str) -> AgentTool {
    let agent = ReActAgent::new(
      ReActConfig::new("gpt-4o"),
      Box::new(SessionMemory::default_window()),
      Arc::new(ToolRegistry::new()),
    );
    AgentTool::new(name, desc, agent)
  }

  // ── Trait accessors ───────────────────────────────────────────────────────

  #[test]
  fn name_matches_constructor() {
    let t = make_agent_tool("researcher", "Finds information");
    assert_eq!(t.name(), "researcher");
  }

  #[test]
  fn description_matches_constructor() {
    let t = make_agent_tool("researcher", "Finds information");
    assert_eq!(t.description(), "Finds information");
  }

  #[test]
  fn parameters_schema_has_message_field() {
    let t = make_agent_tool("any", "any");
    let schema = t.parameters_schema();
    assert_eq!(schema["type"], "object");
    assert!(
      schema["properties"]["message"].is_object(),
      "schema must have a 'message' property"
    );
    let required = schema["required"]
      .as_array()
      .expect("required must be array");
    assert!(
      required.iter().any(|v| v.as_str() == Some("message")),
      "'message' must be in required array"
    );
  }

  #[test]
  fn from_shared_returns_same_arc() {
    let agent = ReActAgent::new(
      ReActConfig::new("gpt-4o"),
      Box::new(SessionMemory::default_window()),
      Arc::new(ToolRegistry::new()),
    );
    let shared = Arc::new(Mutex::new(agent));
    let t = AgentTool::from_shared("a", "b", shared.clone());
    assert!(Arc::ptr_eq(&t.agent_handle(), &shared));
  }

  // ── execute() — missing message parameter ────────────────────────────────

  #[tokio::test]
  async fn execute_without_message_returns_invalid_input_error() {
    let t = make_agent_tool("test", "test");
    let result = t.execute(json!({})).await;
    assert!(
      matches!(result, Err(ToolError::InvalidParams { .. })),
      "missing 'message' should return InvalidParams"
    );
  }

  #[tokio::test]
  async fn execute_with_null_message_returns_invalid_input_error() {
    let t = make_agent_tool("test", "test");
    let result = t.execute(json!({"message": null})).await;
    assert!(matches!(result, Err(ToolError::InvalidParams { .. })));
  }

  // ── execute() — LLM failures are soft errors ──────────────────────────────
  //
  // When the sub-agent returns ReActError (e.g. LLM unreachable), we convert
  // it to ToolOutput::error (not Err(ToolError)) so the orchestrator can
  // continue reasoning.

  #[tokio::test]
  async fn execute_agent_failure_returns_tool_output_error_not_tool_error() {
    let agent = ReActAgent::new(
      ReActConfig::new(""), // empty model → LLM call will fail
      Box::new(SessionMemory::default_window()),
      Arc::new(ToolRegistry::new()),
    );
    let t = AgentTool::new("failing", "will fail", agent);
    let result = t.execute(json!({"message": "hello"})).await;
    // Must be Ok(ToolOutput { is_error: true, … }), NOT Err(ToolError)
    let output = result.expect("should be Ok(ToolOutput::error)");
    assert!(
      output.is_error,
      "sub-agent failure should produce ToolOutput::error"
    );
  }

  // ── prompt_description contains name and description ─────────────────────

  #[test]
  fn prompt_description_includes_name_and_description() {
    let t = make_agent_tool("code-reviewer", "Reviews Rust code for correctness");
    let desc = t.prompt_description();
    assert!(desc.contains("code-reviewer"));
    assert!(desc.contains("Reviews Rust code for correctness"));
  }
}

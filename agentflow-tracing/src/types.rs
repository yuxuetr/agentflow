//! Core data structures for workflow execution tracing
//!
//! This module defines the structure of execution traces, which capture
//! detailed information about workflow execution including node inputs/outputs,
//! LLM interactions, and execution metrics.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Complete execution trace for a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionTrace {
  /// Unique workflow execution ID
  pub workflow_id: String,

  /// Correlation context for this workflow run.
  #[serde(default)]
  pub context: TraceContext,

  /// Optional workflow name for easier identification
  pub workflow_name: Option<String>,

  /// When the workflow started
  pub started_at: DateTime<Utc>,

  /// When the workflow completed (if finished)
  pub completed_at: Option<DateTime<Utc>>,

  /// Current execution status
  pub status: TraceStatus,

  /// Traces for each node executed
  pub nodes: Vec<NodeTrace>,

  /// Additional metadata
  pub metadata: TraceMetadata,
}

impl ExecutionTrace {
  /// Create a new execution trace
  pub fn new(workflow_id: String) -> Self {
    Self {
      context: TraceContext::workflow(workflow_id.clone()),
      workflow_id,
      workflow_name: None,
      started_at: Utc::now(),
      completed_at: None,
      status: TraceStatus::Running,
      nodes: Vec::new(),
      metadata: TraceMetadata::default(),
    }
  }

  /// Calculate total execution duration
  pub fn duration(&self) -> Option<Duration> {
    self
      .completed_at
      .map(|completed| (completed - self.started_at).to_std().unwrap_or_default())
  }

  /// Get duration in milliseconds
  pub fn duration_ms(&self) -> Option<u64> {
    self.duration().map(|d| d.as_millis() as u64)
  }

  /// Check if workflow is still running
  pub fn is_running(&self) -> bool {
    matches!(self.status, TraceStatus::Running)
  }

  /// Check if workflow completed successfully
  pub fn is_completed(&self) -> bool {
    matches!(self.status, TraceStatus::Completed)
  }

  /// Check if workflow failed
  pub fn is_failed(&self) -> bool {
    matches!(self.status, TraceStatus::Failed { .. })
  }
}

/// Correlation identifiers that link workflow, node, agent, tool, MCP, and LLM
/// records in a single persisted trace.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceContext {
  pub run_id: String,
  pub trace_id: String,
  pub span_id: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub parent_span_id: Option<String>,
}

impl TraceContext {
  pub fn workflow(run_id: String) -> Self {
    Self {
      trace_id: run_id.clone(),
      run_id,
      span_id: "workflow".to_string(),
      parent_span_id: None,
    }
  }

  pub fn child(parent: &Self, span_id: impl Into<String>) -> Self {
    Self {
      run_id: parent.run_id.clone(),
      trace_id: parent.trace_id.clone(),
      span_id: span_id.into(),
      parent_span_id: Some(parent.span_id.clone()),
    }
  }
}

impl Default for TraceContext {
  fn default() -> Self {
    Self {
      run_id: String::new(),
      trace_id: String::new(),
      span_id: String::new(),
      parent_span_id: None,
    }
  }
}

/// Execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TraceStatus {
  /// Workflow is currently running
  Running,

  /// Workflow completed successfully
  Completed,

  /// Workflow failed with error
  Failed {
    /// Error message
    error: String,
  },
}

/// Trace for a single node execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeTrace {
  /// Node identifier
  pub node_id: String,

  /// Correlation context for this node span.
  #[serde(default)]
  pub context: TraceContext,

  /// Node type (e.g., "LLMNode", "HttpNode")
  pub node_type: String,

  /// When the node started executing
  pub started_at: DateTime<Utc>,

  /// When the node completed (if finished)
  pub completed_at: Option<DateTime<Utc>>,

  /// Execution duration in milliseconds
  pub duration_ms: Option<u64>,

  /// Node execution status
  pub status: NodeStatus,

  /// Input data to the node
  #[serde(skip_serializing_if = "Option::is_none")]
  pub input: Option<serde_json::Value>,

  /// Output data from the node
  #[serde(skip_serializing_if = "Option::is_none")]
  pub output: Option<serde_json::Value>,

  /// LLM-specific details (if this is an LLM node)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub llm_details: Option<LLMTrace>,

  /// Agent runtime details (if this node executed an agent)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub agent_details: Option<AgentTrace>,

  /// Error message (if failed)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error: Option<String>,
}

impl NodeTrace {
  /// Create a new node trace
  pub fn new(node_id: String, node_type: String) -> Self {
    Self {
      context: TraceContext {
        span_id: format!("node:{node_id}"),
        parent_span_id: Some("workflow".to_string()),
        ..TraceContext::default()
      },
      node_id,
      node_type,
      started_at: Utc::now(),
      completed_at: None,
      duration_ms: None,
      status: NodeStatus::Running,
      input: None,
      output: None,
      llm_details: None,
      agent_details: None,
      error: None,
    }
  }

  /// Mark node as completed
  pub fn complete(&mut self) {
    self.completed_at = Some(Utc::now());
    self.duration_ms = Some(
      (self.completed_at.unwrap() - self.started_at)
        .to_std()
        .unwrap_or_default()
        .as_millis() as u64,
    );
    self.status = NodeStatus::Completed;
  }

  /// Mark node as failed
  pub fn fail(&mut self, error: String) {
    self.completed_at = Some(Utc::now());
    self.duration_ms = Some(
      (self.completed_at.unwrap() - self.started_at)
        .to_std()
        .unwrap_or_default()
        .as_millis() as u64,
    );
    self.status = NodeStatus::Failed;
    self.error = Some(error);
  }
}

/// Agent runtime details attached to an agent-capable workflow node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTrace {
  #[serde(default)]
  pub context: TraceContext,
  pub session_id: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub answer: Option<String>,
  pub stop_reason: serde_json::Value,
  #[serde(default)]
  pub steps: Vec<serde_json::Value>,
  #[serde(default)]
  pub events: Vec<serde_json::Value>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub tool_calls: Vec<ToolCallTrace>,
}

/// Tool call observed inside an agent runtime trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallTrace {
  #[serde(default)]
  pub context: TraceContext,
  pub tool: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub params: Option<serde_json::Value>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub is_error: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub duration_ms: Option<u64>,
  pub is_mcp: bool,
}

impl AgentTrace {
  pub fn from_agent_result(value: &serde_json::Value) -> Option<Self> {
    let session_id = value.get("session_id")?.as_str()?.to_string();
    let answer = value
      .get("answer")
      .and_then(|value| value.as_str())
      .map(ToString::to_string);
    let stop_reason = value.get("stop_reason").cloned().unwrap_or_default();
    let steps = value
      .get("steps")
      .and_then(|value| value.as_array())
      .cloned()
      .unwrap_or_default();
    let events = value
      .get("events")
      .and_then(|value| value.as_array())
      .cloned()
      .unwrap_or_default();
    let tool_calls = collect_tool_calls(&steps, &events);

    Some(Self {
      context: TraceContext::default(),
      session_id,
      answer,
      stop_reason,
      steps,
      events,
      tool_calls,
    })
  }

  pub fn attach_context(&mut self, parent: &TraceContext) {
    self.context = TraceContext::child(
      parent,
      format!("agent:{}", sanitize_span_component(&self.session_id)),
    );
    for (index, tool_call) in self.tool_calls.iter_mut().enumerate() {
      tool_call.context = TraceContext::child(
        &self.context,
        format!(
          "tool:{}:{}",
          index,
          sanitize_span_component(&tool_call.tool)
        ),
      );
    }
  }
}

fn sanitize_span_component(value: &str) -> String {
  value
    .chars()
    .map(|ch| {
      if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':') {
        ch
      } else {
        '_'
      }
    })
    .collect()
}

fn collect_tool_calls(
  steps: &[serde_json::Value],
  events: &[serde_json::Value],
) -> Vec<ToolCallTrace> {
  let mut calls = Vec::new();
  for step in steps {
    let Some(kind) = step.get("kind") else {
      continue;
    };
    if kind.get("type").and_then(|value| value.as_str()) != Some("tool_call") {
      continue;
    }
    let Some(tool) = kind.get("tool").and_then(|value| value.as_str()) else {
      continue;
    };
    calls.push(ToolCallTrace {
      context: TraceContext::default(),
      tool: tool.to_string(),
      params: kind.get("params").cloned(),
      is_error: None,
      duration_ms: None,
      is_mcp: tool.starts_with("mcp_"),
    });
  }

  for event in events {
    if event.get("event").and_then(|value| value.as_str()) != Some("tool_call_completed") {
      continue;
    }
    let Some(tool) = event.get("tool").and_then(|value| value.as_str()) else {
      continue;
    };
    if let Some(call) = calls.iter_mut().rev().find(|call| call.tool == tool) {
      call.is_error = event.get("is_error").and_then(|value| value.as_bool());
      call.duration_ms = event.get("duration_ms").and_then(|value| value.as_u64());
    }
  }

  calls
}

/// Node execution status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
  /// Node is currently running
  Running,

  /// Node completed successfully
  Completed,

  /// Node execution failed
  Failed,

  /// Node was skipped (e.g., due to condition)
  Skipped,
}

/// LLM execution details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMTrace {
  /// Model used (e.g., "gpt-4", "claude-3-opus")
  pub model: String,

  /// Provider name (e.g., "openai", "anthropic")
  pub provider: String,

  /// System prompt (if any)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub system_prompt: Option<String>,

  /// User prompt
  pub user_prompt: String,

  /// Model response
  pub response: String,

  /// Temperature parameter
  #[serde(skip_serializing_if = "Option::is_none")]
  pub temperature: Option<f32>,

  /// Max tokens parameter
  #[serde(skip_serializing_if = "Option::is_none")]
  pub max_tokens: Option<u32>,

  /// Token usage statistics
  #[serde(skip_serializing_if = "Option::is_none")]
  pub usage: Option<TokenUsage>,

  /// API call latency in milliseconds
  pub latency_ms: u64,
}

/// Token usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
  /// Tokens in the prompt
  pub prompt_tokens: u32,

  /// Tokens in the completion
  pub completion_tokens: u32,

  /// Total tokens used
  pub total_tokens: u32,

  /// Estimated cost in USD (if available)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub estimated_cost_usd: Option<f64>,
}

impl TokenUsage {
  /// Create token usage from counts
  pub fn new(prompt_tokens: u32, completion_tokens: u32) -> Self {
    Self {
      prompt_tokens,
      completion_tokens,
      total_tokens: prompt_tokens + completion_tokens,
      estimated_cost_usd: None,
    }
  }

  /// Add cost estimation
  pub fn with_cost(mut self, cost_usd: f64) -> Self {
    self.estimated_cost_usd = Some(cost_usd);
    self
  }
}

/// Trace metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceMetadata {
  /// User ID who executed the workflow
  #[serde(skip_serializing_if = "Option::is_none")]
  pub user_id: Option<String>,

  /// Session ID
  #[serde(skip_serializing_if = "Option::is_none")]
  pub session_id: Option<String>,

  /// Tags for categorization
  #[serde(default)]
  pub tags: Vec<String>,

  /// Environment (e.g., "production", "development")
  pub environment: String,
}

impl Default for TraceMetadata {
  fn default() -> Self {
    Self {
      user_id: None,
      session_id: None,
      tags: Vec::new(),
      environment: "development".to_string(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_execution_trace_creation() {
    let trace = ExecutionTrace::new("wf-123".to_string());
    assert_eq!(trace.workflow_id, "wf-123");
    assert!(trace.is_running());
    assert!(!trace.is_completed());
    assert!(!trace.is_failed());
  }

  #[test]
  fn test_node_trace_lifecycle() {
    let mut node = NodeTrace::new("node1".to_string(), "LLMNode".to_string());
    assert_eq!(node.status, NodeStatus::Running);

    node.complete();
    assert_eq!(node.status, NodeStatus::Completed);
    assert!(node.duration_ms.is_some());
  }

  #[test]
  fn test_node_trace_failure() {
    let mut node = NodeTrace::new("node1".to_string(), "HttpNode".to_string());

    node.fail("Connection timeout".to_string());
    assert_eq!(node.status, NodeStatus::Failed);
    assert_eq!(node.error.as_deref(), Some("Connection timeout"));
  }

  #[test]
  fn test_token_usage() {
    let usage = TokenUsage::new(100, 50).with_cost(0.005);
    assert_eq!(usage.total_tokens, 150);
    assert_eq!(usage.estimated_cost_usd, Some(0.005));
  }

  #[test]
  fn test_trace_serialization() {
    let trace = ExecutionTrace::new("wf-test".to_string());
    let json = serde_json::to_string(&trace).expect("Failed to serialize");
    let deserialized: ExecutionTrace = serde_json::from_str(&json).expect("Failed to deserialize");
    assert_eq!(deserialized.workflow_id, "wf-test");
    assert_eq!(deserialized.context.run_id, "wf-test");
    assert_eq!(deserialized.context.span_id, "workflow");
  }

  #[test]
  fn test_agent_trace_context_links_tool_calls() {
    let mut agent = AgentTrace::from_agent_result(&serde_json::json!({
      "session_id": "session-1",
      "answer": "done",
      "stop_reason": {"reason": "final_answer"},
      "steps": [
        {
          "index": 0,
          "kind": {
            "type": "tool_call",
            "tool": "mcp_fixture_echo",
            "params": {"message": "hello"}
          }
        }
      ],
      "events": []
    }))
    .expect("agent trace");
    let workflow = TraceContext::workflow("run-1".to_string());
    let node = TraceContext::child(&workflow, "node:agent_node");

    agent.attach_context(&node);

    assert_eq!(agent.context.run_id, "run-1");
    assert_eq!(
      agent.context.parent_span_id.as_deref(),
      Some("node:agent_node")
    );
    assert_eq!(
      agent.tool_calls[0].context.parent_span_id.as_deref(),
      Some("agent:session-1")
    );
    assert_eq!(
      agent.tool_calls[0].context.span_id,
      "tool:0:mcp_fixture_echo"
    );
  }
}

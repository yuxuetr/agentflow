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

  /// Error message (if failed)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error: Option<String>,
}

impl NodeTrace {
  /// Create a new node trace
  pub fn new(node_id: String, node_type: String) -> Self {
    Self {
      node_id,
      node_type,
      started_at: Utc::now(),
      completed_at: None,
      duration_ms: None,
      status: NodeStatus::Running,
      input: None,
      output: None,
      llm_details: None,
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
  }
}

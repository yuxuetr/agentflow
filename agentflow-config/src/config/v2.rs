use serde::Deserialize;
use std::collections::HashMap;

/// Defines the structure of a V2 workflow YAML file.
#[derive(Debug, Deserialize)]
pub struct FlowDefinitionV2 {
  pub name: String,
  #[serde(default)]
  #[allow(dead_code)]
  pub inputs: HashMap<String, InputDefinitionV2>,
  pub nodes: Vec<NodeDefinitionV2>,
}

/// Defines a required input for the workflow.
#[derive(Debug, Deserialize)]
pub struct InputDefinitionV2 {
  #[allow(dead_code)]
  pub description: Option<String>,
  #[allow(dead_code)]
  pub required: bool,
  #[allow(dead_code)]
  pub default: Option<serde_yaml::Value>,
}

/// Defines a single node in the V2 workflow graph.
#[derive(Debug, Deserialize)]
pub struct NodeDefinitionV2 {
  pub id: String,
  #[serde(rename = "type")]
  pub node_type: String,
  #[serde(default)]
  pub dependencies: Vec<String>,
  #[serde(default)]
  pub input_mapping: HashMap<String, String>,
  #[serde(default)]
  pub run_if: Option<String>,
  /// T3.1: generic, node-type-agnostic outer execution timeout, in
  /// milliseconds. Sibling of `run_if`, not nested inside `parameters` —
  /// applies uniformly to any node type (see
  /// `executor::timeout_retry::TimeoutRetryNode`), unlike the
  /// node-specific `mcp` node's `parameters.timeout_ms` (which governs its
  /// MCP-client connection, a different and unrelated setting).
  #[serde(default)]
  pub timeout_ms: Option<u64>,
  /// T3.1: generic, node-type-agnostic retry count on transient
  /// (network/timeout/rate-limit-class) failures. See `timeout_ms`'s doc
  /// comment for the same `parameters`-vs-top-level distinction.
  #[serde(default)]
  pub max_retries: Option<u32>,
  #[serde(default)]
  pub parameters: HashMap<String, serde_yaml::Value>,
}

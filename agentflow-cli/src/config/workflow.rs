// Comprehensive workflow configuration structures
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowConfig {
  pub name: String,
  pub version: String,
  pub description: Option<String>,
  pub author: Option<String>,
  pub metadata: Option<WorkflowMetadata>,
  pub config: Option<WorkflowExecutionConfig>,
  pub inputs: Option<HashMap<String, InputDefinition>>,
  pub environment: Option<HashMap<String, String>>,
  pub workflow: WorkflowDefinition,
  pub outputs: Option<HashMap<String, OutputDefinition>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowMetadata {
  pub created: Option<String>,
  pub tags: Option<Vec<String>>,
  pub category: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowExecutionConfig {
  pub timeout: Option<String>,
  pub max_retries: Option<u32>,
  pub output_format: Option<String>,
  pub log_level: Option<String>,
  pub parallel_limit: Option<usize>,
  pub batch_size: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InputDefinition {
  #[serde(rename = "type")]
  pub input_type: String,
  pub required: Option<bool>,
  pub default: Option<Value>,
  pub description: Option<String>,
  pub example: Option<Value>,
  #[serde(rename = "enum")]
  pub enum_values: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowDefinition {
  #[serde(rename = "type")]
  pub workflow_type: WorkflowType,
  pub nodes: Vec<NodeDefinition>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowType {
  Sequential,
  Parallel,
  Conditional,
  Mixed,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodeDefinition {
  pub name: String,
  #[serde(rename = "type")]
  pub node_type: NodeType,
  pub description: Option<String>,
  pub config: NodeConfig,
  pub depends_on: Option<Vec<String>>,
  pub outputs: Option<HashMap<String, String>>,
  pub conditions: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeType {
  Llm,
  Template,
  File,
  Http,
  Batch,
  Conditional,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum NodeConfig {
  Llm(LlmNodeConfig),
  Template(TemplateNodeConfig),
  File(FileNodeConfig),
  Http(HttpNodeConfig),
  Batch(BatchNodeConfig),
  Conditional(ConditionalNodeConfig),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmNodeConfig {
  pub model: String,
  pub prompt: String,
  pub system: Option<String>,
  pub temperature: Option<Value>, // Allow both numbers and template strings
  pub max_tokens: Option<Value>,  // Allow both numbers and template strings
  pub top_p: Option<f32>,
  pub frequency_penalty: Option<f32>,
  pub stop: Option<Vec<String>>,
  pub timeout: Option<String>,
  pub stream: Option<bool>,
  pub input_files: Option<Vec<String>>,
  pub response_format: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TemplateNodeConfig {
  pub template: String,
  pub variables: Option<HashMap<String, Value>>,
  pub format: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileNodeConfig {
  pub operation: FileOperation,
  pub path: String,
  pub content: Option<String>,
  pub format: Option<String>,
  pub encoding: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileOperation {
  Read,
  Write,
  Append,
  Delete,
  Copy,
  Move,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpNodeConfig {
  pub method: HttpMethod,
  pub url: String,
  pub headers: Option<HashMap<String, String>>,
  pub body: Option<Value>,
  pub timeout: Option<String>,
  pub retry_count: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
  Get,
  Post,
  Put,
  Delete,
  Patch,
  Head,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BatchNodeConfig {
  pub items_source: String,
  pub batch_size: usize,
  pub parallel_limit: Option<usize>,
  pub processor: Box<NodeConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConditionalNodeConfig {
  pub condition: String,
  pub if_true: String,
  pub if_false: Option<String>,
  pub default_action: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OutputDefinition {
  pub source: String,
  pub format: Option<String>,
  pub file: Option<String>,
  pub include: Option<Vec<String>>,
  pub exclude: Option<Vec<String>>,
}

//! OpenTelemetry export primitives for AgentFlow execution traces.
//!
//! The exporter maps AgentFlow's internal [`ExecutionTrace`] model into a
//! stable span representation that follows OpenTelemetry naming and attribute
//! conventions. Transport-specific OTLP HTTP/gRPC clients can implement
//! [`OtelSpanSink`] without changing trace collection.

use crate::types::{
  AgentTrace, ExecutionTrace, LLMTrace, NodeStatus, NodeTrace, ToolCallTrace, TraceStatus,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Exporter configuration shared by all OpenTelemetry sinks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtelExporterConfig {
  pub service_name: String,
  pub service_version: Option<String>,
  pub environment: Option<String>,
}

impl OtelExporterConfig {
  pub fn new(service_name: impl Into<String>) -> Self {
    Self {
      service_name: service_name.into(),
      service_version: None,
      environment: None,
    }
  }

  pub fn with_service_version(mut self, service_version: impl Into<String>) -> Self {
    self.service_version = Some(service_version.into());
    self
  }

  pub fn with_environment(mut self, environment: impl Into<String>) -> Self {
    self.environment = Some(environment.into());
    self
  }
}

impl Default for OtelExporterConfig {
  fn default() -> Self {
    Self::new("agentflow")
  }
}

/// Minimal span sink boundary for OTLP transport implementations.
#[async_trait]
pub trait OtelSpanSink: Send + Sync {
  async fn export_spans(&self, spans: Vec<OtelSpan>) -> Result<(), anyhow::Error>;
}

/// Export boundary for completed AgentFlow execution traces.
#[async_trait]
pub trait TraceExporter: Send + Sync {
  async fn export_trace(&self, trace: &ExecutionTrace) -> Result<(), anyhow::Error>;
}

/// OpenTelemetry exporter that converts AgentFlow traces and forwards spans.
pub struct OtelTraceExporter<S> {
  config: OtelExporterConfig,
  sink: S,
}

impl<S> OtelTraceExporter<S>
where
  S: OtelSpanSink,
{
  pub fn new(config: OtelExporterConfig, sink: S) -> Self {
    Self { config, sink }
  }

  pub fn spans_for_trace(&self, trace: &ExecutionTrace) -> Vec<OtelSpan> {
    trace_to_spans(trace, &self.config)
  }

  pub async fn export_trace_spans(&self, trace: &ExecutionTrace) -> Result<(), anyhow::Error> {
    self.sink.export_spans(self.spans_for_trace(trace)).await
  }
}

#[async_trait]
impl<S> TraceExporter for OtelTraceExporter<S>
where
  S: OtelSpanSink,
{
  async fn export_trace(&self, trace: &ExecutionTrace) -> Result<(), anyhow::Error> {
    self.export_trace_spans(trace).await
  }
}

/// Span representation used at the AgentFlow/OpenTelemetry boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OtelSpan {
  pub trace_id: String,
  pub span_id: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub parent_span_id: Option<String>,
  pub name: String,
  pub kind: OtelSpanKind,
  pub start_time_unix_nano: u64,
  pub end_time_unix_nano: u64,
  pub attributes: Vec<OtelAttribute>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub events: Vec<OtelSpanEvent>,
  pub status: OtelStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OtelSpanKind {
  Internal,
  Client,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OtelAttribute {
  pub key: String,
  pub value: OtelValue,
}

impl OtelAttribute {
  pub fn string(key: impl Into<String>, value: impl Into<String>) -> Self {
    Self {
      key: key.into(),
      value: OtelValue::String(value.into()),
    }
  }

  pub fn bool(key: impl Into<String>, value: bool) -> Self {
    Self {
      key: key.into(),
      value: OtelValue::Bool(value),
    }
  }

  pub fn i64(key: impl Into<String>, value: i64) -> Self {
    Self {
      key: key.into(),
      value: OtelValue::I64(value),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum OtelValue {
  String(String),
  Bool(bool),
  I64(i64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OtelSpanEvent {
  pub name: String,
  pub time_unix_nano: u64,
  pub attributes: Vec<OtelAttribute>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OtelStatus {
  pub code: OtelStatusCode,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub message: Option<String>,
}

impl OtelStatus {
  pub fn ok() -> Self {
    Self {
      code: OtelStatusCode::Ok,
      message: None,
    }
  }

  pub fn error(message: impl Into<String>) -> Self {
    Self {
      code: OtelStatusCode::Error,
      message: Some(message.into()),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OtelStatusCode {
  Ok,
  Error,
  Unset,
}

pub fn trace_to_spans(trace: &ExecutionTrace, config: &OtelExporterConfig) -> Vec<OtelSpan> {
  let trace_id = trace_id(&trace.workflow_id);
  let workflow_span_id = span_id(&trace.workflow_id, "workflow");
  let workflow_start = unix_nanos(trace.started_at);
  let workflow_end = trace.completed_at.map(unix_nanos).unwrap_or(workflow_start);

  let mut spans = vec![OtelSpan {
    trace_id: trace_id.clone(),
    span_id: workflow_span_id.clone(),
    parent_span_id: None,
    name: format!("agentflow.workflow {}", trace.workflow_id),
    kind: OtelSpanKind::Internal,
    start_time_unix_nano: workflow_start,
    end_time_unix_nano: workflow_end,
    attributes: workflow_attributes(trace, config),
    events: Vec::new(),
    status: status_from_trace(&trace.status),
  }];

  for node in &trace.nodes {
    let node_span_id = span_id(&trace.workflow_id, &format!("node:{}", node.node_id));
    spans.push(node_span(
      &trace_id,
      &workflow_span_id,
      &node_span_id,
      trace,
      node,
    ));

    if let Some(llm) = &node.llm_details {
      spans.push(llm_span(&trace_id, &node_span_id, trace, node, llm));
    }

    if let Some(agent) = &node.agent_details {
      let agent_span_id = span_id(
        &trace.workflow_id,
        &format!("agent:{}:{}", node.node_id, agent.session_id),
      );
      spans.push(agent_span(
        &trace_id,
        &node_span_id,
        &agent_span_id,
        node,
        agent,
      ));

      for (index, tool_call) in agent.tool_calls.iter().enumerate() {
        spans.push(tool_span(
          &trace_id,
          &agent_span_id,
          trace,
          node,
          tool_call,
          index,
        ));
      }
    }
  }

  spans
}

fn workflow_attributes(trace: &ExecutionTrace, config: &OtelExporterConfig) -> Vec<OtelAttribute> {
  let mut attrs = vec![
    OtelAttribute::string("service.name", &config.service_name),
    OtelAttribute::string("agentflow.workflow.id", &trace.workflow_id),
    OtelAttribute::string("agentflow.trace.status", trace_status_name(&trace.status)),
    OtelAttribute::i64("agentflow.workflow.node_count", trace.nodes.len() as i64),
  ];

  if let Some(service_version) = &config.service_version {
    attrs.push(OtelAttribute::string("service.version", service_version));
  }
  if let Some(environment) = config
    .environment
    .as_ref()
    .or(Some(&trace.metadata.environment))
  {
    attrs.push(OtelAttribute::string(
      "deployment.environment.name",
      environment,
    ));
  }
  if let Some(workflow_name) = &trace.workflow_name {
    attrs.push(OtelAttribute::string(
      "agentflow.workflow.name",
      workflow_name,
    ));
  }
  if let Some(duration_ms) = trace.duration_ms() {
    attrs.push(OtelAttribute::i64(
      "agentflow.workflow.duration_ms",
      duration_ms as i64,
    ));
  }

  attrs
}

fn node_span(
  trace_id: &str,
  workflow_span_id: &str,
  node_span_id: &str,
  trace: &ExecutionTrace,
  node: &NodeTrace,
) -> OtelSpan {
  let start = unix_nanos(node.started_at);
  let end = node.completed_at.map(unix_nanos).unwrap_or(start);
  let mut attributes = vec![
    OtelAttribute::string("agentflow.workflow.id", &trace.workflow_id),
    OtelAttribute::string("agentflow.node.id", &node.node_id),
    OtelAttribute::string("agentflow.node.type", &node.node_type),
    OtelAttribute::string("agentflow.node.status", node_status_name(&node.status)),
  ];

  if let Some(duration_ms) = node.duration_ms {
    attributes.push(OtelAttribute::i64(
      "agentflow.node.duration_ms",
      duration_ms as i64,
    ));
  }

  OtelSpan {
    trace_id: trace_id.to_string(),
    span_id: node_span_id.to_string(),
    parent_span_id: Some(workflow_span_id.to_string()),
    name: format!("agentflow.node {}", node.node_id),
    kind: OtelSpanKind::Internal,
    start_time_unix_nano: start,
    end_time_unix_nano: end,
    attributes,
    events: Vec::new(),
    status: status_from_node(node),
  }
}

fn llm_span(
  trace_id: &str,
  node_span_id: &str,
  trace: &ExecutionTrace,
  node: &NodeTrace,
  llm: &LLMTrace,
) -> OtelSpan {
  let start = unix_nanos(node.started_at);
  let end = add_ms(start, llm.latency_ms);
  let mut attributes = vec![
    OtelAttribute::string("agentflow.workflow.id", &trace.workflow_id),
    OtelAttribute::string("agentflow.node.id", &node.node_id),
    OtelAttribute::string("gen_ai.system", &llm.provider),
    OtelAttribute::string("gen_ai.request.model", &llm.model),
    OtelAttribute::i64("gen_ai.response.latency_ms", llm.latency_ms as i64),
  ];

  if let Some(max_tokens) = llm.max_tokens {
    attributes.push(OtelAttribute::i64(
      "gen_ai.request.max_tokens",
      max_tokens as i64,
    ));
  }
  if let Some(usage) = &llm.usage {
    attributes.push(OtelAttribute::i64(
      "gen_ai.usage.input_tokens",
      usage.prompt_tokens as i64,
    ));
    attributes.push(OtelAttribute::i64(
      "gen_ai.usage.output_tokens",
      usage.completion_tokens as i64,
    ));
    attributes.push(OtelAttribute::i64(
      "gen_ai.usage.total_tokens",
      usage.total_tokens as i64,
    ));
  }

  OtelSpan {
    trace_id: trace_id.to_string(),
    span_id: span_id(
      &trace.workflow_id,
      &format!("llm:{}:{}", node.node_id, llm.model),
    ),
    parent_span_id: Some(node_span_id.to_string()),
    name: format!("agentflow.llm {}", llm.model),
    kind: OtelSpanKind::Client,
    start_time_unix_nano: start,
    end_time_unix_nano: end,
    attributes,
    events: Vec::new(),
    status: OtelStatus::ok(),
  }
}

fn agent_span(
  trace_id: &str,
  node_span_id: &str,
  agent_span_id: &str,
  node: &NodeTrace,
  agent: &AgentTrace,
) -> OtelSpan {
  let start = unix_nanos(node.started_at);
  let end = node.completed_at.map(unix_nanos).unwrap_or(start);
  let mut attributes = vec![
    OtelAttribute::string("agentflow.node.id", &node.node_id),
    OtelAttribute::string("agentflow.agent.session_id", &agent.session_id),
    OtelAttribute::i64("agentflow.agent.step_count", agent.steps.len() as i64),
    OtelAttribute::i64("agentflow.agent.event_count", agent.events.len() as i64),
    OtelAttribute::i64(
      "agentflow.agent.tool_call_count",
      agent.tool_calls.len() as i64,
    ),
  ];

  if let Some(answer) = &agent.answer {
    attributes.push(OtelAttribute::bool(
      "agentflow.agent.final_answer_present",
      !answer.is_empty(),
    ));
  }

  OtelSpan {
    trace_id: trace_id.to_string(),
    span_id: agent_span_id.to_string(),
    parent_span_id: Some(node_span_id.to_string()),
    name: format!("agentflow.agent {}", agent.session_id),
    kind: OtelSpanKind::Internal,
    start_time_unix_nano: start,
    end_time_unix_nano: end,
    attributes,
    events: Vec::new(),
    status: status_from_node(node),
  }
}

fn tool_span(
  trace_id: &str,
  agent_span_id: &str,
  trace: &ExecutionTrace,
  node: &NodeTrace,
  tool_call: &ToolCallTrace,
  index: usize,
) -> OtelSpan {
  let start = unix_nanos(node.started_at);
  let end = tool_call
    .duration_ms
    .map(|duration_ms| add_ms(start, duration_ms))
    .unwrap_or(start);
  let is_error = tool_call.is_error.unwrap_or(false);
  let mut attributes = vec![
    OtelAttribute::string("agentflow.workflow.id", &trace.workflow_id),
    OtelAttribute::string("agentflow.node.id", &node.node_id),
    OtelAttribute::string("agentflow.tool.name", &tool_call.tool),
    OtelAttribute::bool("agentflow.tool.is_mcp", tool_call.is_mcp),
    OtelAttribute::i64("agentflow.tool.call_index", index as i64),
  ];

  if let Some(duration_ms) = tool_call.duration_ms {
    attributes.push(OtelAttribute::i64(
      "agentflow.tool.duration_ms",
      duration_ms as i64,
    ));
  }

  OtelSpan {
    trace_id: trace_id.to_string(),
    span_id: span_id(
      &trace.workflow_id,
      &format!("tool:{}:{}:{}", node.node_id, index, tool_call.tool),
    ),
    parent_span_id: Some(agent_span_id.to_string()),
    name: format!("agentflow.tool {}", tool_call.tool),
    kind: OtelSpanKind::Client,
    start_time_unix_nano: start,
    end_time_unix_nano: end,
    attributes,
    events: Vec::new(),
    status: if is_error {
      OtelStatus::error("tool call failed")
    } else {
      OtelStatus::ok()
    },
  }
}

fn status_from_trace(status: &TraceStatus) -> OtelStatus {
  match status {
    TraceStatus::Running => OtelStatus {
      code: OtelStatusCode::Unset,
      message: None,
    },
    TraceStatus::Completed => OtelStatus::ok(),
    TraceStatus::Failed { error } => OtelStatus::error(error),
  }
}

fn status_from_node(node: &NodeTrace) -> OtelStatus {
  match node.status {
    NodeStatus::Running | NodeStatus::Skipped => OtelStatus {
      code: OtelStatusCode::Unset,
      message: node.error.clone(),
    },
    NodeStatus::Completed => OtelStatus::ok(),
    NodeStatus::Failed => OtelStatus::error(
      node
        .error
        .clone()
        .unwrap_or_else(|| "node failed".to_string()),
    ),
  }
}

fn trace_status_name(status: &TraceStatus) -> &'static str {
  match status {
    TraceStatus::Running => "running",
    TraceStatus::Completed => "completed",
    TraceStatus::Failed { .. } => "failed",
  }
}

fn node_status_name(status: &NodeStatus) -> &'static str {
  match status {
    NodeStatus::Running => "running",
    NodeStatus::Completed => "completed",
    NodeStatus::Failed => "failed",
    NodeStatus::Skipped => "skipped",
  }
}

fn unix_nanos(time: DateTime<Utc>) -> u64 {
  time.timestamp_nanos_opt().unwrap_or_default() as u64
}

fn add_ms(start_unix_nano: u64, duration_ms: u64) -> u64 {
  start_unix_nano.saturating_add(duration_ms.saturating_mul(1_000_000))
}

fn trace_id(workflow_id: &str) -> String {
  hex_hash(workflow_id, 16)
}

fn span_id(workflow_id: &str, name: &str) -> String {
  hex_hash(&format!("{workflow_id}:{name}"), 8)
}

fn hex_hash(input: &str, bytes: usize) -> String {
  let mut hash = 0xcbf29ce484222325u64;
  for byte in input.as_bytes() {
    hash ^= u64::from(*byte);
    hash = hash.wrapping_mul(0x100000001b3);
  }

  let mut out = format!("{hash:016x}");
  let required_len = bytes * 2;
  while out.len() < required_len {
    hash ^= hash.rotate_left(13);
    hash = hash.wrapping_mul(0x100000001b3);
    out.push_str(&format!("{hash:016x}"));
  }
  out.truncate(required_len);
  out
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::types::{AgentTrace, NodeTrace, TokenUsage};

  #[derive(Default)]
  struct RecordingSink {
    spans: tokio::sync::Mutex<Vec<OtelSpan>>,
  }

  #[async_trait]
  impl OtelSpanSink for RecordingSink {
    async fn export_spans(&self, spans: Vec<OtelSpan>) -> Result<(), anyhow::Error> {
      self.spans.lock().await.extend(spans);
      Ok(())
    }
  }

  #[test]
  fn maps_workflow_agent_tool_and_mcp_to_spans() {
    let mut trace = ExecutionTrace::new("wf-otel".to_string());
    trace.status = TraceStatus::Completed;
    trace.completed_at = Some(trace.started_at + chrono::Duration::milliseconds(42));

    let mut node = NodeTrace::new("agent".to_string(), "agent".to_string());
    node.status = NodeStatus::Completed;
    node.completed_at = Some(node.started_at + chrono::Duration::milliseconds(24));
    node.duration_ms = Some(24);
    node.agent_details = Some(AgentTrace {
      context: Default::default(),
      session_id: "session-1".to_string(),
      answer: Some("done".to_string()),
      stop_reason: serde_json::json!({"reason": "final_answer"}),
      steps: vec![serde_json::json!({"index": 0})],
      events: vec![serde_json::json!({"event": "tool_call_completed"})],
      tool_calls: vec![ToolCallTrace {
        context: Default::default(),
        tool: "mcp_fixture_echo".to_string(),
        params: Some(serde_json::json!({"message": "hello"})),
        is_error: Some(false),
        duration_ms: Some(7),
        is_mcp: true,
      }],
    });
    trace.nodes.push(node);

    let spans = trace_to_spans(&trace, &OtelExporterConfig::default());

    assert_eq!(spans.len(), 4);
    assert_eq!(spans[0].name, "agentflow.workflow wf-otel");
    assert_eq!(spans[1].name, "agentflow.node agent");
    assert_eq!(spans[2].name, "agentflow.agent session-1");
    assert_eq!(spans[3].name, "agentflow.tool mcp_fixture_echo");
    assert_eq!(spans[3].kind, OtelSpanKind::Client);
    assert!(has_attr(
      &spans[3],
      "agentflow.tool.is_mcp",
      &OtelValue::Bool(true)
    ));
    assert_eq!(spans[3].status.code, OtelStatusCode::Ok);
  }

  #[test]
  fn maps_llm_usage_to_gen_ai_attributes() {
    let mut trace = ExecutionTrace::new("wf-llm".to_string());
    trace.status = TraceStatus::Completed;

    let mut node = NodeTrace::new("llm_node".to_string(), "llm".to_string());
    node.status = NodeStatus::Completed;
    node.llm_details = Some(LLMTrace {
      model: "gpt-test".to_string(),
      provider: "openai".to_string(),
      system_prompt: None,
      user_prompt: "hello".to_string(),
      response: "world".to_string(),
      temperature: Some(0.0),
      max_tokens: Some(32),
      usage: Some(TokenUsage::new(10, 5)),
      latency_ms: 15,
    });
    trace.nodes.push(node);

    let spans = trace_to_spans(&trace, &OtelExporterConfig::default());
    let llm_span = spans
      .iter()
      .find(|span| span.name == "agentflow.llm gpt-test")
      .expect("llm span");

    assert!(has_attr(
      llm_span,
      "gen_ai.system",
      &OtelValue::String("openai".to_string())
    ));
    assert!(has_attr(
      llm_span,
      "gen_ai.usage.total_tokens",
      &OtelValue::I64(15)
    ));
  }

  #[tokio::test]
  async fn exporter_forwards_spans_to_sink() {
    let sink = RecordingSink::default();
    let exporter = OtelTraceExporter::new(OtelExporterConfig::default(), sink);
    let mut trace = ExecutionTrace::new("wf-export".to_string());
    trace.status = TraceStatus::Completed;

    exporter.export_trace_spans(&trace).await.unwrap();

    let exported = exporter.sink.spans.lock().await;
    assert_eq!(exported.len(), 1);
    assert_eq!(exported[0].name, "agentflow.workflow wf-export");
  }

  fn has_attr(span: &OtelSpan, key: &str, value: &OtelValue) -> bool {
    span
      .attributes
      .iter()
      .any(|attr| attr.key == key && &attr.value == value)
  }
}

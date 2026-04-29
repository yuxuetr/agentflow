//! Replay formatting for workflow and agent execution traces.
//!
//! Replay is intentionally read-only: it reconstructs the timeline from a
//! persisted [`ExecutionTrace`](crate::ExecutionTrace) without calling tools,
//! MCP servers, workflows, or LLM providers again.

use crate::{
  redact_trace,
  types::{
    AgentTrace, ExecutionTrace, LLMTrace, NodeStatus, NodeTrace, ToolCallTrace, TraceStatus,
  },
  RedactionConfig,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplayOptions {
  pub include_json: bool,
  pub max_field_chars: usize,
}

impl Default for ReplayOptions {
  fn default() -> Self {
    Self {
      include_json: false,
      max_field_chars: 160,
    }
  }
}

pub fn format_trace_replay(trace: &ExecutionTrace, options: ReplayOptions) -> String {
  let mut trace = trace.clone();
  redact_trace(&mut trace, &RedactionConfig::default());
  let mut out = String::new();
  out.push_str(&format!("Trace Replay: {}\n", trace.workflow_id));
  if let Some(name) = &trace.workflow_name {
    out.push_str(&format!("Workflow: {name}\n"));
  }
  out.push_str(&format!("Status: {}\n", trace_status_label(&trace.status)));
  out.push_str(&format!("Started: {}\n", trace.started_at));
  if let Some(completed_at) = trace.completed_at {
    out.push_str(&format!("Completed: {completed_at}\n"));
  }
  if let Some(duration_ms) = trace.duration_ms() {
    out.push_str(&format!("Duration: {duration_ms}ms\n"));
  }
  out.push_str(&format!("Nodes: {}\n", trace.nodes.len()));
  out.push('\n');

  for (index, node) in trace.nodes.iter().enumerate() {
    append_node(&mut out, index + 1, node, options);
  }

  if options.include_json {
    out.push_str("\nRaw Trace JSON:\n");
    match serde_json::to_string_pretty(&trace) {
      Ok(json) => out.push_str(&json),
      Err(err) => out.push_str(&format!("failed to serialize trace: {err}")),
    }
    out.push('\n');
  }

  out
}

fn append_node(out: &mut String, index: usize, node: &NodeTrace, options: ReplayOptions) {
  out.push_str(&format!(
    "[{index}] node {} ({}) - {}\n",
    node.node_id,
    node.node_type,
    node_status_label(&node.status)
  ));
  if let Some(duration_ms) = node.duration_ms {
    out.push_str(&format!("    duration: {duration_ms}ms\n"));
  }
  if let Some(error) = &node.error {
    out.push_str(&format!(
      "    error: {}\n",
      truncate(error, options.max_field_chars)
    ));
  }

  if let Some(llm) = &node.llm_details {
    append_llm(out, llm, options);
  }
  if let Some(agent) = &node.agent_details {
    append_agent(out, agent, options);
  }
  if node.llm_details.is_none() && node.agent_details.is_none() {
    append_io_summary(out, node, options);
  }
  out.push('\n');
}

fn append_llm(out: &mut String, llm: &LLMTrace, options: ReplayOptions) {
  out.push_str(&format!(
    "    llm: provider={} model={} latency={}ms\n",
    llm.provider, llm.model, llm.latency_ms
  ));
  out.push_str(&format!(
    "    prompt: {}\n",
    truncate(&llm.user_prompt, options.max_field_chars)
  ));
  out.push_str(&format!(
    "    response: {}\n",
    truncate(&llm.response, options.max_field_chars)
  ));
  if let Some(usage) = &llm.usage {
    out.push_str(&format!(
      "    tokens: input={} output={} total={}\n",
      usage.prompt_tokens, usage.completion_tokens, usage.total_tokens
    ));
  }
}

fn append_agent(out: &mut String, agent: &AgentTrace, options: ReplayOptions) {
  out.push_str(&format!(
    "    agent: session={} steps={} events={} tools={}\n",
    agent.session_id,
    agent.steps.len(),
    agent.events.len(),
    agent.tool_calls.len()
  ));
  if let Some(answer) = &agent.answer {
    out.push_str(&format!(
      "    final_answer: {}\n",
      truncate(answer, options.max_field_chars)
    ));
  }

  for step in &agent.steps {
    append_agent_step(out, step, options);
  }
  for tool_call in &agent.tool_calls {
    append_tool_call(out, tool_call, options);
  }
}

fn append_agent_step(out: &mut String, step: &serde_json::Value, options: ReplayOptions) {
  let index = step
    .get("index")
    .and_then(|value| value.as_u64())
    .map(|value| value.to_string())
    .unwrap_or_else(|| "?".to_string());
  let Some(kind) = step.get("kind") else {
    out.push_str(&format!("      step {index}: <missing kind>\n"));
    return;
  };
  let step_type = kind
    .get("type")
    .and_then(|value| value.as_str())
    .unwrap_or("unknown");

  match step_type {
    "observe" => {
      let input = kind
        .get("input")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
      out.push_str(&format!(
        "      step {index}: observe {}\n",
        truncate(input, options.max_field_chars)
      ));
    }
    "tool_call" => {
      let tool = kind
        .get("tool")
        .and_then(|value| value.as_str())
        .unwrap_or("<unknown>");
      out.push_str(&format!("      step {index}: tool_call {tool}\n"));
    }
    "tool_result" => {
      let tool = kind
        .get("tool")
        .and_then(|value| value.as_str())
        .unwrap_or("<unknown>");
      let is_error = kind
        .get("is_error")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
      let content = kind
        .get("content")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
      out.push_str(&format!(
        "      step {index}: tool_result {tool} error={} {}\n",
        is_error,
        truncate(content, options.max_field_chars)
      ));
    }
    "final_answer" => {
      let answer = kind
        .get("answer")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
      out.push_str(&format!(
        "      step {index}: final_answer {}\n",
        truncate(answer, options.max_field_chars)
      ));
    }
    other => out.push_str(&format!("      step {index}: {other}\n")),
  }
}

fn append_tool_call(out: &mut String, tool_call: &ToolCallTrace, options: ReplayOptions) {
  out.push_str(&format!(
    "      tool: {} source={} error={}\n",
    tool_call.tool,
    tool_call
      .source
      .as_deref()
      .unwrap_or(if tool_call.is_mcp { "mcp" } else { "tool" }),
    tool_call.is_error.unwrap_or(false)
  ));
  if !tool_call.permissions.is_empty() {
    out.push_str(&format!(
      "        permissions: {}\n",
      tool_call.permissions.join(",")
    ));
  }
  if let Some(duration_ms) = tool_call.duration_ms {
    out.push_str(&format!("        duration: {duration_ms}ms\n"));
  }
  if let Some(params) = &tool_call.params {
    out.push_str(&format!(
      "        params: {}\n",
      truncate(&params.to_string(), options.max_field_chars)
    ));
  }
}

fn append_io_summary(out: &mut String, node: &NodeTrace, options: ReplayOptions) {
  if let Some(input) = &node.input {
    out.push_str(&format!(
      "    input: {}\n",
      truncate(&input.to_string(), options.max_field_chars)
    ));
  }
  if let Some(output) = &node.output {
    out.push_str(&format!(
      "    output: {}\n",
      truncate(&output.to_string(), options.max_field_chars)
    ));
  }
}

fn trace_status_label(status: &TraceStatus) -> String {
  match status {
    TraceStatus::Running => "running".to_string(),
    TraceStatus::Completed => "completed".to_string(),
    TraceStatus::Failed { error } => format!("failed ({error})"),
  }
}

fn node_status_label(status: &NodeStatus) -> &'static str {
  match status {
    NodeStatus::Running => "running",
    NodeStatus::Completed => "completed",
    NodeStatus::Failed => "failed",
    NodeStatus::Skipped => "skipped",
  }
}

fn truncate(value: &str, max_chars: usize) -> String {
  if value.chars().count() <= max_chars {
    return value.to_string();
  }
  let take = max_chars.saturating_sub(3);
  format!("{}...", value.chars().take(take).collect::<String>())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::types::{AgentTrace, ExecutionTrace, NodeStatus, NodeTrace, ToolCallTrace};

  #[test]
  fn replay_includes_agent_steps_and_mcp_tool_calls() {
    let mut trace = ExecutionTrace::new("wf-replay".to_string());
    trace.status = TraceStatus::Completed;
    let mut node = NodeTrace::new("agent".to_string(), "agent".to_string());
    node.status = NodeStatus::Completed;
    node.agent_details = Some(AgentTrace {
      context: Default::default(),
      session_id: "session-1".to_string(),
      answer: Some("done".to_string()),
      stop_reason: serde_json::json!({"reason": "final_answer"}),
      steps: vec![
        serde_json::json!({"index": 0, "kind": {"type": "observe", "input": "hello"}}),
        serde_json::json!({"index": 1, "kind": {"type": "tool_call", "tool": "mcp_echo"}}),
        serde_json::json!({"index": 2, "kind": {"type": "tool_result", "tool": "mcp_echo", "content": "ok", "is_error": false}}),
      ],
      events: vec![],
      tool_calls: vec![ToolCallTrace {
        context: Default::default(),
        tool: "mcp_echo".to_string(),
        source: Some("mcp".to_string()),
        permissions: vec!["mcp".to_string(), "network".to_string()],
        params: Some(serde_json::json!({"message": "hello"})),
        is_error: Some(false),
        duration_ms: Some(12),
        is_mcp: true,
      }],
    });
    trace.nodes.push(node);

    let replay = format_trace_replay(&trace, ReplayOptions::default());

    assert!(replay.contains("Trace Replay: wf-replay"));
    assert!(replay.contains("step 1: tool_call mcp_echo"));
    assert!(replay.contains("tool: mcp_echo source=mcp error=false"));
  }

  #[test]
  fn replay_can_include_raw_json() {
    let mut trace = ExecutionTrace::new("wf-json".to_string());
    let mut node = NodeTrace::new("node".to_string(), "tool".to_string());
    node.input = Some(serde_json::json!({"api_key": "secret"}));
    trace.nodes.push(node);

    let replay = format_trace_replay(
      &trace,
      ReplayOptions {
        include_json: true,
        ..ReplayOptions::default()
      },
    );

    assert!(replay.contains("Raw Trace JSON:"));
    assert!(replay.contains("\"workflow_id\": \"wf-json\""));
    assert!(replay.contains("[REDACTED]"));
    assert!(!replay.contains("secret"));
  }
}

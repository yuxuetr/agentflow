//! Terminal debugger formatting for persisted execution traces.

use crate::{
  redact_trace,
  types::{AgentTrace, ExecutionTrace, NodeStatus, NodeTrace, ToolCallTrace, TraceStatus},
  RedactionConfig,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceTuiFilter {
  All,
  Workflow,
  Agent,
  Tool,
  Mcp,
}

impl TraceTuiFilter {
  pub fn as_str(self) -> &'static str {
    match self {
      Self::All => "all",
      Self::Workflow => "workflow",
      Self::Agent => "agent",
      Self::Tool => "tool",
      Self::Mcp => "mcp",
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceTuiOptions {
  pub filter: TraceTuiFilter,
  pub details: bool,
  pub max_field_chars: usize,
}

impl Default for TraceTuiOptions {
  fn default() -> Self {
    Self {
      filter: TraceTuiFilter::All,
      details: false,
      max_field_chars: 120,
    }
  }
}

pub fn format_trace_tui(trace: &ExecutionTrace, options: TraceTuiOptions) -> String {
  let mut trace = trace.clone();
  redact_trace(&mut trace, &RedactionConfig::default());

  let mut out = String::new();
  append_header(&mut out, &trace, options);
  append_timeline(&mut out, &trace, options);
  out
}

fn append_header(out: &mut String, trace: &ExecutionTrace, options: TraceTuiOptions) {
  out.push_str("AgentFlow Trace TUI\n");
  out.push_str("===================\n");
  out.push_str(&format!("Run: {}\n", trace.workflow_id));
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
  out.push_str(&format!(
    "Filter: {} | Details: {}\n",
    options.filter.as_str(),
    if options.details { "on" } else { "off" }
  ));
  out.push_str("Hint: rerun with --filter agent|tool|mcp|workflow and --details to focus.\n\n");
}

fn append_timeline(out: &mut String, trace: &ExecutionTrace, options: TraceTuiOptions) {
  out.push_str("Timeline\n");
  out.push_str("--------\n");

  let mut row = 0usize;
  if matches!(
    options.filter,
    TraceTuiFilter::All | TraceTuiFilter::Workflow
  ) {
    append_workflow_row(out, row, trace);
    row += 1;
  }

  for node in &trace.nodes {
    if matches!(
      options.filter,
      TraceTuiFilter::All | TraceTuiFilter::Workflow
    ) {
      append_node_row(out, row, node);
      row += 1;
      if options.details {
        append_node_details(out, node, options);
      }
    }

    if let Some(agent) = &node.agent_details {
      if matches!(options.filter, TraceTuiFilter::All | TraceTuiFilter::Agent) {
        append_agent_row(out, row, node, agent);
        row += 1;
        if options.details {
          append_agent_details(out, agent, options);
        }
      }

      if matches!(
        options.filter,
        TraceTuiFilter::All | TraceTuiFilter::Tool | TraceTuiFilter::Mcp
      ) {
        for tool_call in &agent.tool_calls {
          if options.filter == TraceTuiFilter::Mcp && !tool_call.is_mcp {
            continue;
          }
          append_tool_row(out, row, tool_call);
          row += 1;
          if options.details {
            append_tool_details(out, tool_call, options);
          }
        }
      }
    }
  }

  if row == 0 {
    out.push_str("(no matching trace entries)\n");
  }
}

fn append_workflow_row(out: &mut String, row: usize, trace: &ExecutionTrace) {
  let duration = trace
    .duration_ms()
    .map(|value| format!(" duration={value}ms"))
    .unwrap_or_default();
  out.push_str(&format!(
    "{row:02} workflow {} status={}{}\n",
    trace.workflow_id,
    trace_status_label(&trace.status),
    duration
  ));
}

fn append_node_row(out: &mut String, row: usize, node: &NodeTrace) {
  let duration = node
    .duration_ms
    .map(|value| format!(" duration={value}ms"))
    .unwrap_or_default();
  out.push_str(&format!(
    "{row:02} node {} type={} status={}{}\n",
    node.node_id,
    node.node_type,
    node_status_label(&node.status),
    duration
  ));
}

fn append_agent_row(out: &mut String, row: usize, node: &NodeTrace, agent: &AgentTrace) {
  let stop = compact_json(&agent.stop_reason);
  out.push_str(&format!(
    "{row:02} agent node={} session={} steps={} tools={} stop={}\n",
    node.node_id,
    agent.session_id,
    agent.steps.len(),
    agent.tool_calls.len(),
    stop
  ));
}

fn append_tool_row(out: &mut String, row: usize, tool_call: &ToolCallTrace) {
  let duration = tool_call
    .duration_ms
    .map(|value| format!(" duration={value}ms"))
    .unwrap_or_default();
  out.push_str(&format!(
    "{row:02} tool {} source={} error={}{}\n",
    tool_call.tool,
    if tool_call.is_mcp { "mcp" } else { "tool" },
    tool_call.is_error.unwrap_or(false),
    duration
  ));
}

fn append_node_details(out: &mut String, node: &NodeTrace, options: TraceTuiOptions) {
  if let Some(error) = &node.error {
    out.push_str(&format!(
      "    error: {}\n",
      truncate(error, options.max_field_chars)
    ));
  }
  if let Some(input) = &node.input {
    out.push_str(&format!(
      "    input: {}\n",
      truncate(&compact_json(input), options.max_field_chars)
    ));
  }
  if let Some(output) = &node.output {
    out.push_str(&format!(
      "    output: {}\n",
      truncate(&compact_json(output), options.max_field_chars)
    ));
  }
}

fn append_agent_details(out: &mut String, agent: &AgentTrace, options: TraceTuiOptions) {
  if let Some(answer) = &agent.answer {
    out.push_str(&format!(
      "    final_answer: {}\n",
      truncate(answer, options.max_field_chars)
    ));
  }
  for step in &agent.steps {
    out.push_str(&format!(
      "    step: {}\n",
      truncate(&compact_json(step), options.max_field_chars)
    ));
  }
}

fn append_tool_details(out: &mut String, tool_call: &ToolCallTrace, options: TraceTuiOptions) {
  if let Some(params) = &tool_call.params {
    out.push_str(&format!(
      "    params: {}\n",
      truncate(&compact_json(params), options.max_field_chars)
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

fn compact_json(value: &serde_json::Value) -> String {
  serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
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
  fn tui_filters_mcp_tool_calls_and_redacts_params() {
    let mut trace = ExecutionTrace::new("wf-tui".to_string());
    trace.status = TraceStatus::Completed;
    trace.workflow_name = Some("TUI Test".to_string());

    let mut node = NodeTrace::new("agent".to_string(), "agent".to_string());
    node.status = NodeStatus::Completed;
    node.agent_details = Some(AgentTrace {
      context: Default::default(),
      session_id: "session-1".to_string(),
      answer: Some("done".to_string()),
      stop_reason: serde_json::json!({"reason": "final_answer"}),
      steps: vec![],
      events: vec![],
      tool_calls: vec![
        ToolCallTrace {
          context: Default::default(),
          tool: "local_tool".to_string(),
          params: Some(serde_json::json!({"value": "ok"})),
          is_error: Some(false),
          duration_ms: Some(5),
          is_mcp: false,
        },
        ToolCallTrace {
          context: Default::default(),
          tool: "mcp_echo".to_string(),
          params: Some(serde_json::json!({"api_key": "secret", "message": "hello"})),
          is_error: Some(false),
          duration_ms: Some(12),
          is_mcp: true,
        },
      ],
    });
    trace.nodes.push(node);

    let output = format_trace_tui(
      &trace,
      TraceTuiOptions {
        filter: TraceTuiFilter::Mcp,
        details: true,
        ..TraceTuiOptions::default()
      },
    );

    assert!(output.contains("AgentFlow Trace TUI"));
    assert!(output.contains("Filter: mcp"));
    assert!(output.contains("tool mcp_echo source=mcp"));
    assert!(output.contains("[REDACTED]"));
    assert!(!output.contains("local_tool"));
    assert!(!output.contains("secret"));
  }
}

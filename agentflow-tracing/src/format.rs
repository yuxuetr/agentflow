//! Formatting utilities for execution traces

use crate::types::*;

/// Format a trace as human-readable text
pub fn format_trace_human_readable(trace: &ExecutionTrace) -> String {
  let mut output = String::new();

  // Header
  output.push_str("═══════════════════════════════════════════════════════════\n");
  output.push_str(&format!(
    "Workflow: {}\n",
    trace
      .workflow_name
      .as_deref()
      .unwrap_or(&trace.workflow_id)
  ));
  output.push_str(&format!("ID: {}\n", trace.workflow_id));
  output.push_str("═══════════════════════════════════════════════════════════\n\n");

  // Status and timing
  output.push_str(&format!("Status: {:?}\n", trace.status));
  output.push_str(&format!("Started: {}\n", trace.started_at));

  if let Some(completed_at) = trace.completed_at {
    output.push_str(&format!("Completed: {}\n", completed_at));
    if let Some(duration_ms) = trace.duration_ms() {
      output.push_str(&format!("Duration: {}ms\n", duration_ms));
    }
  } else {
    output.push_str("Duration: (still running)\n");
  }

  // Metadata
  output.push_str(&format!("Environment: {}\n", trace.metadata.environment));
  if let Some(ref user_id) = trace.metadata.user_id {
    output.push_str(&format!("User: {}\n", user_id));
  }
  if !trace.metadata.tags.is_empty() {
    output.push_str(&format!("Tags: {}\n", trace.metadata.tags.join(", ")));
  }

  // Nodes
  output.push_str("\n───────────────────────────────────────────────────────────\n");
  output.push_str(&format!("Nodes Executed: {}\n", trace.nodes.len()));
  output.push_str("───────────────────────────────────────────────────────────\n\n");

  for (i, node) in trace.nodes.iter().enumerate() {
    output.push_str(&format!("[{}] {} ({})\n", i + 1, node.node_id, node.node_type));
    output.push_str(&format!("    Status: {:?}\n", node.status));

    if let Some(duration_ms) = node.duration_ms {
      output.push_str(&format!("    Duration: {}ms\n", duration_ms));
    }

    // LLM details
    if let Some(ref llm) = node.llm_details {
      output.push_str(&format!("    Model: {} ({})\n", llm.model, llm.provider));

      if let Some(ref system_prompt) = llm.system_prompt {
        output.push_str(&format!(
          "    System Prompt: {}\n",
          truncate_string(system_prompt, 80)
        ));
      }

      output.push_str(&format!(
        "    User Prompt: {}\n",
        truncate_string(&llm.user_prompt, 80)
      ));
      output.push_str(&format!(
        "    Response: {}\n",
        truncate_string(&llm.response, 120)
      ));

      if let Some(ref usage) = llm.usage {
        output.push_str(&format!(
          "    Tokens: {} (prompt) + {} (completion) = {}\n",
          usage.prompt_tokens, usage.completion_tokens, usage.total_tokens
        ));

        if let Some(cost) = usage.estimated_cost_usd {
          output.push_str(&format!("    Cost: ${:.4}\n", cost));
        }
      }

      output.push_str(&format!("    Latency: {}ms\n", llm.latency_ms));
    }

    // Input/Output (truncated)
    if let Some(ref input) = node.input {
      let input_str = serde_json::to_string(input).unwrap_or_default();
      output.push_str(&format!("    Input: {}\n", truncate_string(&input_str, 100)));
    }

    if let Some(ref output_val) = node.output {
      let output_str = serde_json::to_string(output_val).unwrap_or_default();
      output.push_str(&format!("    Output: {}\n", truncate_string(&output_str, 100)));
    }

    // Error
    if let Some(ref error) = node.error {
      output.push_str(&format!("    Error: {}\n", error));
    }

    output.push('\n');
  }

  output.push_str("═══════════════════════════════════════════════════════════\n");

  output
}

/// Format trace as compact summary
pub fn format_trace_summary(trace: &ExecutionTrace) -> String {
  let status_symbol = match trace.status {
    TraceStatus::Running => "⏳",
    TraceStatus::Completed => "✅",
    TraceStatus::Failed { .. } => "❌",
  };

  let duration = trace
    .duration_ms()
    .map(|ms| format!("{}ms", ms))
    .unwrap_or_else(|| "running".to_string());

  format!(
    "{} {} | {} | {} nodes | {}",
    status_symbol,
    trace.workflow_id,
    match &trace.status {
      TraceStatus::Running => "running",
      TraceStatus::Completed => "completed",
      TraceStatus::Failed { .. } => "failed",
    },
    trace.nodes.len(),
    duration
  )
}

/// Export trace as JSON
pub fn export_trace_json(trace: &ExecutionTrace) -> Result<String, anyhow::Error> {
  Ok(serde_json::to_string_pretty(trace)?)
}

/// Export trace as compact JSON (no pretty printing)
pub fn export_trace_json_compact(trace: &ExecutionTrace) -> Result<String, anyhow::Error> {
  Ok(serde_json::to_string(trace)?)
}

/// Truncate string with ellipsis
fn truncate_string(s: &str, max_len: usize) -> String {
  if s.len() <= max_len {
    s.to_string()
  } else {
    format!("{}...", &s[..max_len - 3])
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_format_trace_summary() {
    let trace = ExecutionTrace::new("wf-test".to_string());
    let summary = format_trace_summary(&trace);
    assert!(summary.contains("wf-test"));
    assert!(summary.contains("running"));
  }

  #[test]
  fn test_format_trace_human_readable() {
    let mut trace = ExecutionTrace::new("wf-test".to_string());
    trace.workflow_name = Some("Test Workflow".to_string());

    let mut node = NodeTrace::new("node1".to_string(), "TestNode".to_string());
    node.complete();
    trace.nodes.push(node);

    let output = format_trace_human_readable(&trace);
    assert!(output.contains("Test Workflow"));
    assert!(output.contains("node1"));
    assert!(output.contains("TestNode"));
  }

  #[test]
  fn test_truncate_string() {
    assert_eq!(truncate_string("hello", 10), "hello");
    assert_eq!(truncate_string("hello world this is long", 10), "hello w...");
  }

  #[test]
  fn test_export_json() {
    let trace = ExecutionTrace::new("wf-json".to_string());
    let json = export_trace_json(&trace).unwrap();
    assert!(json.contains("wf-json"));

    // Should be able to deserialize
    let _: ExecutionTrace = serde_json::from_str(&json).unwrap();
  }
}

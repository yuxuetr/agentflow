//! ReAct prompt assembly benchmarks.
//!
//! Run with:
//!
//! ```bash
//! cargo test -p agentflow-agents --test prompt_assembly_benchmarks --target-dir /tmp/agentflow-target -- --nocapture
//! ```

use agentflow_agents::react::{MemorySummaryStrategy, ReActAgent, ReActConfig};
use agentflow_memory::{MemoryStore, Message, SessionMemory};
use agentflow_tools::{Tool, ToolError, ToolMetadata, ToolOutput, ToolRegistry};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};

const SESSION_ID: &str = "prompt-bench-session";

struct MockTool;

#[async_trait]
impl Tool for MockTool {
  fn name(&self) -> &str {
    "mock_tool"
  }

  fn description(&self) -> &str {
    "Mock tool for prompt assembly benchmarks."
  }

  fn parameters_schema(&self) -> Value {
    json!({
      "type": "object",
      "properties": {
        "query": {"type": "string"}
      },
      "required": ["query"]
    })
  }

  fn metadata(&self) -> ToolMetadata {
    ToolMetadata::builtin_named("mock")
  }

  async fn execute(&self, _params: Value) -> Result<ToolOutput, ToolError> {
    Ok(ToolOutput::success("ok"))
  }
}

#[tokio::test]
async fn benchmark_react_prompt_assembly() {
  println!("\nReAct prompt assembly benchmarks");
  println!("{}", "=".repeat(80));

  let short = build_agent(20, None, MemorySummaryStrategy::Disabled).await;
  let long = build_agent(1_000, None, MemorySummaryStrategy::Disabled).await;
  let summarized = build_agent(1_000, Some(256), MemorySummaryStrategy::Compact).await;

  let short_samples = measure_prompt_assembly(&short, 200).await;
  let long_samples = measure_prompt_assembly(&long, 50).await;
  let summarized_samples = measure_prompt_assembly(&summarized, 50).await;

  print_stats("short context", &short_samples);
  print_stats("long context", &long_samples);
  print_stats("summary-triggering context", &summarized_samples);

  let short_messages = short.preview_llm_messages().await.unwrap();
  let long_messages = long.preview_llm_messages().await.unwrap();
  let summarized_messages = summarized.preview_llm_messages().await.unwrap();
  assert_eq!(short_messages.len(), 21);
  assert_eq!(long_messages.len(), 1_001);
  assert!(
    summarized_messages.len() < long_messages.len(),
    "summary budget should reduce prompt message count"
  );
}

async fn build_agent(
  message_count: usize,
  memory_budget: Option<u32>,
  strategy: MemorySummaryStrategy,
) -> ReActAgent {
  let mut memory = SessionMemory::default_window();
  for idx in 0..message_count {
    let message = match idx % 3 {
      0 => Message::user(
        SESSION_ID,
        format!("user request {idx}: explain prompt assembly"),
      ),
      1 => Message::assistant(SESSION_ID, format!("assistant response {idx}: noted")),
      _ => Message::tool_result(SESSION_ID, "mock_tool", format!("tool observation {idx}")),
    };
    memory.add_message(message).await.unwrap();
  }

  let mut registry = ToolRegistry::new();
  registry.register(Arc::new(MockTool));

  let mut config = ReActConfig::new("mock-model")
    .with_persona("You assemble benchmark prompts.")
    .with_memory_summary_strategy(strategy);
  if let Some(budget) = memory_budget {
    config = config.with_memory_prompt_token_budget(budget);
  }

  ReActAgent::new(config, Box::new(memory), Arc::new(registry)).with_session_id(SESSION_ID)
}

async fn measure_prompt_assembly(agent: &ReActAgent, iterations: usize) -> Vec<Duration> {
  let mut samples = Vec::with_capacity(iterations);
  for _ in 0..iterations {
    let start = Instant::now();
    let messages = agent.preview_llm_messages().await.unwrap();
    assert!(!messages.is_empty());
    samples.push(start.elapsed());
  }
  samples
}

fn print_stats(label: &str, samples: &[Duration]) {
  println!(
    "  {label}: p50 {:?}, p95 {:?}, avg {:?} ({} samples)",
    percentile(samples, 50),
    percentile(samples, 95),
    average(samples),
    samples.len()
  );
}

fn percentile(samples: &[Duration], percentile: usize) -> Duration {
  let mut sorted = samples.to_vec();
  sorted.sort();
  let idx = ((sorted.len() - 1) * percentile) / 100;
  sorted[idx]
}

fn average(samples: &[Duration]) -> Duration {
  let total: Duration = samples.iter().copied().sum();
  total / samples.len() as u32
}

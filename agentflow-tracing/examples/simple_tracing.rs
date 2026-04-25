//! Simple tracing example demonstrating workflow execution tracking
//!
//! This example shows how to use the TraceCollector to capture workflow events.
//!
//! Run with:
//! ```bash
//! cargo run --example simple_tracing
//! ```

use agentflow_core::events::{EventListener, TokenUsage, WorkflowEvent};
use agentflow_tracing::{
  format_trace_human_readable, storage::file::FileTraceStorage, TraceCollector, TraceConfig,
  TraceStorage,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::tempdir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("╔══════════════════════════════════════════════════════╗");
  println!("║   AgentFlow Tracing System - Simple Example         ║");
  println!("╚══════════════════════════════════════════════════════╝\n");

  // 1. Create storage (using temp dir for example)
  let temp_dir = tempdir()?;
  let storage = Arc::new(FileTraceStorage::new(temp_dir.path().to_path_buf())?);
  println!("📁 Storage created at: {:?}\n", temp_dir.path());

  // 2. Create trace collector with development config
  let collector = Arc::new(TraceCollector::new(
    storage.clone(),
    TraceConfig::development(),
  ));
  println!("🔍 Trace collector initialized\n");

  // 3. Simulate a workflow execution
  let workflow_id = "demo-workflow-001".to_string();
  println!("🚀 Simulating workflow execution: {}\n", workflow_id);

  // Workflow started
  collector.on_event(&WorkflowEvent::WorkflowStarted {
    workflow_id: workflow_id.clone(),
    timestamp: Instant::now(),
  });
  println!("   ✓ Workflow started");

  tokio::time::sleep(Duration::from_millis(100)).await;

  // Node 1: HTTP Request
  collector.on_event(&WorkflowEvent::NodeStarted {
    workflow_id: workflow_id.clone(),
    node_id: "fetch_papers".to_string(),
    timestamp: Instant::now(),
  });
  println!("   ✓ Node 'fetch_papers' started");

  tokio::time::sleep(Duration::from_millis(200)).await;

  collector.on_event(&WorkflowEvent::NodeCompleted {
    workflow_id: workflow_id.clone(),
    node_id: "fetch_papers".to_string(),
    duration: Duration::from_millis(200),
    timestamp: Instant::now(),
  });
  println!("   ✓ Node 'fetch_papers' completed (200ms)");

  tokio::time::sleep(Duration::from_millis(50)).await;

  // Node 2: LLM Summarization
  collector.on_event(&WorkflowEvent::NodeStarted {
    workflow_id: workflow_id.clone(),
    node_id: "summarize".to_string(),
    timestamp: Instant::now(),
  });
  println!("   ✓ Node 'summarize' started");

  tokio::time::sleep(Duration::from_millis(50)).await;

  // LLM Prompt Sent
  collector.on_event(&WorkflowEvent::LLMPromptSent {
    workflow_id: workflow_id.clone(),
    node_id: "summarize".to_string(),
    model: "gpt-4o".to_string(),
    provider: "openai".to_string(),
    system_prompt: Some("You are a research assistant.".to_string()),
    user_prompt: "Summarize the following papers: [papers...]".to_string(),
    temperature: Some(0.7),
    max_tokens: Some(2000),
    timestamp: Instant::now(),
  });
  println!("   ✓ LLM prompt sent to gpt-4o");

  tokio::time::sleep(Duration::from_millis(300)).await;

  // LLM Response Received
  collector.on_event(&WorkflowEvent::LLMResponseReceived {
    workflow_id: workflow_id.clone(),
    node_id: "summarize".to_string(),
    model: "gpt-4o".to_string(),
    response: "Here is a concise summary of the papers...".to_string(),
    usage: Some(TokenUsage {
      prompt_tokens: 1500,
      completion_tokens: 500,
      total_tokens: 2000,
    }),
    duration: Duration::from_millis(300),
    timestamp: Instant::now(),
  });
  println!("   ✓ LLM response received (300ms, 2000 tokens)");

  tokio::time::sleep(Duration::from_millis(50)).await;

  collector.on_event(&WorkflowEvent::NodeCompleted {
    workflow_id: workflow_id.clone(),
    node_id: "summarize".to_string(),
    duration: Duration::from_millis(400),
    timestamp: Instant::now(),
  });
  println!("   ✓ Node 'summarize' completed (400ms)");

  tokio::time::sleep(Duration::from_millis(50)).await;

  // Workflow completed
  collector.on_event(&WorkflowEvent::WorkflowCompleted {
    workflow_id: workflow_id.clone(),
    duration: Duration::from_millis(750),
    timestamp: Instant::now(),
  });
  println!("   ✓ Workflow completed (750ms total)\n");

  // Give async storage time to complete
  tokio::time::sleep(Duration::from_millis(200)).await;

  // 4. Query the trace
  println!("📊 Retrieving execution trace...\n");
  let trace = storage
    .get_trace(&workflow_id)
    .await?
    .expect("Trace should exist");

  // 5. Display human-readable trace
  println!("{}", format_trace_human_readable(&trace));

  // 6. Show trace statistics
  println!("\n📈 Statistics:");
  println!("   - Total nodes executed: {}", trace.nodes.len());
  println!("   - Duration: {}ms", trace.duration_ms().unwrap_or(0));

  if let Some(node) = trace.nodes.iter().find(|n| n.llm_details.is_some()) {
    if let Some(ref llm) = node.llm_details {
      if let Some(ref usage) = llm.usage {
        println!("   - Total tokens used: {}", usage.total_tokens);
        println!("   - LLM latency: {}ms", llm.latency_ms);
      }
    }
  }

  println!("\n✅ Example completed successfully!");
  println!("   Traces stored in: {:?}", temp_dir.path());

  Ok(())
}

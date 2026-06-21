//! Dynamic workflow from a declarative JSON plan (P-A4.4).
//!
//! The productized form of `dynamic_workflow_spike.rs`: a **plan** — the JSON an
//! LLM would emit — is parsed, compiled to a `Flow` of *real tool calls* via
//! `agentflow_agents::dynamic::compile_plan_to_flow`, and executed by the core
//! engine under `Concurrent` mode. Independent steps (`fetch_docs`, `fetch_api`)
//! run in parallel; `summarize` waits for both and receives their outputs.
//!
//! This is "the agent compiles its intent into one deterministic artifact, then
//! the engine runs it" — flexibility of an agent plan, reliability of a DAG.
//!
//! ```bash
//! cargo run -p agentflow-agents --example dynamic_workflow_plan
//! ```

use std::sync::Arc;

use agentflow_agents::dynamic::{WorkflowPlan, compile_plan_to_flow};
use agentflow_core::{FlowExecutionConfig, FlowExt};
use agentflow_tools::{Tool, ToolError, ToolMetadata, ToolOutput, ToolRegistry};
use serde_json::{Value, json};

/// Pretends to fetch a source named by `params.source`.
struct FetchTool;
#[async_trait::async_trait]
impl Tool for FetchTool {
  fn name(&self) -> &str {
    "fetch"
  }
  fn description(&self) -> &str {
    "fetch a named source"
  }
  fn parameters_schema(&self) -> Value {
    json!({ "type": "object", "properties": { "source": { "type": "string" } } })
  }
  fn metadata(&self) -> ToolMetadata {
    ToolMetadata::builtin_named("fetch")
  }
  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
    let source = params.get("source").and_then(Value::as_str).unwrap_or("?");
    Ok(ToolOutput::success(format!("contents of {source}")))
  }
}

/// Summarizes everything it received (its dependency outputs).
struct SummarizeTool;
#[async_trait::async_trait]
impl Tool for SummarizeTool {
  fn name(&self) -> &str {
    "summarize"
  }
  fn description(&self) -> &str {
    "summarize the inputs"
  }
  fn parameters_schema(&self) -> Value {
    json!({ "type": "object" })
  }
  fn metadata(&self) -> ToolMetadata {
    ToolMetadata::builtin_named("summarize")
  }
  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
    let parts: Vec<String> = params
      .as_object()
      .into_iter()
      .flatten()
      .filter_map(|(_, v)| v.as_str().map(str::to_string))
      .collect();
    Ok(ToolOutput::success(format!(
      "summary[{}]",
      parts.join(" + ")
    )))
  }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  // The plan — exactly the JSON shape an LLM would produce. The graph's shape
  // (two parallel fetches feeding a summary) is data, not compiled-in code.
  let plan_json = r#"{
    "steps": [
      { "id": "fetch_docs", "tool": "fetch", "params": { "source": "docs" } },
      { "id": "fetch_api",  "tool": "fetch", "params": { "source": "api"  } },
      { "id": "summarize",  "tool": "summarize", "depends_on": ["fetch_docs", "fetch_api"] }
    ]
  }"#;
  let plan: WorkflowPlan = serde_json::from_str(plan_json)?;
  println!("LLM-shaped plan: {} steps", plan.steps.len());

  let mut registry = ToolRegistry::new();
  registry.register(Arc::new(FetchTool));
  registry.register(Arc::new(SummarizeTool));

  // Compile the plan to a Flow (needs only the graph IR + tool contract) …
  let flow = compile_plan_to_flow(&plan, Arc::new(registry))?;
  println!(
    "compiled to a Flow; execution order: {:?}",
    flow.execution_order()?
  );

  // … and execute it deterministically, in parallel where the plan allows.
  let state = flow
    .execute_from_inputs_with_config(Default::default(), FlowExecutionConfig::concurrent(8))
    .await?;

  let summary = state
    .get("summarize")
    .and_then(|r| r.as_ref().ok())
    .and_then(|o| o.get("result"))
    .map(|v| format!("{v:?}"))
    .unwrap_or_else(|| "<none>".to_string());
  println!("result: {summary}");

  assert!(
    summary.contains("contents of docs") && summary.contains("contents of api"),
    "summary should fold in both parallel fetches: {summary}"
  );
  println!("✓ dynamic workflow compiled from a JSON plan and executed (parallel) via the kernel");
  Ok(())
}

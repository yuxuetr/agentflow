//! DAG + Agent hybrid example.
//!
//! This example runs a parent DAG with an `AgentNode`. The agent calls a
//! `WorkflowTool`, which wraps a child DAG as a normal tool.
//!
//! Run:
//! ```sh
//! cargo run -p yanshi-agents --example hybrid_workflow_agent
//! ```

use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use yanshi_core::FlowExt;

use async_trait::async_trait;
use serde_json::json;
use yanshi_agents::nodes::AgentNode;
use yanshi_agents::react::{ReActAgent, ReActConfig};
use yanshi_agents::tools::WorkflowTool;
use yanshi_core::FlowValue;
use yanshi_core::async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult};
use yanshi_core::flow::{Flow, GraphNode, NodeType};
use yanshi_memory::SessionMemory;
use yanshi_tools::ToolRegistry;

struct FormatSummaryNode;

#[async_trait]
impl AsyncNode for FormatSummaryNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let topic = inputs
      .get("topic")
      .and_then(|value| match value {
        FlowValue::Json(value) => value.as_str(),
        _ => None,
      })
      .unwrap_or("unknown topic");

    let mut outputs = HashMap::new();
    outputs.insert(
      "summary".to_string(),
      FlowValue::Json(json!(format!("workflow summary for {topic}"))),
    );
    Ok(outputs)
  }
}

fn child_workflow() -> Flow {
  Flow::new(vec![GraphNode {
    id: "format_summary".to_string(),
    node_type: NodeType::Standard(Arc::new(FormatSummaryNode)),
    dependencies: Vec::new(),
    input_mapping: None,
    run_if: None,
    initial_inputs: HashMap::new(),
  }])
}

fn parent_workflow(agent: ReActAgent) -> Flow {
  let agent_node = AgentNode::from_agent("hybrid_agent", agent);
  Flow::new(vec![GraphNode {
    id: "agent".to_string(),
    node_type: NodeType::Standard(Arc::new(agent_node)),
    dependencies: Vec::new(),
    input_mapping: None,
    run_if: None,
    initial_inputs: HashMap::from([(
      "message".to_string(),
      FlowValue::Json(json!("Summarize the hybrid runtime architecture.")),
    )]),
  }])
}

fn setup_mock_model() -> anyhow::Result<String> {
  let home = std::env::temp_dir().join(format!("yanshi-hybrid-example-{}", uuid::Uuid::new_v4()));
  fs::create_dir_all(home.join(".yanshi"))?;
  // SAFETY: this standalone example configures its process environment before
  // starting any concurrent work that reads Yanshi config.
  unsafe {
    std::env::set_var("HOME", &home);
  }

  let model = "mock-hybrid-model";
  fs::write(
    home.join(".yanshi").join("models.yml"),
    format!(
      r#"
models:
  {model}:
    vendor: mock
    type: text
    model_id: {model}
providers:
  mock:
    api_key_env: MOCK_API_KEY
"#
    ),
  )?;

  // SAFETY: this standalone example configures mock responses before running
  // the agent loop.
  unsafe {
    std::env::set_var(
      "YANSHI_MOCK_RESPONSES",
      serde_json::to_string(&vec![
        r#"{"thought":"delegate stable formatting to workflow","action":{"tool":"format_summary_workflow","params":{"topic":"hybrid DAG + agent runtime"}}}"#,
        r#"{"thought":"workflow returned a summary","answer":"Hybrid answer: workflow summary for hybrid DAG + agent runtime"}"#,
      ])?,
    );
  }

  Ok(model.to_string())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  let model = setup_mock_model()?;
  yanshi_llm::Yanshi::init().await?;

  let workflow_tool = WorkflowTool::new(
    "format_summary_workflow",
    "Run a deterministic child workflow that formats a summary.",
    child_workflow(),
    std::sync::Arc::new(yanshi_core::CoreFlowRunner::serial()),
  );

  let mut registry = ToolRegistry::new();
  registry.register(Arc::new(workflow_tool));

  let agent = ReActAgent::new(
    ReActConfig::new(model)
      .with_persona("Use workflow tools for deterministic formatting tasks.")
      .with_max_iterations(4),
    Box::new(SessionMemory::default_window()),
    Arc::new(registry),
  );

  let state = parent_workflow(agent).run().await?;
  let agent_outputs = state
    .get("agent")
    .expect("agent node output")
    .as_ref()
    .expect("agent node succeeded");

  println!("Agent response:");
  match &agent_outputs["response"] {
    FlowValue::Json(value) => println!("{value}"),
    other => println!("{other:?}"),
  }
  println!();
  println!("Agent runtime result:");
  if let FlowValue::Json(value) = &agent_outputs["agent_result"] {
    println!("{}", serde_json::to_string_pretty(value)?);
  }

  Ok(())
}

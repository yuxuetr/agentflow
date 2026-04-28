use crate::config::v2::{FlowDefinitionV2, NodeDefinitionV2};
use crate::executor::factory;
use crate::redaction::redact_cli_value;
use agentflow_core::{async_node::AsyncNodeInputs, flow::Flow, value::FlowValue};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::fs;
use std::time::Duration;

pub async fn execute(
  workflow_file: String,
  watch: bool,
  output: Option<String>,
  model: Option<String>,
  input: Vec<(String, String)>,
  dry_run: bool,
  timeout: String,
  max_retries: u32,
) -> Result<()> {
  if watch {
    bail!("--watch is not implemented yet; run without --watch or use workflow debug --dry-run");
  }

  println!(
    "🚀 Starting AgentFlow V2 workflow execution: {}",
    workflow_file
  );

  // 1. Read and parse the V2 workflow file
  let yaml_content = fs::read_to_string(&workflow_file)
    .with_context(|| format!("Failed to read workflow file: {}", workflow_file))?;
  let flow_def: FlowDefinitionV2 =
    serde_yaml::from_str(&yaml_content).with_context(|| "Failed to parse V2 workflow YAML.")?;

  println!("📄 Workflow '\'{}\'\' loaded.", flow_def.name);

  let flow = build_flow(&flow_def, model.as_deref())?;
  if let Some(model) = &model {
    println!("🤖 Model override: {}", model);
  }

  if dry_run {
    let order = flow
      .execution_order()
      .context("Failed to build workflow execution order")?;
    println!("\n🧪 Dry run complete. No nodes were executed.");
    println!("📅 Execution order:");
    for (idx, node_id) in order.iter().enumerate() {
      println!("  {}. {}", idx + 1, node_id);
    }
    return Ok(());
  }

  let initial_inputs = parse_inputs(input)?;
  if !initial_inputs.is_empty() {
    println!("📥 Loaded {} CLI input value(s).", initial_inputs.len());
  }

  let timeout_duration =
    parse_duration(&timeout).with_context(|| format!("Invalid --timeout value '{}'", timeout))?;

  // 3. Execute the flow
  println!("\n▶️  Running flow...");
  let start_time = std::time::Instant::now();
  let final_state = run_with_retries(flow, initial_inputs, timeout_duration, max_retries).await?;
  let duration = start_time.elapsed();
  println!("\n✅ Workflow completed in {:.2?}.", duration);

  // 4. Print or save the results
  let mut redacted_final_state =
    serde_json::to_value(&final_state).context("Failed to serialize final state for redaction.")?;
  redact_cli_value(&mut redacted_final_state);
  let final_state_json = serde_json::to_string_pretty(&redacted_final_state)
    .context("Failed to serialize redacted final state to JSON.")?;

  match output.as_deref() {
    Some("-") => {
      println!("{}", final_state_json);
    }
    Some(path) => {
      fs::write(path, &final_state_json)
        .with_context(|| format!("Failed to write workflow output to {}", path))?;
      println!("💾 Final state written to {}", path);
    }
    None => {
      println!("\n📊 Final State Pool:");
      println!("{}", final_state_json);
    }
  }

  Ok(())
}

fn build_flow(flow_def: &FlowDefinitionV2, model_override: Option<&str>) -> Result<Flow> {
  let mut flow = Flow::default();
  println!(
    "🔨 Building workflow graph with {} nodes...",
    flow_def.nodes.len()
  );
  for node_def in &flow_def.nodes {
    let mut graph_node = factory::create_graph_node(node_def)
      .with_context(|| format!("Failed to create graph node for id: {}", node_def.id))?;
    apply_model_override(node_def, &mut graph_node, model_override);
    flow.add_node(graph_node);
    println!(
      "  - Added node '{}' (type: '{}')",
      node_def.id, node_def.node_type
    );
  }

  Ok(flow)
}

fn apply_model_override(
  node_def: &NodeDefinitionV2,
  graph_node: &mut agentflow_core::flow::GraphNode,
  model_override: Option<&str>,
) {
  let Some(model) = model_override else {
    return;
  };
  if node_def.node_type == "llm" {
    graph_node.initial_inputs.insert(
      "model".to_string(),
      FlowValue::Json(Value::String(model.to_string())),
    );
  }
}

fn parse_inputs(input: Vec<(String, String)>) -> Result<AsyncNodeInputs> {
  let mut inputs = AsyncNodeInputs::new();
  for (key, raw_value) in input {
    if key.trim().is_empty() {
      bail!("Input key cannot be empty");
    }
    let value = parse_input_value(&raw_value);
    inputs.insert(key, FlowValue::Json(value));
  }
  Ok(inputs)
}

fn parse_input_value(raw_value: &str) -> Value {
  serde_json::from_str(raw_value).unwrap_or_else(|_| Value::String(raw_value.to_string()))
}

fn parse_duration(raw: &str) -> Result<Duration> {
  let raw = raw.trim();
  if raw.is_empty() {
    bail!("duration cannot be empty");
  }

  let (number, multiplier) = if let Some(value) = raw.strip_suffix("ms") {
    (value, 1)
  } else if let Some(value) = raw.strip_suffix('s') {
    (value, 1_000)
  } else if let Some(value) = raw.strip_suffix('m') {
    (value, 60_000)
  } else {
    (raw, 1_000)
  };

  let amount: u64 = number
    .trim()
    .parse()
    .with_context(|| "duration must be an integer followed by ms, s, or m")?;
  Ok(Duration::from_millis(amount.saturating_mul(multiplier)))
}

async fn run_with_retries(
  flow: Flow,
  initial_inputs: AsyncNodeInputs,
  timeout_duration: Duration,
  max_retries: u32,
) -> Result<std::collections::HashMap<String, agentflow_core::async_node::AsyncNodeResult>> {
  let attempts = max_retries.saturating_add(1);
  let mut last_error = None;

  for attempt in 1..=attempts {
    if attempts > 1 {
      println!("🔁 Workflow attempt {}/{}", attempt, attempts);
    }

    let run = flow.execute_from_inputs(initial_inputs.clone());
    match tokio::time::timeout(timeout_duration, run).await {
      Ok(Ok(state)) => return Ok(state),
      Ok(Err(err)) => {
        last_error = Some(
          anyhow::Error::new(err)
            .context(format!("workflow attempt {}/{} failed", attempt, attempts)),
        );
      }
      Err(_) => {
        last_error = Some(anyhow::anyhow!(
          "workflow attempt {}/{} timed out after {:?}",
          attempt,
          attempts,
          timeout_duration
        ));
      }
    }

    if attempt < attempts {
      println!("⚠️  Attempt {} failed; retrying...", attempt);
    }
  }

  Err(last_error.unwrap_or_else(|| anyhow::anyhow!("workflow execution failed")))
}

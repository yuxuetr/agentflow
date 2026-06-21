pub mod factory;
pub mod multi_agent;
pub mod shell;

#[cfg(feature = "plugin")]
pub mod plugin;

use crate::config::{
  schema::{WorkflowValidationReport, validate_flow_definition},
  v2::{FlowDefinitionV2, NodeDefinitionV2},
};
use agentflow_core::{
  flow::{Flow, GraphNode},
  value::FlowValue,
};
use anyhow::{Context, Result, bail};
use serde_json::Value;

/// Parse and validate config-first workflow YAML.
pub fn parse_workflow_definition(yaml_content: &str) -> Result<FlowDefinitionV2> {
  let flow_def: FlowDefinitionV2 =
    serde_yaml::from_str(yaml_content).context("Failed to parse V2 workflow YAML.")?;
  let report = validate_flow_definition(&flow_def);
  if !report.is_valid() {
    bail!(format_validation_error(&flow_def, &report));
  }
  Ok(flow_def)
}

/// Build a runnable `Flow` from config-first workflow YAML.
pub fn build_flow_from_yaml(yaml_content: &str, model_override: Option<&str>) -> Result<Flow> {
  let flow_def = parse_workflow_definition(yaml_content)?;
  build_flow_from_definition(&flow_def, model_override)
}

/// Build a runnable `Flow` from a parsed config-first definition.
pub fn build_flow_from_definition(
  flow_def: &FlowDefinitionV2,
  model_override: Option<&str>,
) -> Result<Flow> {
  let mut flow = Flow::default();
  for node_def in &flow_def.nodes {
    let mut graph_node = factory::create_graph_node(node_def)
      .with_context(|| format!("Failed to create graph node for id: {}", node_def.id))?;
    apply_model_override(node_def, &mut graph_node, model_override);
    flow.add_node(graph_node);
  }
  Ok(flow)
}

/// Apply the CLI/server model override to node kinds that invoke agents or LLMs.
pub fn apply_model_override(
  node_def: &NodeDefinitionV2,
  graph_node: &mut GraphNode,
  model_override: Option<&str>,
) {
  let Some(model) = model_override else {
    return;
  };
  if matches!(
    node_def.node_type.as_str(),
    "llm" | "skill_agent" | "agent" | "multi_agent"
  ) {
    graph_node.initial_inputs.insert(
      "model".to_string(),
      FlowValue::Json(Value::String(model.to_string())),
    );
  }
}

fn format_validation_error(
  flow_def: &FlowDefinitionV2,
  report: &WorkflowValidationReport,
) -> String {
  let mut message = format!(
    "workflow '{}' failed schema validation with {} issue(s)",
    flow_def.name,
    report.issues.len()
  );
  for issue in &report.issues {
    message.push_str("\n- ");
    message.push_str(issue);
  }
  message
}

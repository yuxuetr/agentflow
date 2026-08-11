//! W4.3b — distributed run execution: wiring `DistributedDagScheduler`
//! into `POST /v1/runs` via an opt-in `execution_mode: "distributed"`.
//!
//! See `docs/DISTRIBUTED.md` § Planned Control-Plane Flow for the
//! high-level design this implements, and the approved plan this shipped
//! against for the full research/decision record.

use agentflow_config::config::v2::FlowDefinitionV2;

use crate::error::ApiError;

/// Node types `agentflow-worker::execute_supported_node_payload` actually
/// executes today (`agentflow-worker/src/lib.rs`). Anything outside this
/// set — `skill_agent`/`multi_agent`, `plugin`, `shell`, media/RAG nodes,
/// `while`/`map` loop constructs — has no distributed execution path;
/// expanding worker payload coverage is separate future work, not
/// silently degraded here.
const DISTRIBUTED_SUPPORTED_NODE_TYPES: &[&str] =
  &["template", "file", "mock", "llm", "http", "mcp", "agent"];

/// Reject (rather than silently mis-execute) workflow shapes
/// `DistributedDagScheduler` can't correctly express, before any DB row
/// or worker task is created:
///
/// - a declared `inputs:` block — `DistributedDagScheduler::gather_inputs`
///   only resolves `input_mapping` values shaped `nodes.<id>.outputs.<key>`
///   plus each node's own literal `parameters`; there is no path for
///   `FlowDefinitionV2.inputs` (the `T3.2` default-filling mechanism
///   `flow_execute` uses for in-process runs) to reach a node at all.
/// - any node with `run_if` set — `DistributedDagScheduler::is_ready`
///   never reads it, so a conditional node would execute unconditionally
///   instead of being silently skipped as the in-process executor would.
/// - any node whose `type` isn't in [`DISTRIBUTED_SUPPORTED_NODE_TYPES`].
pub fn validate_distributed_flow(flow: &FlowDefinitionV2) -> Result<(), ApiError> {
  if !flow.inputs.is_empty() {
    return Err(ApiError::BadRequest(
      "distributed execution does not support workflows with a declared `inputs:` block yet \
       (DistributedDagScheduler has no path for default-filled inputs to reach a node) — \
       remove `inputs:` or submit with the default in-process execution_mode"
        .to_string(),
    ));
  }
  for node in &flow.nodes {
    if node.run_if.is_some() {
      return Err(ApiError::BadRequest(format!(
        "distributed execution does not support conditional nodes yet (`run_if` on node '{}' \
         would execute unconditionally instead of being evaluated) — remove `run_if` or submit \
         with the default in-process execution_mode",
        node.id
      )));
    }
    if !DISTRIBUTED_SUPPORTED_NODE_TYPES.contains(&node.node_type.as_str()) {
      return Err(ApiError::BadRequest(format!(
        "distributed execution does not support node type '{}' (node '{}') — the worker \
         supports {:?}; submit with the default in-process execution_mode instead",
        node.node_type, node.id, DISTRIBUTED_SUPPORTED_NODE_TYPES
      )));
    }
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use agentflow_config::config::v2::NodeDefinitionV2;
  use std::collections::HashMap;

  fn node(id: &str, node_type: &str) -> NodeDefinitionV2 {
    NodeDefinitionV2 {
      id: id.to_string(),
      node_type: node_type.to_string(),
      dependencies: Vec::new(),
      input_mapping: HashMap::new(),
      run_if: None,
      timeout_ms: None,
      max_retries: None,
      parameters: Default::default(),
    }
  }

  fn flow(nodes: Vec<NodeDefinitionV2>) -> FlowDefinitionV2 {
    FlowDefinitionV2 {
      name: "test".to_string(),
      inputs: HashMap::new(),
      nodes,
    }
  }

  #[test]
  fn accepts_a_clean_template_to_http_chain() {
    let mut render = node("render", "template");
    let mut fetch = node("fetch", "http");
    fetch.dependencies = vec!["render".to_string()];
    fetch.input_mapping.insert(
      "body".to_string(),
      "{{nodes.render.outputs.text}}".to_string(),
    );
    render.dependencies = Vec::new();
    assert!(validate_distributed_flow(&flow(vec![render, fetch])).is_ok());
  }

  #[test]
  fn rejects_a_declared_inputs_block() {
    let mut f = flow(vec![node("render", "template")]);
    f.inputs.insert(
      "topic".to_string(),
      agentflow_config::config::v2::InputDefinitionV2 {
        description: None,
        required: false,
        default: None,
      },
    );
    let err = validate_distributed_flow(&f).expect_err("must reject declared inputs");
    assert!(matches!(err, ApiError::BadRequest(msg) if msg.contains("inputs:")));
  }

  #[test]
  fn rejects_a_run_if_node() {
    let mut n = node("maybe", "template");
    n.run_if = Some("{{ nodes.render.outputs.ok }}".to_string());
    let err = validate_distributed_flow(&flow(vec![n])).expect_err("must reject run_if");
    assert!(matches!(err, ApiError::BadRequest(msg) if msg.contains("run_if")));
  }

  #[test]
  fn rejects_an_unsupported_node_type() {
    let err = validate_distributed_flow(&flow(vec![node("agent1", "skill_agent")]))
      .expect_err("must reject unsupported node type");
    assert!(matches!(err, ApiError::BadRequest(msg) if msg.contains("skill_agent")));
  }

  #[test]
  fn accepts_every_worker_supported_node_type() {
    for node_type in DISTRIBUTED_SUPPORTED_NODE_TYPES {
      let f = flow(vec![node("n", node_type)]);
      assert!(
        validate_distributed_flow(&f).is_ok(),
        "expected '{node_type}' to be accepted"
      );
    }
  }
}

//! `PluginNode` — adapter that exposes a plugin-declared node type as an
//! ordinary [`AsyncNode`] inside an AgentFlow DAG.

use crate::async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult};
use crate::error::AgentFlowError;
use crate::plugin::host::PluginHost;
use async_trait::async_trait;
use std::sync::Arc;

/// Adapter that turns a plugin-declared node type into an `AsyncNode`. One
/// instance handles one workflow node id; the underlying [`PluginHost`] is
/// shared across nodes from the same plugin.
#[derive(Debug, Clone)]
pub struct PluginNode {
  /// Workflow-side node id (e.g. the `id` field in the YAML).
  pub name: String,
  /// Node type as declared by the plugin during initialize.
  pub plugin_node_type: String,
  pub host: Arc<PluginHost>,
}

impl PluginNode {
  pub fn new(
    name: impl Into<String>,
    plugin_node_type: impl Into<String>,
    host: Arc<PluginHost>,
  ) -> Self {
    Self {
      name: name.into(),
      plugin_node_type: plugin_node_type.into(),
      host,
    }
  }
}

#[async_trait]
impl AsyncNode for PluginNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let result = self
      .host
      .execute_node(&self.plugin_node_type, inputs.clone())
      .await
      .map_err(AgentFlowError::from)?;
    Ok(result.outputs)
  }
}

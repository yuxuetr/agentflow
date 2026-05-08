//! `PluginWorkflowNode` — adapter that exposes a subprocess plugin to a YAML
//! workflow as `type: plugin`.
//!
//! The wrapper holds the manifest path + plugin-declared node type. On first
//! `execute` it lazily spawns the plugin via [`PluginHost::load`] and caches
//! the resulting [`Arc<PluginHost>`] in a process-wide table keyed by the
//! canonicalized manifest path. Subsequent workflow nodes pointing at the
//! same manifest reuse the same subprocess, which keeps initialize cost paid
//! once per `agentflow workflow run` invocation.
//!
//! See `docs/PLUGIN_DESIGN.md` §6 for the wire protocol and §6.4 for the
//! workflow integration covered here.

use agentflow_core::{
  async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
  error::AgentFlowError,
  plugin::PluginHost,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

/// Process-wide cache of loaded plugin hosts, keyed by canonicalized manifest
/// path. Multiple workflow nodes pointing at the same `plugin.toml` share a
/// single subprocess so we pay the spawn + handshake cost exactly once.
fn host_cache() -> &'static Mutex<HashMap<PathBuf, Arc<PluginHost>>> {
  static CELL: OnceLock<Mutex<HashMap<PathBuf, Arc<PluginHost>>>> = OnceLock::new();
  CELL.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Clone)]
pub struct PluginWorkflowNode {
  pub workflow_node_id: String,
  pub manifest_path: PathBuf,
  pub plugin_node_type: String,
}

impl PluginWorkflowNode {
  pub fn new(
    workflow_node_id: impl Into<String>,
    manifest_path: PathBuf,
    plugin_node_type: impl Into<String>,
  ) -> Self {
    Self {
      workflow_node_id: workflow_node_id.into(),
      manifest_path,
      plugin_node_type: plugin_node_type.into(),
    }
  }

  async fn ensure_loaded(&self) -> Result<Arc<PluginHost>, AgentFlowError> {
    let canonical =
      self
        .manifest_path
        .canonicalize()
        .map_err(|err| AgentFlowError::NodeInputError {
          message: format!(
            "plugin '{}': manifest path '{}' not accessible: {}",
            self.workflow_node_id,
            self.manifest_path.display(),
            err
          ),
        })?;
    let mut cache = host_cache().lock().await;
    if let Some(existing) = cache.get(&canonical) {
      return Ok(existing.clone());
    }
    let host =
      PluginHost::load(&canonical)
        .await
        .map_err(|err| AgentFlowError::AsyncExecutionError {
          message: format!(
            "plugin '{}': failed to load manifest '{}': {}",
            self.workflow_node_id,
            canonical.display(),
            err
          ),
        })?;
    let arc = Arc::new(host);
    cache.insert(canonical, arc.clone());
    Ok(arc)
  }
}

#[async_trait]
impl AsyncNode for PluginWorkflowNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let host = self.ensure_loaded().await?;
    let result = host
      .execute_node(&self.plugin_node_type, inputs.clone())
      .await
      .map_err(AgentFlowError::from)?;
    Ok(result.outputs)
  }
}

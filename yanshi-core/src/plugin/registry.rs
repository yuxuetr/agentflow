//! `PluginRegistry` — holds loaded `PluginHost`s and maps declared node types
//! back to the plugin that owns them.

use crate::plugin::host::{PluginError, PluginHost};
use crate::plugin::node::PluginNode;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Default)]
pub struct PluginRegistry {
  plugins: Mutex<HashMap<String, Arc<PluginHost>>>,
  /// node_type → plugin_name. A node type may only be declared by one plugin
  /// at a time; conflicts are rejected at registration.
  node_index: Mutex<HashMap<String, String>>,
}

impl PluginRegistry {
  pub fn new() -> Self {
    Self::default()
  }

  /// Register a loaded plugin and index every node type it declared. Returns
  /// `DuplicateNodeType` if any node type collides with an existing plugin.
  pub async fn register(&self, host: Arc<PluginHost>) -> Result<(), PluginError> {
    let plugin_name = host.manifest().plugin.name.clone();
    let declared = host.declared_node_types();

    let mut plugins = self.plugins.lock().await;
    let mut index = self.node_index.lock().await;

    for node_type in &declared {
      if let Some(existing) = index.get(node_type)
        && existing != &plugin_name
      {
        return Err(PluginError::DuplicateNodeType {
          plugin: existing.clone(),
          node_type: node_type.clone(),
        });
      }
    }

    for node_type in declared {
      index.insert(node_type, plugin_name.clone());
    }
    plugins.insert(plugin_name, host);
    Ok(())
  }

  /// Build a [`PluginNode`] for a given workflow node id and plugin-declared
  /// node type.
  pub async fn create_node(
    &self,
    node_type: &str,
    workflow_node_id: &str,
  ) -> Result<PluginNode, PluginError> {
    let index = self.node_index.lock().await;
    let plugin_name = index
      .get(node_type)
      .cloned()
      .ok_or_else(|| PluginError::UnknownNodeType(node_type.to_string()))?;
    drop(index);

    let plugins = self.plugins.lock().await;
    let host = plugins
      .get(&plugin_name)
      .cloned()
      .ok_or_else(|| PluginError::UnknownNodeType(node_type.to_string()))?;
    Ok(PluginNode::new(workflow_node_id, node_type, host))
  }

  /// Names of plugins currently registered.
  pub async fn plugin_names(&self) -> Vec<String> {
    self.plugins.lock().await.keys().cloned().collect()
  }

  /// Node types currently routable through the registry.
  pub async fn node_types(&self) -> Vec<String> {
    self.node_index.lock().await.keys().cloned().collect()
  }

  /// Shut down every registered plugin. Returns a per-plugin result; never
  /// fails the whole call so a stuck plugin does not block the rest.
  pub async fn shutdown_all(&self) -> Vec<(String, Result<(), PluginError>)> {
    let drained: Vec<(String, Arc<PluginHost>)> = {
      let mut plugins = self.plugins.lock().await;
      let mut index = self.node_index.lock().await;
      index.clear();
      plugins.drain().collect()
    };
    let mut results = Vec::with_capacity(drained.len());
    for (name, host) in drained {
      let res = host.shutdown().await;
      results.push((name, res));
    }
    results
  }
}

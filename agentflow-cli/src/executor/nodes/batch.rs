// Batch processing node implementation (placeholder)
use agentflow_core::Result;
use async_trait::async_trait;
use serde_json::Value;
// Removed unused Arc import

use agentflow_core::{AsyncNode, SharedState};
// Batch processing node implementation (placeholder)
use crate::config::workflow::{BatchNodeConfig, NodeConfig, NodeDefinition};

pub struct BatchNode {
  name: String,
  config: BatchNodeConfig,
}

impl BatchNode {
  pub async fn new(node_def: &NodeDefinition) -> Result<Self> {
    let config = match &node_def.config {
      NodeConfig::Batch(batch_config) => batch_config.clone(),
      _ => {
        return Err(agentflow_core::AgentFlowError::Generic(anyhow::anyhow!(
          "Invalid configuration for Batch node"
        )))
      }
    };

    Ok(Self {
      name: node_def.name.clone(),
      config,
    })
  }
}

#[async_trait]
impl AsyncNode for BatchNode {
  async fn prep_async(&self, _shared_state: &SharedState) -> Result<Value> {
    Ok(Value::String("batch_prep".to_string()))
  }

  async fn exec_async(&self, _prep_result: Value) -> Result<Value> {
    // TODO: Implement batch processing
    Ok(Value::String("batch_exec".to_string()))
  }

  async fn post_async(
    &self,
    shared_state: &SharedState,
    _prep_result: Value,
    _exec_result: Value,
  ) -> Result<Option<String>> {
    shared_state.insert(format!("{}_executed", self.name), Value::Bool(true));
    Ok(None)
  }
}

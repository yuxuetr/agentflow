// HTTP request node implementation (placeholder)
use agentflow_core::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::config::workflow::{HttpNodeConfig, NodeConfig, NodeDefinition};
use agentflow_core::{AsyncNode, SharedState};

pub struct HttpNode {
  name: String,
  config: HttpNodeConfig,
}

impl HttpNode {
  pub fn new(node_def: &NodeDefinition) -> Result<Self> {
    let config = match &node_def.config {
      NodeConfig::Http(http_config) => http_config.clone(),
      _ => {
        return Err(agentflow_core::AgentFlowError::Generic(anyhow::anyhow!(
          "Invalid configuration for HTTP node"
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
impl AsyncNode for HttpNode {
  async fn prep_async(&self, _shared_state: &SharedState) -> Result<Value> {
    Ok(Value::String("http_prep".to_string()))
  }

  async fn exec_async(&self, _prep_result: Value) -> Result<Value> {
    // TODO: Implement HTTP requests
    Ok(Value::String("http_exec".to_string()))
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

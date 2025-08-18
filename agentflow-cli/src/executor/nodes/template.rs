// Template node implementation
use async_trait::async_trait;
use serde_json::Value;
// Removed unused Arc import

use crate::config::workflow::{NodeConfig, NodeDefinition, TemplateNodeConfig};
use agentflow_core::{AsyncNode, Result, SharedState};

pub struct TemplateNode {
  name: String,
  config: TemplateNodeConfig,
}

impl TemplateNode {
  pub fn new(node_def: &NodeDefinition) -> Result<Self> {
    let config = match &node_def.config {
      NodeConfig::Template(template_config) => template_config.clone(),
      _ => {
        return Err(agentflow_core::AgentFlowError::Generic(anyhow::anyhow!(
          "Invalid configuration for Template node"
        )))
      }
    };

    Ok(Self {
      name: node_def.name.clone(),
      config,
    })
  }

  fn expand_template(&self, template: &str, shared_state: &SharedState) -> String {
    let mut result = template.to_string();

    // Replace template variables with shared state values
    for (key, value) in shared_state.iter() {
      let placeholder = format!("{{{{{}}}}}", key);
      let replacement = match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => serde_json::to_string(&value).unwrap_or_default(),
      };
      result = result.replace(&placeholder, &replacement);
    }

    result
  }
}

#[async_trait]
impl AsyncNode for TemplateNode {
  async fn prep_async(&self, shared_state: &SharedState) -> Result<Value> {
    let expanded_template = self.expand_template(&self.config.template, shared_state);

    let prep_data = serde_json::json!({
      "expanded_template": expanded_template,
      "node_name": self.name
    });

    Ok(prep_data)
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value> {
    let expanded_template = prep_result["expanded_template"].as_str().ok_or_else(|| {
      agentflow_core::AgentFlowError::Generic(anyhow::anyhow!(
        "Missing expanded_template in prep result"
      ))
    })?;

    let exec_result = serde_json::json!({
      "rendered": expanded_template,
      "format": self.config.format.clone().unwrap_or_default(),
    });

    Ok(exec_result)
  }

  async fn post_async(
    &self,
    shared_state: &SharedState,
    _prep_result: Value,
    exec_result: Value,
  ) -> Result<Option<String>> {
    let rendered = exec_result["rendered"].as_str().ok_or_else(|| {
      agentflow_core::AgentFlowError::Generic(anyhow::anyhow!("Missing rendered in exec result"))
    })?;

    // Store rendered template in shared state
    shared_state.insert(
      format!("{}_rendered", self.name),
      Value::String(rendered.to_string()),
    );
    shared_state.insert(format!("{}_executed", self.name), Value::Bool(true));

    Ok(None)
  }
}

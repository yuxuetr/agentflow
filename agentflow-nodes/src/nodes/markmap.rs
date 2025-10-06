use crate::common::utils::flow_value_to_string;
use agentflow_core::{
    async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
    error::AgentFlowError,
    value::FlowValue,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkMapConfig {
  pub api_url: Option<String>,
  pub title: Option<String>,
  pub theme: Option<String>,
  pub color_freeze_level: Option<u8>,
  pub initial_expand_level: Option<i8>,
  pub max_width: Option<u32>,
  pub timeout_seconds: Option<u64>,
}

impl Default for MarkMapConfig {
  fn default() -> Self {
    Self {
      api_url: Some("https://markmap-api.jinpeng-ti.workers.dev".to_string()),
      title: Some("Mind Map".to_string()),
      theme: Some("light".to_string()),
      color_freeze_level: Some(6),
      initial_expand_level: Some(-1),
      max_width: Some(200),
      timeout_seconds: Some(30),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkMapNode {
  pub name: String,
  pub markdown: String,
  pub config: Option<MarkMapConfig>,
  pub save_to_file: Option<String>,
}

impl MarkMapNode {
  pub fn new(name: impl Into<String>, markdown: impl Into<String>) -> Self {
    Self {
      name: name.into(),
      markdown: markdown.into(),
      config: Some(MarkMapConfig::default()),
      save_to_file: None,
    }
  }

  pub fn with_file_output(mut self, path: impl Into<String>) -> Self {
    self.save_to_file = Some(path.into());
    self
  }

  pub fn with_config(mut self, config: MarkMapConfig) -> Self {
    self.config = Some(config);
    self
  }

  fn resolve_markdown(&self, inputs: &AsyncNodeInputs) -> Result<String, AgentFlowError> {
    let mut resolved = self.markdown.clone();
    
    for (key, value) in inputs {
      let placeholder = format!("{{{{{}}}}}", key);
      if resolved.contains(&placeholder) {
        resolved = resolved.replace(&placeholder, &flow_value_to_string(value));
      }
    }
    
    Ok(resolved)
  }

  fn build_request_payload(&self, markdown: &str) -> Value {
    let default_config = MarkMapConfig::default();
    let config = self.config.as_ref().unwrap_or(&default_config);
    
    let mut payload = json!({
      "markdown": markdown,
    });

    if let Some(title) = &config.title {
      payload["title"] = json!(title);
    }
    if let Some(theme) = &config.theme {
      payload["theme"] = json!(theme);
    }
    if let Some(level) = config.color_freeze_level {
      payload["colorFreezeLevel"] = json!(level);
    }
    if let Some(level) = config.initial_expand_level {
      payload["initialExpandLevel"] = json!(level);
    }
    if let Some(width) = config.max_width {
      payload["maxWidth"] = json!(width);
    }

    payload
  }

  async fn save_html_to_file(&self, html: &str) -> Result<(), AgentFlowError> {
    if let Some(file_path) = &self.save_to_file {
      tokio::fs::write(file_path, html).await.map_err(|e| {
        AgentFlowError::AsyncExecutionError {
          message: format!("Failed to save HTML to file {}: {}", file_path, e),
        }
      })?;
    }
    Ok(())
  }
}

#[async_trait]
impl AsyncNode for MarkMapNode {
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        let resolved_markdown = self.resolve_markdown(inputs)?;
        let payload = self.build_request_payload(&resolved_markdown);

        let default_config = MarkMapConfig::default();
        let config = self.config.as_ref().unwrap_or(&default_config);
        let api_url = config.api_url.as_ref()
            .ok_or_else(|| AgentFlowError::ConfigurationError {
                message: "API URL not configured".to_string(),
            })?;

        let timeout_duration = std::time::Duration::from_secs(
            config.timeout_seconds.unwrap_or(30)
        );
        let client = Client::builder()
            .timeout(timeout_duration)
            .build()
            .map_err(|e| AgentFlowError::ConfigurationError {
                message: format!("Failed to create HTTP client: {}", e),
            })?;

        let response = client
            .post(format!("{}/api/render", api_url))
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| AgentFlowError::AsyncExecutionError {
                message: format!("Failed to call markmap API: {}", e),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentFlowError::AsyncExecutionError {
                message: format!("API request failed with status {}: {}", status, error_text),
            });
        }

        let response_text = response.text().await.map_err(|e| AgentFlowError::AsyncExecutionError {
            message: format!("Failed to read response body: {}", e),
        })?;

        self.save_html_to_file(&response_text).await?;

        let mut outputs = HashMap::new();
        outputs.insert("html".to_string(), FlowValue::Json(Value::String(response_text)));

        Ok(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_markmap_node_execution() {
        let node = MarkMapNode::new("test_map", "# {{title}}\n## Item 1\n## Item 2");
        let mut inputs = AsyncNodeInputs::new();
        inputs.insert("title".to_string(), FlowValue::Json(json!("My Test Map")));

        let result = node.execute(&inputs).await;
        assert!(result.is_ok());
    }
}
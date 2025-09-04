//! MarkMap Node Implementation
//!
//! This module provides the MarkMapNode which converts Markdown content into
//! interactive mind map HTML files using the markmap-api service.

use crate::error::NodeError;
use agentflow_core::{AsyncNode, Result, SharedState};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Configuration for MarkMap node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkMapConfig {
  /// API base URL for markmap service
  pub api_url: Option<String>,
  /// Title for the generated mind map
  pub title: Option<String>,
  /// Theme for the mind map: "light", "dark", or "auto"
  pub theme: Option<String>,
  /// Color freeze level (0-10)
  pub color_freeze_level: Option<u8>,
  /// Initial expand level (-1 to 10)
  pub initial_expand_level: Option<i8>,
  /// Maximum width for nodes in pixels
  pub max_width: Option<u32>,
  /// Request timeout in seconds
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

/// MarkMap Node for converting Markdown to interactive mind map HTML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkMapNode {
  /// Node identifier
  pub name: String,
  /// Markdown content to convert (supports template variables)
  pub markdown: String,
  /// Configuration options
  pub config: Option<MarkMapConfig>,
  /// Where to store the result in shared state
  pub output_key: Option<String>,
  /// Whether to save HTML to file
  pub save_to_file: Option<String>,
}

/// Response structure from the markmap API
#[derive(Debug, Deserialize)]
struct MarkMapResponse {
  #[serde(rename = "html")]
  html_content: String,
}

impl MarkMapNode {
  /// Create a new MarkMap node with basic configuration
  pub fn new(name: impl Into<String>, markdown: impl Into<String>) -> Self {
    Self {
      name: name.into(),
      markdown: markdown.into(),
      config: Some(MarkMapConfig::default()),
      output_key: None,
      save_to_file: None,
    }
  }

  /// Set the output key for storing results in shared state
  pub fn with_output_key(mut self, key: impl Into<String>) -> Self {
    self.output_key = Some(key.into());
    self
  }

  /// Set the file path to save the HTML output
  pub fn with_file_output(mut self, path: impl Into<String>) -> Self {
    self.save_to_file = Some(path.into());
    self
  }

  /// Set custom configuration
  pub fn with_config(mut self, config: MarkMapConfig) -> Self {
    self.config = Some(config);
    self
  }

  /// Resolve template variables in markdown content
  fn resolve_markdown(&self, shared: &SharedState) -> Result<String> {
    let mut resolved = self.markdown.clone();
    
    // Simple template variable resolution - replace {{key}} with values from shared state
    for (key, value) in shared.iter() {
      let placeholder = format!("{{{{{}}}}}", key);
      if resolved.contains(&placeholder) {
        let replacement = match value {
          Value::String(s) => s.clone(),
          _ => value.to_string(),
        };
        resolved = resolved.replace(&placeholder, &replacement);
      }
    }
    
    Ok(resolved)
  }

  /// Build request payload for markmap API
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

  /// Save HTML content to file if specified
  async fn save_html_to_file(&self, html: &str) -> Result<()> {
    if let Some(file_path) = &self.save_to_file {
      tokio::fs::write(file_path, html).await.map_err(|e| {
        NodeError::FileOperationError {
          message: format!("Failed to save HTML to file {}: {}", file_path, e),
        }
      })?;
    }
    Ok(())
  }
}

#[async_trait]
impl AsyncNode for MarkMapNode {
  async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
    // Resolve markdown template variables
    let resolved_markdown = self.resolve_markdown(shared)?;
    
    // Build request payload
    let payload = self.build_request_payload(&resolved_markdown);
    
    Ok(json!({
      "payload": payload,
      "resolved_markdown": resolved_markdown
    }))
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value> {
    let default_config = MarkMapConfig::default();
    let config = self.config.as_ref().unwrap_or(&default_config);
    let api_url = config.api_url.as_ref()
      .ok_or_else(|| NodeError::ConfigurationError {
        message: "API URL not configured".to_string(),
      })?;

    let payload = prep_result["payload"].clone();
    
    // Create HTTP client with timeout
    let timeout_duration = std::time::Duration::from_secs(
      config.timeout_seconds.unwrap_or(30)
    );
    let client = Client::builder()
      .timeout(timeout_duration)
      .build()
      .map_err(|e| NodeError::HttpError {
        message: format!("Failed to create HTTP client: {}", e),
      })?;

    // Make request to markmap API
    let response = client
      .post(format!("{}/api/render", api_url))
      .header("Content-Type", "application/json")
      .json(&payload)
      .send()
      .await
      .map_err(|e| NodeError::HttpError {
        message: format!("Failed to call markmap API: {}", e),
      })?;

    if !response.status().is_success() {
      let status = response.status();
      let error_text = response.text().await.unwrap_or_default();
      return Err(NodeError::HttpError {
        message: format!("API request failed with status {}: {}", status, error_text),
      }.into());
    }

    // Parse response
    let response_text = response.text().await.map_err(|e| NodeError::HttpError {
      message: format!("Failed to read response body: {}", e),
    })?;

    // The API returns HTML directly, not JSON
    Ok(json!({
      "html": response_text,
      "original_markdown": prep_result["resolved_markdown"]
    }))
  }

  async fn post_async(
    &self,
    shared: &SharedState,
    _prep_result: Value,
    exec_result: Value,
  ) -> Result<Option<String>> {
    let html = exec_result["html"].as_str()
      .ok_or_else(|| NodeError::ExecutionError {
        message: "No HTML content in execution result".to_string(),
      })?;

    // Save to file if specified
    self.save_html_to_file(html).await?;

    // Store result in shared state if key is specified
    if let Some(output_key) = &self.output_key {
      shared.insert(output_key.clone(), json!({
        "html": html,
        "original_markdown": exec_result["original_markdown"]
      }));
    }

    // Store in default output key
    shared.insert(format!("{}_output", self.name), json!({
      "html": html,
      "node_name": self.name,
      "timestamp": chrono::Utc::now().to_rfc3339(),
    }));

    Ok(None)
  }

  fn get_node_id(&self) -> Option<String> {
    Some(format!("markmap_{}", self.name))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use agentflow_core::SharedState;

  #[tokio::test]
  async fn test_markmap_node_creation() {
    let node = MarkMapNode::new("test_map", "# Test\n## Item 1\n## Item 2")
      .with_output_key("mind_map_result")
      .with_file_output("output.html");

    assert_eq!(node.name, "test_map");
    assert!(node.markdown.contains("# Test"));
    assert_eq!(node.output_key, Some("mind_map_result".to_string()));
    assert_eq!(node.save_to_file, Some("output.html".to_string()));
  }

  #[tokio::test]
  async fn test_template_resolution() {
    let shared = SharedState::new();
    shared.insert("project_name".to_string(), json!("AgentFlow"));
    shared.insert("version".to_string(), json!("1.0.0"));

    let node = MarkMapNode::new(
      "template_test",
      "# {{project_name}}\n## Version {{version}}\n## Features"
    );

    let resolved = node.resolve_markdown(&shared).unwrap();
    assert!(resolved.contains("# AgentFlow"));
    assert!(resolved.contains("## Version 1.0.0"));
  }

  #[tokio::test]
  async fn test_request_payload_building() {
    let config = MarkMapConfig {
      title: Some("Test Map".to_string()),
      theme: Some("dark".to_string()),
      color_freeze_level: Some(5),
      initial_expand_level: Some(2),
      max_width: Some(300),
      ..Default::default()
    };

    let node = MarkMapNode::new("test", "# Test").with_config(config);
    let payload = node.build_request_payload("# Test Content");

    assert_eq!(payload["markdown"], "# Test Content");
    assert_eq!(payload["title"], "Test Map");
    assert_eq!(payload["theme"], "dark");
    assert_eq!(payload["colorFreezeLevel"], 5);
    assert_eq!(payload["initialExpandLevel"], 2);
    assert_eq!(payload["maxWidth"], 300);
  }

  #[test]
  fn test_default_config() {
    let config = MarkMapConfig::default();
    assert!(config.api_url.is_some());
    assert_eq!(config.theme, Some("light".to_string()));
    assert_eq!(config.color_freeze_level, Some(6));
    assert_eq!(config.initial_expand_level, Some(-1));
  }

  #[tokio::test]
  async fn test_node_id_generation() {
    let node = MarkMapNode::new("my_mindmap", "# Test");
    assert_eq!(node.get_node_id(), Some("markmap_my_mindmap".to_string()));
  }
}

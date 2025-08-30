use crate::{AsyncNode, SharedState};
use agentflow_core::{AgentFlowError, Result};
use async_trait::async_trait;
use serde_json::Value;
use std::path::Path;

/// File I/O node for reading and writing files
#[derive(Debug, Clone)]
pub struct FileNode {
  pub name: String,
  pub operation: String, // "read", "write", "append"
  pub path: String,
  pub content: Option<String>, // For write/append operations
  pub encoding: String,
}

impl FileNode {
  pub fn new(name: &str, operation: &str, path: &str) -> Self {
    Self {
      name: name.to_string(),
      operation: operation.to_string(),
      path: path.to_string(),
      content: None,
      encoding: "utf-8".to_string(),
    }
  }

  pub fn with_content(mut self, content: &str) -> Self {
    self.content = Some(content.to_string());
    self
  }

  pub fn with_encoding(mut self, encoding: &str) -> Self {
    self.encoding = encoding.to_string();
    self
  }
}

#[async_trait]
impl AsyncNode for FileNode {
  async fn prep_async(&self, _shared: &SharedState) -> Result<Value> {
    // TODO: Resolve template variables in path and content
    Ok(serde_json::json!({
        "operation": self.operation,
        "path": self.path,
        "content": self.content,
        "encoding": self.encoding
    }))
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value> {
    let operation = prep_result["operation"].as_str().unwrap_or(&self.operation);
    let path = prep_result["path"].as_str().unwrap_or(&self.path);
    let encoding = prep_result["encoding"].as_str().unwrap_or(&self.encoding);

    match operation {
      "read" => {
        // Read file contents
        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
          AgentFlowError::AsyncExecutionError {
            message: format!("Failed to read file '{}': {}", path, e),
          }
        })?;

        Ok(serde_json::json!({
            "operation": "read",
            "path": path,
            "content": content,
            "size": content.len(),
            "encoding": encoding
        }))
      }
      "write" => {
        // Write content to file
        let content =
          prep_result["content"]
            .as_str()
            .ok_or_else(|| AgentFlowError::AsyncExecutionError {
              message: "Write operation requires content".to_string(),
            })?;

        // Ensure parent directory exists
        if let Some(parent) = Path::new(path).parent() {
          tokio::fs::create_dir_all(parent).await.map_err(|e| {
            AgentFlowError::AsyncExecutionError {
              message: format!("Failed to create directory '{}': {}", parent.display(), e),
            }
          })?;
        }

        tokio::fs::write(path, content)
          .await
          .map_err(|e| AgentFlowError::AsyncExecutionError {
            message: format!("Failed to write file '{}': {}", path, e),
          })?;

        Ok(serde_json::json!({
            "operation": "write",
            "path": path,
            "size": content.len(),
            "encoding": encoding
        }))
      }
      "append" => {
        // Append content to file
        let content =
          prep_result["content"]
            .as_str()
            .ok_or_else(|| AgentFlowError::AsyncExecutionError {
              message: "Append operation requires content".to_string(),
            })?;

        // Ensure parent directory exists
        if let Some(parent) = Path::new(path).parent() {
          tokio::fs::create_dir_all(parent).await.map_err(|e| {
            AgentFlowError::AsyncExecutionError {
              message: format!("Failed to create directory '{}': {}", parent.display(), e),
            }
          })?;
        }

        // Read existing content if file exists
        let mut existing_content = String::new();
        if Path::new(path).exists() {
          existing_content = tokio::fs::read_to_string(path).await.map_err(|e| {
            AgentFlowError::AsyncExecutionError {
              message: format!("Failed to read existing file '{}': {}", path, e),
            }
          })?;
        }

        existing_content.push_str(content);

        tokio::fs::write(path, &existing_content)
          .await
          .map_err(|e| AgentFlowError::AsyncExecutionError {
            message: format!("Failed to append to file '{}': {}", path, e),
          })?;

        Ok(serde_json::json!({
            "operation": "append",
            "path": path,
            "size": existing_content.len(),
            "encoding": encoding
        }))
      }
      _ => Err(AgentFlowError::AsyncExecutionError {
        message: format!("Unsupported file operation: {}", operation),
      }),
    }
  }

  async fn post_async(
    &self,
    shared: &SharedState,
    _prep_result: Value,
    exec_result: Value,
  ) -> Result<Option<String>> {
    // Store result in shared state
    shared.insert(format!("{}_result", self.name), exec_result.clone());

    // Store content if it was a read operation
    if let Some(content) = exec_result["content"].as_str() {
      shared.insert(
        format!("{}_content", self.name),
        Value::String(content.to_string()),
      );
    }

    // Store size
    if let Some(size) = exec_result["size"].as_u64() {
      shared.insert(
        format!("{}_size", self.name),
        Value::Number(serde_json::Number::from(size)),
      );
    }

    Ok(None)
  }
}

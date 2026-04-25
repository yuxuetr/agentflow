use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::{sandbox::SandboxPolicy, Tool, ToolError, ToolOutput};

/// Read, write, and list filesystem entries with sandbox path enforcement.
pub struct FileTool {
  policy: Arc<SandboxPolicy>,
}

impl FileTool {
  pub fn new(policy: Arc<SandboxPolicy>) -> Self {
    Self { policy }
  }

  pub fn default_policy() -> Self {
    Self::new(Arc::new(SandboxPolicy::default()))
  }
}

#[async_trait]
impl Tool for FileTool {
  fn name(&self) -> &str {
    "file"
  }

  fn description(&self) -> &str {
    "Read or write files and list directory contents on the local filesystem. \
        Operations: 'read' returns file contents, 'write' saves content to a path, \
        'list' shows entries in a directory."
  }

  fn parameters_schema(&self) -> Value {
    json!({
        "type": "object",
        "properties": {
            "operation": {
                "type": "string",
                "enum": ["read", "write", "list"],
                "description": "Filesystem operation to perform"
            },
            "path": {
                "type": "string",
                "description": "Absolute or relative path to file or directory"
            },
            "content": {
                "type": "string",
                "description": "Content to write (only required for 'write')"
            }
        },
        "required": ["operation", "path"]
    })
  }

  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
    let operation = params["operation"]
      .as_str()
      .ok_or_else(|| ToolError::InvalidParams {
        message: "Missing required parameter 'operation'".to_string(),
      })?;

    let path_str = params["path"]
      .as_str()
      .ok_or_else(|| ToolError::InvalidParams {
        message: "Missing required parameter 'path'".to_string(),
      })?;

    let path = Path::new(path_str);

    if !self.policy.is_path_allowed(path) {
      return Err(ToolError::SandboxViolation {
        message: format!("Path '{}' is outside allowed path prefixes", path_str),
      });
    }

    match operation {
      "read" => {
        let metadata = tokio::fs::metadata(path)
          .await
          .map_err(ToolError::IoError)?;

        if metadata.len() > self.policy.max_file_read_bytes {
          return Err(ToolError::SandboxViolation {
            message: format!(
              "File size {} bytes exceeds limit of {} bytes",
              metadata.len(),
              self.policy.max_file_read_bytes
            ),
          });
        }

        let content = tokio::fs::read_to_string(path)
          .await
          .map_err(ToolError::IoError)?;

        Ok(ToolOutput::success(content))
      }

      "write" => {
        let content = params["content"]
          .as_str()
          .ok_or_else(|| ToolError::InvalidParams {
            message: "Parameter 'content' is required for 'write' operation".to_string(),
          })?;

        if let Some(parent) = path.parent() {
          if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent)
              .await
              .map_err(ToolError::IoError)?;
          }
        }

        tokio::fs::write(path, content)
          .await
          .map_err(ToolError::IoError)?;

        Ok(ToolOutput::success(format!(
          "Wrote {} bytes to {}",
          content.len(),
          path_str
        )))
      }

      "list" => {
        let mut entries = Vec::new();
        let mut dir = tokio::fs::read_dir(path)
          .await
          .map_err(ToolError::IoError)?;

        while let Some(entry) = dir.next_entry().await.map_err(ToolError::IoError)? {
          let name = entry.file_name().to_string_lossy().to_string();
          let is_dir = entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false);
          entries.push(if is_dir { format!("{}/", name) } else { name });
        }
        entries.sort();
        Ok(ToolOutput::success(entries.join("\n")))
      }

      other => Err(ToolError::InvalidParams {
        message: format!(
          "Unknown operation '{}'. Valid values: read, write, list",
          other
        ),
      }),
    }
  }
}

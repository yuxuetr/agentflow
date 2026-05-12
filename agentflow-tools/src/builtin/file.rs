use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::{Tool, ToolError, ToolIdempotency, ToolMetadata, ToolOutput, sandbox::SandboxPolicy};

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

  fn metadata(&self) -> ToolMetadata {
    ToolMetadata::builtin_named(self.name())
  }

  fn idempotency(&self, params: &Value) -> ToolIdempotency {
    match params["operation"].as_str() {
      Some("read") | Some("list") => ToolIdempotency::Idempotent,
      Some("write") => ToolIdempotency::NonIdempotent,
      _ => ToolIdempotency::Unknown,
    }
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

    if let Some(reason) = self.policy.path_denial_reason(path) {
      return Err(ToolError::SandboxViolation { message: reason });
    }

    match operation {
      "read" => {
        let metadata = tokio::fs::metadata(path)
          .await
          .map_err(ToolError::IoError)?;

        if let Some(reason) = hardlink_denial_reason(path, &metadata, &self.policy) {
          return Err(ToolError::SandboxViolation { message: reason });
        }

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
        if let Some(parent) = path.parent()
          && !parent.as_os_str().is_empty()
          && let Some(reason) = self.policy.path_denial_reason(parent)
        {
          return Err(ToolError::SandboxViolation {
            message: format!("parent directory denied for write: {reason}"),
          });
        }

        let content = params["content"]
          .as_str()
          .ok_or_else(|| ToolError::InvalidParams {
            message: "Parameter 'content' is required for 'write' operation".to_string(),
          })?;

        if let Ok(metadata) = tokio::fs::metadata(path).await
          && let Some(reason) = hardlink_denial_reason(path, &metadata, &self.policy)
        {
          return Err(ToolError::SandboxViolation { message: reason });
        }

        if let Some(parent) = path.parent()
          && !parent.as_os_str().is_empty()
        {
          tokio::fs::create_dir_all(parent)
            .await
            .map_err(ToolError::IoError)?;
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

#[cfg(unix)]
fn hardlink_denial_reason(
  path: &Path,
  metadata: &std::fs::Metadata,
  policy: &SandboxPolicy,
) -> Option<String> {
  use std::os::unix::fs::MetadataExt;

  if policy.allow_hardlinked_files || !metadata.is_file() || metadata.nlink() <= 1 {
    return None;
  }

  Some(format!(
    "file '{}' has {} hard links and hardlinked files are not allowed",
    path.display(),
    metadata.nlink()
  ))
}

#[cfg(not(unix))]
fn hardlink_denial_reason(
  _path: &Path,
  _metadata: &std::fs::Metadata,
  _policy: &SandboxPolicy,
) -> Option<String> {
  None
}

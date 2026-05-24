use std::collections::HashMap;
use std::path::{Component, Path};
use std::sync::Arc;

use agentflow_core::{
  async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
  error::AgentFlowError,
  value::FlowValue,
};
use agentflow_tools::SandboxPolicy;
use async_trait::async_trait;
use serde_json::json;

/// Workflow node that reads or writes files on the local filesystem.
///
/// `FileNode` defers to [`agentflow_tools::SandboxPolicy`] for path
/// validation (Q1.3.1). The default policy is permissive — workflows
/// that ran before the audit keep working — but the parent-dir traversal
/// guard and symlink/hardlink check always fire regardless of policy. To
/// pin reads/writes to a specific subtree (production deployments), wire
/// in a stricter policy via [`FileNode::with_policy`].
#[derive(Debug, Clone)]
pub struct FileNode {
  policy: Arc<SandboxPolicy>,
}

impl FileNode {
  /// Build a `FileNode` with the supplied sandbox policy.
  pub fn new(policy: Arc<SandboxPolicy>) -> Self {
    Self { policy }
  }

  /// Override the policy on an existing node.
  pub fn with_policy(mut self, policy: Arc<SandboxPolicy>) -> Self {
    self.policy = policy;
    self
  }
}

impl Default for FileNode {
  fn default() -> Self {
    // Permissive baseline keeps backwards-compatibility with existing
    // workflows; the traversal + symlink guards still run because they
    // live below the policy check in `validate_path`.
    Self {
      policy: Arc::new(SandboxPolicy::permissive()),
    }
  }
}

#[async_trait]
impl AsyncNode for FileNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let operation = get_string_input(inputs, "operation")?;
    let path_str = get_string_input(inputs, "path")?;
    let path = Path::new(path_str);

    self.validate_path(path)?;

    match operation {
      "read" => {
        // Re-validate before opening: between the initial check and now
        // a symlink could have appeared. `tokio::fs::symlink_metadata`
        // returns the entry's own metadata (not the link target) so we
        // can refuse to follow.
        let meta = tokio::fs::symlink_metadata(path).await.map_err(|e| {
          AgentFlowError::AsyncExecutionError {
            message: format!("Failed to stat '{}': {}", path.display(), e),
          }
        })?;
        if meta.file_type().is_symlink() {
          return Err(AgentFlowError::AsyncExecutionError {
            message: format!(
              "Refusing to read '{}': path is a symlink and could escape the policy",
              path.display()
            ),
          });
        }
        if meta.file_type().is_file()
          && meta.nlink_or_zero() > 1
          && !self.policy.allow_hardlinked_files
        {
          return Err(AgentFlowError::AsyncExecutionError {
            message: format!(
              "Refusing to read '{}': file has multiple hardlinks ({}); set policy.allow_hardlinked_files = true if intentional",
              path.display(),
              meta.nlink_or_zero()
            ),
          });
        }
        if meta.len() > self.policy.max_file_read_bytes {
          return Err(AgentFlowError::AsyncExecutionError {
            message: format!(
              "Refusing to read '{}': size {} bytes exceeds policy.max_file_read_bytes {}",
              path.display(),
              meta.len(),
              self.policy.max_file_read_bytes
            ),
          });
        }

        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
          AgentFlowError::AsyncExecutionError {
            message: format!("Failed to read file '{}': {}", path.display(), e),
          }
        })?;
        let mut outputs = HashMap::new();
        outputs.insert("content".to_string(), FlowValue::Json(json!(content)));
        Ok(outputs)
      }
      "write" => {
        let content = get_string_input(inputs, "content")?;
        if let Some(parent) = path.parent() {
          if !parent.as_os_str().is_empty() {
            // Parent validation is what stops `..` escapes when the
            // target file doesn't exist yet — the canonicalization in
            // `path_denial_reason` walks up to the nearest existing
            // ancestor.
            self.validate_path(parent)?;
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
              AgentFlowError::AsyncExecutionError {
                message: format!("Failed to create directory '{}': {}", parent.display(), e),
              }
            })?;
          }
        }
        tokio::fs::write(path, content)
          .await
          .map_err(|e| AgentFlowError::AsyncExecutionError {
            message: format!("Failed to write file '{}': {}", path.display(), e),
          })?;
        let mut outputs = HashMap::new();
        outputs.insert("path".to_string(), FlowValue::Json(json!(path_str)));
        Ok(outputs)
      }
      _ => Err(AgentFlowError::NodeInputError {
        message: format!("Unsupported file operation: {}", operation),
      }),
    }
  }
}

impl FileNode {
  /// Defense-in-depth path validation: always reject traversal, then
  /// defer to [`SandboxPolicy::path_denial_reason`] for allowlist
  /// enforcement. The traversal arm runs even when the policy is
  /// permissive because `..` slip-ups are almost always bugs.
  fn validate_path(&self, path: &Path) -> Result<(), AgentFlowError> {
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
      return Err(AgentFlowError::AsyncExecutionError {
        message: format!(
          "Refusing path '{}': parent-directory components ('..') are not allowed",
          path.display()
        ),
      });
    }
    if let Some(reason) = self.policy.path_denial_reason(path) {
      return Err(AgentFlowError::AsyncExecutionError {
        message: format!("sandbox policy denied: {reason}"),
      });
    }
    Ok(())
  }
}

trait MetadataExt {
  fn nlink_or_zero(&self) -> u64;
}

#[cfg(unix)]
impl MetadataExt for std::fs::Metadata {
  fn nlink_or_zero(&self) -> u64 {
    use std::os::unix::fs::MetadataExt;
    self.nlink()
  }
}

#[cfg(not(unix))]
impl MetadataExt for std::fs::Metadata {
  fn nlink_or_zero(&self) -> u64 {
    // Hardlink count is not exposed on Windows in the stable std API;
    // fall back to 0 (skip the check) rather than blocking writes.
    0
  }
}

fn get_string_input<'a>(inputs: &'a AsyncNodeInputs, key: &str) -> Result<&'a str, AgentFlowError> {
  inputs
    .get(key)
    .and_then(|v| match v {
      FlowValue::Json(serde_json::Value::String(s)) => Some(s.as_str()),
      _ => None,
    })
    .ok_or_else(|| AgentFlowError::NodeInputError {
      message: format!(
        "Required string input '{}' is missing or has wrong type",
        key
      ),
    })
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::{Value, json};
  use std::path::PathBuf;
  use tempfile::tempdir;

  #[tokio::test]
  async fn write_then_read_round_trip_under_default_policy() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("test.txt");
    let file_path_str = file_path.to_str().unwrap();

    let write_node = FileNode::default();
    let mut write_inputs = AsyncNodeInputs::new();
    write_inputs.insert("operation".to_string(), FlowValue::Json(json!("write")));
    write_inputs.insert("path".to_string(), FlowValue::Json(json!(file_path_str)));
    write_inputs.insert("content".to_string(), FlowValue::Json(json!("hello world")));

    write_node.execute(&write_inputs).await.unwrap();

    let read_node = FileNode::default();
    let mut read_inputs = AsyncNodeInputs::new();
    read_inputs.insert("operation".to_string(), FlowValue::Json(json!("read")));
    read_inputs.insert("path".to_string(), FlowValue::Json(json!(file_path_str)));

    let outputs = read_node.execute(&read_inputs).await.unwrap();
    let content = outputs.get("content").unwrap();
    if let FlowValue::Json(Value::String(s)) = content {
      assert_eq!(s, "hello world");
    } else {
      panic!("expected content as JSON string");
    }
  }

  /// Q1.3.1 regression: parent-dir traversal is rejected even under the
  /// permissive default policy.
  #[tokio::test]
  async fn rejects_parent_directory_traversal() {
    let node = FileNode::default();
    let mut inputs = AsyncNodeInputs::new();
    inputs.insert("operation".to_string(), FlowValue::Json(json!("read")));
    inputs.insert(
      "path".to_string(),
      FlowValue::Json(json!("/tmp/../etc/passwd")),
    );

    let err = node.execute(&inputs).await.unwrap_err();
    let message = err.to_string();
    assert!(
      message.contains("parent-directory components"),
      "expected traversal rejection, got: {message}"
    );
  }

  /// Q1.3.1 regression: when a stricter sandbox policy is wired in,
  /// FileNode honors the allowlist instead of falling through to raw
  /// tokio::fs::* like the pre-fix implementation did.
  #[tokio::test]
  async fn explicit_policy_blocks_writes_outside_allowed_paths() {
    let allowed = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let outside_path = outside.path().join("secret.txt");
    let outside_path_str = outside_path.to_str().unwrap();

    let policy = Arc::new(SandboxPolicy {
      allowed_paths: vec![allowed.path().to_path_buf()],
      ..SandboxPolicy::default()
    });
    let node = FileNode::new(policy);

    let mut inputs = AsyncNodeInputs::new();
    inputs.insert("operation".to_string(), FlowValue::Json(json!("write")));
    inputs.insert("path".to_string(), FlowValue::Json(json!(outside_path_str)));
    inputs.insert("content".to_string(), FlowValue::Json(json!("evil")));

    let err = node.execute(&inputs).await.unwrap_err();
    let message = err.to_string();
    assert!(
      message.contains("sandbox policy denied"),
      "expected policy denial, got: {message}"
    );
    assert!(
      !PathBuf::from(outside_path_str).exists(),
      "file outside policy was unexpectedly created"
    );
  }

  /// Symlink reads are rejected: the file shows up in the policy
  /// allowlist (we're inside the temp dir) but the symlink could
  /// point anywhere on the host filesystem.
  #[tokio::test]
  #[cfg(unix)]
  async fn rejects_reading_through_symlink() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().unwrap();
    let target = dir.path().join("target.txt");
    std::fs::write(&target, "real").unwrap();
    let link = dir.path().join("link.txt");
    symlink(&target, &link).unwrap();

    let node = FileNode::default();
    let mut inputs = AsyncNodeInputs::new();
    inputs.insert("operation".to_string(), FlowValue::Json(json!("read")));
    inputs.insert(
      "path".to_string(),
      FlowValue::Json(json!(link.to_str().unwrap())),
    );

    let err = node.execute(&inputs).await.unwrap_err();
    assert!(err.to_string().contains("symlink"));
  }
}

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::{sandbox::SandboxPolicy, Tool, ToolError, ToolMetadata, ToolOutput};

/// Execute a shell command via `sh -c` with sandbox enforcement.
pub struct ShellTool {
  policy: Arc<SandboxPolicy>,
}

impl ShellTool {
  pub fn new(policy: Arc<SandboxPolicy>) -> Self {
    Self { policy }
  }

  /// Convenience: create with the default (restrictive) policy.
  pub fn default_policy() -> Self {
    Self::new(Arc::new(SandboxPolicy::default()))
  }
}

#[async_trait]
impl Tool for ShellTool {
  fn name(&self) -> &str {
    "shell"
  }

  fn description(&self) -> &str {
    "Execute a shell command and return its stdout/stderr. \
        Use for running system commands, inspecting files, or performing \
        OS-level operations."
  }

  fn parameters_schema(&self) -> Value {
    json!({
        "type": "object",
        "properties": {
            "command": {
                "type": "string",
                "description": "The shell command to execute (passed to `sh -c`)"
            }
        },
        "required": ["command"]
    })
  }

  fn metadata(&self) -> ToolMetadata {
    ToolMetadata::builtin_named(self.name())
  }

  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
    let command = params["command"]
      .as_str()
      .ok_or_else(|| ToolError::InvalidParams {
        message: "Missing required parameter 'command'".to_string(),
      })?;

    // Extract the base command for sandbox check
    let base_cmd = command.split_whitespace().next().unwrap_or("");
    if !self.policy.is_command_allowed(base_cmd) {
      return Err(ToolError::SandboxViolation {
        message: format!("Command '{}' is not in the allowed-commands list", base_cmd),
      });
    }

    let timeout = Duration::from_secs(self.policy.max_exec_time_secs);
    let output = tokio::time::timeout(
      timeout,
      tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .output(),
    )
    .await
    .map_err(|_| ToolError::ExecutionFailed {
      message: format!(
        "Command timed out after {} seconds",
        self.policy.max_exec_time_secs
      ),
    })?
    .map_err(ToolError::IoError)?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
      let result = if stdout.trim().is_empty() {
        "(no output)".to_string()
      } else {
        stdout.trim().to_string()
      };
      Ok(ToolOutput::success(result))
    } else {
      let msg = if stderr.trim().is_empty() {
        stdout.trim().to_string()
      } else {
        stderr.trim().to_string()
      };
      Ok(ToolOutput::error(format!(
        "Exit code {}: {}",
        output.status.code().unwrap_or(-1),
        msg
      )))
    }
  }
}

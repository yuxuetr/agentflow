use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::sandbox::{NoopSandboxBackend, SandboxBackend, SandboxPolicy, SandboxScope};
use crate::{Tool, ToolError, ToolIdempotency, ToolMetadata, ToolOutput};

/// Execute a shell command via `sh -c` with sandbox enforcement.
pub struct ShellTool {
  policy: Arc<SandboxPolicy>,
  backend: Arc<dyn SandboxBackend>,
}

impl ShellTool {
  /// Create a `ShellTool` whose OS sandbox is a no-op (current behaviour).
  /// Capability and policy gating still apply via [`crate::ToolRegistry::execute`].
  pub fn new(policy: Arc<SandboxPolicy>) -> Self {
    Self {
      policy,
      backend: Arc::new(NoopSandboxBackend::new(
        "ShellTool default backend; opt in via with_os_sandbox()",
      )),
    }
  }

  /// Convenience: create with the default (restrictive) policy and no OS sandbox.
  pub fn default_policy() -> Self {
    Self::new(Arc::new(SandboxPolicy::default()))
  }

  /// Wrap subsequent invocations in the platform's enforcing sandbox backend
  /// ([`crate::sandbox::default_backend`]). On macOS this is `sandbox-exec`; on
  /// Linux this is a seccomp BPF filter installed via `pre_exec`. On other
  /// platforms the wrap returns an error and the call fails before spawn.
  pub fn with_os_sandbox(mut self) -> Self {
    self.backend = crate::sandbox::default_backend();
    self
  }

  /// Inject a custom backend (e.g. for tests).
  pub fn with_backend(mut self, backend: Arc<dyn SandboxBackend>) -> Self {
    self.backend = backend;
    self
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

  fn idempotency(&self, _params: &Value) -> ToolIdempotency {
    ToolIdempotency::NonIdempotent
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
    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg("-c").arg(command);

    let scope = build_scope_from_policy(&self.policy);
    let caps = self.requires_capabilities();
    self.backend.wrap_command(&mut cmd, &caps, &scope).map_err(|err| {
      ToolError::SandboxViolation {
        message: format!("OS sandbox preparation failed: {err}"),
      }
    })?;

    let output = tokio::time::timeout(timeout, cmd.output())
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

/// Project a [`SandboxPolicy`] into the [`SandboxScope`] consumed by OS-level
/// backends. We treat `allowed_paths` as both read- and write-allowed because
/// the in-process layer does not split read vs. write at policy level — that
/// distinction is enforced by `Capability::FsRead` vs. `Capability::FsWrite`.
///
/// When the policy is permissive (empty allowlist) we fall back to a
/// conservative default: `/tmp` plus the current working directory. This
/// keeps shell builtins working without granting the entire filesystem.
pub(crate) fn build_scope_from_policy(policy: &SandboxPolicy) -> SandboxScope {
  let mut scope = SandboxScope::new();
  if policy.allowed_paths.is_empty() {
    scope.read_paths.push(PathBuf::from("/tmp"));
    if let Ok(cwd) = std::env::current_dir() {
      scope.read_paths.push(cwd.clone());
      scope.write_paths.push(cwd.clone());
      scope.working_directory = Some(cwd);
    }
  } else {
    for path in &policy.allowed_paths {
      scope.read_paths.push(path.clone());
      scope.write_paths.push(path.clone());
    }
  }
  scope
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn default_backend_is_noop_so_existing_behaviour_holds() {
    let tool = ShellTool::default_policy();
    let result = tool.execute(json!({"command": "echo hello"})).await.unwrap();
    assert!(result.content.contains("hello"));
  }

  #[tokio::test]
  async fn unknown_command_is_rejected_before_backend_runs() {
    let tool = ShellTool::default_policy();
    let result = tool.execute(json!({"command": "rm -rf /"})).await;
    assert!(matches!(result, Err(ToolError::SandboxViolation { .. })));
  }

  #[test]
  fn permissive_policy_falls_back_to_tmp_plus_cwd() {
    let policy = SandboxPolicy::permissive();
    let scope = build_scope_from_policy(&policy);

    assert!(scope.read_paths.iter().any(|p| p == std::path::Path::new("/tmp")));
    assert!(scope.working_directory.is_some());
  }

  #[test]
  fn restrictive_policy_uses_only_allowed_paths() {
    let policy = SandboxPolicy {
      allowed_paths: vec![PathBuf::from("/var/agentflow")],
      ..SandboxPolicy::default()
    };
    let scope = build_scope_from_policy(&policy);

    assert_eq!(
      scope.read_paths,
      vec![PathBuf::from("/var/agentflow")]
    );
    assert_eq!(
      scope.write_paths,
      vec![PathBuf::from("/var/agentflow")]
    );
    assert!(scope.working_directory.is_none());
  }
}

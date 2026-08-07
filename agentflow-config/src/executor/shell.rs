//! F-A7-2 closure: `type: shell` YAML workflow node.
//!
//! `agentflow-cli`'s workflow factory historically didn't build a
//! shell node — the honest-note in `commands/workflow/validate.rs`
//! pointed authors at skills / harness / hand-rolled binaries
//! instead. That gap blocked any YAML workflow that wanted to do
//! file discovery (`find input/*.md`), git probes (`git log`), or
//! other host-OS work.
//!
//! This module ships an inline `ShellWorkflowNode` that wraps the
//! existing [`agentflow_tools::builtin::ShellTool`] with a
//! [`SandboxPolicy`] built from YAML parameters. The policy is
//! **mandatory** (`allowed_commands` is a required schema field, see
//! `config/schema.rs`) — there's no permissive default that would
//! turn a typo'd workflow into arbitrary code execution.
//!
//! At execute time, the node reads the `command` string from its
//! inputs (`input_mapping` or initial_inputs) and delegates to
//! `ShellTool::execute`. The resulting `ToolOutput` is unwrapped
//! into the node's standard output map:
//!
//! - `stdout`: the command's stdout as a JSON string
//! - `exit_code`: 0 on success, otherwise the non-zero status
//! - `error`: only present when the tool returned an error (sandbox
//!   violation, timeout, non-zero exit)
//!
//! Sandbox violations and command failures become `AsyncNodeError`s
//! that surface in the state pool exactly like any other node-level
//! error (per F-A6-3 design — they don't bubble to the Flow level).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use agentflow_core::{
  async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
  error::AgentFlowError,
  value::FlowValue,
};
use agentflow_tools::SecurityProfile;
use agentflow_tools::Tool;
use agentflow_tools::builtin::ShellTool;
use agentflow_tools::sandbox::{SandboxBackend, SandboxPolicy, default_backend};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde_json::{Value, json};

/// Inline YAML workflow shell node. Built from the `parameters`
/// block of a `type: shell` YAML entry, owns its own [`ShellTool`]
/// for the lifetime of the workflow.
pub struct ShellWorkflowNode {
  name: String,
  tool: ShellTool,
}

impl ShellWorkflowNode {
  /// Construct from a YAML parameters block. `allowed_commands`
  /// is required (an empty/missing list would block every command,
  /// so we surface this as a config error at parse time rather
  /// than at run time).
  pub fn from_params(name: &str, parameters: &HashMap<String, serde_yaml::Value>) -> Result<Self> {
    let allowed_commands: Vec<String> = parameters
      .get("allowed_commands")
      .and_then(|v| v.as_sequence())
      .ok_or_else(|| {
        anyhow!(
          "shell node '{}' requires 'allowed_commands' as a YAML sequence of command names \
           (e.g. ['git', 'find', 'ls']) — empty / missing would block every command",
          name
        )
      })?
      .iter()
      .filter_map(|v| v.as_str().map(|s| s.to_string()))
      .collect();

    if allowed_commands.is_empty() {
      return Err(anyhow!(
        "shell node '{}': 'allowed_commands' must contain at least one command name",
        name
      ));
    }

    let allowed_paths: Vec<PathBuf> = parameters
      .get("allowed_paths")
      .and_then(|v| v.as_sequence())
      .map(|seq| {
        seq
          .iter()
          .filter_map(|v| v.as_str().map(PathBuf::from))
          .collect()
      })
      .unwrap_or_default();

    let policy = Arc::new(SandboxPolicy {
      allowed_commands,
      allowed_paths,
      ..SandboxPolicy::default()
    });

    let profile = SecurityProfile::from_env().unwrap_or_default();
    let mut tool = ShellTool::new(policy);
    if let Some(backend) = resolve_shell_backend(name, profile, default_backend())? {
      tool = tool.with_backend(backend);
    }

    Ok(Self {
      name: name.to_string(),
      tool,
    })
  }
}

/// V3.2: resolve whether a DAG `shell` node's Argv-mode subprocess should
/// be wrapped with the platform OS-sandbox backend, based on
/// `SecurityProfile::defaults().sandboxing.require_os_sandbox` (Production
/// only, today). `ShellInterpretation::Shell` (`sh -c`) mode already
/// refuses to spawn without an enforcing backend
/// (`ShellTool::prepare_shell`); this closes the equivalent gap for the
/// `Argv` mode DAG `shell` nodes use — an allow-listed command still
/// deserves OS-level confinement, not just the allow-list membership
/// check alone.
///
/// Pure/testable: takes `backend` as a parameter (mirrors
/// `executor::plugin::select_preparer`) so a test can simulate an
/// unsupported platform without depending on what's actually available
/// on the machine running the suite.
fn resolve_shell_backend(
  name: &str,
  profile: SecurityProfile,
  backend: Arc<dyn SandboxBackend>,
) -> Result<Option<Arc<dyn SandboxBackend>>> {
  if !profile.defaults().sandboxing.require_os_sandbox {
    return Ok(None);
  }
  if !backend.is_enforcing() {
    return Err(anyhow!(
      "shell node '{name}': {profile} profile requires an enforcing OS sandbox backend for \
       shell nodes, but the resolved backend ('{}') is not enforcing — refusing to run \
       unsandboxed under this profile",
      backend.name()
    ));
  }
  Ok(Some(backend))
}

#[async_trait]
impl AsyncNode for ShellWorkflowNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let command = inputs
      .get("command")
      .and_then(|v| match v {
        FlowValue::Json(Value::String(s)) => Some(s.as_str()),
        _ => None,
      })
      .ok_or_else(|| AgentFlowError::NodeInputError {
        message: format!(
          "shell node '{}': required input 'command' (string) is missing — \
           pass via input_mapping or initial_inputs",
          self.name
        ),
      })?;

    let params = json!({ "command": command });
    let output =
      self
        .tool
        .execute(params)
        .await
        .map_err(|e| AgentFlowError::AsyncExecutionError {
          message: format!("shell node '{}': {}", self.name, e),
        })?;

    let mut outputs = HashMap::new();
    outputs.insert(
      "stdout".to_string(),
      FlowValue::Json(Value::String(output.content.clone())),
    );
    outputs.insert(
      "is_error".to_string(),
      FlowValue::Json(Value::Bool(output.is_error)),
    );
    Ok(outputs)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use agentflow_tools::sandbox::{NoopSandboxBackend, SandboxError, SandboxScope};

  /// Minimal enforcing backend double — `resolve_shell_backend` only
  /// consults `is_enforcing()`/`name()`, never actually spawns through
  /// it in these unit tests.
  struct FakeEnforcingBackend;

  impl SandboxBackend for FakeEnforcingBackend {
    fn name(&self) -> &'static str {
      "fake-enforcing"
    }

    fn is_enforcing(&self) -> bool {
      true
    }

    fn wrap_command(
      &self,
      _command: &mut tokio::process::Command,
      _effective_capabilities: &[agentflow_tools::Capability],
      _scope: &SandboxScope,
    ) -> Result<(), SandboxError> {
      Ok(())
    }
  }

  #[test]
  fn resolve_shell_backend_dev_and_local_skip_wrapping() {
    // `require_os_sandbox` is false for Dev/Local — no wrap requested,
    // regardless of what backend is passed (even a non-enforcing one).
    for profile in [SecurityProfile::Dev, SecurityProfile::Local] {
      let result = resolve_shell_backend("n", profile, Arc::new(NoopSandboxBackend::new("test")));
      assert!(result.unwrap().is_none(), "profile {profile} must not wrap");
    }
  }

  #[test]
  fn resolve_shell_backend_production_wraps_an_enforcing_backend() {
    let result = resolve_shell_backend(
      "n",
      SecurityProfile::Production,
      Arc::new(FakeEnforcingBackend),
    );
    assert!(result.unwrap().is_some());
  }

  #[test]
  fn resolve_shell_backend_production_rejects_a_non_enforcing_backend() {
    let result = resolve_shell_backend(
      "my-shell-node",
      SecurityProfile::Production,
      Arc::new(NoopSandboxBackend::new("no backend on this platform")),
    );
    let Err(err) = result else {
      panic!("expected an error, got Ok");
    };
    let message = err.to_string();
    assert!(message.contains("my-shell-node"), "got: {message}");
    assert!(message.contains("not enforcing"), "got: {message}");
  }

  #[test]
  fn from_params_dev_profile_does_not_require_a_command() {
    // Sanity: existing construction path still works when no
    // AGENTFLOW_SECURITY_PROFILE is set (defaults to Local, no wrap
    // requested) — a regression guard for the new code path added to
    // `from_params`.
    let mut parameters = HashMap::new();
    parameters.insert(
      "allowed_commands".to_string(),
      serde_yaml::Value::Sequence(vec![serde_yaml::Value::String("echo".into())]),
    );
    let node = ShellWorkflowNode::from_params("n", &parameters).unwrap();
    assert_eq!(node.name, "n");
  }
}

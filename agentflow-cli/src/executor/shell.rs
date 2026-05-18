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
use agentflow_tools::Tool;
use agentflow_tools::builtin::ShellTool;
use agentflow_tools::sandbox::SandboxPolicy;
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

    Ok(Self {
      name: name.to_string(),
      tool: ShellTool::new(policy),
    })
  }
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

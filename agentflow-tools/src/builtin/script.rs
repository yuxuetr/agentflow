use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::{sandbox::SandboxPolicy, Tool, ToolError, ToolOutput};

/// Execute a named script from the skill's `scripts/` directory.
///
/// The agent passes:
/// - `script`: filename relative to the scripts directory (e.g. `"check_syntax.py"`)
/// - `args`: optional JSON object forwarded to the script as JSON on stdin
///
/// The interpreter is inferred from the file extension:
/// | Extension | Interpreter  |
/// |-----------|-------------|
/// | `.py`     | `python3`   |
/// | `.sh`     | `bash`      |
/// | `.js`     | `node`      |
///
/// Arguments are serialised to JSON and piped to the script on **stdin**.
/// The script's **stdout** is returned as the tool output.
pub struct ScriptTool {
  /// Absolute path to the `scripts/` directory for the current skill.
  scripts_dir: PathBuf,
  policy: Arc<SandboxPolicy>,
  /// Optional JSON schema for validating input parameters.
  parameters_schema: Option<Value>,
}

impl ScriptTool {
  pub fn new(scripts_dir: PathBuf, policy: Arc<SandboxPolicy>) -> Self {
    Self {
      scripts_dir,
      policy,
      parameters_schema: None,
    }
  }

  /// Convenience constructor with the default (restrictive) sandbox policy.
  pub fn with_default_policy(scripts_dir: PathBuf) -> Self {
    Self::new(scripts_dir, Arc::new(SandboxPolicy::default()))
  }

  /// Sets the parameters schema for validation.
  pub fn with_parameters_schema(mut self, schema: Value) -> Self {
    self.parameters_schema = Some(schema);
    self
  }
}

#[async_trait]
impl Tool for ScriptTool {
  fn name(&self) -> &str {
    "script"
  }

  fn description(&self) -> &str {
    "Execute a script from the skill's scripts/ directory. \
        Pass the script filename and optional arguments as JSON. \
        Supported languages: Python (.py), Bash (.sh), JavaScript (.js)."
  }

  fn parameters_schema(&self) -> Value {
    json!({
        "type": "object",
        "properties": {
            "script": {
                "type": "string",
                "description": "Script filename (e.g. 'check_syntax.py'). Must be inside the skill scripts/ directory."
            },
            "args": {
                "description": "Optional arguments forwarded to the script as JSON on stdin. Can be any JSON value.",
                "default": null
            }
        },
        "required": ["script"]
    })
  }

  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
    // ── Schema validation ────────────────────────────────────────────────
    if let Some(schema) = &self.parameters_schema {
      if let Ok(compiled_schema) = jsonschema::JSONSchema::options().compile(schema) {
        if let Err(errors) = compiled_schema.validate(&params) {
          let mut error_messages = Vec::new();
          for error in errors {
            error_messages.push(error.to_string());
          }
          return Err(ToolError::InvalidParams {
            message: format!(
              "Parameters failed schema validation: {}",
              error_messages.join(", ")
            ),
          });
        }
      } else {
        tracing::warn!("Failed to compile JSON schema for script tool");
      }
    }

    // ── Parameter extraction ─────────────────────────────────────────────
    let script_name = params["script"]
      .as_str()
      .ok_or_else(|| ToolError::InvalidParams {
        message: "Missing required parameter 'script'".to_string(),
      })?;

    // ── Path resolution + sandbox check ──────────────────────────────────
    // Reject any path traversal attempts (e.g. "../../../etc/passwd")
    if script_name.contains("..") || script_name.contains('/') || script_name.contains('\\') {
      return Err(ToolError::SandboxViolation {
        message: format!(
          "Script name '{}' must be a plain filename, not a path",
          script_name
        ),
      });
    }

    let script_path = self.scripts_dir.join(script_name);
    if !script_path.exists() {
      return Err(ToolError::ExecutionFailed {
        message: format!(
          "Script '{}' not found in scripts directory '{}'",
          script_name,
          self.scripts_dir.display()
        ),
      });
    }

    // ── Interpreter selection ────────────────────────────────────────────
    let ext = script_path
      .extension()
      .and_then(|e| e.to_str())
      .unwrap_or("");
    let interpreter = interpreter_for(ext).ok_or_else(|| ToolError::ExecutionFailed {
      message: format!(
        "Unsupported script extension '.{}'. Supported: .py, .sh, .js",
        ext
      ),
    })?;

    // Check that the interpreter is allowed by the sandbox policy.
    if !self.policy.is_command_allowed(interpreter) {
      return Err(ToolError::SandboxViolation {
        message: format!(
          "Interpreter '{}' is not in the allowed-commands list",
          interpreter
        ),
      });
    }

    // ── Serialise args as JSON for stdin ─────────────────────────────────
    let stdin_json = match &params["args"] {
      Value::Null | Value::Object(_) if params.get("args").is_none() => String::new(),
      v => serde_json::to_string(v).unwrap_or_default(),
    };

    // ── Execution ────────────────────────────────────────────────────────
    let timeout = Duration::from_secs(self.policy.max_exec_time_secs);

    let mut cmd = tokio::process::Command::new(interpreter);
    cmd
      .arg(&script_path)
      .current_dir(&self.scripts_dir)
      .stdin(std::process::Stdio::piped())
      .stdout(std::process::Stdio::piped())
      .stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| ToolError::ExecutionFailed {
      message: format!("Failed to spawn '{}': {}", interpreter, e),
    })?;

    // Write args to stdin if present.
    if !stdin_json.is_empty() {
      if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin
          .write_all(stdin_json.as_bytes())
          .await
          .map_err(ToolError::IoError)?;
        // stdin is dropped here, signalling EOF to the child.
      }
    }

    let output = tokio::time::timeout(timeout, child.wait_with_output())
      .await
      .map_err(|_| ToolError::ExecutionFailed {
        message: format!(
          "Script '{}' timed out after {} seconds",
          script_name, self.policy.max_exec_time_secs
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
        "Script exited with code {}: {}",
        output.status.code().unwrap_or(-1),
        msg
      )))
    }
  }
}

/// Map a file extension to a known interpreter binary name.
fn interpreter_for(ext: &str) -> Option<&'static str> {
  match ext {
    "py" => Some("python3"),
    "sh" => Some("bash"),
    "js" => Some("node"),
    _ => None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::Write;
  use tempfile::TempDir;

  fn make_tool(dir: &std::path::Path) -> ScriptTool {
    let mut policy = SandboxPolicy::default();
    policy.allowed_commands = vec![
      "python3".to_string(),
      "bash".to_string(),
      "node".to_string(),
    ];
    ScriptTool::new(dir.to_path_buf(), Arc::new(policy))
  }

  #[tokio::test]
  async fn executes_bash_script() {
    let dir = TempDir::new().unwrap();
    let script = dir.path().join("hello.sh");
    let mut f = std::fs::File::create(&script).unwrap();
    writeln!(f, "#!/bin/bash\necho 'hello from script'").unwrap();

    let tool = make_tool(dir.path());
    let result = tool.execute(json!({"script": "hello.sh"})).await.unwrap();
    assert!(result.content.contains("hello from script"));
  }

  #[tokio::test]
  async fn rejects_path_traversal() {
    let dir = TempDir::new().unwrap();
    let tool = make_tool(dir.path());
    let result = tool.execute(json!({"script": "../etc/passwd"})).await;
    assert!(matches!(result, Err(ToolError::SandboxViolation { .. })));
  }

  #[tokio::test]
  async fn rejects_unknown_extension() {
    let dir = TempDir::new().unwrap();
    // Create a dummy .rb file
    std::fs::File::create(dir.path().join("run.rb")).unwrap();
    let tool = make_tool(dir.path());
    let result = tool.execute(json!({"script": "run.rb"})).await;
    assert!(matches!(result, Err(ToolError::ExecutionFailed { .. })));
  }
}

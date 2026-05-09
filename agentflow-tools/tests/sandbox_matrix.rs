use std::sync::Arc;

use agentflow_tools::builtin::{FileTool, ShellTool};
use agentflow_tools::sandbox::SandboxPolicy;
use agentflow_tools::{Tool, ToolError, ToolMetadata, ToolPermission, ToolPolicy};
use serde_json::{Value, json};
use tempfile::TempDir;

struct SandboxTestCase {
  name: &'static str,
  params: Value,
  expect_fragment: &'static str,
}

fn policy_for(root: &std::path::Path) -> Arc<SandboxPolicy> {
  Arc::new(SandboxPolicy {
    allowed_paths: vec![root.to_path_buf()],
    max_exec_time_secs: 1,
    ..SandboxPolicy::default()
  })
}

async fn assert_file_denied(tool: &FileTool, case: SandboxTestCase) {
  let error = tool
    .execute(case.params)
    .await
    .unwrap_err_or_else(|| panic!("{} should be denied", case.name));

  match error {
    ToolError::SandboxViolation { message } => {
      assert!(
        message.contains(case.expect_fragment),
        "{}: expected '{}' in '{}'",
        case.name,
        case.expect_fragment,
        message
      );
    }
    other => panic!("{}: expected sandbox violation, got {other:?}", case.name),
  }
}

trait ResultExt<T> {
  fn unwrap_err_or_else(self, make_message: impl FnOnce() -> String) -> ToolError;
}

impl<T> ResultExt<T> for Result<T, ToolError> {
  fn unwrap_err_or_else(self, make_message: impl FnOnce() -> String) -> ToolError {
    match self {
      Ok(_) => panic!("{}", make_message()),
      Err(error) => error,
    }
  }
}

#[tokio::test]
async fn file_tool_blocks_traversal_absolute_and_symlink_escape() {
  let temp = TempDir::new().unwrap();
  let allowed = temp.path().join("allowed");
  let outside = temp.path().join("outside");
  std::fs::create_dir_all(&allowed).unwrap();
  std::fs::create_dir_all(&outside).unwrap();
  std::fs::write(outside.join("secret.txt"), "secret").unwrap();

  #[cfg(unix)]
  std::os::unix::fs::symlink(outside.join("secret.txt"), allowed.join("secret-link")).unwrap();

  let tool = FileTool::new(policy_for(&allowed));
  let mut cases = vec![
    SandboxTestCase {
      name: "path traversal",
      params: json!({"operation": "read", "path": allowed.join("../outside/secret.txt")}),
      expect_fragment: "traversal",
    },
    SandboxTestCase {
      name: "absolute outside read",
      params: json!({"operation": "read", "path": outside.join("secret.txt")}),
      expect_fragment: "outside allowed path prefixes",
    },
    SandboxTestCase {
      name: "absolute outside write",
      params: json!({"operation": "write", "path": outside.join("write.txt"), "content": "x"}),
      expect_fragment: "outside allowed path prefixes",
    },
  ];

  #[cfg(unix)]
  cases.push(SandboxTestCase {
    name: "symlink escape",
    params: json!({"operation": "read", "path": allowed.join("secret-link")}),
    expect_fragment: "outside allowed path prefixes",
  });

  for case in cases {
    assert_file_denied(&tool, case).await;
  }
}

#[tokio::test]
async fn shell_tool_blocks_unallowed_command_before_spawn() {
  let tool = ShellTool::default_policy();
  let error = tool
    .execute(json!({"command": "rm -rf /tmp/agentflow-should-not-run"}))
    .await
    .unwrap_err_or_else(|| "rm should be denied".to_string());

  assert!(matches!(error, ToolError::SandboxViolation { .. }));
}

#[tokio::test]
async fn shell_tool_times_out_long_running_process() {
  let policy = Arc::new(SandboxPolicy {
    allowed_commands: vec!["sleep".to_string()],
    max_exec_time_secs: 1,
    ..SandboxPolicy::default()
  });
  let tool = ShellTool::new(policy);

  let error = tool
    .execute(json!({"command": "sleep 5"}))
    .await
    .unwrap_err_or_else(|| "sleep should time out".to_string());

  match error {
    ToolError::ExecutionFailed { message } => assert!(message.contains("timed out")),
    other => panic!("expected timeout, got {other:?}"),
  }
}

#[tokio::test]
async fn shell_tool_handles_large_stdout_without_policy_leakage() {
  let policy = Arc::new(SandboxPolicy {
    allowed_commands: vec!["awk".to_string()],
    max_exec_time_secs: 5,
    ..SandboxPolicy::default()
  });
  let tool = ShellTool::new(policy);

  let output = tool
    .execute(json!({"command": "awk 'BEGIN { for (i = 0; i < 4096; i++) print \"agentflow\" }'"}))
    .await
    .unwrap();

  assert!(!output.is_error);
  assert!(output.content.len() > 32_000);
  assert!(!output.content.contains("API_KEY"));
}

#[test]
fn tool_policy_decision_records_deny_reason() {
  let policy = ToolPolicy::allow_permissions([ToolPermission::Network]);
  let decision = policy.evaluate(
    "shell",
    &ToolMetadata::builtin_named("shell"),
    &json!({"command": "echo ok"}),
  );

  assert!(!decision.allowed);
  assert_eq!(decision.matched_rule, "permission_allowlist");
  assert!(
    decision
      .deny_reason
      .as_deref()
      .unwrap_or_default()
      .contains("permission 'process_exec' is not allowed")
  );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[tokio::test]
async fn os_sandbox_blocks_hardlink_creation_without_fs_write() {
  let temp = TempDir::new().unwrap();
  let outside = temp.path().join("outside.txt");
  let inside = temp.path().join("inside-hardlink.txt");
  std::fs::write(&outside, "secret").unwrap();

  let policy = Arc::new(SandboxPolicy {
    allowed_commands: vec!["ln".to_string()],
    allowed_paths: vec![temp.path().to_path_buf()],
    max_exec_time_secs: 5,
    ..SandboxPolicy::default()
  });
  let tool = ShellTool::new(policy).with_os_sandbox();
  let command = format!("ln {} {}", outside.display(), inside.display());
  let output = tool
    .execute(json!({ "command": command }))
    .await
    .expect("sandboxed shell call should complete");

  assert!(
    output.is_error,
    "hardlink creation should fail without fs.write capability: {}",
    output.content
  );
  assert!(
    !inside.exists(),
    "hardlink target was created despite missing fs.write capability"
  );
}

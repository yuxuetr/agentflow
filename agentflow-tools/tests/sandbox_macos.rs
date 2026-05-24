//! macOS `sandbox-exec` enforcement integration tests.
//!
//! These tests exercise the full `ShellTool::with_os_sandbox()` path against
//! the real `/usr/bin/sandbox-exec` binary, confirming that:
//!
//! 1. A baseline command (`echo`) still succeeds.
//! 2. A write to a path outside the sandbox scope is blocked by the kernel,
//!    not by the in-process [`SandboxPolicy`] (which is permissive in these
//!    tests).
//!
//! The `Capability::FsWrite` capability is *not* in `ShellTool`'s required
//! set (it only declares `Exec`), so the generated SBPL profile omits any
//! `(allow file-write*)` rules — every write should be denied.

#![cfg(target_os = "macos")]

use std::sync::Arc;

use agentflow_tools::Tool;
use agentflow_tools::builtin::ShellTool;
use agentflow_tools::sandbox::SandboxPolicy;
use serde_json::json;

fn permissive_shell_with_sandbox() -> ShellTool {
  ShellTool::new(Arc::new(SandboxPolicy::permissive())).with_os_sandbox()
}

fn sandbox_exec_usable() -> bool {
  std::process::Command::new("/usr/bin/sandbox-exec")
    .arg("-p")
    .arg("(version 1)(allow default)")
    .arg("/bin/echo")
    .arg("ok")
    .output()
    .map(|out| out.status.success())
    .unwrap_or(false)
}

#[tokio::test]
async fn macos_sandbox_allows_baseline_echo() {
  if !sandbox_exec_usable() {
    eprintln!("skipping: sandbox-exec is present but not usable in this environment");
    return;
  }

  let tool = permissive_shell_with_sandbox();
  let result = tool
    .execute(json!({"command": "/bin/echo hello-from-sandbox"}))
    .await
    .expect("echo should succeed under sandbox-exec");

  assert!(
    !result.is_error,
    "expected success, got error output: {}",
    result.content
  );
  assert!(
    result.content.contains("hello-from-sandbox"),
    "stdout did not contain expected token: {}",
    result.content
  );
}

#[tokio::test]
async fn macos_sandbox_blocks_write_outside_scope() {
  if !sandbox_exec_usable() {
    eprintln!("skipping: sandbox-exec is present but not usable in this environment");
    return;
  }

  // Pick a unique path that will not exist before the call. The default
  // capability set for ShellTool is just `Exec`, so the SBPL profile grants
  // no write paths — even /tmp writes must be denied.
  let pid = std::process::id();
  let path = format!("/tmp/agentflow_sandbox_macos_blocked_{pid}.txt");
  // Make sure the file doesn't already exist from a previous run.
  let _ = std::fs::remove_file(&path);

  // Stream redirection (`>`) needs the shell, so opt into shell
  // interpretation. The enforcing backend wired in by
  // `with_os_sandbox()` is what `with_shell_interpretation()` requires.
  let tool = permissive_shell_with_sandbox().with_shell_interpretation();
  let cmd = format!("/bin/echo blocked > {path}");
  let result = tool
    .execute(json!({"command": cmd}))
    .await
    .expect("tool call must complete");

  assert!(
    result.is_error,
    "expected sandbox to block file write, but got success: {}",
    result.content
  );
  assert!(
    !std::path::Path::new(&path).exists(),
    "sandbox failed: file '{path}' was created despite missing FsWrite capability"
  );
}

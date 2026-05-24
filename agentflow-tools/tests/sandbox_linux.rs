//! Linux seccomp-bpf enforcement integration tests.
//!
//! These tests assume the `python3` interpreter is present (the standard CI
//! image satisfies this). They confirm that when `ShellTool` runs under
//! `with_os_sandbox()`:
//!
//! 1. A baseline command that does no I/O outside stdout still succeeds.
//! 2. A program that creates a network socket fails with `EPERM` because
//!    `Capability::Net` is absent from the tool's required capability set.
//!
//! The seccomp filter is installed via `pre_exec`; the kernel returns
//! `EPERM` to denied syscalls. Python surfaces this as `PermissionError`,
//! which we match in stderr.

#![cfg(target_os = "linux")]

use std::sync::Arc;

use agentflow_tools::Tool;
use agentflow_tools::builtin::ShellTool;
use agentflow_tools::sandbox::SandboxPolicy;
use serde_json::json;

fn permissive_shell_with_sandbox() -> ShellTool {
  ShellTool::new(Arc::new(SandboxPolicy::permissive())).with_os_sandbox()
}

fn python3_available() -> bool {
  std::process::Command::new("python3")
    .arg("--version")
    .output()
    .map(|out| out.status.success())
    .unwrap_or(false)
}

#[tokio::test]
async fn linux_seccomp_allows_baseline_echo() {
  let tool = permissive_shell_with_sandbox();
  let result = tool
    .execute(json!({"command": "echo hello-from-seccomp"}))
    .await
    .expect("echo should succeed under seccomp filter");

  assert!(
    !result.is_error,
    "expected success, got error output: {}",
    result.content
  );
  assert!(
    result.content.contains("hello-from-seccomp"),
    "stdout did not contain expected token: {}",
    result.content
  );
}

#[tokio::test]
async fn linux_seccomp_blocks_socket_when_net_capability_absent() {
  if !python3_available() {
    eprintln!("skipping: python3 not on PATH");
    return;
  }

  let tool = permissive_shell_with_sandbox();
  // ShellTool's required capability set is `[Exec]` only; without Net, the
  // seccomp filter denies `socket(2)` with EPERM.
  let cmd = "python3 -c 'import socket; s = socket.socket(); print(\"opened\")'";
  let result = tool
    .execute(json!({"command": cmd}))
    .await
    .expect("tool call must complete");

  assert!(
    result.is_error,
    "expected seccomp to block socket(), but got success: {}",
    result.content
  );
  assert!(
    !result.content.contains("opened"),
    "socket() unexpectedly succeeded: {}",
    result.content
  );
}

/// Q1.1.2 deny-flow regression: without `FsWrite` the seccomp filter must
/// block `openat(AT_FDCWD, "/tmp/...", O_WRONLY|O_CREAT, ...)` even though
/// the legacy filter only denied path-mutation syscalls (`unlinkat`,
/// `linkat`, etc.). Pre-fix this test would have observed the file being
/// created successfully.
#[tokio::test]
async fn linux_seccomp_blocks_openat_with_o_creat_when_fs_write_absent() {
  if !python3_available() {
    eprintln!("skipping: python3 not on PATH");
    return;
  }

  let pid = std::process::id();
  let path = format!("/tmp/agentflow_sandbox_openat_creat_blocked_{pid}.txt");
  let _ = std::fs::remove_file(&path);

  let tool = permissive_shell_with_sandbox();
  // The single quotes pin the python source so the argv parser keeps the
  // metacharacters literal. The python program uses os.open() (which goes
  // directly through openat(2)) with O_WRONLY|O_CREAT — the seccomp filter
  // must convert this into EPERM.
  let cmd = format!(
    "python3 -c 'import os; \
       fd = os.open(\"{path}\", os.O_WRONLY | os.O_CREAT, 0o600); \
       os.write(fd, b\"breach\"); \
       os.close(fd); \
       print(\"created\")'"
  );
  let result = tool
    .execute(json!({"command": cmd}))
    .await
    .expect("tool call must complete");

  assert!(
    result.is_error,
    "expected seccomp to block openat(O_CREAT), but got success: {}",
    result.content
  );
  assert!(
    !result.content.contains("created"),
    "openat(O_CREAT) unexpectedly succeeded: {}",
    result.content
  );
  assert!(
    !std::path::Path::new(&path).exists(),
    "seccomp failed: file '{path}' was created despite missing FsWrite capability"
  );
}

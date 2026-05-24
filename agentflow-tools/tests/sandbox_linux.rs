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
use agentflow_tools::sandbox::{
  LinuxSeccompBackend, SandboxBackend as _, SandboxPolicy, SandboxScope,
};
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

/// Q1.1.4 deny-flow regression: a child process whose seccomp filter is
/// built without `Capability::Exec` must not be able to start at all. The
/// kernel applies the BPF filter via `pre_exec` after the fork but before
/// the parent's `execve(target_binary)`. Without `execve` in the allow set
/// that initial transition fails with `EPERM`, surfacing as an `io::Error`
/// from `Command::output()`.
///
/// This guarantees that even if a future change skipped the in-process
/// capability check, the kernel filter would still gate process creation.
#[tokio::test]
async fn linux_seccomp_no_exec_filter_blocks_child_spawn() {
  let backend = LinuxSeccompBackend::new();
  if !backend.is_enforcing() {
    eprintln!("skipping: seccomp backend is not enforcing on this arch");
    return;
  }

  let mut cmd = tokio::process::Command::new("/bin/true");
  backend
    .wrap_command(&mut cmd, &[], &SandboxScope::new())
    .expect("wrap_command compiles a no-Exec filter");

  // Spawning must fail (the std lib reports the EPERM from execve back to
  // the parent through the post-fork status pipe). We don't pin a specific
  // ErrorKind because Rust's Command can report this as PermissionDenied
  // or Other depending on the libc layer.
  let outcome = cmd.output().await;
  assert!(
    outcome.is_err(),
    "spawn under no-Exec seccomp filter must fail; instead got: {:?}",
    outcome
      .as_ref()
      .ok()
      .map(|o| String::from_utf8_lossy(&o.stderr).into_owned())
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

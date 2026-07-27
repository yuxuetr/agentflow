//! `code_exec` real-isolation integration tests — Linux/Podman path (S4.2
//! stretch goal).
//!
//! Mirrors a subset of `tests/code_exec_macos.rs`'s real-enforcement
//! coverage against rootless Podman instead of Apple's `container` CLI.
//!
//! `CodeExecTool` always requests a memory limit (unconditionally, by
//! design — see `code_exec.rs`'s module docs on mandatory strong
//! isolation), so **every** call here fails outright on a host where
//! Podman's `--memory` flag itself can't work, not just a
//! memory-limit-specific test. In the Linux dev environment this was
//! verified against (a minimal container init with no systemd), that's
//! exactly what happens: `crun: opening file 'memory.max' for writing: No
//! such file or directory`, because cgroup v2 delegation is unavailable —
//! the *exact same* "no systemd, root cgroup has member processes"
//! constraint S3.2 already documented and built a public probe for
//! (`agentflow_tools::sandbox::linux::cgroup_v2_delegation_available`).
//! Reused here as this file's skip guard rather than inventing new
//! detection logic or having `code_exec` silently degrade its isolation
//! guarantee. Network isolation, the CPU-seconds ulimit, and the
//! pids-limit were all independently confirmed working via raw `podman
//! run` in this same environment (S4.2 session notes) — the blocker is
//! specifically the memory cgroup controller, not Podman integration in
//! general. A host with proper cgroup v2 delegation (e.g. a standard
//! systemd-based Linux server) would not hit this at all.

#![cfg(target_os = "linux")]

use agentflow_tools::Tool;
use agentflow_tools::builtin::CodeExecTool;
use serde_json::json;

static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn container_engine_available() -> bool {
  CodeExecTool::new()
    .sandbox_status()
    .is_some_and(|status| status.enforcement == agentflow_tools::SandboxEnforcement::Enforcing)
}

fn cgroup_delegation_available() -> bool {
  agentflow_tools::sandbox::linux::cgroup_v2_delegation_available()
}

#[tokio::test]
async fn code_exec_runs_real_computation() {
  let _guard = TEST_LOCK.lock().await;
  if !container_engine_available() {
    eprintln!("skipping: no container engine ('container' or 'podman') on PATH");
    return;
  }
  if !cgroup_delegation_available() {
    eprintln!(
      "skipping: no writable cgroup v2 delegation available on this host — code_exec's \
       mandatory memory limit can't be applied (same root cause as S3.2's Linux cgroup \
       tests: no systemd / root cgroup has member processes)"
    );
    return;
  }
  let tool = CodeExecTool::new();
  let result = tool
    .execute(json!({"code": "print(sum(range(1000)))"}))
    .await
    .expect("tool call must complete");
  assert!(
    !result.is_error,
    "expected success, got: {}",
    result.content
  );
  assert_eq!(result.content, "499500");
}

#[tokio::test]
async fn code_exec_blocks_all_network_access() {
  let _guard = TEST_LOCK.lock().await;
  if !container_engine_available() {
    eprintln!("skipping: no container engine ('container' or 'podman') on PATH");
    return;
  }
  if !cgroup_delegation_available() {
    eprintln!(
      "skipping: no writable cgroup v2 delegation available on this host — code_exec's \
       mandatory memory limit can't be applied (same root cause as S3.2's Linux cgroup \
       tests: no systemd / root cgroup has member processes)"
    );
    return;
  }
  let tool = CodeExecTool::new();
  let code = r#"
import socket
import urllib.request

results = []
try:
    socket.setdefaulttimeout(3)
    socket.gethostbyname("example.com")
    results.append("DNS_RESOLVED")
except Exception:
    results.append("DNS_BLOCKED")
try:
    urllib.request.urlopen("http://example.com", timeout=3)
    results.append("HTTP_REACHABLE")
except Exception:
    results.append("HTTP_BLOCKED")
try:
    s = socket.create_connection(("8.8.8.8", 53), timeout=3)
    s.close()
    results.append("SOCKET_REACHABLE")
except Exception:
    results.append("SOCKET_BLOCKED")
print(",".join(results))
"#;
  let result = tool
    .execute(json!({"code": code}))
    .await
    .expect("tool call must complete");
  assert!(
    !result.is_error,
    "expected success, got: {}",
    result.content
  );
  assert_eq!(
    result.content, "DNS_BLOCKED,HTTP_BLOCKED,SOCKET_BLOCKED",
    "code_exec must have zero network access by default"
  );
}

#[tokio::test]
async fn code_exec_enforces_pids_limit() {
  let _guard = TEST_LOCK.lock().await;
  if !container_engine_available() {
    eprintln!("skipping: no container engine ('container' or 'podman') on PATH");
    return;
  }
  if !cgroup_delegation_available() {
    eprintln!(
      "skipping: no writable cgroup v2 delegation available on this host — code_exec's \
       mandatory memory limit can't be applied (same root cause as S3.2's Linux cgroup \
       tests: no systemd / root cgroup has member processes)"
    );
    return;
  }
  let tool = CodeExecTool::new();
  let code = r#"
import os
ok = 0
for i in range(60):
    pid = os.fork()
    if pid == 0:
        os._exit(0)
    ok += 1
print(ok)
for _ in range(ok):
    try:
        os.wait()
    except ChildProcessError:
        pass
"#;
  let result = tool
    .execute(json!({"code": code}))
    .await
    .expect("tool call must complete");
  // See the identical assertion in `tests/code_exec_macos.rs` for why an
  // `is_error` result must be checked for a fork-related signature rather
  // than accepted unconditionally — this exact leniency previously masked
  // a real `--uid` flag bug on Podman.
  if result.is_error {
    assert!(
      result.content.contains("BlockingIOError") || result.content.contains("OSError"),
      "expected a fork-related error proving the pids limit fired, got an unrelated \
       failure instead: {}",
      result.content
    );
  } else {
    let forked: i32 = result
      .content
      .trim()
      .parse()
      .unwrap_or_else(|_| panic!("expected a fork count, got: {}", result.content));
    assert!(
      forked < 60,
      "expected the pids limit to cap fork() well below 60, got {forked}"
    );
  }
}

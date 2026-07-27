//! `code_exec` real-isolation integration tests (S4.2).
//!
//! These drive the actual `CodeExecTool` end to end against a real
//! container engine — proving genuine isolation (network blocked, host
//! filesystem unreachable, resource limits enforced by the kernel), not
//! just that the tool compiles and returns success. Skips gracefully (with
//! a clear message) on a host with no container engine on PATH, mirroring
//! the `landlock_enforcing()` / `cgroup_delegation_available()` skip-guard
//! pattern already established for S3.1/S3.2's real-enforcement tests.

#![cfg(target_os = "macos")]

use agentflow_tools::Tool;
use agentflow_tools::builtin::CodeExecTool;
use serde_json::json;

fn container_engine_available() -> bool {
  CodeExecTool::new()
    .sandbox_status()
    .is_some_and(|status| status.enforcement == agentflow_tools::SandboxEnforcement::Enforcing)
}

/// `code_exec_tears_down_its_ephemeral_workdir` below scans the shared
/// system temp directory for entries matching `code_exec`'s workdir
/// prefix — a real flake was observed (this session) when Rust's default
/// parallel test runner ran that scan concurrently with a sibling test's
/// own `CodeExecTool` call: the sibling's still-in-flight workdir showed up
/// as a false "leak". Every test in this file takes this lock for its
/// duration so the scan never overlaps another call creating one of these
/// directories.
static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[tokio::test]
async fn code_exec_runs_real_computation() {
  let _guard = TEST_LOCK.lock().await;
  if !container_engine_available() {
    eprintln!("skipping: no container engine ('container' or 'podman') on PATH");
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

/// The single most important test: proves the code actually runs inside a
/// separate kernel, not just as a subprocess of the test runner. A bug that
/// silently downgraded `ContainerBackend::wrap_command` into a no-op would
/// still pass `code_exec_runs_real_computation` above (the host almost
/// certainly has *some* `python3`) — only an assertion about which OS
/// actually executed the code catches that.
#[tokio::test]
async fn code_exec_runs_inside_a_separate_linux_kernel_not_the_host() {
  let _guard = TEST_LOCK.lock().await;
  if !container_engine_available() {
    eprintln!("skipping: no container engine ('container' or 'podman') on PATH");
    return;
  }
  let tool = CodeExecTool::new();
  let result = tool
    .execute(json!({"code": "import platform; print(platform.system())"}))
    .await
    .expect("tool call must complete");
  assert!(
    !result.is_error,
    "expected success, got: {}",
    result.content
  );
  assert_eq!(
    result.content, "Linux",
    "code_exec must run inside a Linux container/VM even on a macOS host — \
     if this reports anything else, isolation is not actually active"
  );
}

#[tokio::test]
async fn code_exec_blocks_all_network_access() {
  let _guard = TEST_LOCK.lock().await;
  if !container_engine_available() {
    eprintln!("skipping: no container engine ('container' or 'podman') on PATH");
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
async fn code_exec_host_filesystem_is_unreachable() {
  let _guard = TEST_LOCK.lock().await;
  if !container_engine_available() {
    eprintln!("skipping: no container engine ('container' or 'podman') on PATH");
    return;
  }
  let tool = CodeExecTool::new();
  // `/Users` only exists on the macOS host, never inside a Linux container
  // image — its absence proves the host root filesystem isn't bind-mounted
  // anywhere reachable, not just that this particular file wasn't granted.
  let code = r#"
import os
print("HOST_ROOT_VISIBLE" if os.path.exists("/Users") else "HOST_ROOT_HIDDEN")
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
  assert_eq!(result.content, "HOST_ROOT_HIDDEN");
}

#[tokio::test]
async fn code_exec_enforces_memory_limit() {
  let _guard = TEST_LOCK.lock().await;
  if !container_engine_available() {
    eprintln!("skipping: no container engine ('container' or 'podman') on PATH");
    return;
  }
  let tool = CodeExecTool::new();
  // Comfortably above the 256 MiB default cap; must be denied rather than
  // silently allowed to consume unbounded host memory.
  let code = "data = bytearray(512 * 1024 * 1024)\nprint('should not get here')";
  let result = tool
    .execute(json!({"code": code}))
    .await
    .expect("tool call must complete");
  assert!(
    result.is_error,
    "expected the 256 MiB cap to reject a 512 MiB allocation, got success: {}",
    result.content
  );
}

#[tokio::test]
async fn code_exec_enforces_cpu_limit() {
  let _guard = TEST_LOCK.lock().await;
  if !container_engine_available() {
    eprintln!("skipping: no container engine ('container' or 'podman') on PATH");
    return;
  }
  let tool = CodeExecTool::new();
  // A busy loop with no I/O — must be killed by the CPU-seconds ulimit
  // well before the 30s spawn timeout would otherwise fire.
  let code = "\
x = 0
while True:
    x += 1
";
  let started = std::time::Instant::now();
  let result = tool
    .execute(json!({"code": code}))
    .await
    .expect("tool call must complete");
  let elapsed = started.elapsed();
  assert!(
    result.is_error,
    "expected the CPU-seconds ulimit to kill the busy loop, got success: {}",
    result.content
  );
  assert!(
    elapsed < std::time::Duration::from_secs(40),
    "expected the ulimit (30s CPU budget) to fire noticeably before the 45s wall-clock \
     spawn timeout backstop, took {elapsed:?}"
  );
}

#[tokio::test]
async fn code_exec_enforces_pids_limit() {
  let _guard = TEST_LOCK.lock().await;
  if !container_engine_available() {
    eprintln!("skipping: no container engine ('container' or 'podman') on PATH");
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
  // Either the fixture's own `os.fork()` starts failing once the pids limit
  // is hit (a `BlockingIOError`/`OSError` traceback, `is_error`) or it
  // successfully counts how many succeeded before running out — either way
  // the count must be well below the 60 attempted, proving the cap actually
  // bit. Any *other* error (e.g. an unrelated engine flag failure — this
  // exact "either/or" shape previously masked a real `--uid` flag bug on
  // Podman by treating any `is_error` as proof of enforcement) must fail
  // loudly instead of being silently accepted as the expected outcome.
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

#[tokio::test]
async fn code_exec_tears_down_its_ephemeral_workdir() {
  let _guard = TEST_LOCK.lock().await;
  if !container_engine_available() {
    eprintln!("skipping: no container engine ('container' or 'podman') on PATH");
    return;
  }
  // Filter by code_exec's distinguishable workdir prefix rather than
  // diffing the whole system temp dir — the latter is racy against
  // unrelated churn from other processes (e.g. the container engine's own
  // transient bookkeeping files), which caused a real flake here.
  fn code_exec_workdirs() -> std::collections::HashSet<std::path::PathBuf> {
    std::fs::read_dir(std::env::temp_dir())
      .expect("read temp dir")
      .filter_map(|e| e.ok().map(|e| e.path()))
      .filter(|p| {
        p.file_name()
          .and_then(|n| n.to_str())
          .is_some_and(|n| n.starts_with("agentflow-code-exec-"))
      })
      .collect()
  }

  let before = code_exec_workdirs();

  let tool = CodeExecTool::new();
  let result = tool
    .execute(json!({"code": "print('done')"}))
    .await
    .expect("tool call must complete");
  assert!(
    !result.is_error,
    "expected success, got: {}",
    result.content
  );

  let after = code_exec_workdirs();
  let leaked: Vec<_> = after.difference(&before).collect();
  assert!(
    leaked.is_empty(),
    "code_exec's ephemeral workdir must be torn down after execute() returns, leaked: {leaked:?}"
  );
}

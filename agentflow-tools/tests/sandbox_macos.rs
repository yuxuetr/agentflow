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

use std::path::Path;
use std::sync::Arc;

use agentflow_tools::builtin::{ScriptTool, ShellTool};
use agentflow_tools::sandbox::{
  MacosSandboxExecBackend, SandboxBackend as _, SandboxPolicy, SandboxScope,
};
use agentflow_tools::{Capability, Tool};
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

/// Q1.1.3 regression: the SBPL profile used to grant `(allow file-read*
/// (subpath "/Library"))` wholesale. We now grant only `/Library/Frameworks`
/// and the global preferences plist literal, so reading an arbitrary
/// `/Library` subtree must be denied by the sandbox kernel.
#[tokio::test]
async fn macos_sandbox_denies_blanket_library_read() {
  if !sandbox_exec_usable() {
    eprintln!("skipping: sandbox-exec is present but not usable in this environment");
    return;
  }

  // `/Library/Preferences/SystemConfiguration/preferences.plist` exists on
  // every macOS host (network configuration plist) and is precisely the
  // kind of file the old blanket grant exposed. It is outside the narrow
  // literals we still allow.
  let target = "/Library/Preferences/SystemConfiguration/preferences.plist";
  if !std::path::Path::new(target).exists() {
    eprintln!("skipping: '{target}' not present, cannot exercise read-denied path");
    return;
  }

  let tool = permissive_shell_with_sandbox();
  let cmd = format!("/bin/cat {target}");
  let result = tool
    .execute(json!({"command": cmd}))
    .await
    .expect("tool call must complete");

  assert!(
    result.is_error,
    "expected sandbox to block /Library blanket read, but got success: {}",
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

// ── S3.3: real interpreter environments under enforcing mode ──────────────
//
// Before S3.3, `os_sandbox: true` broke every `.py` script whose
// interpreter lived outside the hardcoded baseline (`/usr/bin`, `/System`,
// ...) — which is virtually every real Python install: Homebrew, pyenv, and
// (S2) a per-skill venv, whose `bin/python3` is itself typically a symlink
// into a Homebrew Cellar path. dyld refused to load the interpreter's own
// runtime library because that real location was never granted read
// access. These tests build a *real* venv with a *real* installed
// dependency and prove it now imports successfully under enforcing mode.

fn python3_available() -> bool {
  std::process::Command::new("python3")
    .arg("--version")
    .output()
    .is_ok_and(|o| o.status.success())
}

/// Hand-builds a minimal, dependency-free wheel via stdlib `zipfile` (no
/// pip/setuptools/build-backend required) and installs it into a fresh venv
/// under `dir` via `pip install --require-hashes`, mirroring
/// `agentflow-skills/src/python_env.rs`'s offline install path. Returns the
/// venv's python3 path.
fn build_real_venv_with_test_package(dir: &Path) -> std::path::PathBuf {
  let vendor_dir = dir.join("vendor");
  std::fs::create_dir(&vendor_dir).unwrap();
  let wheel_path = vendor_dir.join("agentflow_test_pkg-1.0.0-py3-none-any.whl");
  let build_wheel_script = dir.join("build_wheel.py");
  std::fs::write(&build_wheel_script, WHEEL_BUILDER_SCRIPT).unwrap();
  let status = std::process::Command::new("python3")
    .arg(&build_wheel_script)
    .arg(&wheel_path)
    .status()
    .expect("spawn wheel builder");
  assert!(status.success(), "failed to build the test fixture wheel");

  let wheel_hash = {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(std::fs::read(&wheel_path).unwrap());
    format!("{:x}", hasher.finalize())
  };
  let requirements_path = dir.join("requirements.txt");
  std::fs::write(
    &requirements_path,
    format!("agentflow-test-pkg==1.0.0 --hash=sha256:{wheel_hash}\n"),
  )
  .unwrap();

  let venv_dir = dir.join(".venv");
  let status = std::process::Command::new("python3")
    .arg("-m")
    .arg("venv")
    .arg(&venv_dir)
    .status()
    .expect("spawn python3 -m venv");
  assert!(status.success(), "failed to create venv");
  let python_bin = venv_dir.join("bin").join("python3");

  let status = std::process::Command::new(&python_bin)
    .arg("-m")
    .arg("pip")
    .arg("install")
    .arg("--no-index")
    .arg("--find-links")
    .arg(&vendor_dir)
    .arg("--require-hashes")
    .arg("-r")
    .arg(&requirements_path)
    .status()
    .expect("spawn pip install");
  assert!(status.success(), "failed to install the test fixture wheel");

  python_bin
}

const WHEEL_BUILDER_SCRIPT: &str = r#"
import sys
import zipfile

out_path = sys.argv[1]
dist_info = "agentflow_test_pkg-1.0.0.dist-info"

with zipfile.ZipFile(out_path, "w") as zf:
    zf.writestr("agentflow_test_pkg.py", "VALUE = 42\n")
    zf.writestr(
        f"{dist_info}/METADATA",
        "Metadata-Version: 2.1\nName: agentflow-test-pkg\nVersion: 1.0.0\n",
    )
    zf.writestr(
        f"{dist_info}/WHEEL",
        "Wheel-Version: 1.0\nGenerator: agentflow-test\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
    )
    zf.writestr(f"{dist_info}/RECORD", "")
"#;

#[tokio::test]
async fn macos_sandbox_allows_venv_python_to_import_installed_package() {
  if !sandbox_exec_usable() {
    eprintln!("skipping: sandbox-exec is present but not usable in this environment");
    return;
  }
  if !python3_available() {
    eprintln!("skipping: python3 not on PATH");
    return;
  }

  let dir = tempfile::TempDir::new().unwrap();
  let scripts_dir = dir.path().join("scripts");
  std::fs::create_dir(&scripts_dir).unwrap();
  std::fs::write(
    scripts_dir.join("run.py"),
    "import agentflow_test_pkg\nprint(agentflow_test_pkg.VALUE)\n",
  )
  .unwrap();

  let python_bin = build_real_venv_with_test_package(dir.path());

  let policy = SandboxPolicy {
    allowed_commands: vec!["python3".to_string()],
    allowed_paths: vec![scripts_dir.clone()],
    ..SandboxPolicy::default()
  };
  let tool = ScriptTool::new(scripts_dir, Arc::new(policy))
    .with_python_interpreter(python_bin)
    .with_os_sandbox();

  let result = tool
    .execute(json!({"script": "run.py"}))
    .await
    .expect("tool call must complete");

  assert!(
    !result.is_error,
    "expected the venv-installed package to import successfully under enforcing \
     sandbox, got: {}",
    result.content
  );
  assert_eq!(result.content.trim(), "42");
}

/// The S3.3 grant is scoped to the resolved interpreter's own install
/// prefix — it must not become a general escape hatch. A venv-backed
/// script still can't read an arbitrary path outside every granted scope.
#[tokio::test]
async fn macos_sandbox_venv_python_still_denied_outside_granted_scope() {
  if !sandbox_exec_usable() {
    eprintln!("skipping: sandbox-exec is present but not usable in this environment");
    return;
  }
  if !python3_available() {
    eprintln!("skipping: python3 not on PATH");
    return;
  }

  let target = "/Library/Preferences/SystemConfiguration/preferences.plist";
  if !std::path::Path::new(target).exists() {
    eprintln!("skipping: '{target}' not present, cannot exercise read-denied path");
    return;
  }

  let dir = tempfile::TempDir::new().unwrap();
  let scripts_dir = dir.path().join("scripts");
  std::fs::create_dir(&scripts_dir).unwrap();
  std::fs::write(
    scripts_dir.join("run.py"),
    format!("open({target:?}, 'rb').read()\n"),
  )
  .unwrap();

  let python_bin = build_real_venv_with_test_package(dir.path());

  let policy = SandboxPolicy {
    allowed_commands: vec!["python3".to_string()],
    allowed_paths: vec![scripts_dir.clone()],
    ..SandboxPolicy::default()
  };
  let tool = ScriptTool::new(scripts_dir, Arc::new(policy))
    .with_python_interpreter(python_bin)
    .with_os_sandbox();

  let result = tool
    .execute(json!({"script": "run.py"}))
    .await
    .expect("tool call must complete");

  assert!(
    result.is_error,
    "expected sandbox to still deny a read outside every granted scope, got success: {}",
    result.content
  );
}

// ── S3.2: resource limits via RLIMIT_* ──────────────────────────────────

/// Compile a tiny, dependency-free, pure-CPU busy-loop binary (`for(;;)
/// x++;`, no syscalls in the hot loop, no output) and return its path.
///
/// Deliberately not `/usr/bin/python3` (a modern-macOS `xcrun` shim
/// needing Xcode/CommandLineTools read grants no minimal test scope has)
/// nor `/usr/bin/yes` (empirically — confirmed independently of this
/// crate's code via plain `bash -c 'ulimit -t 1; exec /usr/bin/yes'` —
/// does *not* reliably get killed by `RLIMIT_CPU` within several seconds
/// on this platform, apparently because its tight `write(2)` loop doesn't
/// accumulate accounted CPU time the way a pure user-space loop does;
/// `python3`'s equivalent busy loop *does* get killed correctly under the
/// exact same `ulimit -t 1`, confirming this is a property of the test
/// *subject*, not of `RLIMIT_CPU` or this module's `pre_exec` wiring).
/// Only depends on `/usr/lib/libSystem.B.dylib`, already covered by the
/// backend's unconditional baseline SBPL grant, so it needs no additional
/// scope beyond what every sandboxed spawn already gets.
fn compile_busy_loop_binary(dir: &std::path::Path) -> std::path::PathBuf {
  let src = dir.join("busy_loop.c");
  std::fs::write(
    &src,
    "int main(void) { volatile long x = 0; for (;;) { x++; } return 0; }\n",
  )
  .expect("write busy_loop.c");
  let bin = dir.join("busy_loop");
  let status = std::process::Command::new("cc")
    .arg("-O2")
    .arg("-o")
    .arg(&bin)
    .arg(&src)
    .status()
    .expect("invoke cc");
  assert!(
    status.success(),
    "cc failed to compile the busy-loop test fixture"
  );
  bin
}

/// S3.2 literal regression: `max_cpu_secs` must actually bound CPU time —
/// a busy loop that would otherwise spin forever must be killed once it
/// exceeds its CPU-second budget.
///
/// Drives [`MacosSandboxExecBackend::wrap_command`] directly (rather than
/// through `ShellTool`) so the test isolates the S3.2 mechanism itself
/// from unrelated `ShellTool`/SBPL-profile plumbing (command parsing,
/// shell-mode `getcwd` requirements, output capture) that isn't what
/// this test is about.
#[tokio::test]
async fn macos_sandbox_enforces_max_cpu_secs_via_rlimit() {
  if !sandbox_exec_usable() {
    eprintln!("skipping: sandbox-exec is present but not usable in this environment");
    return;
  }
  if std::process::Command::new("cc")
    .arg("--version")
    .output()
    .map(|out| !out.status.success())
    .unwrap_or(true)
  {
    eprintln!("skipping: no working `cc` to compile the busy-loop test fixture");
    return;
  }

  let dir = tempfile::TempDir::new().expect("create temp dir");
  let busy_loop = compile_busy_loop_binary(dir.path());

  let backend = MacosSandboxExecBackend::new();
  let scope = SandboxScope::new().with_max_cpu_secs(1);
  let mut cmd = tokio::process::Command::new(&busy_loop);
  backend
    .wrap_command(&mut cmd, &[Capability::Exec], &scope)
    .expect("wrap_command must succeed");
  cmd.stdout(std::process::Stdio::null());
  cmd.stderr(std::process::Stdio::null());

  let start = std::time::Instant::now();
  let status = cmd.status().await.expect("child must spawn");
  let elapsed = start.elapsed();

  assert!(
    !status.success(),
    "expected RLIMIT_CPU to kill the busy loop, but it exited successfully"
  );
  assert!(
    elapsed < std::time::Duration::from_secs(10),
    "expected the RLIMIT_CPU kill within a few seconds of the 1s budget, took {elapsed:?}"
  );
}

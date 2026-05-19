//! Plugin entrypoint smoke runner (P3.4-PR.1).
//!
//! Spawns a plugin's `[plugin.dry_run]` invocation and verifies it
//! exits cleanly within the configured timeout. The smoke is
//! intentionally cheaper than the full JSON-RPC handshake — it's
//! "does this binary at least start" rather than "can it serve
//! requests". Doctor uses it to flag plugins whose entrypoint binary
//! is broken (wrong arch, missing libs, refuses to spawn) before
//! the operator notices at workflow-run time.
//!
//! The runner does not wrap the spawn in any OS sandbox. Production
//! plugin spawns go through `agentflow-cli`'s sandbox-aware
//! `CommandPreparer`; the smoke runs the entrypoint verbatim because
//! it's a host-side diagnostic, not a production execution path. The
//! `dry_run` invocation is expected to be side-effect-free by
//! contract.

use crate::plugin::manifest::{DryRunSpec, PluginManifest};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use thiserror::Error;
use tokio::process::Command;
use tokio::time::timeout;

/// Outcome of running `[plugin.dry_run]` against a plugin manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DryRunOutcome {
  /// Manifest has no `[plugin.dry_run]` section — nothing to smoke.
  Skipped,
  /// Smoke ran and the entrypoint exited with the expected status
  /// inside the timeout. Carries the actual exit code so consumers
  /// can render diagnostics.
  Passed { exit_code: i32 },
  /// Smoke ran but the entrypoint exited with the wrong code, hung
  /// past the timeout, or failed to spawn.
  Failed(DryRunFailure),
}

/// Why a dry-run smoke failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DryRunFailure {
  /// Spawn returned a non-zero / unexpected exit code. Wall-clock
  /// duration is informational only — the failure is the exit code.
  #[error("plugin dry_run exited with code {actual} (expected {expected})")]
  WrongExitCode { expected: i32, actual: i32 },
  /// Entrypoint exited via signal (Unix only). Treated as failure
  /// because the smoke contract says "exit cleanly", not "die from
  /// SIGTERM".
  #[error("plugin dry_run was terminated by signal{}", match .signal { Some(s) => format!(" {s}"), None => String::new() })]
  KilledBySignal { signal: Option<i32> },
  /// Wall-clock timeout exceeded.
  #[error("plugin dry_run timed out after {timeout_ms}ms")]
  Timeout { timeout_ms: u32 },
  /// Spawn itself failed — usually missing binary, wrong arch, or
  /// permission error.
  #[error("plugin dry_run failed to spawn: {reason}")]
  SpawnFailed { reason: String },
}

/// Run the `[plugin.dry_run]` smoke for `manifest` whose
/// `entrypoint` resolves relative to `manifest_dir`.
///
/// Returns [`DryRunOutcome::Skipped`] when the manifest has no
/// `dry_run` section — the operator opted out, no signal is needed.
///
/// The function holds no global state and is safe to call from
/// concurrent tasks (each call gets its own child process).
pub async fn run_dry_run(
  manifest: &PluginManifest,
  manifest_dir: &Path,
) -> DryRunOutcome {
  let Some(spec) = manifest.plugin.dry_run.as_ref() else {
    return DryRunOutcome::Skipped;
  };
  let entrypoint = manifest.resolve_entrypoint(manifest_dir);
  run_dry_run_spec(&entrypoint, spec).await
}

/// Lower-level helper exposed for tests + reuse: given an absolute
/// entrypoint path and a [`DryRunSpec`], spawn and wait.
pub async fn run_dry_run_spec(entrypoint: &Path, spec: &DryRunSpec) -> DryRunOutcome {
  let mut cmd = Command::new(entrypoint);
  cmd.args(&spec.args)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .kill_on_drop(true);

  let mut child = match cmd.spawn() {
    Ok(child) => child,
    Err(e) => {
      return DryRunOutcome::Failed(DryRunFailure::SpawnFailed {
        reason: e.to_string(),
      });
    }
  };

  let wait_fut = child.wait();
  let status = match timeout(Duration::from_millis(spec.timeout_ms.into()), wait_fut).await {
    Ok(Ok(status)) => status,
    Ok(Err(e)) => {
      return DryRunOutcome::Failed(DryRunFailure::SpawnFailed {
        reason: format!("wait failed: {e}"),
      });
    }
    Err(_) => {
      // tokio::time::timeout fired. The child stays referenced for
      // kill_on_drop to terminate when we leave the function; no
      // explicit kill needed here.
      return DryRunOutcome::Failed(DryRunFailure::Timeout {
        timeout_ms: spec.timeout_ms,
      });
    }
  };

  match status.code() {
    Some(code) if code == spec.expected_exit => DryRunOutcome::Passed { exit_code: code },
    Some(code) => DryRunOutcome::Failed(DryRunFailure::WrongExitCode {
      expected: spec.expected_exit,
      actual: code,
    }),
    None => {
      #[cfg(unix)]
      let signal = {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
      };
      #[cfg(not(unix))]
      let signal = None;
      DryRunOutcome::Failed(DryRunFailure::KilledBySignal { signal })
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::path::PathBuf;

  fn spec(args: &[&str], timeout_ms: u32, expected_exit: i32) -> DryRunSpec {
    DryRunSpec {
      args: args.iter().map(|s| s.to_string()).collect(),
      timeout_ms,
      expected_exit,
    }
  }

  // ── Pure spec validation (no spawn) ─────────────────────────────────

  #[test]
  fn dry_run_spec_validate_rejects_empty_args() {
    let s = DryRunSpec {
      args: vec![],
      timeout_ms: 1000,
      expected_exit: 0,
    };
    let err = s.validate().unwrap_err();
    assert!(err.to_string().contains("at least one argument"));
  }

  #[test]
  fn dry_run_spec_validate_rejects_zero_timeout() {
    let s = DryRunSpec {
      args: vec!["--smoke".to_string()],
      timeout_ms: 0,
      expected_exit: 0,
    };
    let err = s.validate().unwrap_err();
    assert!(err.to_string().contains("timeout_ms must be > 0"));
  }

  #[test]
  fn dry_run_spec_validate_accepts_minimal_valid_shape() {
    let s = DryRunSpec {
      args: vec!["--smoke".to_string()],
      timeout_ms: 1,
      expected_exit: 0,
    };
    assert!(s.validate().is_ok());
  }

  // ── Smoke against real /bin/sh — Unix-only ──────────────────────────

  #[cfg(unix)]
  #[tokio::test]
  async fn passes_when_shell_exits_zero_within_timeout() {
    let outcome =
      run_dry_run_spec(&PathBuf::from("/bin/sh"), &spec(&["-c", "exit 0"], 5000, 0)).await;
    assert_eq!(outcome, DryRunOutcome::Passed { exit_code: 0 });
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn fails_with_wrong_exit_code_when_status_mismatches() {
    // /bin/false always exits 1. Expecting 0 ⇒ WrongExitCode.
    let outcome =
      run_dry_run_spec(&PathBuf::from("/bin/sh"), &spec(&["-c", "exit 1"], 5000, 0)).await;
    assert_eq!(
      outcome,
      DryRunOutcome::Failed(DryRunFailure::WrongExitCode {
        expected: 0,
        actual: 1,
      })
    );
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn fails_with_timeout_when_smoke_hangs_past_budget() {
    // Sleep longer than the budget so timeout fires deterministically.
    let outcome =
      run_dry_run_spec(&PathBuf::from("/bin/sh"), &spec(&["-c", "sleep 5"], 100, 0)).await;
    assert_eq!(
      outcome,
      DryRunOutcome::Failed(DryRunFailure::Timeout { timeout_ms: 100 })
    );
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn fails_with_spawn_error_when_entrypoint_missing() {
    let outcome = run_dry_run_spec(
      &PathBuf::from("/nonexistent/path/that/should/never/exist"),
      &spec(&["--smoke"], 1000, 0),
    )
    .await;
    match outcome {
      DryRunOutcome::Failed(DryRunFailure::SpawnFailed { reason }) => {
        // Don't assert exact OS-specific message; just check it surfaces.
        assert!(!reason.is_empty(), "spawn-failed reason should be populated");
      }
      other => panic!("expected SpawnFailed, got {other:?}"),
    }
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn nonzero_expected_exit_passes_when_status_matches() {
    // A plugin that uses exit code 64 (usage error) as its dry-run
    // success: expected = 64, actual = 64 ⇒ Passed.
    let outcome = run_dry_run_spec(
      &PathBuf::from("/bin/sh"),
      &spec(&["-c", "exit 64"], 5000, 64),
    )
    .await;
    assert_eq!(outcome, DryRunOutcome::Passed { exit_code: 64 });
  }

  // ── Manifest-level wrapper ──────────────────────────────────────────

  #[tokio::test]
  async fn run_dry_run_skips_when_manifest_has_no_dry_run() {
    use crate::plugin::manifest::{
      Capabilities, PluginManifest, PluginRuntime, PluginSection, SUPPORTED_PROTOCOL_VERSION,
    };
    let manifest = PluginManifest {
      plugin: PluginSection {
        name: "no-smoke".into(),
        version: "0.1.0".into(),
        runtime: PluginRuntime::Subprocess,
        entrypoint: PathBuf::from("./bin/unused"),
        protocol: SUPPORTED_PROTOCOL_VERSION.into(),
        nodes: vec![],
        capabilities: Capabilities::default(),
        dry_run: None,
      },
    };
    let outcome = run_dry_run(&manifest, Path::new(".")).await;
    assert_eq!(outcome, DryRunOutcome::Skipped);
  }
}

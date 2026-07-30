//! Platform-abstracted OS sandbox contract.
//!
//! A [`SandboxBackend`] wraps a [`tokio::process::Command`] in OS-level
//! sandbox primitives before the caller spawns it. The capability set
//! drives what the kernel will allow the child to do.
//!
//! T3.3: this module holds only the *contract* — the trait, its DTOs
//! (`SandboxScope`/`SandboxStatus`/`SandboxEnforcement`/`SandboxError`) —
//! not any concrete backend. The concrete backends (`sandbox-exec` profile
//! generation, seccomp+Landlock+cgroup v2, a real container-engine driver,
//! a no-op) and the `default_backend()` platform-dispatch function that
//! picks between them live in `agentflow-tools` (the builtin-impl crate),
//! which depends on this crate and re-exports everything here so existing
//! `agentflow_tools::sandbox::*` call sites are unaffected.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::process::Command;

use crate::capability::Capability;

/// Scope passed to the backend describing which paths and network access the
/// child process should be permitted, and the file under which a generated
/// profile (if any) may live.
#[derive(Debug, Clone, Default)]
pub struct SandboxScope {
  /// Paths the child may read.
  pub read_paths: Vec<PathBuf>,
  /// Paths the child may write to.
  pub write_paths: Vec<PathBuf>,
  /// Working directory of the child (helps backends pre-allow access).
  pub working_directory: Option<PathBuf>,
  /// S3.2: maximum resident memory in bytes. `None` means no limit. See
  /// `agentflow_tools::sandbox::SandboxPolicy::max_memory_bytes` for the
  /// per-platform enforcement mechanism.
  pub max_memory_bytes: Option<u64>,
  /// S3.2: maximum process/thread count. `None` means no limit. See
  /// `agentflow_tools::sandbox::SandboxPolicy::max_pids`.
  pub max_pids: Option<u32>,
  /// S3.2: maximum CPU time in seconds. `None` means no limit. See
  /// `agentflow_tools::sandbox::SandboxPolicy::max_cpu_secs`.
  pub max_cpu_secs: Option<u64>,
  /// S4.2: a stable, caller-chosen name/ID for this spawn, consumed only
  /// by `agentflow_tools::sandbox::ContainerBackend` (passed as `--name` to
  /// the container engine) so [`SandboxBackend::terminate`] can later
  /// address this specific instance even after the `Command`'s own `Child`
  /// process (the engine CLI client, not the container itself) is gone.
  /// `None` means the backend picks whatever default naming its engine
  /// uses — fine for callers that never need to force-terminate early.
  pub container_name: Option<String>,
}

impl SandboxScope {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn with_read_paths<I, P>(mut self, paths: I) -> Self
  where
    I: IntoIterator<Item = P>,
    P: Into<PathBuf>,
  {
    self.read_paths.extend(paths.into_iter().map(Into::into));
    self
  }

  pub fn with_write_paths<I, P>(mut self, paths: I) -> Self
  where
    I: IntoIterator<Item = P>,
    P: Into<PathBuf>,
  {
    self.write_paths.extend(paths.into_iter().map(Into::into));
    self
  }

  pub fn with_working_directory<P: Into<PathBuf>>(mut self, dir: P) -> Self {
    self.working_directory = Some(dir.into());
    self
  }

  pub fn with_max_memory_bytes(mut self, bytes: u64) -> Self {
    self.max_memory_bytes = Some(bytes);
    self
  }

  pub fn with_max_pids(mut self, pids: u32) -> Self {
    self.max_pids = Some(pids);
    self
  }

  pub fn with_max_cpu_secs(mut self, secs: u64) -> Self {
    self.max_cpu_secs = Some(secs);
    self
  }

  pub fn with_container_name(mut self, name: impl Into<String>) -> Self {
    self.container_name = Some(name.into());
    self
  }
}

/// Errors returned by sandbox backends when they cannot enforce a request.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SandboxError {
  /// The current platform has no enforcing backend available.
  #[error("sandbox backend '{platform}' is not available: {message}")]
  Unsupported {
    platform: &'static str,
    message: String,
  },

  /// The backend could not produce its profile or filter.
  #[error("sandbox backend failed to prepare enforcement: {message}")]
  Prepare { message: String },

  /// I/O error materialising profile or temp files.
  #[error("sandbox backend I/O error: {0}")]
  Io(#[from] std::io::Error),
}

/// Observable enforcement state of a [`SandboxBackend`].
///
/// `is_enforcing()` collapses this to a boolean for legacy code paths;
/// `enforcement_level()` differentiates between "actively enforcing",
/// "platform supports a backend but it cannot enforce right now"
/// (`Permissive`), and "no enforcing backend is available on this platform"
/// (`Disabled`). The distinction matters in trace events and doctor output
/// because `Permissive` usually points at a misconfiguration (missing
/// `sandbox-exec`, unsupported arch) while `Disabled` is the steady state on
/// Windows or other platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxEnforcement {
  /// Backend is installed and actively constrains the child process.
  Enforcing,
  /// Backend exists for this platform but cannot enforce in the current
  /// environment (e.g. `sandbox-exec` binary missing, arch unsupported).
  Permissive,
  /// No enforcing backend is available on this platform (no-op).
  Disabled,
}

impl SandboxEnforcement {
  /// Stable token used in trace events and CLI output.
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::Enforcing => "enforcing",
      Self::Permissive => "permissive",
      Self::Disabled => "disabled",
    }
  }

  /// Whether this state should be treated as a guarantee that the OS will
  /// constrain the child. Only `Enforcing` returns `true`.
  pub fn is_enforcing(&self) -> bool {
    matches!(self, Self::Enforcing)
  }
}

/// Snapshot of a sandbox backend suitable for serialisation into trace
/// events, capability decisions, and doctor diagnostics. Always emitted by
/// tools that may wrap a child process through a backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxStatus {
  /// Stable backend name (`"sandbox-exec"`, `"seccomp"`, `"noop"`).
  pub backend: String,
  /// Current enforcement state.
  pub enforcement: SandboxEnforcement,
}

impl SandboxStatus {
  pub fn new(backend: impl Into<String>, enforcement: SandboxEnforcement) -> Self {
    Self {
      backend: backend.into(),
      enforcement,
    }
  }

  /// Snapshot the state of a backend through a trait reference. Convenience
  /// helper used by tools that hold an `Arc<dyn SandboxBackend>`.
  pub fn from_backend(backend: &dyn SandboxBackend) -> Self {
    Self {
      backend: backend.name().to_string(),
      enforcement: backend.enforcement_level(),
    }
  }
}

/// Wrap a child process in OS-level sandbox primitives.
pub trait SandboxBackend: Send + Sync {
  /// Stable name (`"sandbox-exec"`, `"seccomp"`, `"noop"`).
  fn name(&self) -> &'static str;

  /// Whether this backend actually enforces anything. `NoopSandboxBackend`
  /// returns `false`; callers can use this to refuse silent fall-through.
  fn is_enforcing(&self) -> bool;

  /// Tri-state enforcement classification.
  ///
  /// The default implementation derives from [`Self::is_enforcing`]: `true`
  /// maps to [`SandboxEnforcement::Enforcing`] and `false` to
  /// [`SandboxEnforcement::Disabled`]. Backends that can be in a non-enforcing
  /// state for a *recoverable* reason (e.g. macOS without `sandbox-exec` on
  /// the path, Linux on an unsupported arch) should override to return
  /// [`SandboxEnforcement::Permissive`] so operators can distinguish "no
  /// platform backend exists" from "platform backend exists but isn't
  /// enforcing right now".
  fn enforcement_level(&self) -> SandboxEnforcement {
    if self.is_enforcing() {
      SandboxEnforcement::Enforcing
    } else {
      SandboxEnforcement::Disabled
    }
  }

  /// Configure `command` so that, when spawned, the child runs inside the
  /// sandbox bounded by `effective_capabilities` and `scope`.
  ///
  /// Backends may rewrite the command (e.g. macOS wraps it in
  /// `sandbox-exec`). Backends that install in-child filters (e.g. Linux
  /// seccomp via `pre_exec`) return without rewriting the program.
  ///
  /// **Caller contract (S3.3 finding):** configure `command`'s program,
  /// args, `current_dir`, and `env` before calling this, but configure
  /// `stdin`/`stdout`/`stderr` *after* — a backend that rewrites the
  /// command wholesale (macOS) cannot read back and re-apply stdio
  /// settings a caller made beforehand (`std::process::Command` exposes no
  /// getter for them), so anything set before this call is silently lost
  /// for a rewriting backend. `ShellTool` never trips this because it
  /// configures stdio via `Command::output()`'s own internals, which run
  /// after `wrap_command`; `ScriptTool` needs an explicit stdin pipe (to
  /// write JSON args) and must set it up after, not before.
  fn wrap_command(
    &self,
    command: &mut Command,
    effective_capabilities: &[Capability],
    scope: &SandboxScope,
  ) -> Result<(), SandboxError>;

  /// Best-effort, forceful termination of whatever `wrap_command` most
  /// recently prepared for `scope`, when that work runs somewhere the
  /// spawned `Command`'s own `Child` handle does not fully control.
  ///
  /// Default no-op — sufficient for backends where the `Child` genuinely
  /// *is* the work (macOS `sandbox-exec` re-execs the target program
  /// in-place; Linux seccomp installs an in-process filter via
  /// `pre_exec`), so a caller killing its own `Child` (e.g.
  /// `Command::kill_on_drop(true)`) already tears everything down.
  ///
  /// `agentflow_tools::sandbox::ContainerBackend` overrides this: its
  /// `Child` is the `container`/`podman` **client** process, not the
  /// container/VM itself, and killing the client does not stop the
  /// container it launched — confirmed empirically (S4.2 follow-up): a
  /// `container run` client killed with `SIGKILL` left its container in
  /// the `running` state indefinitely. Callers whose work might outlive
  /// their own timeout (llm-generated code that mostly sleeps/blocks, e.g.,
  /// never hits a CPU or memory cap) must call this on the timeout path,
  /// not just drop the `Child`.
  ///
  /// Synchronous and intentionally best-effort: this runs on a cleanup
  /// path, not the hot path, so a brief blocking call is acceptable, and
  /// there is nothing meaningful to do if cleanup itself fails (errors
  /// are swallowed).
  fn terminate(&self, _scope: &SandboxScope) {}
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn enforcement_token_strings_are_stable() {
    assert_eq!(SandboxEnforcement::Enforcing.as_str(), "enforcing");
    assert_eq!(SandboxEnforcement::Permissive.as_str(), "permissive");
    assert_eq!(SandboxEnforcement::Disabled.as_str(), "disabled");
  }

  #[test]
  fn only_enforcing_state_is_enforcing() {
    assert!(SandboxEnforcement::Enforcing.is_enforcing());
    assert!(!SandboxEnforcement::Permissive.is_enforcing());
    assert!(!SandboxEnforcement::Disabled.is_enforcing());
  }

  #[test]
  fn enforcement_round_trips_through_json() {
    let levels = [
      SandboxEnforcement::Enforcing,
      SandboxEnforcement::Permissive,
      SandboxEnforcement::Disabled,
    ];
    for level in levels {
      let json = serde_json::to_value(level).unwrap();
      let back: SandboxEnforcement = serde_json::from_value(json).unwrap();
      assert_eq!(level, back);
    }
  }

  #[test]
  fn sandbox_status_round_trips_through_serde() {
    let original = SandboxStatus::new("sandbox-exec", SandboxEnforcement::Enforcing);
    let json = serde_json::to_value(&original).unwrap();
    let back: SandboxStatus = serde_json::from_value(json).unwrap();
    assert_eq!(original, back);
  }
}

//! Platform-abstracted OS sandbox backend.
//!
//! A [`SandboxBackend`] wraps a [`tokio::process::Command`] in OS-level
//! sandbox primitives before the caller spawns it. The capability set
//! drives what the kernel will allow the child to do.

use std::path::PathBuf;
use std::sync::Arc;

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
  fn wrap_command(
    &self,
    command: &mut Command,
    effective_capabilities: &[Capability],
    scope: &SandboxScope,
  ) -> Result<(), SandboxError>;
}

/// Return the appropriate enforcing backend for the current platform, or a
/// [`NoopSandboxBackend`](crate::sandbox::NoopSandboxBackend) when no
/// enforcing backend is available.
///
/// Callers that require enforcement should check
/// [`SandboxBackend::is_enforcing`] and refuse to spawn if it returns false.
pub fn default_backend() -> Arc<dyn SandboxBackend> {
  #[cfg(target_os = "macos")]
  {
    Arc::new(crate::sandbox::macos::MacosSandboxExecBackend::new())
  }
  #[cfg(target_os = "linux")]
  {
    Arc::new(crate::sandbox::linux::LinuxSeccompBackend::new())
  }
  #[cfg(not(any(target_os = "macos", target_os = "linux")))]
  {
    Arc::new(crate::sandbox::noop::NoopSandboxBackend::new(
      "current platform has no OS sandbox backend; install or run on macOS / Linux",
    ))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::sandbox::noop::NoopSandboxBackend;

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
  fn noop_backend_is_disabled_not_silently_hidden() {
    let backend = NoopSandboxBackend::default();
    let status = SandboxStatus::from_backend(&backend);
    assert_eq!(status.backend, "noop");
    assert_eq!(status.enforcement, SandboxEnforcement::Disabled);
    // The no-op backend must be observable in traces — a silent fall-through
    // would mask the fact that the OS isn't constraining the child.
    let json = serde_json::to_value(&status).unwrap();
    assert_eq!(json["backend"], "noop");
    assert_eq!(json["enforcement"], "disabled");
  }

  #[test]
  fn sandbox_status_round_trips_through_serde() {
    let original = SandboxStatus::new("sandbox-exec", SandboxEnforcement::Enforcing);
    let json = serde_json::to_value(&original).unwrap();
    let back: SandboxStatus = serde_json::from_value(json).unwrap();
    assert_eq!(original, back);
  }

  #[cfg(target_os = "macos")]
  #[test]
  fn macos_backend_enforcement_matches_availability() {
    use crate::sandbox::macos::MacosSandboxExecBackend;
    let backend = MacosSandboxExecBackend::new();
    let level = backend.enforcement_level();
    if std::path::Path::new("/usr/bin/sandbox-exec").exists() {
      assert_eq!(level, SandboxEnforcement::Enforcing);
      assert!(backend.is_enforcing());
    } else {
      // No sandbox-exec on this host: backend exists for the platform but
      // cannot enforce — must report Permissive rather than Disabled.
      assert_eq!(level, SandboxEnforcement::Permissive);
      assert!(!backend.is_enforcing());
    }
  }

  #[cfg(target_os = "linux")]
  #[test]
  fn linux_backend_enforcement_matches_arch_support() {
    use crate::sandbox::linux::LinuxSeccompBackend;
    let backend = LinuxSeccompBackend::new();
    let level = backend.enforcement_level();
    if cfg!(any(target_arch = "x86_64", target_arch = "aarch64")) {
      assert_eq!(level, SandboxEnforcement::Enforcing);
      assert!(backend.is_enforcing());
    } else {
      assert_eq!(level, SandboxEnforcement::Permissive);
      assert!(!backend.is_enforcing());
    }
  }
}

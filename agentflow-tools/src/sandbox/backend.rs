//! Platform-abstracted OS sandbox backend.
//!
//! A [`SandboxBackend`] wraps a [`tokio::process::Command`] in OS-level
//! sandbox primitives before the caller spawns it. The capability set
//! drives what the kernel will allow the child to do.

use std::path::PathBuf;
use std::sync::Arc;

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

/// Wrap a child process in OS-level sandbox primitives.
pub trait SandboxBackend: Send + Sync {
  /// Stable name (`"sandbox-exec"`, `"seccomp"`, `"noop"`).
  fn name(&self) -> &'static str;

  /// Whether this backend actually enforces anything. `NoopSandboxBackend`
  /// returns `false`; callers can use this to refuse silent fall-through.
  fn is_enforcing(&self) -> bool;

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
/// [`NoopSandboxBackend`] when no enforcing backend is available.
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

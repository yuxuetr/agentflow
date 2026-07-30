//! Platform dispatch for the default [`SandboxBackend`].
//!
//! T3.3: the trait itself and its DTOs (`SandboxScope`/`SandboxStatus`/
//! `SandboxEnforcement`/`SandboxError`) moved to `agentflow-tool` (the
//! contract crate) and are re-exported from `crate::sandbox` unchanged.
//! This file keeps only the concrete platform-selection logic, which
//! genuinely belongs in the builtin-impl crate since it names the concrete
//! backend types (`macos`/`linux`/`noop`).

use std::sync::Arc;

use crate::sandbox::SandboxBackend;

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
  use agentflow_tool::sandbox::{SandboxEnforcement, SandboxStatus};

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
  fn linux_backend_enforcement_matches_arch_and_landlock_support() {
    use crate::sandbox::linux::{LinuxSeccompBackend, probe_landlock_abi};
    let backend = LinuxSeccompBackend::new();
    let level = backend.enforcement_level();
    if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64")) {
      assert_eq!(level, SandboxEnforcement::Permissive);
      assert!(!backend.is_enforcing());
      return;
    }
    // On a supported arch, seccomp's own containment is unconditional —
    // `is_enforcing` doesn't depend on Landlock (see its doc comment).
    assert!(backend.is_enforcing());
    // S3.1: the tri-state `enforcement_level` additionally reflects
    // whether *this* kernel actually supports Landlock — not guaranteed
    // in every test environment (e.g. some container/VM kernels ship
    // without `CONFIG_SECURITY_LANDLOCK`). Assert consistency with the
    // same probe the backend itself uses rather than a hardcoded
    // expectation, so this test is meaningful on both kinds of host.
    if probe_landlock_abi().is_some() {
      assert_eq!(level, SandboxEnforcement::Enforcing);
    } else {
      assert_eq!(level, SandboxEnforcement::Permissive);
    }
  }
}

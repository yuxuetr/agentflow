//! In-process sandbox policy and OS-level sandbox backends.
//!
//! [`SandboxPolicy`] is the in-process allowlist (path / domain / command)
//! that built-in tools consult before spawning subprocesses or opening URLs.
//! [`SandboxBackend`] is the OS-level enforcement layer that wraps a child
//! process so that, even if a tool's in-process check were bypassed, the
//! kernel would still prevent the child from escaping its allowed scope.
//!
//! Backends are platform-specific:
//!
//! * macOS: `MacosSandboxExecBackend` generates a `sandbox-exec` profile
//!   from the policy + capability set and re-runs the inner command via
//!   `sandbox-exec -f <profile> <cmd>`.
//! * Linux: `LinuxSeccompBackend` installs a seccomp BPF filter through
//!   `Command::pre_exec` so the filter is active before `execve` returns.
//! * Other platforms: [`NoopSandboxBackend`] is a pass-through. Callers can
//!   detect this via [`SandboxBackend::is_enforcing`] and decide whether to
//!   refuse the call rather than run unsandboxed.

pub mod backend;
pub mod policy;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod noop;

pub use backend::{
  SandboxBackend, SandboxEnforcement, SandboxError, SandboxScope, SandboxStatus, default_backend,
};
#[cfg(target_os = "linux")]
pub use linux::LinuxSeccompBackend;
#[cfg(target_os = "macos")]
pub use macos::MacosSandboxExecBackend;
pub use noop::NoopSandboxBackend;
pub use policy::{NetworkAddressClass, SandboxPolicy};

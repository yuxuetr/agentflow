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
//!   `Command::pre_exec` so the filter is active before `execve` returns,
//!   layered (S3.1) with a Landlock ruleset (kernel >= 5.13, probed at
//!   runtime) for path-scoped `FsRead`/`FsWrite` containment that seccomp
//!   alone cannot express.
//! * Other platforms: [`NoopSandboxBackend`] is a pass-through. Callers can
//!   detect this via [`SandboxBackend::is_enforcing`] and decide whether to
//!   refuse the call rather than run unsandboxed.
//!
//! [`ContainerBackend`] (S4.2) is a separate, stronger tier used only by the
//! `code_exec` tool: instead of syscall/path-scoped containment on the
//! shared host kernel, it shells out to a real container engine (Apple's
//! `container` CLI or rootless Podman) so llm-generated code — adversarial
//! by construction, unlike the author-signed content the OS-sandbox backends
//! above are designed for — gets its own kernel boundary per invocation. It
//! hard-refuses (`Err`) rather than degrading when no engine is available;
//! see its module docs for the full rationale.

pub mod backend;
pub mod container;
pub mod policy;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod noop;

pub use backend::{
  SandboxBackend, SandboxEnforcement, SandboxError, SandboxScope, SandboxStatus, default_backend,
};
pub use container::{ContainerBackend, code_exec_backend};
#[cfg(target_os = "linux")]
pub use linux::LinuxSeccompBackend;
#[cfg(target_os = "macos")]
pub use macos::MacosSandboxExecBackend;
pub use noop::NoopSandboxBackend;
pub use policy::{NetworkAddressClass, SandboxPolicy};

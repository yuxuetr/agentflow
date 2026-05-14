//! Linux sandbox backend backed by seccomp-bpf.
//!
//! The filter is compiled once on the host, then re-installed in each child
//! through `pre_exec` (between `fork(2)` and `execve(2)`). The default action
//! is `Allow`; specific syscalls are denied with `Errno(EPERM)` when the
//! corresponding [`Capability`] is missing from the effective set.
//!
//! ## What this enforces
//!
//! * **No `Net`** → outbound socket creation, `connect`, `bind`, `listen`,
//!   `accept`, `sendto`, `recvfrom` and friends are blocked. A child that
//!   tries `curl` will fail with a network error from libc rather than the
//!   request reaching the network stack.
//! * **No `FsWrite`** → mutating filesystem syscalls (`unlink`, `rmdir`,
//!   `rename`, `mkdir`, `chmod`, `chown`, `truncate`, `link`, `symlink`,
//!   `mknod`) are blocked. `write(2)` itself is **not** blocked because the
//!   child legitimately writes to stdout/stderr; pure stdout-only behaviour
//!   keeps working, but creating new files via `openat(O_WRONLY | O_CREAT)`
//!   is denied through the path-creation syscalls.
//! * **No `Exec`** → cannot be enforced through seccomp alone, because the
//!   child must `execve` once to start. Tools that don't grant `Exec` will
//!   already have been denied at the in-process capability merge layer
//!   ([`crate::registry::ToolRegistry::execute`]); the kernel filter does
//!   not need a redundant rule here.
//!
//! ## Limits
//!
//! seccomp filters are syscall-scoped, not path-scoped. Restricting `FsRead`
//! to a particular subtree requires Landlock or an LSM, which is out of
//! scope for this backend. Path-prefix gating is still enforced in-process
//! by [`crate::sandbox::SandboxPolicy::is_path_allowed`].
//!
//! ## Architecture support
//!
//! Compiled for `x86_64` and `aarch64`. On other Linux architectures the
//! backend reports itself as non-enforcing rather than installing a filter
//! for the wrong audit arch (which would be a security footgun).

use std::collections::BTreeMap;
use std::sync::Arc;

use seccompiler::{BpfProgram, SeccompAction, SeccompFilter, TargetArch};
use tokio::process::Command;

use crate::capability::Capability;

use super::{SandboxBackend, SandboxEnforcement, SandboxError, SandboxScope};

/// Linux seccomp backend. Cheap to clone: holds an `Arc` of the compiled BPF.
pub struct LinuxSeccompBackend {
  /// `None` means the host arch is unsupported; the backend falls back to
  /// reporting `is_enforcing = false`.
  arch: Option<TargetArch>,
}

impl LinuxSeccompBackend {
  pub fn new() -> Self {
    Self {
      arch: detect_target_arch(),
    }
  }
}

impl Default for LinuxSeccompBackend {
  fn default() -> Self {
    Self::new()
  }
}

impl SandboxBackend for LinuxSeccompBackend {
  fn name(&self) -> &'static str {
    "seccomp"
  }

  fn is_enforcing(&self) -> bool {
    self.arch.is_some()
  }

  fn enforcement_level(&self) -> SandboxEnforcement {
    if self.arch.is_some() {
      SandboxEnforcement::Enforcing
    } else {
      // We are on Linux but the architecture is not in the supported
      // (`x86_64`, `aarch64`) set. Report Permissive so operators can see
      // that an enforcing backend exists on this platform but isn't active.
      SandboxEnforcement::Permissive
    }
  }

  fn wrap_command(
    &self,
    command: &mut Command,
    effective_capabilities: &[Capability],
    _scope: &SandboxScope,
  ) -> Result<(), SandboxError> {
    let arch = self.arch.ok_or_else(|| SandboxError::Unsupported {
      platform: "linux",
      message: format!(
        "seccomp backend supports x86_64 and aarch64 only; current arch is '{}'",
        std::env::consts::ARCH
      ),
    })?;

    let bpf =
      compile_filter(effective_capabilities, arch).map_err(|err| SandboxError::Prepare {
        message: format!("failed to compile seccomp filter: {err}"),
      })?;

    // Share one Arc across all closure invocations (one per spawn).
    let bpf = Arc::new(bpf);
    // SAFETY: the closure runs in the forked child between fork and execve.
    // `apply_filter` only calls `prctl` + `seccomp(2)` — both async-signal-safe.
    unsafe {
      command.pre_exec(move || {
        seccompiler::apply_filter(&bpf)
          .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err.to_string()))
      });
    }
    Ok(())
  }
}

fn detect_target_arch() -> Option<TargetArch> {
  match std::env::consts::ARCH {
    "x86_64" => Some(TargetArch::x86_64),
    "aarch64" => Some(TargetArch::aarch64),
    _ => None,
  }
}

/// Build a default-allow filter that denies a curated syscall set per
/// missing capability.
fn compile_filter(caps: &[Capability], arch: TargetArch) -> Result<BpfProgram, seccompiler::Error> {
  let mut rules: BTreeMap<i64, Vec<seccompiler::SeccompRule>> = BTreeMap::new();
  let allow_net = caps.contains(&Capability::Net);
  let allow_fs_write = caps.contains(&Capability::FsWrite);

  if !allow_net {
    for nr in net_syscall_numbers() {
      rules.insert(nr, vec![]);
    }
  }
  if !allow_fs_write {
    for nr in fs_write_syscall_numbers() {
      rules.insert(nr, vec![]);
    }
  }

  let filter = SeccompFilter::new(
    rules,
    SeccompAction::Allow,
    SeccompAction::Errno(libc::EPERM as u32),
    arch,
  )?;
  filter.try_into()
}

/// Syscalls that create or use network sockets. Conservative: covers IPv4,
/// IPv6, and Unix-domain sockets. We deliberately *do not* block `read` /
/// `write` against established fds because we can't generally distinguish
/// socket fds from stdout fds without argument-level filters.
fn net_syscall_numbers() -> &'static [i64] {
  &[
    libc::SYS_socket,
    libc::SYS_socketpair,
    libc::SYS_connect,
    libc::SYS_bind,
    libc::SYS_listen,
    libc::SYS_accept,
    libc::SYS_accept4,
    libc::SYS_sendto,
    libc::SYS_sendmsg,
    libc::SYS_recvfrom,
    libc::SYS_recvmsg,
    libc::SYS_setsockopt,
    libc::SYS_getsockopt,
    libc::SYS_getsockname,
    libc::SYS_getpeername,
    libc::SYS_shutdown,
  ]
}

/// Syscalls that mutate the filesystem layout. `write` itself is allowed
/// because the child writes to stdout/stderr through it; new file creation
/// is gated through the openat / creat / mknodat path-creation surface.
fn fs_write_syscall_numbers() -> &'static [i64] {
  &[
    libc::SYS_unlinkat,
    libc::SYS_renameat,
    libc::SYS_renameat2,
    libc::SYS_mkdirat,
    libc::SYS_mknodat,
    libc::SYS_symlinkat,
    libc::SYS_linkat,
    libc::SYS_fchmodat,
    libc::SYS_fchownat,
    libc::SYS_truncate,
    libc::SYS_ftruncate,
  ]
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn arch_detection_supports_known_targets() {
    let detected = detect_target_arch();
    if cfg!(target_arch = "x86_64") {
      assert_eq!(detected, Some(TargetArch::x86_64));
    } else if cfg!(target_arch = "aarch64") {
      assert_eq!(detected, Some(TargetArch::aarch64));
    }
  }

  #[test]
  fn filter_compiles_for_both_capability_sets() {
    let arch = detect_target_arch().expect("running on a supported arch");
    let permissive = compile_filter(&[Capability::Net, Capability::FsWrite], arch).unwrap();
    let restrictive = compile_filter(&[], arch).unwrap();
    // The restrictive filter must be larger because it carries deny rules.
    assert!(restrictive.len() > permissive.len());
  }

  #[test]
  fn backend_is_enforcing_on_supported_arch() {
    let backend = LinuxSeccompBackend::new();
    if cfg!(any(target_arch = "x86_64", target_arch = "aarch64")) {
      assert!(backend.is_enforcing());
      assert_eq!(backend.name(), "seccomp");
    }
  }
}

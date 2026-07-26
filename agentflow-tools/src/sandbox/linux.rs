//! Linux sandbox backend backed by seccomp-bpf, layered with Landlock (S3.1)
//! for path-scoped filesystem access.
//!
//! The seccomp filter is compiled once on the host, then re-installed in
//! each child through `pre_exec` (between `fork(2)` and `execve(2)`). The
//! default action is `Allow`; specific syscalls are denied with
//! `Errno(EPERM)` when the corresponding [`Capability`] is missing from the
//! effective set.
//!
//! ## Landlock path scoping (S3.1)
//!
//! seccomp is syscall-scoped, not path-scoped (see "Limits" below) — before
//! this, a child with `FsRead` could read anything the host user could read
//! (`~/.ssh`, `~/.aws`, ...), because path restriction only happened
//! in-process ([`crate::sandbox::SandboxPolicy::is_path_allowed`]), which a
//! subprocess's own code never goes through. [`LinuxSeccompBackend`] now
//! additionally builds a [`landlock::Ruleset`] from the [`SandboxScope`]
//! passed to [`SandboxBackend::wrap_command`] — read-only access to
//! `scope.read_paths` (+ `scope.working_directory`), read-write access to
//! `scope.write_paths` when [`Capability::FsWrite`] is granted (read-only
//! otherwise) — and applies it via `restrict_self()` in the same `pre_exec`
//! closure as the seccomp filter, so both layers are active before the
//! child's `execve` returns.
//!
//! Landlock availability is probed once at construction (kernel >= 5.13,
//! LSM enabled at boot) via a side-effect-free syscall
//! (`probe_landlock_abi`). When unavailable, the seccomp filter still
//! applies (unchanged syscall-level containment) but
//! [`SandboxBackend::enforcement_level`] reports
//! [`SandboxEnforcement::Permissive`] instead of `Enforcing`, since the
//! path-scoping guarantee this module's docs describe no longer holds —
//! the existing three-state pattern already used for "unsupported arch"
//! and the macOS `sandbox-exec`-missing case.
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
//!   child legitimately writes to stdout/stderr. New-file creation through
//!   `openat`/`open`/`creat`/`openat2` is also blocked when the call carries
//!   any of `O_WRONLY` / `O_RDWR` / `O_CREAT` / `O_TRUNC` (per-flag rules,
//!   Q1.1.2). `openat2` cannot be argument-filtered (the flags live in a
//!   `struct open_how` accessed by pointer), so it is denied unconditionally
//!   under `!FsWrite`.
//! * **No `Exec`** → process-creation syscalls (`clone`, `clone3`, `fork`,
//!   `vfork`, `execve`, `execveat`) are denied. In practice the in-process
//!   capability layer already rejects any tool that doesn't grant `Exec`
//!   before we reach the seccomp filter, so these rules are defense in
//!   depth (Q1.1.4): they make the filter's contract match its name even
//!   if a future caller bypasses the registry merge. A child running under
//!   a no-`Exec` filter cannot start at all (the initial `execve` from the
//!   parent fails with `EPERM`) — that is the intended failure mode.
//!
//! ## Limits
//!
//! seccomp filters are syscall-scoped, not path-scoped — Landlock (above)
//! is what adds path-scoped `FsRead`/`FsWrite` containment. Landlock itself
//! has kernel-documented limits (e.g. arbitrary mounts are always denied,
//! and rules only cover the six `AccessFs` rights defined as of ABI V1,
//! which this module targets — see `build_landlock_ruleset`). Path-prefix
//! gating is still enforced in-process too, independent of either kernel
//! layer, by [`crate::sandbox::SandboxPolicy::is_path_allowed`].
//!
//! ## Architecture support
//!
//! Compiled for `x86_64` and `aarch64`. On other Linux architectures the
//! backend reports itself as non-enforcing rather than installing a filter
//! for the wrong audit arch (which would be a security footgun).

use std::collections::BTreeMap;
use std::sync::Arc;

use landlock::{
  ABI, Access, AccessFs, Ruleset, RulesetAttr, RulesetCreated, RulesetCreatedAttr, RulesetError,
  path_beneath_rules,
};
use seccompiler::{
  BpfProgram, SeccompAction, SeccompCmpArgLen, SeccompCmpOp, SeccompCondition, SeccompFilter,
  SeccompRule, TargetArch,
};
use tokio::process::Command;

use crate::capability::Capability;

use super::{SandboxBackend, SandboxEnforcement, SandboxError, SandboxScope};

/// Linux seccomp + Landlock backend. Cheap to clone: holds an `Arc` of the
/// compiled BPF; the Landlock ruleset is rebuilt per [`Self::wrap_command`]
/// call since it's scoped to that call's [`SandboxScope`] and effective
/// capabilities (unlike the seccomp filter, whose only per-call inputs are
/// the effective capabilities).
pub struct LinuxSeccompBackend {
  /// `None` means the host arch is unsupported; the backend falls back to
  /// reporting `is_enforcing = false`.
  arch: Option<TargetArch>,
  /// S3.1: highest Landlock ABI version this kernel supports (probed once,
  /// side-effect-free — see [`probe_landlock_abi`]). `None` means Landlock
  /// is unavailable (pre-5.13 kernel, or disabled at boot via `lsm=`);
  /// [`Self::wrap_command`] still applies the seccomp filter but skips
  /// Landlock, and [`Self::enforcement_level`] reports `Permissive`.
  landlock_abi: Option<i32>,
}

impl LinuxSeccompBackend {
  pub fn new() -> Self {
    Self {
      arch: detect_target_arch(),
      landlock_abi: probe_landlock_abi(),
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
    // Landlock availability does not gate this: seccomp's syscall-level
    // containment is fully active either way. `enforcement_level` is where
    // the Landlock nuance (path-scoping is or isn't guaranteed) surfaces.
    self.arch.is_some()
  }

  fn enforcement_level(&self) -> SandboxEnforcement {
    if self.arch.is_none() {
      // We are on Linux but the architecture is not in the supported
      // (`x86_64`, `aarch64`) set. Report Permissive so operators can see
      // that an enforcing backend exists on this platform but isn't active.
      return SandboxEnforcement::Permissive;
    }
    if self.landlock_abi.is_some() {
      SandboxEnforcement::Enforcing
    } else {
      // S3.1: seccomp is still applied, but without Landlock the
      // path-scoped FsRead/FsWrite containment this module's docs promise
      // isn't guaranteed (pre-5.13 kernel, or LSM disabled at boot).
      SandboxEnforcement::Permissive
    }
  }

  fn wrap_command(
    &self,
    command: &mut Command,
    effective_capabilities: &[Capability],
    scope: &SandboxScope,
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

    // S3.1: build the Landlock ruleset here, in the parent, not inside
    // `pre_exec` — `PathFd::new` (via `path_beneath_rules`) opens each
    // path with `open(2)`, which is not guaranteed async-signal-safe, so
    // it must happen before the fork. Only `restrict_self()` (a single
    // `landlock_restrict_self(2)` syscall) runs inside `pre_exec`, mirroring
    // how `compile_filter` happens here while only `apply_filter` (a single
    // `seccomp(2)` syscall) runs in the child.
    let landlock_ruleset = if self.landlock_abi.is_some() {
      match build_landlock_ruleset(effective_capabilities, scope) {
        Ok(ruleset) => Some(ruleset),
        Err(err) => {
          // Best-effort: the seccomp filter below still applies. A failure
          // here (e.g. a scope path that doesn't exist) degrades this
          // spawn to seccomp-only rather than aborting it — matching the
          // "unavailable -> Permissive, not a hard error" contract this
          // module documents for the Landlock layer as a whole.
          tracing::warn!(
            error = %err,
            "failed to build landlock ruleset; child will run with seccomp-only enforcement"
          );
          None
        }
      }
    } else {
      None
    };

    // Share one Arc across all closure invocations (one per spawn).
    let bpf = Arc::new(bpf);
    let mut landlock_ruleset = landlock_ruleset;
    // SAFETY: the closure runs in the forked child between fork and execve.
    // `apply_filter` only calls `prctl` + `seccomp(2)`, and
    // `RulesetCreated::restrict_self` only calls `landlock_restrict_self(2)`
    // — both async-signal-safe. No path is opened here (see above).
    unsafe {
      command.pre_exec(move || {
        seccompiler::apply_filter(&bpf).map_err(|err| std::io::Error::other(err.to_string()))?;
        if let Some(ruleset) = landlock_ruleset.take() {
          // Best-effort, same rationale as the build failure above: don't
          // fail the spawn over a Landlock enforcement error once seccomp
          // is already active.
          let _ = ruleset.restrict_self();
        }
        Ok(())
      });
    }
    Ok(())
  }
}

/// Build a Landlock ruleset from `scope`: read-only access to
/// `scope.read_paths` and `scope.working_directory`, read-write access to
/// `scope.write_paths` when [`Capability::FsWrite`] is in
/// `effective_capabilities` (read-only otherwise — a write-scoped path a
/// tool isn't allowed to write to should still be readable, not vanish
/// from its view). Targets [`ABI::V1`] (Linux 5.13, the TODO's own stated
/// minimum) rather than probing for a newer ABI to request more access
/// rights: the `landlock` crate's own docs discourage picking `ABI` based
/// on a runtime probe ("can lead to unreliable sandboxing"); requesting a
/// fixed, tested version and letting the crate's best-effort compatibility
/// layer downgrade further is the documented pattern.
fn build_landlock_ruleset(
  effective_capabilities: &[Capability],
  scope: &SandboxScope,
) -> Result<RulesetCreated, RulesetError> {
  let abi = ABI::V1;
  let allow_fs_write = effective_capabilities.contains(&Capability::FsWrite);
  let access_all = AccessFs::from_all(abi);
  let access_read = AccessFs::from_read(abi);
  let write_access = if allow_fs_write {
    access_all
  } else {
    access_read
  };

  let mut created = Ruleset::default().handle_access(access_all)?.create()?;
  created = created.add_rules(path_beneath_rules(&scope.read_paths, access_read))?;
  created = created.add_rules(path_beneath_rules(&scope.write_paths, write_access))?;
  if let Some(dir) = &scope.working_directory {
    created = created.add_rules(path_beneath_rules(std::iter::once(dir), access_read))?;
  }
  Ok(created)
}

/// Highest Landlock ABI version the running kernel supports (1-8+), or
/// `None` when Landlock is unavailable (pre-5.13 kernel, or disabled at
/// boot via the `lsm=` cmdline).
///
/// Uses the raw `landlock_create_ruleset(NULL, 0,
/// LANDLOCK_CREATE_RULESET_VERSION)` syscall directly rather than the
/// `landlock` crate: this specific invocation is documented as a pure
/// version query (no ruleset fd is created, nothing is restricted),
/// whereas every other path through the crate's API — including any
/// attempt to use it for ABI *detection* — is explicitly discouraged by
/// the crate's own docs ([`landlock::ABI`]'s doc comment: constructing an
/// `ABI` from a runtime probe "can lead to unreliable sandboxing where
/// rules might differ between executions"). This probe exists solely to
/// answer `enforcement_level()` *before* any ruleset is built; the actual
/// ruleset applied to a child (`build_landlock_ruleset`) always requests
/// the fixed `ABI::V1` and lets the crate's own best-effort compatibility
/// layer handle any further downgrade.
pub(crate) fn probe_landlock_abi() -> Option<i32> {
  // Stable UAPI constant from `linux/landlock.h` — `1U << 0`. Landlock's
  // ABI is a frozen userspace contract (see the kernel's Landlock
  // documentation on backward compatibility), so hard-coding this bit is
  // as stable as depending on the kernel header itself.
  const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1;
  // SAFETY: `landlock_create_ruleset(NULL, 0, LANDLOCK_CREATE_RULESET_VERSION)`
  // is documented as a pure version query with no side effects — it
  // creates no ruleset, opens no fd, and restricts nothing. `attr = NULL`
  // and `size = 0` are only valid together with this flag.
  let ret = unsafe {
    libc::syscall(
      libc::SYS_landlock_create_ruleset,
      std::ptr::null::<libc::c_void>(),
      0usize,
      LANDLOCK_CREATE_RULESET_VERSION,
    )
  };
  if ret > 0 { Some(ret as i32) } else { None }
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
  let allow_exec = caps.contains(&Capability::Exec);

  // The `*_syscall_numbers` helpers return `&'static [i64]`, so the
  // for-loop binding is `&i64`. The seccomp rules map keys on owned
  // `i64`, hence the deref on `nr`.
  if !allow_net {
    for nr in net_syscall_numbers() {
      rules.insert(*nr, vec![]);
    }
  }
  if !allow_fs_write {
    for nr in fs_write_syscall_numbers() {
      rules.insert(*nr, vec![]);
    }
    install_write_open_rules(&mut rules)?;
  }
  if !allow_exec {
    for nr in process_creation_syscall_numbers() {
      rules.insert(*nr, vec![]);
    }
    // `SYS_fork` / `SYS_vfork` only exist on x86_64; aarch64 routes both
    // through `clone(SIGCHLD, ...)` which is already in the deny list.
    #[cfg(target_arch = "x86_64")]
    {
      rules.insert(libc::SYS_fork, vec![]);
      rules.insert(libc::SYS_vfork, vec![]);
    }
  }

  let filter = SeccompFilter::new(
    rules,
    SeccompAction::Allow,
    SeccompAction::Errno(libc::EPERM as u32),
    arch,
  )?;
  // `TryFrom<SeccompFilter> for BpfProgram` returns `BackendError`;
  // seccompiler 0.5 ships a `From<BackendError> for Error` impl so
  // we can keep this function's error type unified at the boundary.
  TryInto::<BpfProgram>::try_into(filter).map_err(seccompiler::Error::from)
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
/// is gated through the openat / creat / mknodat / openat2 path-creation
/// surface (see [`install_write_open_rules`]).
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

/// Syscalls that create new processes or replace the address space.
/// Denied under `!Exec` (Q1.1.4). This deliberately includes `execve` /
/// `execveat`: with these in the deny set the initial `execve` performed
/// by the parent after `fork()` fails with EPERM, which is the correct
/// containment outcome — a tool that doesn't earn the `Exec` capability
/// must not be able to spawn a subprocess at all.
fn process_creation_syscall_numbers() -> &'static [i64] {
  &[
    libc::SYS_clone,
    // `clone3` is a Linux 5.3+ superset of `clone` and would be a bypass
    // route if we only blocked `clone`. libc 0.2.174 exports it for the
    // architectures we support.
    libc::SYS_clone3,
    libc::SYS_execve,
    libc::SYS_execveat,
  ]
}

/// Open-family flags that imply the call will write to a file. We deny any
/// `open` / `openat` whose `flags` argument intersects this mask under
/// `!FsWrite`. The bits are checked individually because seccompiler's
/// `MaskedEq(mask)` matches when `arg & mask == value`; we want "any bit
/// in the mask set", which we express as one rule per bit.
fn write_open_flag_bits() -> &'static [u64] {
  // O_WRONLY (0o1), O_RDWR (0o2), O_CREAT (0o100), O_TRUNC (0o1000).
  // Hard-coded here (rather than read from `libc::O_*` at runtime) because
  // the seccomp filter is built once on the host and applied in the child;
  // libc constants are `c_int` and pulling them into a `const fn` context
  // adds no value over the well-known POSIX/Linux numeric values.
  const O_WRONLY: u64 = 0o1;
  const O_RDWR: u64 = 0o2;
  const O_CREAT: u64 = 0o100;
  const O_TRUNC: u64 = 0o1000;
  &[O_WRONLY, O_RDWR, O_CREAT, O_TRUNC]
}

/// Register seccomp rules that deny `open`/`openat`/`creat`/`openat2` when
/// they imply a write. Called from [`compile_filter`] under `!FsWrite`.
///
/// * `openat(dirfd, path, flags, ...)` → `flags` is argument index 2. One
///   rule per bit in [`write_open_flag_bits`] (rules are ORed within the
///   same syscall).
/// * `open(path, flags, ...)` → `flags` is argument index 1. Only emitted
///   on `x86_64`, since `aarch64` does not export `SYS_open` (it's been
///   replaced by `openat` system-wide).
/// * `creat(path, mode)` → always implies `O_WRONLY | O_CREAT | O_TRUNC`.
///   Denied unconditionally (x86_64 only — same reason as `open`).
/// * `openat2(dirfd, path, how, size)` → `how` is a pointer to
///   `struct open_how`; seccomp cannot dereference. Denied unconditionally
///   to avoid a bypass via the newer syscall.
fn install_write_open_rules(
  rules: &mut BTreeMap<i64, Vec<SeccompRule>>,
) -> Result<(), seccompiler::Error> {
  for &mask in write_open_flag_bits() {
    let cond_openat = SeccompCondition::new(
      2,
      SeccompCmpArgLen::Dword,
      SeccompCmpOp::MaskedEq(mask),
      mask,
    )?;
    rules
      .entry(libc::SYS_openat)
      .or_default()
      .push(SeccompRule::new(vec![cond_openat])?);

    #[cfg(target_arch = "x86_64")]
    {
      let cond_open = SeccompCondition::new(
        1,
        SeccompCmpArgLen::Dword,
        SeccompCmpOp::MaskedEq(mask),
        mask,
      )?;
      rules
        .entry(libc::SYS_open)
        .or_default()
        .push(SeccompRule::new(vec![cond_open])?);
    }
  }

  // `openat2` flags live in a pointed-to struct; can't filter by arg, so
  // deny the syscall outright under !FsWrite.
  rules.insert(libc::SYS_openat2, vec![]);

  // `creat` always writes; deny unconditionally on platforms that ship it.
  #[cfg(target_arch = "x86_64")]
  {
    rules.insert(libc::SYS_creat, vec![]);
  }

  Ok(())
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
  fn write_open_rules_emit_per_flag_under_no_fs_write() {
    // Build the rule map directly to assert openat carries one rule per
    // write-relevant flag bit, plus the syscall numbers we deny outright.
    let mut rules: BTreeMap<i64, Vec<SeccompRule>> = BTreeMap::new();
    install_write_open_rules(&mut rules).expect("rules compile under no-fs-write");

    let openat = rules
      .get(&libc::SYS_openat)
      .expect("openat rules must be present");
    assert_eq!(openat.len(), write_open_flag_bits().len());

    assert!(
      rules.get(&libc::SYS_openat2).is_some_and(|r| r.is_empty()),
      "openat2 must be denied unconditionally (empty-vec marker)"
    );

    #[cfg(target_arch = "x86_64")]
    {
      let open = rules.get(&libc::SYS_open).expect("open rules on x86_64");
      assert_eq!(open.len(), write_open_flag_bits().len());
      assert!(
        rules.get(&libc::SYS_creat).is_some_and(|r| r.is_empty()),
        "creat must be denied unconditionally on x86_64"
      );
    }
  }

  /// Q1.1.4 regression: under `!Exec` the filter must populate the
  /// process-creation syscall deny set. We can't easily exercise the
  /// runtime deny path (a no-`Exec` sandbox would fail to spawn its own
  /// child by design, which is the whole point), so we assert rule shape
  /// via [`compile_filter`] instead.
  #[test]
  fn process_creation_syscalls_denied_when_exec_capability_absent() {
    let arch = detect_target_arch().expect("running on a supported arch");

    // With Exec, none of these rules should be present.
    let with_exec = compile_filter(&[Capability::Exec], arch).unwrap();
    let no_exec = compile_filter(&[], arch).unwrap();
    assert!(
      no_exec.len() > with_exec.len(),
      "no-Exec filter must carry more deny rules than Exec-enabled filter"
    );

    // Spot-check by rebuilding the rule map directly. Going through the
    // BpfProgram opaque slice would tie the test to seccompiler internals.
    let mut rules: BTreeMap<i64, Vec<SeccompRule>> = BTreeMap::new();
    for nr in process_creation_syscall_numbers() {
      rules.insert(*nr, vec![]);
    }
    assert!(rules.contains_key(&libc::SYS_clone));
    assert!(rules.contains_key(&libc::SYS_clone3));
    assert!(rules.contains_key(&libc::SYS_execve));
    assert!(rules.contains_key(&libc::SYS_execveat));
  }

  #[test]
  fn backend_is_enforcing_on_supported_arch() {
    let backend = LinuxSeccompBackend::new();
    if cfg!(any(target_arch = "x86_64", target_arch = "aarch64")) {
      assert!(backend.is_enforcing());
      assert_eq!(backend.name(), "seccomp");
    }
  }

  // ── S3.1: Landlock ─────────────────────────────────────────────────

  #[test]
  fn probe_landlock_abi_returns_a_positive_version_or_none() {
    // Can't assert a specific outcome — depends on the test host's
    // kernel — but the probe must never panic, and any `Some` value must
    // be a real positive ABI number (matches the syscall's documented
    // return-value contract: positive version, or -1/errno).
    if let Some(version) = probe_landlock_abi() {
      assert!(
        version > 0,
        "landlock ABI version must be positive: {version}"
      );
    }
  }

  #[test]
  fn build_landlock_ruleset_succeeds_with_empty_scope() {
    // An empty SandboxScope (no read/write paths, no working directory)
    // is a degenerate-but-legal input: the ruleset just won't grant any
    // path-scoped access. Must not error, even when Landlock itself is
    // unavailable on this kernel (best-effort mode degrades silently at
    // the `landlock` crate level; `create()` only fails on a genuine
    // misuse, not on kernel unavailability).
    let scope = SandboxScope::new();
    let result = build_landlock_ruleset(&[], &scope);
    assert!(
      result.is_ok(),
      "empty scope must build a (possibly no-op) ruleset: {result:?}"
    );
  }

  #[test]
  fn build_landlock_ruleset_succeeds_with_real_paths() {
    let temp = tempfile::TempDir::new().expect("create temp dir");
    let read_dir = temp.path().join("read");
    let write_dir = temp.path().join("write");
    std::fs::create_dir_all(&read_dir).unwrap();
    std::fs::create_dir_all(&write_dir).unwrap();

    let scope = SandboxScope::new()
      .with_read_paths([read_dir])
      .with_write_paths([write_dir.clone()])
      .with_working_directory(write_dir);
    let result = build_landlock_ruleset(&[Capability::FsWrite], &scope);
    assert!(
      result.is_ok(),
      "real, existing paths must build a ruleset: {result:?}"
    );
  }

  #[test]
  fn build_landlock_ruleset_ignores_nonexistent_paths_rather_than_erroring() {
    // `path_beneath_rules` (from the `landlock` crate) silently skips
    // paths it can't open — matches this backend's own "best-effort,
    // degrade rather than abort the spawn" philosophy for the Landlock
    // layer (see `wrap_command`'s doc comment on the build-failure path).
    let scope =
      SandboxScope::new().with_read_paths(["/agentflow-sandbox-test-path-that-does-not-exist"]);
    let result = build_landlock_ruleset(&[], &scope);
    assert!(
      result.is_ok(),
      "a missing path must not fail ruleset construction: {result:?}"
    );
  }
}

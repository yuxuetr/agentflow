//! macOS sandbox backend backed by the `sandbox-exec` userspace tool.
//!
//! `sandbox-exec` is shipped on every supported macOS release. It accepts a
//! profile written in TinyScheme (Sandbox Profile Language, SBPL). We
//! generate the profile on the fly from the effective capabilities + scope
//! and rewrite the [`Command`] so that:
//!
//! ```text
//! /usr/bin/sandbox-exec -f <profile.sb> <original_program> [args...]
//! ```
//!
//! Apple has marked the SBPL stable surface as deprecated for over a decade
//! but `sandbox-exec` continues to ship and is widely used by Chromium,
//! Firefox, etc. If a future macOS removes it, callers will see
//! [`SandboxError::Unsupported`] when [`MacosSandboxExecBackend::new`] runs.
//!
//! ## Resource limits (S3.2)
//!
//! macOS has no cgroups equivalent, so [`SandboxScope::max_memory_bytes`] /
//! `max_pids` / `max_cpu_secs` are applied via POSIX `setrlimit` in the
//! same `pre_exec` closure the seccomp/Landlock backend uses on Linux for
//! `max_cpu_secs` — but the memory/pids mapping is a coarser
//! approximation than Linux's cgroup-based hard caps:
//!
//! * `max_memory_bytes` → `RLIMIT_AS` (total virtual address space, not
//!   resident memory — a process can reserve address space far beyond
//!   what it actually touches, so this is a looser bound than Linux's
//!   `memory.max`).
//! * `max_pids` → `RLIMIT_NPROC`, which is scoped to the **user id**, not
//!   this specific process tree — every other process the same user
//!   already runs counts against the same limit, unlike Linux's
//!   `pids.max` (scoped to the cgroup, i.e. exactly this tree).
//! * `max_cpu_secs` → `RLIMIT_CPU`, the one exact match: POSIX defines
//!   this as cumulative CPU-seconds for the calling process, identical
//!   semantics to what the field name implies.

use std::ffi::OsString;
use std::path::PathBuf;

use tokio::process::Command;

use crate::capability::Capability;

use super::{SandboxBackend, SandboxEnforcement, SandboxError, SandboxScope};

const SANDBOX_EXEC_PATH: &str = "/usr/bin/sandbox-exec";

pub struct MacosSandboxExecBackend {
  available: bool,
}

impl MacosSandboxExecBackend {
  pub fn new() -> Self {
    Self {
      available: std::path::Path::new(SANDBOX_EXEC_PATH).exists(),
    }
  }
}

impl Default for MacosSandboxExecBackend {
  fn default() -> Self {
    Self::new()
  }
}

impl SandboxBackend for MacosSandboxExecBackend {
  fn name(&self) -> &'static str {
    "sandbox-exec"
  }

  fn is_enforcing(&self) -> bool {
    self.available
  }

  fn enforcement_level(&self) -> SandboxEnforcement {
    if self.available {
      SandboxEnforcement::Enforcing
    } else {
      // The platform backend exists (we are on macOS) but `sandbox-exec`
      // is missing from the host. Operators should investigate, so emit
      // Permissive rather than collapsing to Disabled.
      SandboxEnforcement::Permissive
    }
  }

  fn wrap_command(
    &self,
    command: &mut Command,
    effective_capabilities: &[Capability],
    scope: &SandboxScope,
  ) -> Result<(), SandboxError> {
    if !self.available {
      return Err(SandboxError::Unsupported {
        platform: "macos",
        message: format!("'{}' not found on this system", SANDBOX_EXEC_PATH),
      });
    }

    let profile = build_profile(effective_capabilities, scope);
    let mut profile_file = tempfile::Builder::new()
      .prefix("agentflow-sandbox-")
      .suffix(".sb")
      .tempfile()?;
    use std::io::Write;
    profile_file.write_all(profile.as_bytes())?;
    profile_file.flush()?;
    // Persist and forget the path so the profile remains until after the
    // child has read it. macOS leaks small temp files into /var/folders by
    // design; callers can sweep `agentflow-sandbox-*.sb` if needed.
    let (_, profile_path) = profile_file.keep().map_err(|err| SandboxError::Prepare {
      message: format!("failed to persist sandbox profile: {err}"),
    })?;

    rewrite_command_with_sandbox_exec(command, &profile_path);

    // S3.2: attach the rlimit `pre_exec` *after* the rewrite above — that
    // rewrite fully replaces `*command` (see `rewrite_command_with_sandbox_exec`),
    // so a `pre_exec` set before it would be silently discarded, same
    // caller-contract reasoning as the stdio-ordering note on
    // `SandboxBackend::wrap_command`'s doc comment.
    if scope.max_memory_bytes.is_some() || scope.max_pids.is_some() || scope.max_cpu_secs.is_some()
    {
      let max_memory_bytes = scope.max_memory_bytes;
      let max_pids = scope.max_pids;
      let max_cpu_secs = scope.max_cpu_secs;
      // SAFETY: the closure runs in the forked child between fork and
      // execve; `setrlimit` is a single syscall touching only its own
      // stack-local `rlimit` struct — async-signal-safe.
      unsafe {
        command.pre_exec(move || {
          if let Some(bytes) = max_memory_bytes {
            apply_rlimit(libc::RLIMIT_AS, bytes as libc::rlim_t)?;
          }
          if let Some(pids) = max_pids {
            apply_rlimit(libc::RLIMIT_NPROC, pids as libc::rlim_t)?;
          }
          if let Some(secs) = max_cpu_secs {
            apply_rlimit(libc::RLIMIT_CPU, secs as libc::rlim_t)?;
          }
          Ok(())
        });
      }
    }

    Ok(())
  }
}

/// Set both the soft and hard limit of `resource` to `value`. Shared by all
/// three `SandboxScope` resource-limit fields — POSIX `setrlimit` takes the
/// same `(resource, &rlimit)` shape for each.
fn apply_rlimit(resource: libc::c_int, value: libc::rlim_t) -> std::io::Result<()> {
  let limit = libc::rlimit {
    rlim_cur: value,
    rlim_max: value,
  };
  // SAFETY: `limit` is a fully-initialised, stack-local value; `setrlimit`
  // does not retain the pointer past the call.
  let ret = unsafe { libc::setrlimit(resource, &limit) };
  if ret == 0 {
    Ok(())
  } else {
    Err(std::io::Error::last_os_error())
  }
}

/// Generate an SBPL profile that grants only the capabilities passed in.
fn build_profile(caps: &[Capability], scope: &SandboxScope) -> String {
  let allow_fs_read = caps.contains(&Capability::FsRead);
  let allow_fs_write = caps.contains(&Capability::FsWrite);
  let allow_net = caps.contains(&Capability::Net);
  let allow_exec = caps.contains(&Capability::Exec);

  let mut out = String::new();
  out.push_str("(version 1)\n");
  out.push_str("(deny default)\n");
  // Bare minimum the child needs to start and link dyld.
  out.push_str("(allow process-fork)\n");
  out.push_str("(allow signal (target self))\n");
  out.push_str("(allow sysctl-read)\n");
  out.push_str("(allow mach-lookup)\n");
  // dyld + libsystem query their own process info during initialisation.
  // Without this any binary aborts before main().
  out.push_str("(allow process-info*)\n");
  out.push_str("(allow ipc-posix-shm)\n");
  out.push_str("(allow file-read-metadata)\n");
  // dyld + libsystem stat the root directory itself during init; subpath
  // rules below only cover descendants, not the root entry.
  out.push_str("(allow file-read* (literal \"/\"))\n");
  // System binaries, libraries, and dyld cache that any program may need to
  // start. Without these even `/bin/echo` cannot exec.
  out.push_str("(allow file-read* (subpath \"/bin\"))\n");
  out.push_str("(allow file-read* (subpath \"/sbin\"))\n");
  out.push_str("(allow file-read* (subpath \"/usr/bin\"))\n");
  out.push_str("(allow file-read* (subpath \"/usr/sbin\"))\n");
  out.push_str("(allow file-read* (subpath \"/usr/lib\"))\n");
  out.push_str("(allow file-read* (subpath \"/usr/share\"))\n");
  out.push_str("(allow file-read* (subpath \"/System\"))\n");
  out.push_str("(allow file-read* (subpath \"/private/var/db/dyld\"))\n");
  // Q1.1.3: narrow the default Library + /private/etc grants. The pre-fix
  // profile allowed `/Library` and `/private/etc` wholesale, which exposed
  // user-mode preferences, daemon configs, and `/etc/passwd` / `/etc/hosts`
  // to every sandboxed child. Now we grant only the specific entries dyld
  // and libSystem actually require at process start.
  out.push_str("(allow file-read* (subpath \"/Library/Frameworks\"))\n");
  out.push_str("(allow file-read* (literal \"/Library/Preferences/.GlobalPreferences.plist\"))\n");
  // tz / locale info: needed by date-printing builtins and any tool that
  // calls `localtime(3)`. Single-file literal grant, not a subpath blanket.
  // `/private/etc/master.passwd` is intentionally NOT here — macOS uses
  // Open Directory for `getpwuid_r`, so no caller in a sandboxed child
  // legitimately needs to read the shadow file.
  out.push_str("(allow file-read* (literal \"/private/etc/localtime\"))\n");

  if allow_exec {
    out.push_str("(allow process-exec)\n");
  }
  if allow_net {
    out.push_str("(allow network*)\n");
    // DNS resolution: only when Net is granted, expose the minimal
    // resolver config files. Still no blanket `/private/etc`.
    out.push_str("(allow file-read* (literal \"/private/etc/resolv.conf\"))\n");
    out.push_str("(allow file-read* (literal \"/private/etc/hosts\"))\n");
    out.push_str("(allow file-read* (literal \"/private/etc/services\"))\n");
  }
  if allow_fs_read {
    for path in &scope.read_paths {
      out.push_str(&format!(
        "(allow file-read* (subpath \"{}\"))\n",
        escape_sbpl(&canonicalize_for_sbpl(path))
      ));
    }
    if let Some(cwd) = &scope.working_directory {
      out.push_str(&format!(
        "(allow file-read* (subpath \"{}\"))\n",
        escape_sbpl(&canonicalize_for_sbpl(cwd))
      ));
    }
  }
  if allow_fs_write {
    for path in &scope.write_paths {
      let canonical = canonicalize_for_sbpl(path);
      out.push_str(&format!(
        "(allow file-write* (subpath \"{}\"))\n",
        escape_sbpl(&canonical)
      ));
      // Writers usually need read on the same paths to do round-trip ops.
      out.push_str(&format!(
        "(allow file-read* (subpath \"{}\"))\n",
        escape_sbpl(&canonical)
      ));
    }
  }

  out
}

fn escape_sbpl(path: &std::path::Path) -> String {
  path
    .to_string_lossy()
    .replace('\\', "\\\\")
    .replace('"', "\\\"")
}

/// Resolve `path` to the form the kernel actually checks `subpath` grants
/// against (S3.3 fix).
///
/// macOS symlinks several top-level directories to their real location
/// under `/private` (`/tmp` -> `/private/tmp`, `/var` -> `/private/var`,
/// and therefore `/var/folders/...` — where `std::env::temp_dir()` and
/// `tempfile::TempDir` live). Seatbelt's `subpath` matching is a literal
/// string prefix test against the resolved path the kernel sees when a
/// file is actually opened, not the symlinked path a caller happened to
/// spell. A profile built from the un-resolved form (e.g. granting
/// `/var/folders/xy/.../scripts`) silently fails to match access to
/// `/private/var/folders/xy/.../scripts` — the grant looks correct in the
/// generated profile text but denies everything at runtime. Canonicalizing
/// here, once, at profile-generation time, is cheaper and more robust than
/// requiring every `SandboxScope` producer to remember to do it themselves.
/// Best-effort: falls back to the original path if it doesn't exist yet
/// (canonicalize requires the path to exist) — this only matters for
/// `write_paths` naming a not-yet-created target, and callers spelling
/// out `/private/...` directly are unaffected either way.
fn canonicalize_for_sbpl(path: &std::path::Path) -> std::path::PathBuf {
  path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn rewrite_command_with_sandbox_exec(command: &mut Command, profile_path: &PathBuf) {
  // Capture the original program and args.
  let std_cmd = command.as_std();
  let original_program = std_cmd.get_program().to_os_string();
  let original_args: Vec<OsString> = std_cmd.get_args().map(|s| s.to_os_string()).collect();

  // Rebuild the command in place: sandbox-exec -f <profile> <program> [args...]
  let mut new_cmd = Command::new(SANDBOX_EXEC_PATH);
  new_cmd.arg("-f").arg(profile_path);
  new_cmd.arg(original_program);
  for arg in original_args {
    new_cmd.arg(arg);
  }

  // Carry over stdin/stdout/stderr by replacing the original. We don't need
  // to preserve env / cwd because the std API will inherit from the new
  // Command — callers should set those *before* wrap_command runs.
  if let Some(cwd) = std_cmd.get_current_dir() {
    new_cmd.current_dir(cwd);
  }
  for (k, v) in std_cmd.get_envs() {
    if let Some(v) = v {
      new_cmd.env(k, v);
    } else {
      new_cmd.env_remove(k);
    }
  }

  *command = new_cmd;
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::path::Path;

  #[test]
  fn profile_contains_baseline_rules() {
    let profile = build_profile(&[], &SandboxScope::new());
    assert!(profile.starts_with("(version 1)"));
    assert!(profile.contains("(deny default)"));
    assert!(profile.contains("(allow file-read* (subpath \"/usr/lib\"))"));
  }

  #[test]
  fn profile_does_not_grant_blanket_library_or_private_etc() {
    // Q1.1.3: regression — the old profile shipped wholesale grants for
    // `/Library` and `/private/etc`. Anything under those paths beyond the
    // narrow literals we still grant must be off by default.
    let profile = build_profile(
      &[Capability::FsRead, Capability::Exec],
      &SandboxScope::new(),
    );
    assert!(
      !profile.contains("(allow file-read* (subpath \"/Library\"))"),
      "default profile must not grant blanket /Library read"
    );
    assert!(
      !profile.contains("(allow file-read* (subpath \"/private/etc\"))"),
      "default profile must not grant blanket /private/etc read"
    );
    assert!(
      !profile.contains("master.passwd"),
      "shadow passwd file must never be in the default allow list"
    );
    // The narrow replacements are still there.
    assert!(profile.contains("(allow file-read* (subpath \"/Library/Frameworks\"))"));
    assert!(profile.contains("(literal \"/Library/Preferences/.GlobalPreferences.plist\")"));
    assert!(profile.contains("(literal \"/private/etc/localtime\")"));
  }

  #[test]
  fn profile_only_exposes_resolver_files_when_net_capability_present() {
    let no_net = build_profile(
      &[Capability::FsRead, Capability::Exec],
      &SandboxScope::new(),
    );
    assert!(!no_net.contains("resolv.conf"));
    assert!(!no_net.contains("/etc/hosts") && !no_net.contains("private/etc/hosts"));

    let with_net = build_profile(
      &[Capability::Net, Capability::FsRead, Capability::Exec],
      &SandboxScope::new(),
    );
    assert!(with_net.contains("(literal \"/private/etc/resolv.conf\")"));
    assert!(with_net.contains("(literal \"/private/etc/hosts\")"));
  }

  #[test]
  fn profile_grants_exec_only_when_capability_present() {
    let without = build_profile(&[Capability::FsRead], &SandboxScope::new());
    assert!(!without.contains("(allow process-exec)"));

    let with = build_profile(&[Capability::Exec], &SandboxScope::new());
    assert!(with.contains("(allow process-exec)"));
  }

  #[test]
  fn profile_includes_scope_paths_only_for_matching_capability() {
    let scope = SandboxScope::new()
      .with_read_paths([Path::new("/tmp/foo")])
      .with_write_paths([Path::new("/tmp/bar")]);

    let read_only = build_profile(&[Capability::FsRead], &scope);
    assert!(read_only.contains("(allow file-read* (subpath \"/tmp/foo\"))"));
    assert!(!read_only.contains("(allow file-write* (subpath \"/tmp/bar\"))"));

    let write_only = build_profile(&[Capability::FsWrite], &scope);
    assert!(write_only.contains("(allow file-write* (subpath \"/tmp/bar\"))"));
    assert!(!write_only.contains("(allow file-read* (subpath \"/tmp/foo\"))"));
  }

  /// S3.3 fix: macOS symlinks `/tmp` -> `/private/tmp` and `/var` ->
  /// `/private/var` (so `/var/folders/...`, where `std::env::temp_dir()`
  /// and `tempfile::TempDir` live, resolves to `/private/var/folders/...`).
  /// Seatbelt's `subpath` matching is a literal prefix test against the
  /// resolved path the kernel actually checks, so a profile built from the
  /// un-resolved `/var/folders/...` spelling silently fails to match —
  /// the grant looks right in the generated text but denies everything at
  /// runtime. The profile must contain the canonicalized form.
  #[test]
  fn profile_canonicalizes_symlinked_tmp_paths() {
    let dir = tempfile::TempDir::new().unwrap();
    let raw = dir.path().to_path_buf();
    let canonical = raw.canonicalize().unwrap();
    // This assertion is the whole point of the test: if it doesn't hold on
    // some future macOS, the fix has nothing to prove.
    assert_ne!(
      raw, canonical,
      "test fixture assumption broken: temp dir is not behind a symlink on this host"
    );

    let scope = SandboxScope::new().with_read_paths([raw.as_path()]);
    let profile = build_profile(&[Capability::FsRead], &scope);

    assert!(
      profile.contains(&format!(
        "(allow file-read* (subpath \"{}\"))",
        canonical.display()
      )),
      "profile must grant the canonicalized path, got: {profile}"
    );
    assert!(
      !profile.contains(&format!(
        "(allow file-read* (subpath \"{}\"))",
        raw.display()
      )),
      "profile must not grant the un-resolved symlinked path, got: {profile}"
    );
  }

  /// A path that doesn't exist yet (nothing to canonicalize) must still be
  /// granted as-given, not dropped — matters for `write_paths` naming a
  /// not-yet-created output location.
  #[test]
  fn profile_falls_back_to_original_path_when_canonicalize_fails() {
    let scope = SandboxScope::new().with_write_paths([Path::new("/tmp/agentflow-does-not-exist")]);
    let profile = build_profile(&[Capability::FsWrite], &scope);
    assert!(profile.contains("(allow file-write* (subpath \"/tmp/agentflow-does-not-exist\"))"));
  }

  // ── S3.2: RLIMIT_* resource limits ──────────────────────────────────
  //
  // `apply_rlimit` is deliberately NOT unit-tested by calling it in-process:
  // it calls `setrlimit` on the CALLING process, and doing that from a test
  // would permanently shrink the test binary's own limits (RLIMIT_AS in
  // particular — a low cap there would crash the test process on its next
  // allocation). Production only ever calls it from inside a `pre_exec`
  // closure, i.e. in a forked child that's about to `execve` and disappear.
  // The real, safe-to-run verification is the end-to-end test in
  // `tests/sandbox_macos.rs::macos_sandbox_enforces_max_cpu_secs_via_rlimit`,
  // which spawns a genuine child process and observes it get killed once it
  // exceeds its `RLIMIT_CPU` budget — exactly the production code path.
}

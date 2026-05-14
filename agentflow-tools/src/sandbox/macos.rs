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
    Ok(())
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
  out.push_str("(allow file-read* (subpath \"/Library\"))\n");
  out.push_str("(allow file-read* (subpath \"/private/var/db/dyld\"))\n");
  out.push_str("(allow file-read* (subpath \"/private/etc\"))\n");

  if allow_exec {
    out.push_str("(allow process-exec)\n");
  }
  if allow_net {
    out.push_str("(allow network*)\n");
  }
  if allow_fs_read {
    for path in &scope.read_paths {
      out.push_str(&format!(
        "(allow file-read* (subpath \"{}\"))\n",
        escape_sbpl(path)
      ));
    }
    if let Some(cwd) = &scope.working_directory {
      out.push_str(&format!(
        "(allow file-read* (subpath \"{}\"))\n",
        escape_sbpl(cwd)
      ));
    }
  }
  if allow_fs_write {
    for path in &scope.write_paths {
      out.push_str(&format!(
        "(allow file-write* (subpath \"{}\"))\n",
        escape_sbpl(path)
      ));
      // Writers usually need read on the same paths to do round-trip ops.
      out.push_str(&format!(
        "(allow file-read* (subpath \"{}\"))\n",
        escape_sbpl(path)
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
}

//! S2.2: offline-only per-skill Python virtual environment construction.
//!
//! A skill that declares `[dependencies].python` (S2.1) gets its own
//! isolated `.venv/` inside `skill_dir`, built from the skill's own
//! `vendor/` directory of pre-fetched wheels/sdists via
//! `pip install --no-index --find-links vendor/ --require-hashes`.
//! AgentFlow itself never performs network I/O to build this environment —
//! network-fetched dependency installs are an explicit, separately-scoped
//! follow-up (S2.2b), not this path. See docs/RFC_CODE_EXECUTION_TRUST.md.
//!
//! `.venv/` sits beside `scripts/`, not inside it, so it never enters the
//! S1.1 `[[scripts]]` integrity scan (which only walks `scripts/`) and
//! never gets swept into a marketplace archive's signed contents (it's
//! built locally, after install, from already-vendored, already-verified
//! inputs).

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::error::SkillError;

const VENV_DIR_NAME: &str = ".venv";
const VENDOR_DIR_NAME: &str = "vendor";
const LOCK_MARKER_FILE: &str = ".agentflow-lock-sha256";

/// Ensure `skill_dir/.venv` exists and was built from the exact content of
/// `skill_dir/<requirements_rel_path>`, returning the venv's python
/// interpreter path. Idempotent: a venv whose marker matches the current
/// requirements file's sha256 is reused as-is, not rebuilt.
///
/// Fully offline: installs exclusively from `skill_dir/vendor/` via `pip
/// --no-index --find-links --require-hashes`. Fails loudly (never falls
/// back to the global interpreter) if `python3`/`venv`/`pip` aren't usable
/// or the vendored wheels don't satisfy the pinned, hashed requirements —
/// a skill that declared dependencies and can't get them installed
/// correctly must not silently run against the wrong environment.
pub fn ensure_python_venv(
  skill_dir: &Path,
  requirements_rel_path: &str,
) -> Result<PathBuf, SkillError> {
  let requirements_path = skill_dir.join(requirements_rel_path);
  let requirements_bytes = std::fs::read(&requirements_path).map_err(|e| {
    SkillError::IoError(format!(
      "failed to read [dependencies].python requirements file {}: {e}",
      requirements_path.display()
    ))
  })?;
  let requirements_hash = sha256_hex(&requirements_bytes);

  let venv_dir = skill_dir.join(VENV_DIR_NAME);
  let python_bin = venv_python_path(&venv_dir);
  let marker_path = venv_dir.join(LOCK_MARKER_FILE);

  if python_bin.is_file()
    && std::fs::read_to_string(&marker_path)
      .map(|marked| marked.trim() == requirements_hash)
      .unwrap_or(false)
  {
    return Ok(python_bin);
  }

  let vendor_dir = skill_dir.join(VENDOR_DIR_NAME);
  if !vendor_dir.is_dir() {
    return Err(SkillError::ToolBuildError(format!(
      "skill declares [dependencies].python but has no {} directory to install from \
       offline (AgentFlow never fetches dependencies over the network)",
      vendor_dir.display()
    )));
  }

  // A stale or partially-built venv from a previous (possibly failed or
  // now-outdated) attempt must not linger — rebuild clean.
  if venv_dir.exists() {
    std::fs::remove_dir_all(&venv_dir).map_err(|e| {
      SkillError::IoError(format!(
        "failed to remove stale venv at {}: {e}",
        venv_dir.display()
      ))
    })?;
  }

  run_checked(
    Command::new("python3").arg("-m").arg("venv").arg(&venv_dir),
    "python3 -m venv",
  )?;

  run_checked(
    Command::new(&python_bin)
      .arg("-m")
      .arg("pip")
      .arg("install")
      .arg("--no-index")
      .arg("--find-links")
      .arg(&vendor_dir)
      .arg("--require-hashes")
      .arg("-r")
      .arg(&requirements_path),
    "pip install --no-index --require-hashes",
  )?;

  std::fs::write(&marker_path, &requirements_hash).map_err(|e| {
    SkillError::IoError(format!(
      "failed to write venv lock marker {}: {e}",
      marker_path.display()
    ))
  })?;

  Ok(python_bin)
}

fn venv_python_path(venv_dir: &Path) -> PathBuf {
  if cfg!(windows) {
    venv_dir.join("Scripts").join("python.exe")
  } else {
    venv_dir.join("bin").join("python3")
  }
}

fn run_checked(cmd: &mut Command, label: &str) -> Result<(), SkillError> {
  let output = cmd
    .output()
    .map_err(|e| SkillError::ToolBuildError(format!("failed to spawn {label}: {e}")))?;
  if !output.status.success() {
    return Err(SkillError::ToolBuildError(format!(
      "{label} failed (exit {:?}): {}",
      output.status.code(),
      String::from_utf8_lossy(&output.stderr).trim()
    )));
  }
  Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
  let mut hasher = Sha256::new();
  hasher.update(bytes);
  format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::Write;
  use tempfile::TempDir;

  fn write_file(path: &Path, content: &str) {
    if let Some(p) = path.parent() {
      std::fs::create_dir_all(p).expect("mkdir");
    }
    let mut f = std::fs::File::create(path).expect("create");
    f.write_all(content.as_bytes()).expect("write");
  }

  fn python3_available() -> bool {
    Command::new("python3")
      .arg("--version")
      .output()
      .is_ok_and(|o| o.status.success())
  }

  #[test]
  fn rejects_when_vendor_dir_missing() {
    let dir = TempDir::new().unwrap();
    write_file(&dir.path().join("requirements.txt"), "pkg==1.0.0\n");

    let result = ensure_python_venv(dir.path(), "requirements.txt");

    assert!(matches!(result, Err(SkillError::ToolBuildError(_))));
  }

  #[test]
  fn rejects_when_requirements_file_missing() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join("vendor")).unwrap();

    let result = ensure_python_venv(dir.path(), "requirements.txt");

    assert!(matches!(result, Err(SkillError::IoError(_))));
  }

  /// Cheap idempotency check that doesn't require a real pip/venv: if the
  /// interpreter file + marker already agree with the requirements hash,
  /// `ensure_python_venv` must return immediately without touching
  /// `vendor/` at all (which we deliberately leave absent here — if the
  /// function tried to rebuild, it would hit the "vendor dir missing"
  /// error instead of succeeding).
  #[test]
  fn reuses_existing_venv_when_marker_matches_without_touching_vendor() {
    let dir = TempDir::new().unwrap();
    let requirements_content = "pkg==1.0.0 --hash=sha256:deadbeef\n";
    write_file(&dir.path().join("requirements.txt"), requirements_content);

    let venv_dir = dir.path().join(".venv");
    let python_bin = venv_python_path(&venv_dir);
    write_file(&python_bin, "#!/bin/sh\n"); // stand-in "interpreter"
    write_file(
      &venv_dir.join(".agentflow-lock-sha256"),
      &sha256_hex(requirements_content.as_bytes()),
    );
    // No vendor/ dir — proves no rebuild was attempted.

    let result = ensure_python_venv(dir.path(), "requirements.txt").unwrap();
    assert_eq!(result, python_bin);
  }

  /// A stale marker (requirements changed since last build) must trigger
  /// a rebuild attempt, not a silent reuse of the old environment.
  #[test]
  fn stale_marker_triggers_rebuild_attempt() {
    let dir = TempDir::new().unwrap();
    write_file(
      &dir.path().join("requirements.txt"),
      "pkg==2.0.0 --hash=sha256:deadbeef\n",
    );

    let venv_dir = dir.path().join(".venv");
    let python_bin = venv_python_path(&venv_dir);
    write_file(&python_bin, "#!/bin/sh\n");
    write_file(&venv_dir.join(".agentflow-lock-sha256"), "stale-hash");
    // No vendor/ dir: the attempted rebuild must fail with the
    // vendor-missing error, proving reuse was correctly rejected.

    let result = ensure_python_venv(dir.path(), "requirements.txt");
    assert!(matches!(result, Err(SkillError::ToolBuildError(_))));
  }

  /// End-to-end smoke test against a real, hand-built minimal wheel —
  /// skipped when python3 isn't on PATH (matches the `python3_available`
  /// skip idiom used elsewhere in the workspace, e.g.
  /// agentflow-tools/tests/sandbox_linux.rs).
  #[test]
  fn builds_working_venv_from_vendored_wheel() {
    if !python3_available() {
      eprintln!("skipping: python3 not on PATH");
      return;
    }

    let dir = TempDir::new().unwrap();
    let vendor_dir = dir.path().join("vendor");
    std::fs::create_dir(&vendor_dir).unwrap();

    let wheel_path = vendor_dir.join("agentflow_test_pkg-1.0.0-py3-none-any.whl");
    let build_wheel_script = dir.path().join("build_wheel.py");
    write_file(&build_wheel_script, WHEEL_BUILDER_SCRIPT);
    let status = Command::new("python3")
      .arg(&build_wheel_script)
      .arg(&wheel_path)
      .status()
      .expect("spawn wheel builder");
    assert!(status.success(), "failed to build the test fixture wheel");

    let wheel_bytes = std::fs::read(&wheel_path).unwrap();
    let wheel_hash = sha256_hex(&wheel_bytes);
    write_file(
      &dir.path().join("requirements.txt"),
      &format!("agentflow-test-pkg==1.0.0 --hash=sha256:{wheel_hash}\n"),
    );

    let python_bin = ensure_python_venv(dir.path(), "requirements.txt").unwrap();
    assert!(python_bin.is_file());

    // Idempotent second call must reuse the venv (no error, same path),
    // without needing vendor/ to still resolve anything new.
    let python_bin_again = ensure_python_venv(dir.path(), "requirements.txt").unwrap();
    assert_eq!(python_bin, python_bin_again);

    // The installed package must actually be importable.
    let output = Command::new(&python_bin)
      .arg("-c")
      .arg("import agentflow_test_pkg; print(agentflow_test_pkg.VALUE)")
      .output()
      .expect("spawn venv python");
    assert!(
      output.status.success(),
      "import failed: {}",
      String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "42");
  }

  /// Builds a minimal, valid, dependency-free wheel by hand (via stdlib
  /// `zipfile`) — avoids requiring `pip`/`setuptools`/a build backend to
  /// be present just to produce a test fixture.
  const WHEEL_BUILDER_SCRIPT: &str = r#"
import sys
import zipfile

out_path = sys.argv[1]
dist_info = "agentflow_test_pkg-1.0.0.dist-info"

with zipfile.ZipFile(out_path, "w") as zf:
    zf.writestr("agentflow_test_pkg.py", "VALUE = 42\n")
    zf.writestr(
        f"{dist_info}/METADATA",
        "Metadata-Version: 2.1\nName: agentflow-test-pkg\nVersion: 1.0.0\n",
    )
    zf.writestr(
        f"{dist_info}/WHEEL",
        "Wheel-Version: 1.0\nGenerator: agentflow-test\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
    )
    zf.writestr(f"{dist_info}/RECORD", "")
"#;
}

//! Workspace automation entry point.
//!
//! Run with `cargo xtask <subcommand>` (alias defined in `.cargo/config.toml`).
//! Subcommands available today:
//!
//! - `verify-edition` — assert every workspace member declares
//!   `edition = "2024"` so a freshly-added crate cannot silently drift to a
//!   different edition (`M.6`).

use anyhow::{Context, Result, bail};
use std::io::Write;
use std::path::{Path, PathBuf};

const EXPECTED_EDITION: &str = "2024";

fn main() -> Result<()> {
  let mut args = std::env::args().skip(1);
  let subcommand = args.next().unwrap_or_default();
  match subcommand.as_str() {
    "verify-edition" => {
      let workspace_root = workspace_root();
      verify_edition_at(
        &workspace_root,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
      )
    }
    other => {
      print_usage(&mut std::io::stderr());
      if other.is_empty() {
        bail!("missing subcommand");
      }
      bail!("unknown subcommand '{other}'");
    }
  }
}

fn print_usage(sink: &mut impl Write) {
  let _ = writeln!(sink, "usage: cargo xtask <subcommand>");
  let _ = writeln!(sink, "subcommands:");
  let _ = writeln!(
    sink,
    "  verify-edition    fail if any workspace member declares an edition other than \"{EXPECTED_EDITION}\""
  );
}

/// Run the edition-pin check against `workspace_root` and report through the
/// caller-supplied sinks. Returns `Ok(())` on a clean workspace and a context-
/// rich error when one or more members declare an unexpected edition.
fn verify_edition_at(
  workspace_root: &Path,
  stdout: &mut impl Write,
  stderr: &mut impl Write,
) -> Result<()> {
  let members = read_workspace_members(workspace_root)?;
  let mut failures: Vec<String> = Vec::new();
  let mut checked: Vec<String> = Vec::new();
  for member in &members {
    let manifest = workspace_root.join(member).join("Cargo.toml");
    let edition = read_edition(&manifest)
      .with_context(|| format!("Failed to read edition for member '{}'", manifest.display()))?;
    if edition != EXPECTED_EDITION {
      failures.push(format!(
        "  - {}: edition = \"{}\" (expected \"{}\")",
        member, edition, EXPECTED_EDITION
      ));
    }
    checked.push(member.clone());
  }
  writeln!(
    stdout,
    "verify-edition: checked {} workspace member(s) against edition \"{}\"",
    checked.len(),
    EXPECTED_EDITION
  )?;
  if failures.is_empty() {
    writeln!(stdout, "verify-edition: OK")?;
    return Ok(());
  }
  writeln!(stderr, "verify-edition: FAIL")?;
  for line in &failures {
    writeln!(stderr, "{line}")?;
  }
  bail!(
    "{} workspace member(s) declare an unexpected edition",
    failures.len()
  );
}

fn workspace_root() -> PathBuf {
  // `CARGO_MANIFEST_DIR` for the xtask crate is `<workspace>/xtask`.
  let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  manifest_dir
    .parent()
    .map(PathBuf::from)
    .unwrap_or(manifest_dir)
}

fn read_workspace_members(workspace_root: &Path) -> Result<Vec<String>> {
  let manifest_path = workspace_root.join("Cargo.toml");
  let content = std::fs::read_to_string(&manifest_path)
    .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
  let parsed: toml::Value = toml::from_str(&content)
    .with_context(|| format!("Failed to parse {}", manifest_path.display()))?;
  let members = parsed
    .get("workspace")
    .and_then(|w| w.get("members"))
    .and_then(|m| m.as_array())
    .ok_or_else(|| anyhow::anyhow!("workspace.members array missing in root Cargo.toml"))?;
  let mut out: Vec<String> = Vec::with_capacity(members.len());
  for entry in members {
    if let Some(name) = entry.as_str() {
      // Skip xtask itself: it's part of the workspace but its own edition is
      // governed by the same rule, so include it. Only deliberate skip: none.
      out.push(name.to_string());
    }
  }
  // Stable iteration order so CI logs diff cleanly.
  out.sort();
  Ok(out)
}

fn read_edition(manifest: &Path) -> Result<String> {
  let content = std::fs::read_to_string(manifest)
    .with_context(|| format!("Failed to read {}", manifest.display()))?;
  let parsed: toml::Value =
    toml::from_str(&content).with_context(|| format!("Failed to parse {}", manifest.display()))?;
  let edition = parsed
    .get("package")
    .and_then(|p| p.get("edition"))
    .and_then(|e| e.as_str())
    .ok_or_else(|| {
      anyhow::anyhow!(
        "package.edition missing from {} — every workspace member must declare an edition",
        manifest.display()
      )
    })?;
  Ok(edition.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Write a synthetic workspace under `root` with the given members; each
  /// (`name`, `edition`) tuple becomes a `<root>/<name>/Cargo.toml` with the
  /// requested edition.
  fn write_synthetic_workspace(root: &Path, members: &[(&str, &str)]) {
    let members_lines: String = members
      .iter()
      .map(|(name, _)| format!("  \"{name}\",\n"))
      .collect();
    let root_manifest = format!("[workspace]\nmembers = [\n{members_lines}]\nresolver = \"2\"\n");
    std::fs::write(root.join("Cargo.toml"), root_manifest).unwrap();
    for (name, edition) in members {
      let member_dir = root.join(name);
      std::fs::create_dir_all(&member_dir).unwrap();
      let manifest =
        format!("[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"{edition}\"\n");
      std::fs::write(member_dir.join("Cargo.toml"), manifest).unwrap();
    }
  }

  fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir().expect("create tempdir")
  }

  #[test]
  fn passes_when_every_member_is_pinned() {
    let root = tempdir();
    write_synthetic_workspace(
      root.path(),
      &[("alpha", EXPECTED_EDITION), ("beta", EXPECTED_EDITION)],
    );
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let result = verify_edition_at(root.path(), &mut stdout, &mut stderr);
    assert!(result.is_ok(), "{}", String::from_utf8_lossy(&stderr));
    let stdout_s = String::from_utf8(stdout).unwrap();
    assert!(stdout_s.contains("checked 2 workspace member(s)"));
    assert!(stdout_s.contains("verify-edition: OK"));
  }

  #[test]
  fn fails_when_member_uses_wrong_edition() {
    let root = tempdir();
    write_synthetic_workspace(
      root.path(),
      &[("alpha", EXPECTED_EDITION), ("legacy", "2021")],
    );
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let result = verify_edition_at(root.path(), &mut stdout, &mut stderr);
    let err = result.expect_err("expected wrong-edition failure");
    let err_msg = format!("{err:#}");
    assert!(err_msg.contains("1 workspace member"), "error: {err_msg}");
    let stderr_s = String::from_utf8(stderr).unwrap();
    assert!(
      stderr_s.contains("legacy: edition = \"2021\""),
      "stderr: {stderr_s}"
    );
    assert!(stderr_s.contains("verify-edition: FAIL"));
  }

  #[test]
  fn errors_when_member_omits_edition_entirely() {
    let root = tempdir();
    // Skip the helper because it always writes an edition; craft by hand.
    let root_manifest = "[workspace]\nmembers = [\"orphan\"]\nresolver = \"2\"\n";
    std::fs::write(root.path().join("Cargo.toml"), root_manifest).unwrap();
    std::fs::create_dir_all(root.path().join("orphan")).unwrap();
    std::fs::write(
      root.path().join("orphan/Cargo.toml"),
      "[package]\nname = \"orphan\"\nversion = \"0.0.0\"\n",
    )
    .unwrap();
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let result = verify_edition_at(root.path(), &mut stdout, &mut stderr);
    let err = result.expect_err("expected missing-edition failure");
    let err_msg = format!("{err:#}");
    assert!(
      err_msg.contains("package.edition missing"),
      "error: {err_msg}"
    );
  }
}

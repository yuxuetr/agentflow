//! `println-lint` (V1.7) — fail if agentflow-core/agentflow-nodes/agentflow-nodes-ai
//! contain println!/eprintln! used as logging outside test code; suppress a
//! documented exception with `// allow-println-lint: <reason>`.
//!
//! `agentflow-core`/`agentflow-nodes`/`agentflow-nodes-ai` used to lean on
//! `println!`/`eprintln!` as ad-hoc logging (some with emoji, some dumping
//! rendered templates / prompts / tool params), unconditionally polluting
//! stdout/stderr for every consumer including JSON-output CLI modes. V1.7
//! routed all of that through `tracing`; this static gate gives that
//! invariant a CI checkpoint so it can't silently regress. Mirrors
//! `redaction_lint_at`'s shape.

use anyhow::{Context, Result, bail};
use std::io::Write;
use std::path::{Path, PathBuf};

struct PrintlnLintHit {
  path: PathBuf,
  line: usize,
  snippet: String,
}

/// Crate `src/` trees this gate scans — exactly the three the V1.7
/// evaluation finding named.
const PRINTLN_LINT_CRATES: &[&str] = &["agentflow-core", "agentflow-nodes", "agentflow-nodes-ai"];

/// Deliberate, documented exceptions (see the V1.7 commit that introduced
/// this gate):
///  - `bin/echo_plugin.rs`: a standalone reference-plugin binary (its own
///    process), meant as a template for plugin authors, not library code
///    that pollutes a host application's output.
///
/// `shutdown.rs` used to be exempt too (its ctrl_c/SIGTERM install-failure
/// logging had to work in consumers that didn't enable agentflow-core's
/// `observability` feature) — W0.7 made `tracing` an unconditional
/// dependency, so it now routes through `tracing::error!` like everything
/// else and no longer needs the exemption.
const PRINTLN_LINT_EXEMPT_FILES: &[&str] = &["agentflow-core/src/bin/echo_plugin.rs"];

pub(crate) fn println_lint_at(
  workspace_root: &Path,
  stdout: &mut impl Write,
  stderr: &mut impl Write,
) -> Result<()> {
  let mut hits: Vec<PrintlnLintHit> = Vec::new();
  for crate_name in PRINTLN_LINT_CRATES {
    let src = workspace_root.join(crate_name).join("src");
    if src.is_dir() {
      collect_println_hits(workspace_root, &src, &mut hits)?;
    }
  }
  hits.sort_by_key(|h| (h.path.clone(), h.line));

  for hit in &hits {
    let _ = writeln!(
      stderr,
      "{}:{}: println-lint: println!/eprintln! used as logging outside test code\n    {}",
      hit.path.display(),
      hit.line,
      hit.snippet
    );
  }

  if hits.is_empty() {
    let _ = writeln!(
      stdout,
      "println-lint: OK ({} crate dirs scanned)",
      PRINTLN_LINT_CRATES.len()
    );
    Ok(())
  } else {
    let _ = writeln!(
      stderr,
      "\nprintln-lint: {} hit(s); route through tracing::{{info,debug,warn,error}}! instead \
       (V1.7, TODOs.md). Suppress a genuine, documented exception with \
       `// allow-println-lint: <reason>` on the same line.",
      hits.len()
    );
    bail!("println-lint failed: {} hit(s)", hits.len());
  }
}

fn collect_println_hits(
  workspace_root: &Path,
  dir: &Path,
  hits: &mut Vec<PrintlnLintHit>,
) -> Result<()> {
  for entry in std::fs::read_dir(dir)? {
    let entry = entry?;
    let path = entry.path();
    if path.is_dir() {
      collect_println_hits(workspace_root, &path, hits)?;
    } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
      let relative = path.strip_prefix(workspace_root).unwrap_or(&path);
      if PRINTLN_LINT_EXEMPT_FILES
        .iter()
        .any(|f| relative == Path::new(f))
      {
        continue;
      }
      scan_file_for_println(&path, relative, hits)?;
    }
  }
  Ok(())
}

/// Stateful per-file scan: unlike `detect_redaction_hit`, whether a line
/// counts depends on what came before it in the same file (are we past a
/// `#[cfg(test)]` module?), so this isn't a pure per-line function.
///
/// The "past `#[cfg(test)]` ⇒ skip the rest of the file" heuristic assumes
/// the workspace convention of declaring `#[cfg(test)] mod tests { ... }`
/// once, at the end of the file — true everywhere in these three crates
/// today (a file with a second, later non-test item after its test module
/// would produce a false negative here, not a false positive).
fn scan_file_for_println(
  path: &Path,
  relative_path: &Path,
  hits: &mut Vec<PrintlnLintHit>,
) -> Result<()> {
  let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
  let mut in_test_mod = false;
  for (idx, line) in text.lines().enumerate() {
    let trimmed = line.trim_start();
    if trimmed.starts_with("#[cfg(test)]") {
      in_test_mod = true;
    }
    if in_test_mod {
      continue;
    }
    // Covers `//`, `///`, and `//!` alike.
    if trimmed.starts_with("//") {
      continue;
    }
    if line.contains("allow-println-lint") {
      continue;
    }
    if line.contains("println!(") || line.contains("eprintln!(") {
      hits.push(PrintlnLintHit {
        path: relative_path.to_path_buf(),
        line: idx + 1,
        snippet: line.trim().to_string(),
      });
    }
  }
  Ok(())
}

#[cfg(test)]
mod println_lint_tests {
  use super::*;
  use std::fs;
  use tempfile::TempDir;

  fn write_crate_src(workspace: &Path, crate_name: &str, file: &str, content: &str) {
    let src = workspace.join(crate_name).join("src");
    fs::create_dir_all(&src).unwrap();
    let file_path = src.join(file);
    if let Some(parent) = file_path.parent() {
      fs::create_dir_all(parent).unwrap();
    }
    fs::write(file_path, content).unwrap();
  }

  #[test]
  fn flags_println_and_eprintln_outside_test_code() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();
    write_crate_src(
      workspace,
      "agentflow-core",
      "lib.rs",
      "fn a() { println!(\"hi\"); }\nfn b() { eprintln!(\"warn\"); }\n",
    );

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let err = println_lint_at(workspace, &mut stdout, &mut stderr).unwrap_err();
    assert!(format!("{err}").contains("2 hit"));
  }

  #[test]
  fn ignores_println_inside_cfg_test_module() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();
    write_crate_src(
      workspace,
      "agentflow-nodes",
      "lib.rs",
      "fn a() {}\n#[cfg(test)]\nmod tests {\n  fn t() { println!(\"skip me\"); }\n}\n",
    );

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    println_lint_at(workspace, &mut stdout, &mut stderr).expect("test-only println must pass");
    let stdout_text = String::from_utf8(stdout).unwrap();
    assert!(stdout_text.contains("println-lint: OK"));
  }

  #[test]
  fn ignores_doc_comment_examples() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();
    write_crate_src(
      workspace,
      "agentflow-nodes-ai",
      "lib.rs",
      "/// ```\n/// println!(\"example\");\n/// ```\n//! println!(\"module doc\");\nfn a() {}\n",
    );

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    println_lint_at(workspace, &mut stdout, &mut stderr).expect("doc comments must pass");
  }

  #[test]
  fn allow_println_lint_comment_suppresses_hit() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();
    write_crate_src(
      workspace,
      "agentflow-core",
      "lib.rs",
      "fn a() { println!(\"bad\"); }\n\
       fn b() { println!(\"benign\"); } // allow-println-lint: test fixture\n",
    );

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let err = println_lint_at(workspace, &mut stdout, &mut stderr).unwrap_err();
    let err_text = format!("{err}");
    let stderr_text = String::from_utf8(stderr).unwrap();
    assert!(err_text.contains("1 hit"), "expected 1 hit, got {err_text}");
    assert!(
      !stderr_text.contains("lib.rs:2"),
      "allow-marker line must be suppressed, got {stderr_text}"
    );
  }

  #[test]
  fn exempt_files_are_skipped_entirely() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();
    write_crate_src(
      workspace,
      "agentflow-core",
      "bin/echo_plugin.rs",
      "fn a() { eprintln!(\"standalone reference-plugin binary output\"); }\n",
    );

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    println_lint_at(workspace, &mut stdout, &mut stderr).expect("exempt file must not be flagged");
  }

  #[test]
  fn only_scans_the_three_named_crates() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();
    write_crate_src(
      workspace,
      "agentflow-cli",
      "lib.rs",
      "fn a() { println!(\"CLI output is fine here\"); }\n",
    );

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    println_lint_at(workspace, &mut stdout, &mut stderr)
      .expect("crates outside the named three must not be scanned");
  }
}

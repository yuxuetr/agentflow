//! `check-changelog` (P10.18.2) — fail when a non-trivial source change versus
//! the base ref (default `origin/main`) didn't touch `CHANGELOG.md`
//! AND no commit body in the branch range carries the
//! `chore(skip-changelog)` opt-out marker.
//!
//! Run `cargo xtask check-changelog [BASE_REF]` (default `origin/main`).
//! Behaviour:
//!
//!   1. Compute the set of files changed in BASE_REF...HEAD via
//!      `git diff --name-only`.
//!   2. Classify: any file outside the "trivial" allowlist (docs/, tests/,
//!      lockfiles, the CHANGELOG itself, gitignore housekeeping) counts as
//!      "source" — i.e. should normally land with a CHANGELOG entry.
//!   3. If no source change → exit 0 (trivial PR).
//!   4. Else: PASS when EITHER `CHANGELOG.md` is in the changed set OR
//!      any commit body in BASE_REF..HEAD contains `chore(skip-changelog)`.
//!   5. Else: exit 1 with a single-line diagnostic listing the source
//!      files that triggered the requirement.
//!
//! The gate is intentionally **not** wired into `quality.yml` today —
//! landing it as a separate xtask first lets contributors run it
//! locally (`cargo xtask check-changelog`) and gives the workspace
//! experience time to confirm the heuristic before it gates PRs.
//!
//! Determinism notes:
//!   - The `git` calls run from the workspace root; stdout / stderr
//!     are not used for diagnostics so concurrent test invocations
//!     don't interleave.
//!   - The base ref must already exist (the caller is expected to
//!     `git fetch` before calling; CI's checkout action does this).

use anyhow::{Context, Result, bail};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const TRIVIAL_PATH_PREFIXES: &[&str] =
  &["docs/", "CHANGELOG.md", ".gitignore", ".github/workflows/"];

const TRIVIAL_PATH_SUFFIXES: &[&str] = &[
  ".md",
  // lockfiles + dep manifests shift on dep bumps which usually don't
  // need a user-facing CHANGELOG entry on their own.
  "Cargo.lock",
  "package-lock.json",
];

/// Returns `true` when the path is in the trivial set — touching it
/// alone doesn't require a CHANGELOG bump.
fn is_trivial_changelog_path(path: &str) -> bool {
  // `CHANGELOG.md` itself is trivial — the whole point is to catch
  // changes that DIDN'T touch it.
  if TRIVIAL_PATH_PREFIXES.iter().any(|p| path.starts_with(p)) {
    return true;
  }
  if TRIVIAL_PATH_SUFFIXES.iter().any(|s| path.ends_with(s)) {
    return true;
  }
  // Test-only files (tests/*.rs, *.test.ts, fixtures/) — internal
  // coverage; not a user-facing change.
  if path.starts_with("tests/")
    || path.contains("/tests/")
    || path.contains("/fixtures/")
    || path.ends_with(".test.ts")
    || path.ends_with(".test.rs")
  {
    return true;
  }
  false
}

pub(crate) fn check_changelog_from_args(
  workspace_root: &Path,
  args: Vec<String>,
  stdout: &mut impl Write,
  stderr: &mut impl Write,
) -> Result<()> {
  let base_ref = args
    .first()
    .cloned()
    .unwrap_or_else(|| "origin/main".to_string());

  // The diff range `BASE...HEAD` (three-dot) compares HEAD against
  // the merge base, which is what most "PR vs main" semantics want.
  // Two-dot would compare against the tip and include unrelated
  // upstream changes if main moved.
  let diff_range = format!("{base_ref}...HEAD");
  let diff_output = Command::new("git")
    .args(["diff", "--name-only", &diff_range])
    .current_dir(workspace_root)
    .stderr(Stdio::piped())
    .output()
    .context("running `git diff --name-only`")?;
  if !diff_output.status.success() {
    let detail = String::from_utf8_lossy(&diff_output.stderr).into_owned();
    let _ = writeln!(stderr, "check-changelog: git diff failed: {detail}");
    bail!(
      "git diff against '{base_ref}' failed — does the ref exist locally? \
       (CI typically needs `git fetch` first)"
    );
  }

  let changed_files: Vec<String> = String::from_utf8_lossy(&diff_output.stdout)
    .lines()
    .map(|s| s.to_string())
    .filter(|s| !s.is_empty())
    .collect();

  let changelog_touched = changed_files.iter().any(|p| p == "CHANGELOG.md");

  let non_trivial: Vec<&str> = changed_files
    .iter()
    .filter(|path| !is_trivial_changelog_path(path))
    .map(String::as_str)
    .collect();

  if non_trivial.is_empty() {
    let _ = writeln!(
      stdout,
      "check-changelog: only trivial paths changed against '{base_ref}'; no CHANGELOG bump required",
    );
    return Ok(());
  }

  if changelog_touched {
    let _ = writeln!(
      stdout,
      "check-changelog: CHANGELOG.md touched in {} commit(s) against '{base_ref}'; PASS",
      changed_files.len(),
    );
    return Ok(());
  }

  // Last chance: check commit bodies for the opt-out marker.
  let log_output = Command::new("git")
    .args(["log", &format!("{base_ref}..HEAD"), "--format=%B"])
    .current_dir(workspace_root)
    .stderr(Stdio::piped())
    .output()
    .context("running `git log --format=%B`")?;
  if !log_output.status.success() {
    let detail = String::from_utf8_lossy(&log_output.stderr).into_owned();
    let _ = writeln!(stderr, "check-changelog: git log failed: {detail}");
    bail!("git log against '{base_ref}' failed");
  }
  let log_body = String::from_utf8_lossy(&log_output.stdout);
  if log_body.contains("chore(skip-changelog)") {
    let _ = writeln!(
      stdout,
      "check-changelog: commit body in '{base_ref}..HEAD' carries `chore(skip-changelog)`; PASS",
    );
    return Ok(());
  }

  // Failure path — list the offending source files so the operator
  // doesn't have to grep `git diff` themselves.
  let _ = writeln!(
    stderr,
    "check-changelog: non-trivial source changes vs '{base_ref}' but neither CHANGELOG.md \
     was touched nor any commit body carries `chore(skip-changelog)`.",
  );
  let _ = writeln!(stderr, "Non-trivial paths:");
  for path in &non_trivial {
    let _ = writeln!(stderr, "  - {path}");
  }
  let _ = writeln!(
    stderr,
    "\nFix: add an entry to CHANGELOG.md, OR add `chore(skip-changelog)` on its own line \
     in a commit body when the change really is doc-only / refactor-only.",
  );
  bail!("check-changelog: missing CHANGELOG bump")
}

#[cfg(test)]
mod check_changelog_tests {
  use super::*;
  use std::fs;
  use tempfile::TempDir;

  /// Helper: initialise a fresh git repo with one commit on `main`
  /// so the tests have a base ref to diff against. Returns the
  /// repo root.
  fn init_repo() -> TempDir {
    let dir = TempDir::new().expect("tempdir");
    let root = dir.path();
    // Configure local git identity so commits work without the
    // ambient user config — CI runners often have neither.
    for args in [
      vec!["init", "--initial-branch=main", "--quiet"],
      vec!["config", "user.email", "test@example.com"],
      vec!["config", "user.name", "Test"],
    ] {
      assert!(
        Command::new("git")
          .args(args)
          .current_dir(root)
          .status()
          .expect("git config")
          .success()
      );
    }
    // Seed one commit so HEAD~1 is meaningful.
    fs::write(root.join("CHANGELOG.md"), "# Changelog\n").unwrap();
    fs::write(root.join("seed.rs"), "fn main() {}\n").unwrap();
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-m", "seed", "--quiet"]);
    dir
  }

  fn run_git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
      .args(args)
      .current_dir(root)
      .status()
      .expect("git");
    assert!(status.success(), "git {args:?} failed");
  }

  #[test]
  fn trivial_path_classifier_covers_docs_and_lockfiles() {
    // The class set is the single source of truth for the pass /
    // fail boundary. Pin each prefix / suffix so a regression that
    // narrows the set surfaces here, not in the wild.
    assert!(is_trivial_changelog_path("docs/MEMO.md"));
    assert!(is_trivial_changelog_path("docs/whatever.txt"));
    assert!(is_trivial_changelog_path("README.md"));
    assert!(is_trivial_changelog_path("CHANGELOG.md"));
    assert!(is_trivial_changelog_path("Cargo.lock"));
    assert!(is_trivial_changelog_path("agentflow-ui/package-lock.json"));
    assert!(is_trivial_changelog_path(".gitignore"));
    assert!(is_trivial_changelog_path(".github/workflows/quality.yml"));
    assert!(is_trivial_changelog_path("tests/foo.rs"));
    assert!(is_trivial_changelog_path(
      "agentflow-llm/tests/integration.rs"
    ));
    assert!(is_trivial_changelog_path(
      "agentflow-cli/tests/fixtures/x.json"
    ));
    assert!(is_trivial_changelog_path("agentflow-ui/src/foo.test.ts"));

    // Real source files must NOT be classified as trivial.
    assert!(!is_trivial_changelog_path("agentflow-core/src/flow.rs"));
    assert!(!is_trivial_changelog_path("agentflow-cli/Cargo.toml"));
    assert!(!is_trivial_changelog_path("agentflow-ui/src/main.tsx"));
  }

  #[test]
  fn pass_when_only_docs_changed() {
    let repo = init_repo();
    let root = repo.path();
    // Branch + commit a docs-only change. mkdir BEFORE write.
    run_git(root, &["checkout", "-b", "feature", "--quiet"]);
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("docs/new.md"), "hi").unwrap();
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-m", "docs: add note", "--quiet"]);

    let mut out = Vec::new();
    let mut err = Vec::new();
    let res = check_changelog_from_args(root, vec!["main".into()], &mut out, &mut err);
    assert!(res.is_ok(), "stderr: {}", String::from_utf8_lossy(&err));
    let stdout = String::from_utf8(out).unwrap();
    assert!(stdout.contains("no CHANGELOG bump required"), "{stdout}");
  }

  #[test]
  fn pass_when_source_change_touches_changelog() {
    let repo = init_repo();
    let root = repo.path();
    run_git(root, &["checkout", "-b", "feature", "--quiet"]);
    fs::write(root.join("seed.rs"), "fn main() { println!(\"x\"); }\n").unwrap();
    fs::write(root.join("CHANGELOG.md"), "# Changelog\n## entry\n").unwrap();
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-m", "feat: x", "--quiet"]);

    let mut out = Vec::new();
    let mut err = Vec::new();
    let res = check_changelog_from_args(root, vec!["main".into()], &mut out, &mut err);
    assert!(res.is_ok(), "stderr: {}", String::from_utf8_lossy(&err));
    let stdout = String::from_utf8(out).unwrap();
    assert!(stdout.contains("CHANGELOG.md touched"), "{stdout}");
  }

  #[test]
  fn pass_when_commit_body_carries_skip_marker() {
    let repo = init_repo();
    let root = repo.path();
    run_git(root, &["checkout", "-b", "feature", "--quiet"]);
    fs::write(root.join("seed.rs"), "fn main() { /* refactor */ }\n").unwrap();
    run_git(root, &["add", "."]);
    // The marker is on its own line in the commit body, matching
    // the convention documented in `print_usage`.
    run_git(
      root,
      &[
        "commit",
        "-m",
        "refactor: rename",
        "-m",
        "chore(skip-changelog)",
        "--quiet",
      ],
    );

    let mut out = Vec::new();
    let mut err = Vec::new();
    let res = check_changelog_from_args(root, vec!["main".into()], &mut out, &mut err);
    assert!(res.is_ok(), "stderr: {}", String::from_utf8_lossy(&err));
    let stdout = String::from_utf8(out).unwrap();
    assert!(stdout.contains("skip-changelog"), "{stdout}");
  }

  #[test]
  fn fail_when_source_change_without_changelog_or_marker() {
    let repo = init_repo();
    let root = repo.path();
    run_git(root, &["checkout", "-b", "feature", "--quiet"]);
    fs::write(root.join("seed.rs"), "fn main() { println!(\"bug\"); }\n").unwrap();
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-m", "fix: oops", "--quiet"]);

    let mut out = Vec::new();
    let mut err = Vec::new();
    let res = check_changelog_from_args(root, vec!["main".into()], &mut out, &mut err);
    assert!(
      res.is_err(),
      "must fail; stdout={}",
      String::from_utf8_lossy(&out)
    );
    let stderr = String::from_utf8(err).unwrap();
    assert!(
      stderr.contains("seed.rs"),
      "diagnostic must name the offending file: {stderr}"
    );
    assert!(
      stderr.contains("chore(skip-changelog)"),
      "diagnostic must name the escape hatch: {stderr}"
    );
  }
}

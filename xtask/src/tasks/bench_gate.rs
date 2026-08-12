//! `bench-gate` — compare the latest Criterion run under
//! `target/criterion/` against a checked-in baseline JSON; exit
//! non-zero when any benchmark's median wall-clock is at least the
//! regression threshold above baseline. Backs the P7.2 perf gate.
//!
//! The gate reads two inputs:
//!
//!   - `benches/baselines/<host>.json` — the checked-in reference, captured
//!     on the host the gate runs on. Today only `apple-m2-max.json` ships;
//!     a per-runner baseline is captured when the CI runner is wired in.
//!   - `target/criterion/<group>/<bench>/new/estimates.json` — produced by
//!     the most recent `cargo bench` invocation.
//!
//! Output: a deterministic line per benchmark showing the baseline median,
//! current median, ratio, and verdict. Exit non-zero when any ratio crosses
//! the regression threshold.

use anyhow::{Context, Result, bail};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, serde::Deserialize)]
struct BaselineFile {
  benchmarks: std::collections::BTreeMap<String, std::collections::BTreeMap<String, BenchEntry>>,
}

#[derive(Debug, serde::Deserialize)]
struct BenchEntry {
  median_ns: f64,
}

#[derive(Debug, serde::Deserialize)]
struct CriterionEstimates {
  median: CriterionPoint,
}

#[derive(Debug, serde::Deserialize)]
struct CriterionPoint {
  point_estimate: f64,
}

/// Default threshold: a benchmark is flagged when its current median is at
/// least 1.25× the baseline median. Matches the P7.2 spec text.
const DEFAULT_REGRESSION_RATIO: f64 = 1.25;

/// Parse `bench-gate` args + dispatch into [`bench_gate_at`].
/// Supported args:
///   `--baseline <path>`   override the default baseline file
///   `--threshold <ratio>` override the regression ratio (default 1.25)
///   `--allow-missing`     don't fail when a baseline entry has no
///                         matching Criterion result (useful for CI runs
///                         that intentionally only ran a subset of
///                         benches). Also tolerates the baseline file
///                         itself being absent — emits a warning and
///                         returns success, so a runner whose
///                         `<host>.json` hasn't been captured yet
///                         doesn't block the workflow.
pub fn bench_gate_from_args(
  workspace_root: &Path,
  args: Vec<String>,
  out: &mut impl Write,
  err: &mut impl Write,
) -> Result<()> {
  let mut baseline_path: Option<PathBuf> = None;
  let mut threshold = DEFAULT_REGRESSION_RATIO;
  let mut allow_missing = false;
  let mut iter = args.into_iter();
  while let Some(arg) = iter.next() {
    match arg.as_str() {
      "--baseline" => {
        baseline_path =
          Some(PathBuf::from(iter.next().ok_or_else(|| {
            anyhow::anyhow!("--baseline requires a path argument")
          })?));
      }
      "--threshold" => {
        threshold = iter
          .next()
          .ok_or_else(|| anyhow::anyhow!("--threshold requires a numeric argument"))?
          .parse()
          .context("--threshold must be a positive float")?;
        if !threshold.is_finite() || threshold <= 1.0 {
          bail!("--threshold must be > 1.0 (got {threshold})");
        }
      }
      "--allow-missing" => allow_missing = true,
      other => bail!("unknown bench-gate flag '{other}'"),
    }
  }
  let baseline_path = baseline_path.unwrap_or_else(|| default_baseline_path(workspace_root));
  bench_gate_at(
    workspace_root,
    &baseline_path,
    threshold,
    allow_missing,
    out,
    err,
  )
}

fn default_baseline_path(workspace_root: &Path) -> PathBuf {
  // Pick a host-specific baseline by simple naming convention. The
  // workspace ships `apple-m2-max.json` today; the CI runner gets its
  // own file when it lands.
  let host = if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
    "apple-m2-max.json"
  } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
    "ci-ubuntu-latest.json"
  } else {
    "apple-m2-max.json" // fallback — better signal than no signal
  };
  workspace_root.join("benches").join("baselines").join(host)
}

/// Core comparator: read `baseline_path`, walk Criterion outputs, exit
/// non-zero when any ratio crosses `threshold`.
pub fn bench_gate_at(
  workspace_root: &Path,
  baseline_path: &Path,
  threshold: f64,
  allow_missing: bool,
  out: &mut impl Write,
  err: &mut impl Write,
) -> Result<()> {
  let criterion_root = pick_criterion_root(workspace_root);
  bench_gate_at_with_criterion_root(
    &criterion_root,
    baseline_path,
    threshold,
    allow_missing,
    out,
    err,
  )
}

/// Same as [`bench_gate_at`] but with the Criterion output directory
/// passed explicitly. Used by unit tests to bypass [`pick_criterion_root`],
/// which would otherwise consult `~/.cargo/config.toml` and find the
/// developer's real target-dir instead of the test's synthetic root.
pub fn bench_gate_at_with_criterion_root(
  criterion_root: &Path,
  baseline_path: &Path,
  threshold: f64,
  allow_missing: bool,
  out: &mut impl Write,
  err: &mut impl Write,
) -> Result<()> {
  let baseline_text = match std::fs::read_to_string(baseline_path) {
    Ok(text) => text,
    Err(e) if allow_missing && e.kind() == std::io::ErrorKind::NotFound => {
      // CI runners that haven't had a `<host>.json` captured yet need
      // the gate to no-op instead of hard-failing; the workflow opts
      // into this by passing `--allow-missing`. Once a baseline ships,
      // drop the flag and this branch is unreachable.
      let _ = writeln!(
        err,
        "bench-gate: baseline file '{}' not found — skipping gate (--allow-missing)",
        baseline_path.display()
      );
      return Ok(());
    }
    Err(e) => {
      return Err(anyhow::Error::from(e))
        .with_context(|| format!("failed to read baseline file '{}'", baseline_path.display()));
    }
  };
  let baseline: BaselineFile = serde_json::from_str(&baseline_text).with_context(|| {
    format!(
      "baseline file '{}' is not valid bench-gate JSON",
      baseline_path.display()
    )
  })?;

  let criterion_root = criterion_root.to_path_buf();
  let _ = writeln!(
    out,
    "bench-gate: baseline={} threshold={:.2}× criterion-root={}",
    baseline_path.display(),
    threshold,
    criterion_root.display()
  );

  let mut regressions: Vec<String> = Vec::new();
  let mut missing: Vec<String> = Vec::new();
  let mut compared: usize = 0;

  for (group_name, group) in &baseline.benchmarks {
    for (bench_name, entry) in group {
      let current = read_criterion_median(&criterion_root, bench_name);
      match current {
        Some(current_ns) => {
          compared += 1;
          let ratio = if entry.median_ns > 0.0 {
            current_ns / entry.median_ns
          } else {
            f64::INFINITY
          };
          let verdict = if ratio >= threshold {
            "REGRESSION"
          } else {
            "ok"
          };
          let _ = writeln!(
            out,
            "  {group_name}/{bench_name}: baseline={:.0} ns, current={:.0} ns, ratio={:.2}× [{verdict}]",
            entry.median_ns, current_ns, ratio
          );
          if ratio >= threshold {
            regressions.push(format!(
              "{group_name}/{bench_name}: {:.2}× ({} ns → {} ns)",
              ratio, entry.median_ns as u64, current_ns as u64
            ));
          }
        }
        None => {
          missing.push(format!("{group_name}/{bench_name}"));
        }
      }
    }
  }

  if !missing.is_empty() {
    let _ = writeln!(
      err,
      "  {} benchmark(s) had no matching Criterion output:",
      missing.len()
    );
    for line in &missing {
      let _ = writeln!(err, "    - {line}");
    }
    if !allow_missing {
      bail!(
        "{} benchmark(s) missing under {} — re-run `cargo bench` first or pass --allow-missing",
        missing.len(),
        criterion_root.display()
      );
    }
  }

  let _ = writeln!(
    out,
    "\nbench-gate: {} compared, {} regression(s), {} missing",
    compared,
    regressions.len(),
    missing.len()
  );
  if !regressions.is_empty() {
    for line in &regressions {
      let _ = writeln!(err, "  ✗ {line}");
    }
    bail!(
      "{} benchmark(s) regressed beyond threshold",
      regressions.len()
    );
  }
  Ok(())
}

/// Pick the Criterion output root, in priority order:
///   1. `CARGO_TARGET_DIR` env override / `target/criterion`
///   2. `~/.cargo/config.toml` `build.target-dir` (the canonical
///      workspace setting CI sometimes pins to a cache mount)
///   3. `<workspace_root>/target/criterion` fallback.
fn pick_criterion_root(workspace_root: &Path) -> PathBuf {
  if let Ok(custom) = std::env::var("CARGO_TARGET_DIR") {
    return PathBuf::from(custom).join("criterion");
  }
  if let Some(from_cargo_config) = read_cargo_target_dir(workspace_root) {
    return from_cargo_config.join("criterion");
  }
  workspace_root.join("target").join("criterion")
}

/// Walk the well-known `cargo config` lookup chain to find a
/// `build.target-dir` setting. Returns `None` when no config sets it.
fn read_cargo_target_dir(workspace_root: &Path) -> Option<PathBuf> {
  let mut candidates: Vec<PathBuf> = Vec::new();
  // Workspace-level overrides first, then the user-wide fallback.
  candidates.push(workspace_root.join(".cargo").join("config.toml"));
  candidates.push(workspace_root.join(".cargo").join("config"));
  if let Some(home) = std::env::var_os("HOME") {
    candidates.push(PathBuf::from(home.clone()).join(".cargo/config.toml"));
    candidates.push(PathBuf::from(home).join(".cargo/config"));
  }
  for path in candidates {
    let Ok(text) = std::fs::read_to_string(&path) else {
      continue;
    };
    let Ok(parsed) = toml::from_str::<toml::Value>(&text) else {
      continue;
    };
    if let Some(target_dir) = parsed
      .get("build")
      .and_then(|b| b.get("target-dir"))
      .and_then(|t| t.as_str())
    {
      return Some(PathBuf::from(target_dir));
    }
  }
  None
}

/// Walk `criterion_root` looking for a directory matching `bench_name`
/// (Criterion uses `/`-separated nested dirs for parameterized benches).
/// Returns the median point estimate from the most recent run.
fn read_criterion_median(criterion_root: &Path, bench_name: &str) -> Option<f64> {
  // Criterion paths use the same `/` separators that show up in the
  // BenchmarkId — translate into a relative path.
  let mut estimates = criterion_root.join(bench_name);
  estimates.push("new");
  estimates.push("estimates.json");
  if !estimates.is_file() {
    return None;
  }
  let text = std::fs::read_to_string(&estimates).ok()?;
  let parsed: CriterionEstimates = serde_json::from_str(&text).ok()?;
  Some(parsed.median.point_estimate)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir().expect("create tempdir")
  }

  fn write_baseline(dir: &Path, json: &str) -> PathBuf {
    let path = dir.join("baseline.json");
    std::fs::write(&path, json).unwrap();
    path
  }

  fn write_criterion_estimate(criterion_root: &Path, bench_name: &str, median_ns: f64) {
    let dir = criterion_root.join(bench_name).join("new");
    std::fs::create_dir_all(&dir).unwrap();
    let json =
      format!(r#"{{"median":{{"point_estimate":{median_ns},"confidence_interval":{{}}}}}}"#);
    std::fs::write(dir.join("estimates.json"), json).unwrap();
  }

  fn synth_workspace_for_gate(root: &Path) -> PathBuf {
    let target = root.join("target").join("criterion");
    std::fs::create_dir_all(&target).unwrap();
    target
  }

  #[test]
  fn bench_gate_passes_when_every_bench_is_within_threshold() {
    let root = tempdir();
    let criterion = synth_workspace_for_gate(root.path());
    write_criterion_estimate(&criterion, "scheduler/flow_linear/serial/10", 1_000_000.0);
    let baseline = write_baseline(
      root.path(),
      r#"{
        "benchmarks": {
          "agentflow-core/scheduler": {
            "scheduler/flow_linear/serial/10": { "median_ns": 1000000 }
          }
        }
      }"#,
    );
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let result =
      bench_gate_at_with_criterion_root(&criterion, &baseline, 1.25, false, &mut out, &mut err);
    assert!(result.is_ok(), "unexpected error: {:?}", result.err());
    let stdout = String::from_utf8(out).unwrap();
    assert!(stdout.contains("ratio=1.00×"));
    assert!(stdout.contains("1 compared, 0 regression(s)"));
  }

  #[test]
  fn bench_gate_fails_when_regression_exceeds_threshold() {
    let root = tempdir();
    let criterion = synth_workspace_for_gate(root.path());
    // Bench ran 2× slower than baseline → far over the 1.25 threshold.
    write_criterion_estimate(&criterion, "scheduler/flow_linear/serial/10", 2_000_000.0);
    let baseline = write_baseline(
      root.path(),
      r#"{
        "benchmarks": {
          "agentflow-core/scheduler": {
            "scheduler/flow_linear/serial/10": { "median_ns": 1000000 }
          }
        }
      }"#,
    );
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let result =
      bench_gate_at_with_criterion_root(&criterion, &baseline, 1.25, false, &mut out, &mut err);
    assert!(result.is_err(), "expected regression failure");
    let stderr = String::from_utf8(err).unwrap();
    assert!(
      stderr.contains("2.00×"),
      "stderr must surface the ratio: {stderr}"
    );
  }

  #[test]
  fn bench_gate_fails_when_baseline_bench_has_no_criterion_output() {
    let root = tempdir();
    let criterion = synth_workspace_for_gate(root.path());
    // No estimates.json written for this bench.
    let baseline = write_baseline(
      root.path(),
      r#"{
        "benchmarks": {
          "agentflow-core/scheduler": {
            "scheduler/flow_linear/serial/10": { "median_ns": 1000000 }
          }
        }
      }"#,
    );
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let result =
      bench_gate_at_with_criterion_root(&criterion, &baseline, 1.25, false, &mut out, &mut err);
    assert!(result.is_err(), "expected missing-bench failure");
    let stderr = String::from_utf8(err).unwrap();
    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(
      stderr.contains("no matching Criterion output") || err_msg.contains("missing"),
      "stderr={stderr} err={err_msg}"
    );
  }

  #[test]
  fn bench_gate_allow_missing_skips_missing_benches() {
    let root = tempdir();
    let criterion = synth_workspace_for_gate(root.path());
    let baseline = write_baseline(
      root.path(),
      r#"{
        "benchmarks": {
          "agentflow-core/scheduler": {
            "scheduler/flow_linear/serial/10": { "median_ns": 1000000 }
          }
        }
      }"#,
    );
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let result =
      bench_gate_at_with_criterion_root(&criterion, &baseline, 1.25, true, &mut out, &mut err);
    assert!(
      result.is_ok(),
      "--allow-missing should not fail: {:?}",
      result.err()
    );
  }

  #[test]
  fn bench_gate_allow_missing_skips_when_baseline_file_is_absent() {
    let root = tempdir();
    let criterion = synth_workspace_for_gate(root.path());
    let baseline = root.path().join("nonexistent-baseline.json");
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let result =
      bench_gate_at_with_criterion_root(&criterion, &baseline, 1.25, true, &mut out, &mut err);
    assert!(
      result.is_ok(),
      "--allow-missing should swallow NotFound on baseline file: {:?}",
      result.err()
    );
    let err_text = String::from_utf8(err).unwrap();
    assert!(
      err_text.contains("not found") && err_text.contains("--allow-missing"),
      "expected skip warning to mention the file and the flag, got: {err_text}"
    );
  }

  #[test]
  fn bench_gate_without_allow_missing_still_fails_on_absent_baseline_file() {
    let root = tempdir();
    let criterion = synth_workspace_for_gate(root.path());
    let baseline = root.path().join("nonexistent-baseline.json");
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let result =
      bench_gate_at_with_criterion_root(&criterion, &baseline, 1.25, false, &mut out, &mut err);
    let err_obj = result.expect_err("missing baseline must still fail without --allow-missing");
    let msg = format!("{err_obj:#}");
    assert!(
      msg.contains("failed to read baseline file"),
      "error should retain original baseline-read context, got: {msg}"
    );
  }

  #[test]
  fn bench_gate_rejects_invalid_threshold_argument() {
    let root = tempdir();
    let _criterion = synth_workspace_for_gate(root.path());
    let baseline = write_baseline(root.path(), r#"{"benchmarks":{}}"#);
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let result = bench_gate_from_args(
      root.path(),
      vec![
        "--baseline".into(),
        baseline.display().to_string(),
        "--threshold".into(),
        "0.5".into(),
      ],
      &mut out,
      &mut err,
    );
    assert!(result.is_err(), "threshold ≤ 1.0 must be rejected");
  }
}

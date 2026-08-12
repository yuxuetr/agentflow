//! `test-gate` (P10.19.2) — run `cargo test -p <crate>` per workspace member,
//! capture wall-clock per crate, compare against a checked-in
//! baseline JSON, and fail when any crate's ratio crosses the
//! regression threshold (default 1.5×). Pair to `bench-gate` for
//! test-suite-bloat detection.
//!
//! Run `cargo xtask test-gate [--baseline <path>] [--threshold <ratio>]
//! [--allow-missing] [--update] [--input <path>] [--include <crate>...]
//! [--exclude <crate>...]`.
//!
//! Default behaviour (compare mode):
//!
//!   1. Walk `workspace.members`, drop `xtask` itself + any `--exclude`
//!      entries + restrict to `--include` when set.
//!   2. For each remaining crate, run `cargo test -p <crate>
//!      --all-targets` and time it with `Instant::now()`. Compilation +
//!      execution wall-clock is captured together — the gate's job is
//!      noticing bloat, not isolating runtime from build time.
//!   3. Compare the per-crate ratio against the baseline file. Any
//!      `current / baseline >= threshold` is a regression and fails
//!      the gate. Default threshold is `1.5×` (looser than bench-gate's
//!      `1.25×` because test wall-clock is meaningfully noisier than
//!      criterion microbenches).
//!
//! Capture mode (`--update`):
//!
//!   1. Same per-crate `cargo test` sweep.
//!   2. Write the resulting `TestTimingBaseline` JSON to the baseline
//!      path, overwriting any existing file. Stamp `host.captured_at`
//!      with today's date in UTC and `host.rustc` with `rustc --version`
//!      so a future reader knows the provenance.
//!
//! `--input <path>`: skip the cargo invocations and read the "current"
//! timings from a pre-captured JSON of the same shape. Useful when CI
//! captures timings in one job and the gate runs in a downstream job.
//!
//! `--allow-missing`: don't error when the baseline has an entry for a
//! crate the current run didn't cover (mirrors bench-gate's flag).
//! Useful during the rollout phase.

use crate::read_workspace_members;
use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

const DEFAULT_TEST_GATE_RATIO: f64 = 1.5;

#[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
struct TestTimingBaseline {
  /// Capture metadata. Parallels the `host` block in the criterion
  /// baseline file so an operator can correlate timings to a specific
  /// machine + rustc revision.
  host: TestTimingHost,
  /// Free-text notes — never parsed; pinned in the file so future
  /// readers find the rationale next to the numbers.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  notes: Vec<String>,
  /// Per-crate timings. The key is the workspace member name
  /// (`agentflow-core`, etc.); the value is the captured wall-clock
  /// + best-effort test count parsed from `cargo test` stdout.
  timings: std::collections::BTreeMap<String, TestTimingEntry>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
struct TestTimingHost {
  /// Short identifier, e.g. `apple-m2-max`, `ci-ubuntu-latest`.
  id: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  machine: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  arch: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  os: Option<String>,
  /// `rustc -V` of the host that captured the baseline.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  rustc: Option<String>,
  /// ISO-8601 date, e.g. `2026-05-21`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  captured_at: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
struct TestTimingEntry {
  /// Wall-clock nanoseconds for the `cargo test -p <crate>
  /// --all-targets` invocation.
  wall_clock_ns: u128,
  /// Best-effort test count extracted from the `test result: ok. N
  /// passed; ...` summary line. `None` when no summary line was
  /// found (e.g. compile failure — the wall-clock is still useful
  /// telemetry, but the count isn't).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  test_count: Option<u64>,
}

#[derive(Debug, PartialEq)]
struct TestGateReport {
  compared: usize,
  regressions: Vec<TestGateRegression>,
  missing_in_current: Vec<String>,
  missing_in_baseline: Vec<String>,
}

#[derive(Debug, PartialEq)]
struct TestGateRegression {
  krate: String,
  baseline_ns: u128,
  current_ns: u128,
  ratio: f64,
}

/// Pure comparator: produce a verdict from two timing maps without
/// touching the filesystem. Side-effect-free so the unit tests pin
/// the regression boundary without spawning `cargo test`.
fn compare_test_timings(
  baseline: &TestTimingBaseline,
  current: &std::collections::BTreeMap<String, TestTimingEntry>,
  threshold: f64,
) -> TestGateReport {
  let mut compared = 0usize;
  let mut regressions: Vec<TestGateRegression> = Vec::new();
  let mut missing_in_current: Vec<String> = Vec::new();
  for (krate, baseline_entry) in &baseline.timings {
    match current.get(krate) {
      Some(current_entry) => {
        compared += 1;
        let ratio = if baseline_entry.wall_clock_ns > 0 {
          current_entry.wall_clock_ns as f64 / baseline_entry.wall_clock_ns as f64
        } else {
          f64::INFINITY
        };
        if ratio >= threshold {
          regressions.push(TestGateRegression {
            krate: krate.clone(),
            baseline_ns: baseline_entry.wall_clock_ns,
            current_ns: current_entry.wall_clock_ns,
            ratio,
          });
        }
      }
      None => missing_in_current.push(krate.clone()),
    }
  }
  let missing_in_baseline: Vec<String> = current
    .keys()
    .filter(|k| !baseline.timings.contains_key(*k))
    .cloned()
    .collect();
  TestGateReport {
    compared,
    regressions,
    missing_in_current,
    missing_in_baseline,
  }
}

/// Parse the per-crate test count from `cargo test` stdout. Returns
/// `None` when no `test result: ok. N passed;` line is present
/// (compile failure, harness reported nothing, etc.). Sums across
/// multiple suites (lib + integration tests each emit one summary
/// line per binary).
fn parse_test_count_from_output(stdout: &str) -> Option<u64> {
  let mut total: u64 = 0;
  let mut saw_summary = false;
  for line in stdout.lines() {
    // Lines look like: `test result: ok. 42 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.23s`
    // The interesting bit is `<N> passed`. Match the prefix loosely
    // so the rest of the format can drift without breaking us.
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix("test result:") {
      // Look for "ok." then the number; "FAILED." (capital) lines
      // are also valid — we still want to count what passed.
      if let Some(idx) = rest.find(" passed") {
        // Walk backward from `idx` to find the number.
        let prefix = &rest[..idx];
        if let Some(num_str) = prefix.split_whitespace().last()
          && let Ok(n) = num_str.parse::<u64>()
        {
          total += n;
          saw_summary = true;
        }
      }
    }
  }
  if saw_summary { Some(total) } else { None }
}

pub(crate) fn test_gate_from_args(
  workspace_root: &Path,
  args: Vec<String>,
  stdout: &mut impl Write,
  stderr: &mut impl Write,
) -> Result<()> {
  let mut baseline_path: Option<PathBuf> = None;
  let mut input_path: Option<PathBuf> = None;
  let mut threshold = DEFAULT_TEST_GATE_RATIO;
  let mut allow_missing = false;
  let mut update = false;
  let mut include: Vec<String> = Vec::new();
  let mut exclude: Vec<String> = Vec::new();
  let mut iter = args.into_iter();
  while let Some(arg) = iter.next() {
    match arg.as_str() {
      "--baseline" => {
        baseline_path =
          Some(PathBuf::from(iter.next().ok_or_else(|| {
            anyhow::anyhow!("--baseline requires a path argument")
          })?));
      }
      "--input" => {
        input_path =
          Some(PathBuf::from(iter.next().ok_or_else(|| {
            anyhow::anyhow!("--input requires a path argument")
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
      "--update" => update = true,
      "--include" => {
        let value = iter
          .next()
          .ok_or_else(|| anyhow::anyhow!("--include requires a crate name"))?;
        include.push(value);
      }
      "--exclude" => {
        let value = iter
          .next()
          .ok_or_else(|| anyhow::anyhow!("--exclude requires a crate name"))?;
        exclude.push(value);
      }
      other => bail!("unknown test-gate flag '{other}'"),
    }
  }
  if update && input_path.is_some() {
    bail!(
      "--update and --input are mutually exclusive (--update WRITES the baseline; --input only READS pre-captured timings)"
    );
  }
  let baseline_path =
    baseline_path.unwrap_or_else(|| default_test_timing_baseline_path(workspace_root));

  let crates = select_test_gate_crates(workspace_root, &include, &exclude)?;
  let _ = writeln!(
    stdout,
    "test-gate: baseline={} threshold={:.2}× crates={}",
    baseline_path.display(),
    threshold,
    crates.len()
  );

  // Collect the "current" timings. Three sources:
  //   1. `--input <path>`: read a pre-captured JSON. No cargo run.
  //   2. `--update`: invoke `cargo test` per crate (also writes the
  //      baseline as a side-effect at the end).
  //   3. Default: invoke `cargo test` per crate and compare.
  let current = if let Some(input) = &input_path {
    read_test_timing_file(input)?
      .timings
      .into_iter()
      .collect::<std::collections::BTreeMap<_, _>>()
  } else {
    capture_test_timings(workspace_root, &crates, stdout)?
  };

  if update {
    let baseline = TestTimingBaseline {
      host: capture_host_metadata(),
      notes: vec![
        "Captured by `cargo xtask test-gate --update`. Wall-clock includes incremental compile + test execution.".to_string(),
        "Per-crate variance is meaningful (1.2-1.5×) — gate threshold defaults to 1.5×.".to_string(),
      ],
      timings: current,
    };
    write_test_timing_file(&baseline_path, &baseline)?;
    let _ = writeln!(
      stdout,
      "test-gate: wrote {} crate timing(s) to {}",
      baseline.timings.len(),
      baseline_path.display()
    );
    return Ok(());
  }

  let baseline = read_test_timing_file(&baseline_path).with_context(|| {
    format!(
      "failed to read baseline '{}' — first-time capture needs `cargo xtask test-gate --update`",
      baseline_path.display()
    )
  })?;

  let report = compare_test_timings(&baseline, &current, threshold);
  for (krate, baseline_entry) in &baseline.timings {
    let current_entry = current.get(krate);
    let baseline_ms = (baseline_entry.wall_clock_ns / 1_000_000) as u64;
    match current_entry {
      Some(curr) => {
        let current_ms = (curr.wall_clock_ns / 1_000_000) as u64;
        let ratio = if baseline_entry.wall_clock_ns > 0 {
          curr.wall_clock_ns as f64 / baseline_entry.wall_clock_ns as f64
        } else {
          f64::INFINITY
        };
        let verdict = if ratio >= threshold {
          "REGRESSION"
        } else {
          "ok"
        };
        let _ = writeln!(
          stdout,
          "  {krate}: baseline={baseline_ms} ms, current={current_ms} ms, ratio={ratio:.2}× [{verdict}]"
        );
      }
      None => {
        let _ = writeln!(
          stdout,
          "  {krate}: baseline={baseline_ms} ms, current=<missing>"
        );
      }
    }
  }

  if !report.missing_in_current.is_empty() {
    let _ = writeln!(
      stderr,
      "  {} crate(s) in baseline had no current timing:",
      report.missing_in_current.len()
    );
    for krate in &report.missing_in_current {
      let _ = writeln!(stderr, "    - {krate}");
    }
    if !allow_missing {
      bail!(
        "{} crate(s) missing in current timings — re-run without --include/--exclude filters or pass --allow-missing",
        report.missing_in_current.len()
      );
    }
  }
  if !report.missing_in_baseline.is_empty() {
    let _ = writeln!(
      stderr,
      "  {} crate(s) in current timings have no baseline entry (new crate? rerun --update):",
      report.missing_in_baseline.len()
    );
    for krate in &report.missing_in_baseline {
      let _ = writeln!(stderr, "    - {krate}");
    }
  }

  let _ = writeln!(
    stdout,
    "\ntest-gate: {} compared, {} regression(s), {} missing (current), {} missing (baseline)",
    report.compared,
    report.regressions.len(),
    report.missing_in_current.len(),
    report.missing_in_baseline.len()
  );
  if !report.regressions.is_empty() {
    for r in &report.regressions {
      let _ = writeln!(
        stderr,
        "  ✗ {}: {:.2}× ({} ms → {} ms)",
        r.krate,
        r.ratio,
        (r.baseline_ns / 1_000_000) as u64,
        (r.current_ns / 1_000_000) as u64
      );
    }
    bail!(
      "{} crate(s) regressed beyond threshold",
      report.regressions.len()
    );
  }
  Ok(())
}

fn default_test_timing_baseline_path(workspace_root: &Path) -> PathBuf {
  let host = if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
    "apple-m2-max.json"
  } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
    "ci-ubuntu-latest.json"
  } else {
    "apple-m2-max.json"
  };
  workspace_root
    .join("benches")
    .join("baselines")
    .join("test-timings")
    .join(host)
}

/// Resolve the crate set to time: `workspace.members` minus `xtask`,
/// minus `--exclude`, intersected with `--include` when non-empty.
fn select_test_gate_crates(
  workspace_root: &Path,
  include: &[String],
  exclude: &[String],
) -> Result<Vec<String>> {
  let mut members = read_workspace_members(workspace_root)?;
  // Drop xtask: timing its own test suite from inside itself is a
  // reentrancy hazard and rarely interesting.
  members.retain(|m| m != "xtask");
  if !include.is_empty() {
    let include_set: BTreeSet<&str> = include.iter().map(String::as_str).collect();
    members.retain(|m| include_set.contains(m.as_str()));
  }
  if !exclude.is_empty() {
    let exclude_set: BTreeSet<&str> = exclude.iter().map(String::as_str).collect();
    members.retain(|m| !exclude_set.contains(m.as_str()));
  }
  Ok(members)
}

/// Run `cargo test -p <crate> --all-targets` per crate, time each,
/// parse the test summary from stdout, and return a timing map.
fn capture_test_timings(
  workspace_root: &Path,
  crates: &[String],
  stdout: &mut impl Write,
) -> Result<std::collections::BTreeMap<String, TestTimingEntry>> {
  let mut out = std::collections::BTreeMap::<String, TestTimingEntry>::new();
  for krate in crates {
    let _ = writeln!(stdout, "  measuring {krate} ...");
    let started = Instant::now();
    let output = Command::new("cargo")
      .args(["test", "-p", krate, "--all-targets", "--quiet"])
      .current_dir(workspace_root)
      .stderr(Stdio::piped())
      .stdout(Stdio::piped())
      .output()
      .with_context(|| format!("running `cargo test -p {krate}`"))?;
    let elapsed = started.elapsed();
    let stdout_text = String::from_utf8_lossy(&output.stdout);
    let test_count = parse_test_count_from_output(&stdout_text);
    out.insert(
      krate.clone(),
      TestTimingEntry {
        wall_clock_ns: elapsed.as_nanos(),
        test_count,
      },
    );
    let _ = writeln!(
      stdout,
      "    -> {} ms, tests={}{}",
      elapsed.as_millis(),
      test_count
        .map(|n| n.to_string())
        .unwrap_or_else(|| "?".to_string()),
      if !output.status.success() {
        " (test invocation reported non-zero — wall-clock still captured)"
      } else {
        ""
      }
    );
  }
  Ok(out)
}

fn read_test_timing_file(path: &Path) -> Result<TestTimingBaseline> {
  let text = std::fs::read_to_string(path)
    .with_context(|| format!("failed to read '{}'", path.display()))?;
  let parsed: TestTimingBaseline = serde_json::from_str(&text)
    .with_context(|| format!("'{}' is not valid test-gate JSON", path.display()))?;
  Ok(parsed)
}

fn write_test_timing_file(path: &Path, baseline: &TestTimingBaseline) -> Result<()> {
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent).with_context(|| {
      format!(
        "failed to create parent directory '{}' for baseline write",
        parent.display()
      )
    })?;
  }
  let mut text =
    serde_json::to_string_pretty(baseline).context("serializing TestTimingBaseline to JSON")?;
  text.push('\n');
  std::fs::write(path, text)
    .with_context(|| format!("failed to write baseline '{}'", path.display()))?;
  Ok(())
}

fn capture_host_metadata() -> TestTimingHost {
  let id = if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
    "apple-aarch64".to_string()
  } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
    "linux-x86_64".to_string()
  } else {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
  };
  let rustc = Command::new("rustc")
    .arg("--version")
    .output()
    .ok()
    .and_then(|o| {
      if o.status.success() {
        Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
      } else {
        None
      }
    });
  // Best-effort UTC date without pulling chrono into xtask: read
  // `date -u +%F` which is portable across macOS / Linux. Falls back
  // to None if the command isn't on PATH.
  let captured_at = Command::new("date")
    .args(["-u", "+%F"])
    .output()
    .ok()
    .and_then(|o| {
      if o.status.success() {
        Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
      } else {
        None
      }
    });
  TestTimingHost {
    id,
    machine: None,
    arch: Some(std::env::consts::ARCH.to_string()),
    os: Some(std::env::consts::OS.to_string()),
    rustc,
    captured_at,
  }
}

#[cfg(test)]
mod test_gate_tests {
  use super::*;
  use std::collections::BTreeMap;

  fn entry(ns: u128, count: Option<u64>) -> TestTimingEntry {
    TestTimingEntry {
      wall_clock_ns: ns,
      test_count: count,
    }
  }

  fn baseline(map: &[(&str, u128)]) -> TestTimingBaseline {
    let mut timings = BTreeMap::new();
    for (k, ns) in map {
      timings.insert(k.to_string(), entry(*ns, None));
    }
    TestTimingBaseline {
      host: TestTimingHost {
        id: "test-fixture".to_string(),
        machine: None,
        arch: None,
        os: None,
        rustc: None,
        captured_at: None,
      },
      notes: vec![],
      timings,
    }
  }

  fn current(map: &[(&str, u128)]) -> BTreeMap<String, TestTimingEntry> {
    map
      .iter()
      .map(|(k, ns)| (k.to_string(), entry(*ns, None)))
      .collect()
  }

  #[test]
  fn compare_flags_crates_at_or_above_threshold() {
    // Exactly at the threshold (1.5×) is a regression — the gate is
    // `>=`, not `>`. Just below threshold (1.49×) is `ok`.
    let base = baseline(&[("agentflow-core", 1_000_000_000)]);
    let curr = current(&[("agentflow-core", 1_500_000_000)]);
    let report = compare_test_timings(&base, &curr, 1.5);
    assert_eq!(report.regressions.len(), 1, "1.5× must be a regression");
    assert_eq!(report.regressions[0].krate, "agentflow-core");

    let curr_under = current(&[("agentflow-core", 1_490_000_000)]);
    let report_under = compare_test_timings(&base, &curr_under, 1.5);
    assert_eq!(
      report_under.regressions.len(),
      0,
      "1.49× must NOT be a regression at threshold 1.5"
    );
  }

  #[test]
  fn compare_reports_zero_regressions_when_current_is_faster() {
    // Faster current must never count as a regression, regardless of
    // ratio (i.e. 0.5× / 0.01× — these are wins, not problems).
    let base = baseline(&[("agentflow-core", 5_000_000_000)]);
    let curr = current(&[("agentflow-core", 1_000_000_000)]);
    let report = compare_test_timings(&base, &curr, 1.5);
    assert_eq!(report.regressions.len(), 0);
  }

  #[test]
  fn compare_handles_zero_baseline_with_infinity_ratio() {
    // Defensive: a baseline of 0 ns is malformed but we shouldn't
    // panic. The ratio degrades to +inf so any non-zero current is
    // a "regression". (In practice this only fires if someone
    // hand-edits the JSON to nonsense.)
    let base = baseline(&[("weird", 0)]);
    let curr = current(&[("weird", 100)]);
    let report = compare_test_timings(&base, &curr, 1.5);
    assert_eq!(report.regressions.len(), 1);
    assert!(report.regressions[0].ratio.is_infinite());
  }

  #[test]
  fn compare_separates_missing_in_current_and_baseline() {
    let base = baseline(&[
      ("agentflow-core", 1_000_000_000),
      ("agentflow-llm", 2_000_000_000),
    ]);
    let curr = current(&[
      ("agentflow-core", 1_100_000_000),
      ("agentflow-newcrate", 500_000_000),
    ]);
    let report = compare_test_timings(&base, &curr, 1.5);
    assert_eq!(report.compared, 1);
    assert_eq!(report.regressions.len(), 0);
    assert_eq!(report.missing_in_current, vec!["agentflow-llm".to_string()]);
    assert_eq!(
      report.missing_in_baseline,
      vec!["agentflow-newcrate".to_string()]
    );
  }

  #[test]
  fn parse_test_count_sums_multiple_summary_lines() {
    // libtest emits one summary line per binary (lib + each
    // integration test). We sum across all of them.
    let stdout = "\
running 5 tests
test foo ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

running 3 tests
test bar ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
";
    assert_eq!(parse_test_count_from_output(stdout), Some(8));
  }

  #[test]
  fn parse_test_count_returns_none_when_no_summary_line() {
    // Compile failure / empty crate / harness disabled — no test
    // result lines at all.
    let stdout = "Compiling agentflow-core\nerror: something blew up\n";
    assert_eq!(parse_test_count_from_output(stdout), None);
  }

  #[test]
  fn parse_test_count_tolerates_format_drift_in_suffix() {
    // We only anchor on `test result:` ... ` passed`. The "finished
    // in 1.23s" tail is allowed to drift in newer rustc versions
    // without breaking us.
    let stdout =
      "test result: ok. 42 passed; 0 failed; 0 ignored; 0 measured; (something new here)\n";
    assert_eq!(parse_test_count_from_output(stdout), Some(42));
  }

  #[test]
  fn baseline_file_roundtrips_through_serde() {
    let mut timings = BTreeMap::new();
    timings.insert(
      "agentflow-core".to_string(),
      TestTimingEntry {
        wall_clock_ns: 12_345_678_900,
        test_count: Some(139),
      },
    );
    timings.insert(
      "agentflow-tools".to_string(),
      TestTimingEntry {
        wall_clock_ns: 3_000_000_000,
        test_count: None,
      },
    );
    let baseline = TestTimingBaseline {
      host: TestTimingHost {
        id: "apple-aarch64".to_string(),
        machine: Some("MacBookPro".to_string()),
        arch: Some("aarch64".to_string()),
        os: Some("macos".to_string()),
        rustc: Some("rustc 1.85.0".to_string()),
        captured_at: Some("2026-05-21".to_string()),
      },
      notes: vec!["captured by xtask test-gate".to_string()],
      timings,
    };
    let json = serde_json::to_string_pretty(&baseline).unwrap();
    let parsed: TestTimingBaseline = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, baseline);
  }

  #[test]
  fn select_crates_drops_xtask_by_default() {
    let workspace_root = crate::workspace_root();
    let crates = select_test_gate_crates(&workspace_root, &[], &[]).unwrap();
    assert!(
      !crates.contains(&"xtask".to_string()),
      "xtask must be excluded: {crates:?}"
    );
    assert!(
      crates.contains(&"agentflow-core".to_string()),
      "agentflow-core must remain: {crates:?}"
    );
  }

  #[test]
  fn select_crates_respects_include_filter() {
    let workspace_root = crate::workspace_root();
    let crates = select_test_gate_crates(
      &workspace_root,
      &["agentflow-core".to_string(), "agentflow-tools".to_string()],
      &[],
    )
    .unwrap();
    assert_eq!(
      crates,
      vec!["agentflow-core".to_string(), "agentflow-tools".to_string()]
    );
  }

  #[test]
  fn select_crates_respects_exclude_filter() {
    let workspace_root = crate::workspace_root();
    let crates =
      select_test_gate_crates(&workspace_root, &[], &["agentflow-core".to_string()]).unwrap();
    assert!(!crates.contains(&"agentflow-core".to_string()));
    assert!(crates.contains(&"agentflow-tools".to_string()));
  }

  #[test]
  fn write_then_read_baseline_round_trips() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("nested").join("baseline.json");
    let baseline = TestTimingBaseline {
      host: TestTimingHost {
        id: "roundtrip".to_string(),
        machine: None,
        arch: None,
        os: None,
        rustc: None,
        captured_at: None,
      },
      notes: vec![],
      timings: BTreeMap::from([(
        "agentflow-core".to_string(),
        TestTimingEntry {
          wall_clock_ns: 1_000_000,
          test_count: Some(7),
        },
      )]),
    };
    write_test_timing_file(&path, &baseline).unwrap();
    let parsed = read_test_timing_file(&path).unwrap();
    assert_eq!(parsed, baseline);
  }

  #[test]
  fn test_gate_rejects_threshold_at_or_below_one() {
    let workspace_root = crate::workspace_root();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let res = test_gate_from_args(
      &workspace_root,
      vec!["--threshold".to_string(), "0.9".to_string()],
      &mut out,
      &mut err,
    );
    assert!(res.is_err(), "threshold ≤ 1.0 must fail fast");
  }

  #[test]
  fn test_gate_rejects_update_with_input() {
    let workspace_root = crate::workspace_root();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let res = test_gate_from_args(
      &workspace_root,
      vec![
        "--update".to_string(),
        "--input".to_string(),
        "/tmp/whatever.json".to_string(),
      ],
      &mut out,
      &mut err,
    );
    assert!(res.is_err(), "--update + --input must be rejected up front");
  }

  #[test]
  fn test_gate_compares_against_input_without_running_cargo() {
    // End-to-end pure-data path: hand the gate a baseline file + an
    // --input file (both as JSON), no `cargo test` invocations. The
    // baseline lists one crate at 1s; the input lists the same crate
    // at 800ms — under the 1.5× threshold, must PASS.
    let dir = tempfile::TempDir::new().unwrap();
    let baseline_path = dir.path().join("baseline.json");
    let input_path = dir.path().join("input.json");
    let baseline = TestTimingBaseline {
      host: TestTimingHost {
        id: "fix".into(),
        machine: None,
        arch: None,
        os: None,
        rustc: None,
        captured_at: None,
      },
      notes: vec![],
      timings: BTreeMap::from([(
        "agentflow-core".to_string(),
        TestTimingEntry {
          wall_clock_ns: 1_000_000_000,
          test_count: Some(100),
        },
      )]),
    };
    let input = TestTimingBaseline {
      host: baseline.host.clone(),
      notes: vec![],
      timings: BTreeMap::from([(
        "agentflow-core".to_string(),
        TestTimingEntry {
          wall_clock_ns: 800_000_000,
          test_count: Some(100),
        },
      )]),
    };
    write_test_timing_file(&baseline_path, &baseline).unwrap();
    write_test_timing_file(&input_path, &input).unwrap();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let res = test_gate_from_args(
      &crate::workspace_root(),
      vec![
        "--baseline".into(),
        baseline_path.to_string_lossy().into_owned(),
        "--input".into(),
        input_path.to_string_lossy().into_owned(),
        "--include".into(),
        "agentflow-core".into(),
      ],
      &mut out,
      &mut err,
    );
    assert!(
      res.is_ok(),
      "happy path must pass; stdout={} stderr={}",
      String::from_utf8_lossy(&out),
      String::from_utf8_lossy(&err)
    );
    let stdout = String::from_utf8_lossy(&out);
    assert!(stdout.contains("agentflow-core"));
    assert!(stdout.contains("ratio=0.80×"));
  }

  #[test]
  fn test_gate_fails_when_input_crosses_threshold() {
    let dir = tempfile::TempDir::new().unwrap();
    let baseline_path = dir.path().join("baseline.json");
    let input_path = dir.path().join("input.json");
    let baseline = TestTimingBaseline {
      host: TestTimingHost {
        id: "fix".into(),
        machine: None,
        arch: None,
        os: None,
        rustc: None,
        captured_at: None,
      },
      notes: vec![],
      timings: BTreeMap::from([(
        "agentflow-core".to_string(),
        TestTimingEntry {
          wall_clock_ns: 1_000_000_000,
          test_count: None,
        },
      )]),
    };
    // Current is 2× baseline — way over the 1.5× threshold.
    let input = TestTimingBaseline {
      host: baseline.host.clone(),
      notes: vec![],
      timings: BTreeMap::from([(
        "agentflow-core".to_string(),
        TestTimingEntry {
          wall_clock_ns: 2_000_000_000,
          test_count: None,
        },
      )]),
    };
    write_test_timing_file(&baseline_path, &baseline).unwrap();
    write_test_timing_file(&input_path, &input).unwrap();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let res = test_gate_from_args(
      &crate::workspace_root(),
      vec![
        "--baseline".into(),
        baseline_path.to_string_lossy().into_owned(),
        "--input".into(),
        input_path.to_string_lossy().into_owned(),
        "--include".into(),
        "agentflow-core".into(),
      ],
      &mut out,
      &mut err,
    );
    assert!(res.is_err(), "2.0× must fail at the 1.5× threshold");
    let stderr = String::from_utf8_lossy(&err);
    assert!(
      stderr.contains("agentflow-core"),
      "diagnostic must name the regressed crate: {stderr}"
    );
    assert!(
      stderr.contains("2.00×") || stderr.contains("2.00x"),
      "ratio must appear in the diagnostic: {stderr}"
    );
  }
}

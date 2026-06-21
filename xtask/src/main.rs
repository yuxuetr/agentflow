//! Workspace automation entry point.
//!
//! Run with `cargo xtask <subcommand>` (alias defined in `.cargo/config.toml`).
//! Subcommands available today:
//!
//! - `verify-edition` — assert every workspace member declares
//!   `edition = "2024"` so a freshly-added crate cannot silently drift to a
//!   different edition (`M.6`).
//! - `check-agent-sdk-doc` — scan `docs/AGENT_SDK.md` for backtick-quoted
//!   `CamelCase` identifiers and assert each one has a matching definition
//!   (`pub trait|struct|enum|type|fn`) somewhere in the workspace `src/`
//!   tree. Catches doc rot when traits / types referenced in the SDK guide
//!   are renamed or removed without updating the doc (`M.2`).
//! - `examples-smoke` — compile and run each SDK example from the
//!   canonical matrix (`examples/README.md`) with a per-example wall-
//!   clock cap; fail the workspace if any example panics or exceeds the
//!   cap. Backs the P3.2 / P3.10 / P7.3 CI gate.
//! - `bench-gate` — compare the latest Criterion run under
//!   `target/criterion/` against a checked-in baseline JSON; exit
//!   non-zero when any benchmark's median wall-clock is at least the
//!   regression threshold above baseline. Backs the P7.2 perf gate.
//! - `check-changelog` — fail when a non-trivial source change versus
//!   the base ref (default `origin/main`) didn't touch `CHANGELOG.md`
//!   AND no commit body in the branch range carries the
//!   `chore(skip-changelog)` opt-out marker (P10.18.2).
//! - `test-gate` — run `cargo test -p <crate>` per workspace member,
//!   capture wall-clock per crate, compare against a checked-in
//!   baseline JSON, and fail when any crate's ratio crosses the
//!   regression threshold (default 1.5×). Pair to `bench-gate` for
//!   test-suite-bloat detection (P10.19.2).
//! - `refresh-live-models` — for each provider wired into the
//!   `llm-live` nightly workflow, ping the provider's `/models`
//!   endpoint and verify the hard-coded text-model default still
//!   exists. Reports per-provider status + suggests replacements
//!   when the default 404s (P10.3.4).
//! - `redaction-lint` — grep every `agentflow-*/src/**/*.rs` for
//!   `(debug|info|warn|error)!(... danger = %text, ...)` patterns
//!   that interpolate raw user prompt / response / content / body /
//!   params into a log macro without going through
//!   `agentflow_tracing::redaction` or `prompt_fingerprint`. Backs
//!   the Q5.2 workspace redaction audit.
//! - `check-arch` — assert the subset of the eight crate-dependency laws
//!   (`docs/RFC_CRATE_ARCHITECTURE.md` §7) checkable today: runtime-isolation
//!   and surface-isolation. Known current violations live in `ARCH_ALLOWLIST`
//!   with a P-A burndown task; the gate fails on any NEW violation or any
//!   stale allowlist entry, so the list can only shrink (P-A0.2).

use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const EXPECTED_EDITION: &str = "2024";

const AGENT_SDK_DOC: &str = "docs/AGENT_SDK.md";

/// Backtick-quoted identifiers in `AGENT_SDK.md` that aren't workspace types:
/// stdlib / enum variants / pluralised names / one-shot example types. Adding
/// to this set is the escape hatch when the grep heuristic produces a false
/// positive — keep it small and document why each entry is here.
const AGENT_SDK_ALLOWLIST: &[&str] = &[
  // Standard library / language primitives.
  "Err",
  "None",
  // Enum variants of types that *do* exist in the codebase (the variant name
  // doesn't have its own `pub` declaration but the parent type is covered).
  "Step",
  "Plan",
  "Reflect",
  "Failure",
  "Critique",
  "Final",
  "FailureReason",
  // FlowValue variants: `FlowValue::File` and `FlowValue::Url` show up in the
  // typed-value section. The parent `FlowValue` enum is declared in
  // agentflow-core/src/value.rs.
  "File",
  "Url",
  // Example types defined inline in the doc (no real impl file).
  "EchoTool",
];

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
    "check-agent-sdk-doc" => {
      let workspace_root = workspace_root();
      check_agent_sdk_doc_at(
        &workspace_root,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
      )
    }
    "examples-smoke" => {
      let workspace_root = workspace_root();
      examples_smoke_at(
        &workspace_root,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
      )
    }
    "bench-gate" => {
      let workspace_root = workspace_root();
      bench_gate_from_args(
        &workspace_root,
        args.collect::<Vec<_>>(),
        &mut std::io::stdout(),
        &mut std::io::stderr(),
      )
    }
    "check-changelog" => {
      let workspace_root = workspace_root();
      check_changelog_from_args(
        &workspace_root,
        args.collect::<Vec<_>>(),
        &mut std::io::stdout(),
        &mut std::io::stderr(),
      )
    }
    "test-gate" => {
      let workspace_root = workspace_root();
      test_gate_from_args(
        &workspace_root,
        args.collect::<Vec<_>>(),
        &mut std::io::stdout(),
        &mut std::io::stderr(),
      )
    }
    "refresh-live-models" => {
      let workspace_root = workspace_root();
      refresh_live_models_from_args(
        &workspace_root,
        args.collect::<Vec<_>>(),
        &mut std::io::stdout(),
        &mut std::io::stderr(),
      )
    }
    "redaction-lint" => {
      let workspace_root = workspace_root();
      redaction_lint_at(
        &workspace_root,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
      )
    }
    "check-arch" => {
      let workspace_root = workspace_root();
      check_arch_at(
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
    "  verify-edition       fail if any workspace member declares an edition other than \"{EXPECTED_EDITION}\""
  );
  let _ = writeln!(
    sink,
    "  check-agent-sdk-doc  fail if {AGENT_SDK_DOC} references a CamelCase type that does not exist under any agentflow-*/src/**/*.rs"
  );
  let _ = writeln!(
    sink,
    "  examples-smoke       compile + run each SDK example from examples/README.md with a per-example wall-clock cap; fail on panic or timeout"
  );
  let _ = writeln!(
    sink,
    "  bench-gate           compare target/criterion/* against benches/baselines/<host>.json; fail when median ≥ 1.25× baseline"
  );
  let _ = writeln!(
    sink,
    "  check-changelog [BASE]  fail if a non-trivial source change vs BASE (default origin/main) didn't touch CHANGELOG.md AND no commit body carries `chore(skip-changelog)`"
  );
  let _ = writeln!(
    sink,
    "  test-gate            run `cargo test -p <crate>` per workspace member, compare wall-clock against benches/baselines/test-timings/<host>.json; fail when ratio ≥ 1.5×"
  );
  let _ = writeln!(
    sink,
    "  refresh-live-models  ping each provider's /models endpoint with the key from ~/.agentflow/.env (or env), report whether the live-test default still exists, suggest replacements on 404 (P10.3.4)"
  );
  let _ = writeln!(
    sink,
    "  redaction-lint       grep agentflow-*/src/**/*.rs for `(debug|info|warn|error)!(... <danger> = %...)` patterns that interpolate raw user prompt / response / content / body into a log macro without redaction (Q5.2)"
  );
  let _ = writeln!(
    sink,
    "  check-arch           assert the runtime-isolation + surface-isolation dependency laws (docs/RFC_CRATE_ARCHITECTURE.md §7); fail on any new cross-edge or stale allowlist entry (P-A0.2)"
  );
}

// ── bench-gate (P7.2) ──────────────────────────────────────────────────────
//
// The gate reads two inputs:
//
//   - `benches/baselines/<host>.json` — the checked-in reference, captured
//     on the host the gate runs on. Today only `apple-m2-max.json` ships;
//     a per-runner baseline is captured when the CI runner is wired in.
//   - `target/criterion/<group>/<bench>/new/estimates.json` — produced by
//     the most recent `cargo bench` invocation.
//
// Output: a deterministic line per benchmark showing the baseline median,
// current median, ratio, and verdict. Exit non-zero when any ratio crosses
// the regression threshold.

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

// ── examples-smoke ─────────────────────────────────────────────────────────
//
// The smoke list is intentionally explicit (not a filesystem walk) so a new
// example doesn't silently enter the CI gate. Adding a row here is a
// deliberate one-line PR; removing one likewise. Each row carries:
//   - package: the workspace member that owns the example
//   - example: the `<name>` to pass to `cargo run --example`
//   - features: extra `--features` flag (empty when the default set
//     covers it)
//   - timeout: per-example wall-clock cap. Most demos finish well under
//     5 s, but mock-LLM ReAct loops can spend 1-2 s per turn so a
//     generous 30 s default is the floor.
struct SmokeExample {
  package: &'static str,
  example: &'static str,
  features: &'static [&'static str],
  timeout: Duration,
}

const SMOKE_EXAMPLES: &[SmokeExample] = &[
  // Tool policy + sandbox demo (P3.1 row #12). Pure offline; no LLM.
  SmokeExample {
    package: "agentflow-tools",
    example: "tool_policy_sandbox_demo",
    features: &[],
    timeout: Duration::from_secs(20),
  },
  // Simple tracing demo (P3.1 row #11). JSONL writer, no LLM.
  SmokeExample {
    package: "agentflow-tracing",
    example: "simple_tracing",
    features: &[],
    timeout: Duration::from_secs(20),
  },
  // Core DAG fixed-shape walkthrough. No LLM.
  SmokeExample {
    package: "agentflow-core",
    example: "fixed_dag_workflow",
    features: &[],
    timeout: Duration::from_secs(20),
  },
  // ReAct agent (P3.1 row #3). Mock LLM, ~5s.
  SmokeExample {
    package: "agentflow-agents",
    example: "agent_native_react",
    features: &[],
    timeout: Duration::from_secs(45),
  },
  // Plan-execute agent (P3.1 row #4). Mock LLM, ~5s.
  SmokeExample {
    package: "agentflow-agents",
    example: "plan_execute_agent",
    features: &[],
    timeout: Duration::from_secs(45),
  },
  // Hybrid workflow embedding an AgentNode (P3.1 row #2). Mock LLM.
  SmokeExample {
    package: "agentflow-agents",
    example: "hybrid_workflow_agent",
    features: &[],
    timeout: Duration::from_secs(60),
  },
  // Dynamic-workflow vertical-slice spike (P-A1.6): an agent generates a Flow
  // at runtime and core executes it. Pure offline, no LLM.
  SmokeExample {
    package: "agentflow-agents",
    example: "dynamic_workflow_spike",
    features: &[],
    timeout: Duration::from_secs(10),
  },
  // Dynamic workflow from a declarative JSON plan (P-A4.4): plan -> Flow of real
  // tool calls, executed in parallel. Pure offline, no LLM.
  SmokeExample {
    package: "agentflow-agents",
    example: "dynamic_workflow_plan",
    features: &[],
    timeout: Duration::from_secs(10),
  },
  // SkillBuilder direct API (P3.1 row #8). Spawns a real MCP demo
  // subprocess so it's a touch slower than the mock-only examples.
  SmokeExample {
    package: "agentflow-skills",
    example: "skill_calls_mcp_tool",
    features: &[],
    timeout: Duration::from_secs(60),
  },
];

/// Total wall-clock budget for the whole smoke run. The P3.10 spec caps
/// it at 5 minutes; we keep it explicit so a regression in any one
/// example doesn't drag CI past the budget.
const SMOKE_TOTAL_BUDGET: Duration = Duration::from_secs(5 * 60);

/// Run every example in [`SMOKE_EXAMPLES`] under `workspace_root` and
/// report results through the caller-supplied sinks. Returns `Ok(())`
/// when every example exited zero within its per-example cap and the
/// total budget; returns a context-rich error otherwise.
pub fn examples_smoke_at(
  workspace_root: &Path,
  out: &mut impl Write,
  err: &mut impl Write,
) -> Result<()> {
  let total_start = Instant::now();
  let mut failed: Vec<String> = Vec::new();
  let mut passed: Vec<String> = Vec::new();

  for example in SMOKE_EXAMPLES {
    let elapsed = total_start.elapsed();
    if elapsed >= SMOKE_TOTAL_BUDGET {
      let _ = writeln!(
        err,
        "  ! skipping {pkg}::{ex} — total budget {:?} already exceeded ({elapsed:?})",
        SMOKE_TOTAL_BUDGET,
        pkg = example.package,
        ex = example.example,
      );
      failed.push(format!(
        "{pkg}::{ex} (skipped — over total budget)",
        pkg = example.package,
        ex = example.example
      ));
      continue;
    }

    let _ = writeln!(
      out,
      "  → {pkg}::{ex} (cap {cap:?})",
      pkg = example.package,
      ex = example.example,
      cap = example.timeout
    );
    let start = Instant::now();
    let run_result = run_one_example(workspace_root, example);
    let duration = start.elapsed();

    match run_result {
      Ok(()) => {
        let _ = writeln!(out, "    ✓ ok in {duration:?}");
        passed.push(format!("{}::{}", example.package, example.example));
      }
      Err(reason) => {
        let _ = writeln!(err, "    ✗ failed in {duration:?}: {reason}");
        failed.push(format!(
          "{}::{} ({reason})",
          example.package, example.example
        ));
      }
    }
  }

  let total = total_start.elapsed();
  let _ = writeln!(
    out,
    "\nexamples-smoke: {} passed, {} failed in {:?} (budget {:?})",
    passed.len(),
    failed.len(),
    total,
    SMOKE_TOTAL_BUDGET,
  );
  if !failed.is_empty() {
    for line in &failed {
      let _ = writeln!(err, "  failed: {line}");
    }
    bail!("{} example(s) failed", failed.len());
  }
  Ok(())
}

fn run_one_example(workspace_root: &Path, example: &SmokeExample) -> Result<(), String> {
  let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
  let mut cmd = Command::new(&cargo);
  cmd
    .current_dir(workspace_root)
    .arg("run")
    .arg("--quiet")
    .arg("-p")
    .arg(example.package)
    .arg("--example")
    .arg(example.example);
  if !example.features.is_empty() {
    cmd.arg("--features").arg(example.features.join(","));
  }
  cmd.stdout(Stdio::null()).stderr(Stdio::piped());

  let mut child = cmd.spawn().map_err(|e| format!("spawn failed: {e}"))?;

  // Poll for completion or timeout. `Child::wait_timeout` is third-party;
  // we busy-wait in 50 ms slices to avoid the extra dep.
  let deadline = Instant::now() + example.timeout;
  loop {
    match child.try_wait() {
      Ok(Some(status)) => {
        if status.success() {
          return Ok(());
        }
        return Err(format!("non-zero exit ({status})"));
      }
      Ok(None) => {
        if Instant::now() >= deadline {
          let _ = child.kill();
          let _ = child.wait();
          return Err(format!("timed out after {:?}", example.timeout));
        }
        std::thread::sleep(Duration::from_millis(50));
      }
      Err(e) => {
        let _ = child.kill();
        return Err(format!("try_wait failed: {e}"));
      }
    }
  }
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
  let workspace_edition = read_workspace_package_edition(workspace_root)?;
  let mut failures: Vec<String> = Vec::new();
  let mut checked: Vec<String> = Vec::new();
  for member in &members {
    let manifest = workspace_root.join(member).join("Cargo.toml");
    let edition = read_edition(&manifest, workspace_edition.as_deref())
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

/// Run the agent-SDK doc drift check against `workspace_root`. Returns
/// `Ok(())` when every CamelCase identifier the doc cites has either a real
/// `pub` definition under any `agentflow-*/src/**/*.rs` or is on the
/// allowlist.
fn check_agent_sdk_doc_at(
  workspace_root: &Path,
  stdout: &mut impl Write,
  stderr: &mut impl Write,
) -> Result<()> {
  let doc_path = workspace_root.join(AGENT_SDK_DOC);
  let doc = std::fs::read_to_string(&doc_path)
    .with_context(|| format!("Failed to read {}", doc_path.display()))?;
  let mentions = extract_camelcase_mentions(&doc);
  let known_definitions = collect_workspace_pub_definitions(workspace_root)?;
  let allowlist: BTreeSet<&str> = AGENT_SDK_ALLOWLIST.iter().copied().collect();
  let mut missing: Vec<String> = Vec::new();
  let mut checked: usize = 0;
  for name in &mentions {
    if allowlist.contains(name.as_str()) {
      continue;
    }
    checked += 1;
    if !known_definitions.contains(name.as_str()) {
      missing.push(name.clone());
    }
  }
  writeln!(
    stdout,
    "check-agent-sdk-doc: cross-referenced {} CamelCase mention(s) in {} ({} ignored via allowlist)",
    checked,
    AGENT_SDK_DOC,
    mentions.len() - checked
  )?;
  if missing.is_empty() {
    writeln!(stdout, "check-agent-sdk-doc: OK")?;
    return Ok(());
  }
  writeln!(stderr, "check-agent-sdk-doc: FAIL")?;
  for name in &missing {
    writeln!(
      stderr,
      "  - `{name}`: referenced in {AGENT_SDK_DOC} but no `pub (trait|struct|enum|type|fn) {name}` declaration found in any workspace src/ tree"
    )?;
  }
  bail!(
    "{} identifier(s) in {} have no matching workspace declaration",
    missing.len(),
    AGENT_SDK_DOC
  );
}

/// Pull every `` `CamelCaseIdent` `` (non-empty, starts with uppercase letter,
/// only alphanumerics after) out of the doc. Returns a sorted dedup list so
/// CI output diff is stable.
fn extract_camelcase_mentions(doc: &str) -> Vec<String> {
  let mut hits: BTreeSet<String> = BTreeSet::new();
  let bytes = doc.as_bytes();
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i] != b'`' {
      i += 1;
      continue;
    }
    let start = i + 1;
    let mut end = start;
    while end < bytes.len() && bytes[end] != b'`' && bytes[end] != b'\n' {
      end += 1;
    }
    if end < bytes.len() && bytes[end] == b'`' {
      let token = &doc[start..end];
      if is_camelcase_ident(token) {
        hits.insert(token.to_string());
      }
      i = end + 1;
    } else {
      i = end + 1;
    }
  }
  hits.into_iter().collect()
}

fn is_camelcase_ident(s: &str) -> bool {
  let mut chars = s.chars();
  let first = match chars.next() {
    Some(c) => c,
    None => return false,
  };
  if !first.is_ascii_uppercase() {
    return false;
  }
  for c in chars {
    if !c.is_ascii_alphanumeric() {
      return false;
    }
  }
  // Require either a lowercase letter or another uppercase letter after the
  // first character — pure-uppercase tokens like `JSON` or `URL` are usually
  // acronyms in prose, not workspace types.
  s.chars().skip(1).any(|c| c.is_ascii_lowercase())
}

/// Collect every `pub (trait|struct|enum|type|fn) Ident` name declared
/// anywhere under `<workspace_root>/agentflow-*/src/**/*.rs`. Matches both
/// bare `pub` and visibility-restricted (`pub(crate)`, `pub(super)`, …)
/// forms so internal-but-discoverable types still count toward the doc
/// cross-reference.
fn collect_workspace_pub_definitions(workspace_root: &Path) -> Result<BTreeSet<String>> {
  let mut out: BTreeSet<String> = BTreeSet::new();
  for member in read_workspace_members(workspace_root)? {
    let src = workspace_root.join(&member).join("src");
    if !src.exists() {
      continue;
    }
    walk_rs(&src, &mut |path| {
      let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
      for ident in scan_pub_idents(&content) {
        out.insert(ident);
      }
      Ok(())
    })?;
  }
  Ok(out)
}

fn walk_rs(root: &Path, visit: &mut impl FnMut(&Path) -> Result<()>) -> Result<()> {
  for entry in std::fs::read_dir(root)
    .with_context(|| format!("Failed to read directory {}", root.display()))?
  {
    let entry = entry?;
    let path = entry.path();
    let ty = entry.file_type()?;
    if ty.is_dir() {
      walk_rs(&path, visit)?;
    } else if ty.is_file()
      && path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("rs"))
    {
      visit(&path)?;
    }
  }
  Ok(())
}

/// Scan a single `.rs` file for `pub …` declarations and yield the declared
/// identifier names. Handles both naked `pub` and visibility-restricted
/// (`pub(crate)`, `pub(super)`, `pub(in path)`) forms. The matcher is
/// intentionally simple — false negatives are tolerated; false positives are
/// not (they would mask real drift), so the keyword set is short and exact.
fn scan_pub_idents(content: &str) -> Vec<String> {
  let mut out: Vec<String> = Vec::new();
  for line in content.lines() {
    let trimmed = line.trim_start();
    // Skip the "pub(...)" parenthesis prefix if present so the kind keyword
    // comparison below is the same for `pub fn` and `pub(crate) fn`.
    let after_pub = if let Some(rest) = trimmed.strip_prefix("pub") {
      let rest = rest.trim_start();
      if rest.starts_with('(') {
        match rest.find(')') {
          Some(end) => rest[end + 1..].trim_start(),
          None => continue,
        }
      } else {
        rest
      }
    } else {
      continue;
    };
    let (kind, body) = match after_pub.split_once(char::is_whitespace) {
      Some(pair) => pair,
      None => continue,
    };
    let kind = kind.trim();
    if !matches!(kind, "trait" | "struct" | "enum" | "type" | "fn") {
      continue;
    }
    // Strip optional `unsafe`, `async`, `default` modifiers and grab the
    // identifier as the leading [A-Za-z0-9_]+ token.
    let body = body.trim_start();
    let ident: String = body
      .chars()
      .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
      .collect();
    if !ident.is_empty() {
      out.push(ident);
    }
  }
  out
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

fn read_edition(manifest: &Path, workspace_edition: Option<&str>) -> Result<String> {
  let content = std::fs::read_to_string(manifest)
    .with_context(|| format!("Failed to read {}", manifest.display()))?;
  let parsed: toml::Value =
    toml::from_str(&content).with_context(|| format!("Failed to parse {}", manifest.display()))?;
  let edition_value = parsed
    .get("package")
    .and_then(|p| p.get("edition"))
    .ok_or_else(|| {
      anyhow::anyhow!(
        "package.edition missing from {} — every workspace member must declare an edition",
        manifest.display()
      )
    })?;
  if let Some(s) = edition_value.as_str() {
    return Ok(s.to_string());
  }
  // `edition.workspace = true` form: defer to `[workspace.package].edition`.
  let inherits = edition_value
    .as_table()
    .and_then(|t| t.get("workspace"))
    .and_then(|w| w.as_bool())
    .unwrap_or(false);
  if inherits {
    return workspace_edition.map(str::to_string).ok_or_else(|| {
      anyhow::anyhow!(
        "{} inherits edition from workspace but [workspace.package].edition is not set",
        manifest.display()
      )
    });
  }
  Err(anyhow::anyhow!(
    "package.edition in {} must be either a string or `{{ workspace = true }}`",
    manifest.display()
  ))
}

/// Returns the workspace-level edition declared under `[workspace.package]` in
/// the root manifest, if present. Members that opt into `edition.workspace =
/// true` resolve to this value.
fn read_workspace_package_edition(workspace_root: &Path) -> Result<Option<String>> {
  let manifest_path = workspace_root.join("Cargo.toml");
  let content = std::fs::read_to_string(&manifest_path)
    .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
  let parsed: toml::Value = toml::from_str(&content)
    .with_context(|| format!("Failed to parse {}", manifest_path.display()))?;
  Ok(
    parsed
      .get("workspace")
      .and_then(|w| w.get("package"))
      .and_then(|p| p.get("edition"))
      .and_then(|e| e.as_str())
      .map(str::to_string),
  )
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
  fn resolves_workspace_inherited_edition() {
    // Members that declare `edition.workspace = true` should pick up the
    // edition from `[workspace.package].edition` rather than being treated as
    // missing. This is the form every agentflow-* crate uses in production.
    let root = tempdir();
    let root_manifest = "[workspace]\nmembers = [\"inheritor\"]\nresolver = \"2\"\n\n\
       [workspace.package]\nedition = \"2024\"\n";
    std::fs::write(root.path().join("Cargo.toml"), root_manifest).unwrap();
    std::fs::create_dir_all(root.path().join("inheritor")).unwrap();
    std::fs::write(
      root.path().join("inheritor/Cargo.toml"),
      "[package]\nname = \"inheritor\"\nversion = \"0.0.0\"\nedition.workspace = true\n",
    )
    .unwrap();
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let result = verify_edition_at(root.path(), &mut stdout, &mut stderr);
    assert!(result.is_ok(), "{}", String::from_utf8_lossy(&stderr));
    assert!(
      String::from_utf8(stdout)
        .unwrap()
        .contains("verify-edition: OK")
    );
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

  // ── check-agent-sdk-doc ──────────────────────────────────────────────

  /// Build a synthetic workspace under `root` with a single member crate
  /// `mock-crate` containing a `src/lib.rs` with the supplied declarations,
  /// plus a `docs/AGENT_SDK.md` file with the supplied body.
  fn write_synthetic_doc_workspace(root: &Path, src_lib: &str, agent_sdk_doc: &str) {
    let root_manifest = "[workspace]\nmembers = [\"mock-crate\"]\nresolver = \"2\"\n";
    std::fs::write(root.join("Cargo.toml"), root_manifest).unwrap();
    let crate_dir = root.join("mock-crate");
    let src_dir = crate_dir.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
      crate_dir.join("Cargo.toml"),
      "[package]\nname = \"mock-crate\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    std::fs::write(src_dir.join("lib.rs"), src_lib).unwrap();
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::write(root.join(AGENT_SDK_DOC), agent_sdk_doc).unwrap();
  }

  #[test]
  fn agent_sdk_doc_check_passes_when_every_mention_has_a_pub_declaration() {
    let root = tempdir();
    write_synthetic_doc_workspace(
      root.path(),
      "pub trait MyRuntime {}\npub struct MyHandle;\npub enum MyKind { A, B }\n",
      "# SDK\nThe `MyRuntime` trait wraps a `MyHandle` and emits `MyKind` events.\n",
    );
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let result = check_agent_sdk_doc_at(root.path(), &mut stdout, &mut stderr);
    assert!(result.is_ok(), "{}", String::from_utf8_lossy(&stderr));
    let stdout_s = String::from_utf8(stdout).unwrap();
    assert!(stdout_s.contains("check-agent-sdk-doc: OK"));
    assert!(stdout_s.contains("3 CamelCase mention"));
  }

  #[test]
  fn agent_sdk_doc_check_fails_when_doc_references_renamed_type() {
    let root = tempdir();
    // Doc references both `RenamedRuntime` (gone) and `MyRuntime` (still
    // there). Expect the failure to name only the missing one.
    write_synthetic_doc_workspace(
      root.path(),
      "pub trait MyRuntime {}\n",
      "# SDK\nDescribed by `MyRuntime` and the older `RenamedRuntime`.\n",
    );
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let result = check_agent_sdk_doc_at(root.path(), &mut stdout, &mut stderr);
    let err = result.expect_err("expected missing-type failure");
    let err_msg = format!("{err:#}");
    assert!(err_msg.contains("1 identifier"), "error: {err_msg}");
    let stderr_s = String::from_utf8(stderr).unwrap();
    assert!(
      stderr_s.contains("`RenamedRuntime`"),
      "stderr should name the missing type; got:\n{stderr_s}"
    );
    assert!(
      !stderr_s.contains("`MyRuntime`"),
      "stderr should NOT name the still-present type; got:\n{stderr_s}"
    );
  }

  #[test]
  fn agent_sdk_doc_check_honors_allowlist() {
    // `None` and `Err` are on the allowlist; they should never trigger a
    // failure even when they appear in the doc with no matching pub decl.
    let root = tempdir();
    write_synthetic_doc_workspace(
      root.path(),
      "pub trait MyRuntime {}\n",
      "# SDK\nThe `MyRuntime` returns `None` on miss or `Err` on failure.\n",
    );
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let result = check_agent_sdk_doc_at(root.path(), &mut stdout, &mut stderr);
    assert!(result.is_ok(), "{}", String::from_utf8_lossy(&stderr));
    let stdout_s = String::from_utf8(stdout).unwrap();
    assert!(stdout_s.contains("ignored via allowlist"));
  }

  #[test]
  fn camelcase_extractor_skips_lowercase_inline_code_and_acronyms() {
    let doc = "Mix of `myFunc`, `JSON`, `MyType`, `URL`, and `AnotherType`.\n";
    let mentions = extract_camelcase_mentions(doc);
    assert_eq!(
      mentions,
      vec!["AnotherType".to_string(), "MyType".to_string()]
    );
  }

  #[test]
  fn pub_ident_scanner_handles_visibility_restricted_forms() {
    let src = "pub trait A {}\npub(crate) struct B;\npub(super) enum C { X }\nfn private() {}\n";
    let idents = scan_pub_idents(src);
    assert!(idents.contains(&"A".to_string()));
    assert!(idents.contains(&"B".to_string()));
    assert!(idents.contains(&"C".to_string()));
    assert!(!idents.contains(&"private".to_string()));
  }

  // ── examples-smoke ──────────────────────────────────────────────────

  #[test]
  fn smoke_examples_list_is_non_empty_and_unique() {
    // Guard against accidental removal of every entry (which would
    // silently turn the CI gate into a no-op) and against duplicate
    // rows (which would inflate the wall-clock budget without adding
    // coverage).
    assert!(
      !SMOKE_EXAMPLES.is_empty(),
      "the smoke list must always have at least one entry"
    );
    let mut seen: BTreeSet<(&str, &str)> = BTreeSet::new();
    for example in SMOKE_EXAMPLES {
      assert!(
        seen.insert((example.package, example.example)),
        "duplicate smoke entry: {}::{}",
        example.package,
        example.example
      );
      assert!(
        example.timeout >= Duration::from_secs(5),
        "{}::{} per-example timeout below the 5 s floor — set a realistic cap",
        example.package,
        example.example,
      );
    }
  }

  #[test]
  fn smoke_total_budget_caps_at_five_minutes_per_spec() {
    // P3.10 spec target is 5 min total wall-clock. Bumping this is a
    // deliberate change; tests catch silent drift.
    assert_eq!(SMOKE_TOTAL_BUDGET, Duration::from_secs(300));
  }

  #[test]
  fn smoke_per_example_caps_fit_inside_total_budget() {
    // The list mustn't sum to more than the total budget when every
    // example takes its full per-example cap. If it does, we're
    // already over the cap on a worst-case run.
    let sum: Duration = SMOKE_EXAMPLES.iter().map(|e| e.timeout).sum();
    assert!(
      sum <= SMOKE_TOTAL_BUDGET,
      "per-example caps sum to {sum:?}, exceeding the total budget {SMOKE_TOTAL_BUDGET:?}",
    );
  }

  // ── bench-gate ────────────────────────────────────────────────────

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

// ── check-changelog (P10.18.2) ─────────────────────────────────────────────
//
// Run `cargo xtask check-changelog [BASE_REF]` (default `origin/main`).
// Behaviour:
//
//   1. Compute the set of files changed in BASE_REF...HEAD via
//      `git diff --name-only`.
//   2. Classify: any file outside the "trivial" allowlist (docs/, tests/,
//      lockfiles, the CHANGELOG itself, gitignore housekeeping) counts as
//      "source" — i.e. should normally land with a CHANGELOG entry.
//   3. If no source change → exit 0 (trivial PR).
//   4. Else: PASS when EITHER `CHANGELOG.md` is in the changed set OR
//      any commit body in BASE_REF..HEAD contains `chore(skip-changelog)`.
//   5. Else: exit 1 with a single-line diagnostic listing the source
//      files that triggered the requirement.
//
// The gate is intentionally **not** wired into `quality.yml` today —
// landing it as a separate xtask first lets contributors run it
// locally (`cargo xtask check-changelog`) and gives the workspace
// experience time to confirm the heuristic before it gates PRs.
//
// Determinism notes:
//   - The `git` calls run from the workspace root; stdout / stderr
//     are not used for diagnostics so concurrent test invocations
//     don't interleave.
//   - The base ref must already exist (the caller is expected to
//     `git fetch` before calling; CI's checkout action does this).

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

fn check_changelog_from_args(
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

// ── test-gate (P10.19.2) ───────────────────────────────────────────────────
//
// Run `cargo xtask test-gate [--baseline <path>] [--threshold <ratio>]
// [--allow-missing] [--update] [--input <path>] [--include <crate>...]
// [--exclude <crate>...]`.
//
// Default behaviour (compare mode):
//
//   1. Walk `workspace.members`, drop `xtask` itself + any `--exclude`
//      entries + restrict to `--include` when set.
//   2. For each remaining crate, run `cargo test -p <crate>
//      --all-targets` and time it with `Instant::now()`. Compilation +
//      execution wall-clock is captured together — the gate's job is
//      noticing bloat, not isolating runtime from build time.
//   3. Compare the per-crate ratio against the baseline file. Any
//      `current / baseline >= threshold` is a regression and fails
//      the gate. Default threshold is `1.5×` (looser than bench-gate's
//      `1.25×` because test wall-clock is meaningfully noisier than
//      criterion microbenches).
//
// Capture mode (`--update`):
//
//   1. Same per-crate `cargo test` sweep.
//   2. Write the resulting `TestTimingBaseline` JSON to the baseline
//      path, overwriting any existing file. Stamp `host.captured_at`
//      with today's date in UTC and `host.rustc` with `rustc --version`
//      so a future reader knows the provenance.
//
// `--input <path>`: skip the cargo invocations and read the "current"
// timings from a pre-captured JSON of the same shape. Useful when CI
// captures timings in one job and the gate runs in a downstream job.
//
// `--allow-missing`: don't error when the baseline has an entry for a
// crate the current run didn't cover (mirrors bench-gate's flag).
// Useful during the rollout phase.

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

fn test_gate_from_args(
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
    let workspace_root = workspace_root();
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
    let workspace_root = workspace_root();
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
    let workspace_root = workspace_root();
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
    let workspace_root = workspace_root();
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
    let workspace_root = workspace_root();
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
      &super::workspace_root(),
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
      &super::workspace_root(),
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

// ── refresh-live-models (P10.3.4) ───────────────────────────────────────────
//
// Run `cargo xtask refresh-live-models` to validate the hard-coded text-model
// defaults in `agentflow-llm/tests/provider_consistency_live.rs` against each
// provider's live `/models` endpoint. Loads API keys from `~/.agentflow/.env`
// when present, falling back to the ambient environment (which is what the
// `llm-live.yml` workflow uses on CI). Providers whose key is missing
// self-skip with a clear message rather than fail the run.
//
// Output (one row per provider): `status default-id (n alternatives shown)`.
// Status is one of:
//   - `ok`       — the hard-coded default appears in the provider's model list
//   - `missing`  — the default is NOT in the list; treat as a vendor-side
//                  deprecation, copy a suggested replacement into the test
//                  source or override via `AGENTFLOW_LIVE_<P>_TEXT_MODEL` in
//                  `.github/workflows/llm-live.yml::env`
//   - `skipped`  — no API key found for this provider
//   - `error`    — HTTP request failed (network / auth / parse). Exit non-zero
//                  unless --keep-going.
//
// Flags:
//   --keep-going        don't exit non-zero on per-provider HTTP errors
//   --include <name>    repeatable; restrict to a subset of providers
//
// The output is the source of truth — the operator copy-pastes the suggested
// replacement into `provider_consistency_live.rs::run_text_path` (the only
// place these defaults live). No auto-edit: source-code mutation from a CLI
// helper is too easy to get wrong, and the test file's defaults carry
// explanatory comments that an automated rewrite would clobber.

const REFRESH_LIVE_MODELS_PROBES: &[LiveModelProbe] = &[
  LiveModelProbe {
    name: "openai",
    key_envs: &["OPENAI_API_KEY"],
    default_text_model: "gpt-4o-mini",
    endpoint: LiveModelsEndpoint::OpenAICompat("https://api.openai.com/v1/models"),
  },
  LiveModelProbe {
    name: "anthropic",
    key_envs: &["ANTHROPIC_API_KEY"],
    default_text_model: "claude-haiku-4-5",
    endpoint: LiveModelsEndpoint::Anthropic("https://api.anthropic.com/v1/models"),
  },
  LiveModelProbe {
    name: "google",
    key_envs: &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
    default_text_model: "gemini-2.5-flash",
    endpoint: LiveModelsEndpoint::Google("https://generativelanguage.googleapis.com/v1beta/models"),
  },
  LiveModelProbe {
    name: "moonshot",
    key_envs: &["MOONSHOT_API_KEY"],
    default_text_model: "moonshot-v1-8k",
    endpoint: LiveModelsEndpoint::OpenAICompat("https://api.moonshot.cn/v1/models"),
  },
  LiveModelProbe {
    name: "stepfun",
    key_envs: &["STEPFUN_API_KEY", "STEP_API_KEY"],
    default_text_model: "step-1-8k",
    endpoint: LiveModelsEndpoint::OpenAICompat("https://api.stepfun.com/v1/models"),
  },
  LiveModelProbe {
    name: "glm",
    key_envs: &["GLM_API_KEY", "BIGMODEL_API_KEY", "ZHIPU_API_KEY"],
    // `glm-4.5-flash` was de-listed; `glm-5.1` is the current Zhipu
    // BigModel default. Mirror the live test source-of-truth in
    // `agentflow-llm/tests/provider_consistency_live.rs`.
    default_text_model: "glm-5.1",
    endpoint: LiveModelsEndpoint::OpenAICompat("https://open.bigmodel.cn/api/paas/v4/models"),
  },
  LiveModelProbe {
    name: "dashscope",
    key_envs: &["DASHSCOPE_API_KEY"],
    default_text_model: "qwen-plus",
    endpoint: LiveModelsEndpoint::OpenAICompat(
      "https://dashscope.aliyuncs.com/compatible-mode/v1/models",
    ),
  },
  LiveModelProbe {
    name: "deepseek",
    key_envs: &["DEEPSEEK_API_KEY"],
    default_text_model: "deepseek-v4-flash",
    endpoint: LiveModelsEndpoint::OpenAICompat("https://api.deepseek.com/v1/models"),
  },
  LiveModelProbe {
    name: "minimax",
    key_envs: &["MINIMAX_API_KEY"],
    default_text_model: "MiniMax-M2",
    endpoint: LiveModelsEndpoint::OpenAICompat("https://api.minimaxi.com/v1/models"),
  },
];

#[derive(Debug, Clone, Copy)]
struct LiveModelProbe {
  name: &'static str,
  key_envs: &'static [&'static str],
  default_text_model: &'static str,
  endpoint: LiveModelsEndpoint,
}

#[derive(Debug, Clone, Copy)]
enum LiveModelsEndpoint {
  /// `Authorization: Bearer <key>` + `GET <url>`. Response shape:
  /// `{"data": [{"id": "model-id"}, ...]}`. Used by OpenAI itself and
  /// every OpenAI-compat vendor (Moonshot / StepFun / GLM / DashScope /
  /// DeepSeek / MiniMax).
  OpenAICompat(&'static str),
  /// `x-api-key: <key>` + `anthropic-version: 2023-06-01` + `GET <url>`.
  /// Response shape: `{"data": [{"id": "..."}, ...]}` (same outer shape
  /// as OpenAI, but the auth headers differ).
  Anthropic(&'static str),
  /// `GET <url>?key=<key>`. Response shape:
  /// `{"models": [{"name": "models/gemini-..."}, ...]}` (note the leading
  /// `models/` prefix on each name).
  Google(&'static str),
}

#[derive(Debug, Clone)]
struct LiveProbeOutcome {
  provider: &'static str,
  /// `Some(id)` when a key was found and the request succeeded;
  /// `None` when the provider was skipped.
  default_text_model: String,
  status: ProbeStatus,
}

#[derive(Debug, Clone)]
enum ProbeStatus {
  Ok,
  Missing { suggestions: Vec<String> },
  Skipped { reason: String },
  Error { reason: String },
}

pub fn refresh_live_models_from_args(
  workspace_root: &Path,
  args: Vec<String>,
  out: &mut impl Write,
  err: &mut impl Write,
) -> Result<()> {
  let mut keep_going = false;
  let mut include: Vec<String> = Vec::new();
  let mut iter = args.into_iter();
  while let Some(arg) = iter.next() {
    match arg.as_str() {
      "--keep-going" => keep_going = true,
      "--include" => {
        let value = iter
          .next()
          .ok_or_else(|| anyhow::anyhow!("--include requires a provider name"))?;
        include.push(value);
      }
      other => bail!("unknown refresh-live-models flag '{other}'"),
    }
  }

  // Load ~/.agentflow/.env into the process environment, mirroring the
  // existing `AgentFlow::init()` convention. Silent no-op when the file
  // is missing — that's the expected case on CI, where keys come from
  // the workflow's `env:` block.
  let env_path = std::env::var_os("HOME")
    .map(PathBuf::from)
    .map(|home| home.join(".agentflow").join(".env"));
  if let Some(path) = env_path.as_ref()
    && path.exists()
  {
    let loaded = load_dotenv_file(path)?;
    let _ = writeln!(
      out,
      "[refresh] loaded {} key(s) from {}",
      loaded,
      path.display()
    );
  }

  let probes: Vec<LiveModelProbe> = if include.is_empty() {
    REFRESH_LIVE_MODELS_PROBES.to_vec()
  } else {
    let include_set: BTreeSet<&str> = include.iter().map(String::as_str).collect();
    REFRESH_LIVE_MODELS_PROBES
      .iter()
      .copied()
      .filter(|p| include_set.contains(p.name))
      .collect()
  };
  if probes.is_empty() {
    bail!(
      "no providers selected (got --include filter with no matching probe). \
       Known providers: {}",
      REFRESH_LIVE_MODELS_PROBES
        .iter()
        .map(|p| p.name)
        .collect::<Vec<_>>()
        .join(", ")
    );
  }

  // Empty `_ = workspace_root`: kept on the signature to mirror the other
  // xtask subcommands (`bench_gate_from_args`, `test_gate_from_args`).
  // Future extensions may want to read the workspace root to locate the
  // canonical defaults via grep, etc.
  let _ = workspace_root;

  let mut outcomes: Vec<LiveProbeOutcome> = Vec::with_capacity(probes.len());
  for probe in &probes {
    let outcome = probe_provider(probe);
    print_probe_line(out, &outcome);
    outcomes.push(outcome);
  }

  // Summary + exit code.
  let mut ok_count = 0usize;
  let mut missing_count = 0usize;
  let mut skipped_count = 0usize;
  let mut error_count = 0usize;
  for o in &outcomes {
    match o.status {
      ProbeStatus::Ok => ok_count += 1,
      ProbeStatus::Missing { .. } => missing_count += 1,
      ProbeStatus::Skipped { .. } => skipped_count += 1,
      ProbeStatus::Error { .. } => error_count += 1,
    }
  }
  let _ = writeln!(
    out,
    "\n[refresh] summary: {ok_count} ok, {missing_count} missing, {skipped_count} skipped, {error_count} error"
  );

  if missing_count > 0 {
    // Missing models are a soft failure — surface them but don't make
    // operators pass --keep-going. The whole point of the tool is to
    // tell you what to fix; that's not a `cargo xtask` exit-nonzero
    // condition.
    let _ = writeln!(
      out,
      "[refresh] {missing_count} provider(s) need attention; copy the suggested replacement into agentflow-llm/tests/provider_consistency_live.rs::run_text_path"
    );
  }
  if error_count > 0 && !keep_going {
    let _ = writeln!(
      err,
      "[refresh] {error_count} provider(s) failed; rerun with --keep-going to ignore HTTP errors"
    );
    bail!("refresh-live-models: {error_count} probe error(s)");
  }
  Ok(())
}

fn probe_provider(probe: &LiveModelProbe) -> LiveProbeOutcome {
  let api_key = match resolve_api_key(probe.key_envs) {
    Some(k) => k,
    None => {
      return LiveProbeOutcome {
        provider: probe.name,
        default_text_model: probe.default_text_model.to_string(),
        status: ProbeStatus::Skipped {
          reason: format!(
            "no key in {} (or HOME/.agentflow/.env)",
            probe.key_envs.join(" / ")
          ),
        },
      };
    }
  };

  let model_ids = match fetch_provider_models(probe, &api_key) {
    Ok(ids) => ids,
    Err(reason) => {
      return LiveProbeOutcome {
        provider: probe.name,
        default_text_model: probe.default_text_model.to_string(),
        status: ProbeStatus::Error {
          reason: reason.to_string(),
        },
      };
    }
  };

  if model_ids.iter().any(|id| id == probe.default_text_model) {
    return LiveProbeOutcome {
      provider: probe.name,
      default_text_model: probe.default_text_model.to_string(),
      status: ProbeStatus::Ok,
    };
  }
  // Rolling-alias-with-dated-revisions fallback. Anthropic's
  // `/v1/models` lists dated revisions like `claude-haiku-4-5-20251001`
  // but not the rolling-alias-style `claude-haiku-4-5`. The alias still
  // resolves to the latest dated revision in real API calls, so a
  // strict-equality check produces a false positive. Mitigate: if any
  // returned id starts with `<default>-` followed by a digit (the
  // start of a YYYY[MM[DD]] date), treat the alias as still valid.
  // The "followed by a digit" guard prevents matching genuinely
  // different families that happen to share the alias as a prefix
  // (e.g. an unrelated `gpt-4o-mini-realtime` shouldn't satisfy
  // `gpt-4o-mini`'s check).
  if let Some(dated) = find_dated_revision(probe.default_text_model, &model_ids) {
    return LiveProbeOutcome {
      provider: probe.name,
      default_text_model: format!("{} (via dated revision {dated})", probe.default_text_model),
      status: ProbeStatus::Ok,
    };
  }
  // Missing: pick up to 3 plausible alternatives. The heuristic is
  // "ids that share the most prefix characters with the current
  // default" — for cases like `claude-3-5-haiku-20241022 →
  // claude-haiku-4-5` that's noisy, but the operator still gets a
  // useful starting set. The ranking is deterministic so two runs
  // produce the same suggestions.
  let mut scored: Vec<(usize, String)> = model_ids
    .into_iter()
    .map(|id| (shared_prefix_len(&id, probe.default_text_model), id))
    .collect();
  scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
  let suggestions: Vec<String> = scored.into_iter().take(3).map(|(_, id)| id).collect();
  LiveProbeOutcome {
    provider: probe.name,
    default_text_model: probe.default_text_model.to_string(),
    status: ProbeStatus::Missing { suggestions },
  }
}

fn print_probe_line(out: &mut impl Write, outcome: &LiveProbeOutcome) {
  match &outcome.status {
    ProbeStatus::Ok => {
      let _ = writeln!(
        out,
        "  ✓ {:<10} ok       default={}",
        outcome.provider, outcome.default_text_model
      );
    }
    ProbeStatus::Missing { suggestions } => {
      let _ = writeln!(
        out,
        "  ✗ {:<10} missing  default={} not in /models",
        outcome.provider, outcome.default_text_model
      );
      if suggestions.is_empty() {
        let _ = writeln!(
          out,
          "      (no alternatives surfaced; manual review needed)"
        );
      } else {
        let _ = writeln!(
          out,
          "      suggested replacements: {}",
          suggestions.join(", ")
        );
      }
    }
    ProbeStatus::Skipped { reason } => {
      let _ = writeln!(out, "  · {:<10} skipped  ({reason})", outcome.provider);
    }
    ProbeStatus::Error { reason } => {
      let _ = writeln!(out, "  ! {:<10} error    {reason}", outcome.provider);
    }
  }
}

fn resolve_api_key(key_envs: &[&str]) -> Option<String> {
  for env_var in key_envs {
    if let Ok(value) = std::env::var(env_var) {
      let trimmed = value.trim();
      if !trimmed.is_empty() {
        return Some(trimmed.to_string());
      }
    }
  }
  None
}

/// Hit the provider's models endpoint via `curl` and return a sorted
/// list of model ids. Uses curl rather than reqwest to keep xtask's
/// dep graph small — this is a one-shot operator tool, not a hot path.
fn fetch_provider_models(probe: &LiveModelProbe, api_key: &str) -> Result<Vec<String>> {
  let mut cmd = Command::new("curl");
  cmd.arg("--silent").arg("--show-error").arg("--fail");
  let url = match probe.endpoint {
    LiveModelsEndpoint::OpenAICompat(url) => {
      cmd
        .arg("-H")
        .arg(format!("Authorization: Bearer {api_key}"));
      url.to_string()
    }
    LiveModelsEndpoint::Anthropic(url) => {
      cmd.arg("-H").arg(format!("x-api-key: {api_key}"));
      cmd.arg("-H").arg("anthropic-version: 2023-06-01");
      url.to_string()
    }
    LiveModelsEndpoint::Google(url) => format!("{url}?key={api_key}"),
  };
  cmd.arg(&url);

  let output = cmd
    .output()
    .with_context(|| format!("spawning curl for {}", probe.name))?;
  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    bail!("curl exited {} (stderr: {})", output.status, stderr.trim());
  }
  parse_models_response(probe, &output.stdout)
}

/// Pure parser: turn a provider's `/models` JSON response into the
/// model-id list. Exposed for unit testing.
fn parse_models_response(probe: &LiveModelProbe, body: &[u8]) -> Result<Vec<String>> {
  let value: serde_json::Value = serde_json::from_slice(body).with_context(|| {
    format!(
      "parsing {} /models response (body starts with: {:?})",
      probe.name,
      std::str::from_utf8(&body[..body.len().min(120)]).unwrap_or("<non-utf8>")
    )
  })?;
  let raw_ids: Vec<String> = match probe.endpoint {
    LiveModelsEndpoint::OpenAICompat(_) | LiveModelsEndpoint::Anthropic(_) => value
      .get("data")
      .and_then(|d| d.as_array())
      .ok_or_else(|| anyhow::anyhow!("{}: missing `data` array", probe.name))?
      .iter()
      .filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(String::from))
      .collect(),
    LiveModelsEndpoint::Google(_) => value
      .get("models")
      .and_then(|d| d.as_array())
      .ok_or_else(|| anyhow::anyhow!("{}: missing `models` array", probe.name))?
      .iter()
      .filter_map(|m| m.get("name").and_then(|i| i.as_str()).map(String::from))
      .map(|name| {
        // Google's response prefixes with `models/`; strip it so the
        // ids align with the test file's hard-coded `gemini-2.5-flash`
        // style.
        name
          .strip_prefix("models/")
          .map(String::from)
          .unwrap_or(name)
      })
      .collect(),
  };
  let mut ids = raw_ids;
  ids.sort();
  ids.dedup();
  Ok(ids)
}

/// Return the first model id from `list` that looks like a dated
/// revision of `alias` — i.e. matches `<alias>-<digit>…`. Used to
/// suppress the rolling-alias false positive when a provider's
/// `/v1/models` only enumerates dated revisions (Anthropic does
/// this for some model families).
///
/// The "followed by a digit" guard prevents same-prefix unrelated
/// families from satisfying the check: `gpt-4o-mini-realtime`
/// shouldn't match `gpt-4o-mini`, but `gpt-4o-mini-2024-07-18`
/// should.
fn find_dated_revision<'a>(alias: &str, list: &'a [String]) -> Option<&'a str> {
  let prefix = format!("{alias}-");
  list.iter().find_map(|id| {
    id.strip_prefix(&prefix)
      .filter(|rest| rest.chars().next().is_some_and(|c| c.is_ascii_digit()))
      .map(|_| id.as_str())
  })
}

/// Number of leading characters two strings share, case-insensitive.
/// Used by the "suggest a replacement" heuristic.
fn shared_prefix_len(a: &str, b: &str) -> usize {
  a.chars()
    .zip(b.chars())
    .take_while(|(ca, cb)| ca.eq_ignore_ascii_case(cb))
    .count()
}

/// Minimal `.env` loader — parses `KEY=value` lines and writes them
/// to the process environment with `set_var`. Lines starting with
/// `#` are skipped; values may be wrapped in single or double quotes,
/// which are stripped. Returns the count of keys set.
///
/// **Policy: `.env` wins over the ambient shell.** A real-world run
/// (P10.3.4) surfaced an operator-stale `OPENAI_API_KEY` in the shell
/// silently blocking a valid value in `~/.agentflow/.env`. The fix is
/// to make `.env` authoritative when it exists. CI is unaffected
/// because no `.env` file ships in the runner's HOME; secrets reach
/// the process via the workflow's `env:` block exactly as before.
///
/// (Intentionally not via the `dotenv` crate — keeps xtask's dep
/// graph minimal. The parser is a faithful subset of the standard
/// `.env` grammar.)
fn load_dotenv_file(path: &Path) -> Result<usize> {
  let text =
    std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
  let mut count = 0usize;
  for line in text.lines() {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
      continue;
    }
    let Some(eq) = line.find('=') else { continue };
    let key = line[..eq].trim();
    let raw_value = line[eq + 1..].trim();
    let value = strip_dotenv_quotes(raw_value);
    if key.is_empty() {
      continue;
    }
    // SAFETY: xtask is a single-threaded CLI; no concurrent env
    // access while this loop runs.
    unsafe {
      std::env::set_var(key, value);
    }
    count += 1;
  }
  Ok(count)
}

fn strip_dotenv_quotes(value: &str) -> &str {
  if value.len() >= 2 {
    let bytes = value.as_bytes();
    let first = bytes[0];
    let last = bytes[bytes.len() - 1];
    if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
      return &value[1..value.len() - 1];
    }
  }
  value
}

// ── redaction-lint (Q5.2) ──────────────────────────────────────────────────
//
// Walks `agentflow-*/src/**/*.rs` looking for `tracing::*!` macro calls
// (also bare `debug!` / `info!` / `warn!` / `error!`) that interpolate raw
// user-supplied data — prompts, LLM responses, request/response bodies,
// tool params, chat content — without going through
// `agentflow_tracing::redaction` or `agentflow_llm::prompt_fingerprint`.
//
// The grammar of a "bad" call is intentionally narrow:
//   (debug|info|warn|error)!(... <danger> = (%|?) ...)
// where <danger> is one of `DANGER_FIELDS`. Trailing-suffix variants like
// `prompt_len`, `prompt_sha`, `response_fingerprint`, `body_size` are safe
// because they encode a metric, not the raw value, so the pattern only
// matches the danger word followed by whitespace + `=`.
//
// Format-string interpolation (`"... {prompt}"`) is harder to catch
// without parsing — we run a second regex that flags only the most
// obvious shapes: `"... {prompt}"` / `"... {response}"` etc. Anything
// more complex needs a per-site `// allow-redaction-lint: <reason>`
// comment immediately above the call.
//
// Allow list mechanics:
//   - The `// allow-redaction-lint: <reason>` comment on the same line
//     or the line immediately above the macro call suppresses the lint
//     hit for that site. The `reason` text is captured and printed in
//     the report (good citizen check — empty reasons fail the lint).
//   - `TRACE`-level macros are exempt: production deployments
//     intentionally exclude trace, and we already document
//     "TRACE may contain PII" elsewhere.

const DANGER_FIELDS: &[&str] = &[
  "prompt",
  "response",
  "content",
  "body",
  "raw_response",
  "planner_text",
  "user_input",
  "message_body",
  "params",
  "request_body",
  "response_body",
];

/// One redaction-lint hit.
#[derive(Debug, PartialEq, Eq)]
struct RedactionLintHit {
  path: PathBuf,
  line: usize,
  level: String,
  field: String,
  snippet: String,
}

fn redaction_lint_at(
  workspace_root: &Path,
  stdout: &mut impl Write,
  stderr: &mut impl Write,
) -> Result<()> {
  let entries = std::fs::read_dir(workspace_root)
    .with_context(|| format!("read workspace root {}", workspace_root.display()))?;

  let mut crate_dirs: Vec<PathBuf> = Vec::new();
  for entry in entries.flatten() {
    let name = entry.file_name().to_string_lossy().into_owned();
    if name.starts_with("agentflow-") && entry.path().is_dir() {
      let src = entry.path().join("src");
      if src.is_dir() {
        crate_dirs.push(src);
      }
    }
  }
  crate_dirs.sort();

  let mut hits: Vec<RedactionLintHit> = Vec::new();
  for src in &crate_dirs {
    collect_redaction_hits(src, &mut hits)?;
  }

  hits.sort_by_key(|a| (a.path.clone(), a.line));

  for hit in &hits {
    let _ = writeln!(
      stderr,
      "{}:{}: redaction-lint: {}! interpolates `{}` without fingerprint or redaction\n    {}",
      hit.path.display(),
      hit.line,
      hit.level,
      hit.field,
      hit.snippet
    );
  }

  if hits.is_empty() {
    let _ = writeln!(
      stdout,
      "redaction-lint: OK ({} crate dirs scanned)",
      crate_dirs.len()
    );
    Ok(())
  } else {
    let _ = writeln!(
      stderr,
      "\nredaction-lint: {} hit(s); see `agentflow-tracing::redaction::redact_text/value` or `agentflow_llm::prompt_fingerprint` for the canonical helpers. Suppress a false-positive with `// allow-redaction-lint: <reason>` on the same line.",
      hits.len()
    );
    bail!("redaction-lint failed: {} hit(s)", hits.len());
  }
}

fn collect_redaction_hits(dir: &Path, hits: &mut Vec<RedactionLintHit>) -> Result<()> {
  for entry in std::fs::read_dir(dir)? {
    let entry = entry?;
    let path = entry.path();
    if path.is_dir() {
      collect_redaction_hits(&path, hits)?;
    } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
      scan_file_for_redaction(&path, hits)?;
    }
  }
  Ok(())
}

fn scan_file_for_redaction(path: &Path, hits: &mut Vec<RedactionLintHit>) -> Result<()> {
  let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
  for (idx, line) in text.lines().enumerate() {
    if line.contains("allow-redaction-lint") {
      continue;
    }
    if let Some(hit) = detect_redaction_hit(line) {
      hits.push(RedactionLintHit {
        path: path.to_path_buf(),
        line: idx + 1,
        level: hit.0,
        field: hit.1,
        snippet: line.trim().to_string(),
      });
    }
  }
  Ok(())
}

/// Return `(level, danger_field)` when the line contains a redaction
/// violation, otherwise `None`. Pure function so it's straight-line
/// testable.
fn detect_redaction_hit(line: &str) -> Option<(String, String)> {
  let trimmed = line.trim_start();
  if trimmed.starts_with("//") {
    return None;
  }
  // We only flag debug/info/warn/error. `trace!` is intentionally exempt.
  let levels = ["debug", "info", "warn", "error"];
  let mut macro_level: Option<&str> = None;
  for level in levels {
    let needle = format!("{level}!(");
    let needle_ns = format!("::{level}!(");
    if line.contains(&needle) || line.contains(&needle_ns) {
      macro_level = Some(level);
      break;
    }
  }
  let level = macro_level?;

  // Pattern 1: structured field — `field = %expr` / `field = ?expr`.
  for danger in DANGER_FIELDS {
    let patterns = [
      format!("{danger} = %"),
      format!("{danger} = ?"),
      format!("{danger}=%"),
      format!("{danger}=?"),
    ];
    for pat in &patterns {
      if line.contains(pat) {
        return Some((level.to_string(), (*danger).to_string()));
      }
    }
  }

  // Pattern 2: format-string positional `"... {danger} ..."`. Same
  // danger list, but as a literal `{danger}` brace.
  for danger in DANGER_FIELDS {
    let needle = format!("{{{danger}}}");
    if line.contains(&needle) {
      return Some((level.to_string(), (*danger).to_string()));
    }
  }

  None
}

#[cfg(test)]
mod redaction_lint_tests {
  use super::*;
  use std::fs;
  use tempfile::TempDir;

  #[test]
  fn detect_flags_debug_with_structured_prompt_field() {
    let line = r#"        debug!(prompt = %self.prompt, "request");"#;
    assert_eq!(
      detect_redaction_hit(line),
      Some(("debug".to_string(), "prompt".to_string()))
    );
  }

  #[test]
  fn detect_flags_info_with_response_format_brace() {
    let line = r#"info!("LLM responded: {response}");"#;
    assert_eq!(
      detect_redaction_hit(line),
      Some(("info".to_string(), "response".to_string()))
    );
  }

  #[test]
  fn detect_ignores_trace_level() {
    let line = r#"tracing::trace!(prompt = %self.prompt, "TRACE-only full body");"#;
    assert!(detect_redaction_hit(line).is_none());
  }

  #[test]
  fn detect_ignores_fingerprint_suffix_variants() {
    // These encode a metric, not raw text.
    for line in [
      r#"debug!(prompt_len = self.prompt.len(), "request");"#,
      r#"info!(response_sha = %fingerprint, "done");"#,
      r#"warn!(body_size = bytes, "oversized");"#,
    ] {
      assert!(
        detect_redaction_hit(line).is_none(),
        "fingerprint metric must not trip the lint: {line}"
      );
    }
  }

  #[test]
  fn detect_ignores_comments_and_docstrings() {
    for line in [
      r#"  // debug!(prompt = %self.prompt, "request");"#,
      r#"  /// debug!(response = %x, "doc example");"#,
      r#"  //! info!(content = %body, "module-level doc");"#,
    ] {
      assert!(
        detect_redaction_hit(line).is_none(),
        "comment must not trip the lint: {line}"
      );
    }
  }

  #[test]
  fn allow_redaction_lint_comment_suppresses_hit() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();
    let crate_src = workspace.join("agentflow-fake").join("src");
    fs::create_dir_all(&crate_src).unwrap();
    fs::write(
      crate_src.join("lib.rs"),
      // Two near-identical sites; the second carries the allow-comment
      // marker so it must be skipped.
      "fn a() { debug!(prompt = %x, \"bad\"); }\n\
       fn b() { debug!(prompt = %x, \"benign\"); } // allow-redaction-lint: test fixture\n",
    )
    .unwrap();

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let err = redaction_lint_at(workspace, &mut stdout, &mut stderr).unwrap_err();
    let err_text = format!("{err}");
    let stderr_text = String::from_utf8(stderr).unwrap();
    assert!(err_text.contains("1 hit"), "expected 1 hit, got {err_text}");
    assert!(
      stderr_text.contains("lib.rs:1"),
      "expected line 1 hit, got {stderr_text}"
    );
    assert!(
      !stderr_text.contains("lib.rs:2"),
      "allow-marker line must be suppressed, got {stderr_text}"
    );
  }

  #[test]
  fn green_run_emits_ok_summary() {
    let dir = TempDir::new().unwrap();
    let workspace = dir.path();
    let crate_src = workspace.join("agentflow-clean").join("src");
    fs::create_dir_all(&crate_src).unwrap();
    fs::write(
      crate_src.join("lib.rs"),
      "fn a() { debug!(prompt_len = x.len(), \"safe\"); }\n",
    )
    .unwrap();

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    redaction_lint_at(workspace, &mut stdout, &mut stderr).expect("clean workspace must pass");
    let stdout_text = String::from_utf8(stdout).unwrap();
    assert!(stdout_text.contains("redaction-lint: OK"));
  }
}

#[cfg(test)]
mod refresh_live_models_tests {
  use super::*;

  fn openai_probe() -> LiveModelProbe {
    LiveModelProbe {
      name: "openai",
      key_envs: &["OPENAI_API_KEY"],
      default_text_model: "gpt-4o-mini",
      endpoint: LiveModelsEndpoint::OpenAICompat("https://api.openai.com/v1/models"),
    }
  }

  fn google_probe() -> LiveModelProbe {
    LiveModelProbe {
      name: "google",
      key_envs: &["GEMINI_API_KEY"],
      default_text_model: "gemini-2.5-flash",
      endpoint: LiveModelsEndpoint::Google(
        "https://generativelanguage.googleapis.com/v1beta/models",
      ),
    }
  }

  #[test]
  fn parse_openai_compat_extracts_id_array() {
    let body = br#"{"object":"list","data":[
      {"id":"gpt-4o-mini","object":"model"},
      {"id":"gpt-4o","object":"model"}
    ]}"#;
    let ids = parse_models_response(&openai_probe(), body).expect("parse ok");
    assert_eq!(ids, vec!["gpt-4o".to_string(), "gpt-4o-mini".to_string()]);
  }

  #[test]
  fn parse_anthropic_uses_data_array_too() {
    let probe = LiveModelProbe {
      name: "anthropic",
      key_envs: &["ANTHROPIC_API_KEY"],
      default_text_model: "claude-haiku-4-5",
      endpoint: LiveModelsEndpoint::Anthropic("https://api.anthropic.com/v1/models"),
    };
    let body = br#"{"data":[
      {"id":"claude-haiku-4-5","display_name":"Haiku 4.5","type":"model"},
      {"id":"claude-sonnet-4-5","display_name":"Sonnet 4.5","type":"model"}
    ]}"#;
    let ids = parse_models_response(&probe, body).expect("parse ok");
    assert!(ids.contains(&"claude-haiku-4-5".to_string()));
  }

  #[test]
  fn parse_google_strips_models_prefix() {
    // Google's `/v1beta/models` returns names like `models/gemini-2.5-flash`;
    // strip the prefix so the ids align with the test file's `gemini-2.5-flash`
    // style. Critical: without stripping, every Google default would always
    // report missing.
    let body = br#"{"models":[
      {"name":"models/gemini-2.5-flash"},
      {"name":"models/gemini-2.5-pro"}
    ]}"#;
    let ids = parse_models_response(&google_probe(), body).expect("parse ok");
    assert_eq!(
      ids,
      vec!["gemini-2.5-flash".to_string(), "gemini-2.5-pro".to_string()]
    );
  }

  #[test]
  fn parse_handles_missing_id_field_by_skipping_entry() {
    // Defensive: vendors occasionally ship a `data: []` row with no `id`
    // (sometimes pre-release placeholders). Skip them quietly rather
    // than fail the whole parse.
    let body = br#"{"data":[
      {"id":"gpt-4o-mini"},
      {"object":"model"},
      {"id":"gpt-4o"}
    ]}"#;
    let ids = parse_models_response(&openai_probe(), body).expect("parse ok");
    assert_eq!(ids, vec!["gpt-4o".to_string(), "gpt-4o-mini".to_string()]);
  }

  #[test]
  fn parse_returns_clear_error_on_missing_data_array() {
    let body = br#"{"error":{"message":"unauthorized"}}"#;
    let err = parse_models_response(&openai_probe(), body).unwrap_err();
    assert!(
      err.to_string().contains("missing `data` array"),
      "expected clear shape error; got: {err}"
    );
  }

  #[test]
  fn find_dated_revision_matches_anthropic_haiku_alias() {
    // Anthropic's `/v1/models` only lists dated revisions for some
    // model families; the alias still resolves in real API calls. The
    // tool should not flag the alias as missing when at least one
    // matching dated revision is present.
    let list = vec![
      "claude-haiku-4-5-20251001".to_string(),
      "claude-opus-4-1-20250805".to_string(),
    ];
    assert_eq!(
      find_dated_revision("claude-haiku-4-5", &list),
      Some("claude-haiku-4-5-20251001")
    );
  }

  #[test]
  fn find_dated_revision_skips_same_prefix_but_different_family() {
    // `gpt-4o-mini-realtime-preview` shouldn't satisfy `gpt-4o-mini`
    // — the suffix-must-start-with-digit guard guards this.
    let list = vec![
      "gpt-4o".to_string(),
      "gpt-4o-mini-realtime-preview".to_string(),
    ];
    assert_eq!(find_dated_revision("gpt-4o-mini", &list), None);
  }

  #[test]
  fn find_dated_revision_returns_none_when_no_match() {
    let list = vec!["unrelated".to_string(), "other".to_string()];
    assert!(find_dated_revision("claude-haiku-4-5", &list).is_none());
  }

  #[test]
  fn find_dated_revision_matches_openai_style_yyyy_mm_dd_suffix() {
    // The rule generalises beyond Anthropic — OpenAI dated revisions
    // are `<alias>-<YYYY>-<MM>-<DD>`, which also start with a digit
    // after the alias prefix. (In practice the strict-equality path
    // in `probe_provider` catches `gpt-4o-mini` before this function
    // runs, but we confirm the helper itself behaves correctly.)
    let list = vec!["gpt-4o-mini-2024-07-18".to_string()];
    assert_eq!(
      find_dated_revision("gpt-4o-mini", &list),
      Some("gpt-4o-mini-2024-07-18")
    );
  }

  #[test]
  fn shared_prefix_len_is_case_insensitive() {
    assert_eq!(shared_prefix_len("gpt-4o-mini", "gpt-4O-mini"), 11);
    assert_eq!(
      shared_prefix_len("claude-haiku-4-5", "claude-sonnet-4-5"),
      7
    );
    assert_eq!(shared_prefix_len("", "anything"), 0);
  }

  #[test]
  fn strip_dotenv_quotes_handles_both_quote_kinds() {
    assert_eq!(strip_dotenv_quotes("\"abc\""), "abc");
    assert_eq!(strip_dotenv_quotes("'abc'"), "abc");
    assert_eq!(strip_dotenv_quotes("abc"), "abc");
    // Mixed quotes don't strip (would be ambiguous).
    assert_eq!(strip_dotenv_quotes("\"abc'"), "\"abc'");
    // Too short to be a quoted pair.
    assert_eq!(strip_dotenv_quotes("\""), "\"");
    assert_eq!(strip_dotenv_quotes(""), "");
  }

  #[test]
  fn refresh_rejects_unknown_include_filter() {
    let workspace_root = workspace_root();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let res = refresh_live_models_from_args(
      &workspace_root,
      vec!["--include".to_string(), "no-such-provider".to_string()],
      &mut out,
      &mut err,
    );
    assert!(res.is_err(), "unknown provider must fail fast");
    let msg = res.unwrap_err().to_string();
    assert!(msg.contains("no providers selected"), "got: {msg}");
  }

  #[test]
  fn probe_skips_providers_without_api_key() {
    // SAFETY: this test mutates the process environment. The env var
    // name is unique to this test so concurrent tests don't collide.
    const KEY: &str = "OPENAI_API_KEY_REFRESH_PROBE_TEST_SKIP";
    unsafe {
      std::env::remove_var(KEY);
    }
    let probe = LiveModelProbe {
      name: "openai-skip-test",
      key_envs: &[KEY],
      default_text_model: "gpt-4o-mini",
      endpoint: LiveModelsEndpoint::OpenAICompat("https://example.invalid/models"),
    };
    let outcome = probe_provider(&probe);
    match outcome.status {
      ProbeStatus::Skipped { reason } => {
        assert!(
          reason.contains(KEY),
          "diagnostic must name the env var: {reason}"
        );
      }
      other => panic!("expected Skipped, got {other:?}"),
    }
  }
}

// ── check-arch (P-A0.2) ─────────────────────────────────────────────────────
//
// Enforce the subset of the eight crate-dependency laws from
// `docs/RFC_CRATE_ARCHITECTURE.md` §7 that is checkable against the *current*
// crate set — i.e. before the contract-kernel crates (graph / agent-spi /
// store-spi / async-util / value) land. Two laws are active today:
//
//   - runtime-isolation (RFC §7 Law 4/6): a runtime crate must not depend on
//     another runtime crate. Runtimes today = { core (executor),
//     agents (loop), harness (shell) }.
//   - surface-isolation (RFC §10 P-A2): a surface binary crate must not depend
//     on another surface binary crate. Surfaces = { cli, server, worker }.
//
// Every edge that breaks an active law must either be FIXED or recorded in
// `ARCH_ALLOWLIST` with the P-A task that burns it down. The gate FAILS on:
//   (a) any violating edge NOT in the allowlist — a NEW regression; and
//   (b) any allowlist entry that is now stale (its edge is gone or no longer
//       violates a law) — forcing the allowlist to shrink as the migration
//       pays each edge down.
//
// Activating a new law once the kernel crates exist is a one-line change: add
// the crate set + a `classify_arch_edge` clause. Only `[dependencies]` and
// `[build-dependencies]` count; `[dev-dependencies]` are test-only and do not
// shape the shipped dependency graph, so they are intentionally excluded.

/// Runtime-tier crates (RFC §3). No runtime may depend on another runtime.
const ARCH_RUNTIME_CRATES: &[&str] = &["agentflow-core", "agentflow-agents", "agentflow-harness"];

/// Surface-tier binary crates (RFC §3). No surface may depend on another
/// surface — they compose only via shared contract / assembly crates.
const ARCH_SURFACE_CRATES: &[&str] = &["agentflow-cli", "agentflow-server", "agentflow-worker"];

const LAW_RUNTIME_ISOLATION: &str = "runtime-isolation (RFC §7 Law 4/6)";
const LAW_SURFACE_ISOLATION: &str = "surface-isolation (RFC §10 P-A2)";

/// A currently-tolerated dependency-law violation paired with the P-A
/// migration task that removes it. Each entry must correspond to a real edge
/// that breaks a real law today; the staleness check fails the gate when that
/// stops being true, so the list can only shrink.
struct ArchAllow {
  from: &'static str,
  to: &'static str,
  burndown: &'static str,
}

const ARCH_ALLOWLIST: &[ArchAllow] = &[
  // EMPTY — every tracked runtime/surface-isolation violation has been burned
  // down by the P-A track:
  // - P-A1.3/1.4 + P-A (this): agents -> core. agents builds on the graph IR +
  //   the FlowRunner contract + async-util; the executor (`CoreFlowRunner`) is
  //   injected by the surface, and core is only a dev-dependency.
  // - P-A2.1: harness -> agents. harness depends on the agentflow-agent-spi
  //   contract; agents stays a harness dev-dependency for the smoke test.
  // - P-A2.3: worker -> server. the worker protocol + gRPC client moved to
  //   `agentflow-worker-proto`; server stays a worker dev-dependency for tests.
  // - P-A2.4: server -> cli. the config/executor assembly + the diagnostics
  //   report builder moved to `agentflow-config`.
];

/// A latent target-state violation: an edge that does NOT break either of the
/// two *active* laws (runtime-/surface-isolation) but WILL break a contract-tier
/// law (RFC §7 laws 1/2/3/7) once the kernel crates land and that law is
/// activated. This is the full repoint checklist from
/// `docs/ARCHITECTURE_EVALUATION_2026-06-20.md` §2 (rows 5–11 + `tracing→core`),
/// expanded to individual `from -> to` pairs — the complete target-state edge
/// map, code-tracked so it cannot rot (P-A0.4 / evaluation R5).
///
/// The gate self-maintains the list: it FAILS when a latent edge has been paid
/// down (the dep is gone → prune it) or has become an *active* violation (the
/// edge now breaks an enforced law → move it to `ARCH_ALLOWLIST`). It does not
/// fail merely because the edge still exists — that is the expected state until
/// its kernel crate lands.
struct ArchLatent {
  from: &'static str,
  to: &'static str,
  /// The contract-tier law this edge will break once that law is activated.
  becomes: &'static str,
  /// The P-A task that repoints (pays down) this edge.
  burndown: &'static str,
}

const ARCH_LATENT_EDGES: &[ArchLatent] = &[
  // Row 5 — `agents` runtime fused to concrete impls (law 4); inject via
  // agent-spi / store-spi / tool contracts at surfaces (P-A1.1/1.2 + P-A2.1).
  ArchLatent {
    from: "agentflow-agents",
    to: "agentflow-llm",
    becomes: "law 4 runtime→impl",
    burndown: "P-A1.1 — inject LLM via agent-spi at surfaces",
  },
  ArchLatent {
    from: "agentflow-agents",
    to: "agentflow-mcp",
    becomes: "law 4 runtime→impl",
    burndown: "P-A1.1 — inject MCP tools via tool contract",
  },
  ArchLatent {
    from: "agentflow-agents",
    to: "agentflow-memory",
    becomes: "law 4 runtime→impl",
    burndown: "P-A1.2 — depend on store-spi MemoryStore, not the impl",
  },
  ArchLatent {
    from: "agentflow-agents",
    to: "agentflow-tools",
    becomes: "law 4 runtime→impl",
    burndown: "P-A1.1 — depend on the tool contract; builtins injected",
  },
  // Row 6 — `harness` carries 5 impl edges; only `harness→agents` is in the
  // allowlist. These four remain after P-A2.1 repoints harness→agent-spi.
  ArchLatent {
    from: "agentflow-harness",
    to: "agentflow-llm",
    becomes: "law 4 runtime→impl",
    burndown: "P-A1.2 — tokenizer via value/store-spi util (R6)",
  },
  ArchLatent {
    from: "agentflow-harness",
    to: "agentflow-memory",
    becomes: "law 4 runtime→impl",
    burndown: "P-A1.2 — depend on store-spi MemoryStore",
  },
  ArchLatent {
    from: "agentflow-harness",
    to: "agentflow-tools",
    becomes: "law 4 runtime→impl",
    burndown: "P-A2.1 — depend on the tool contract; builtins injected",
  },
  ArchLatent {
    from: "agentflow-harness",
    to: "agentflow-tracing",
    becomes: "law 4 runtime→impl",
    burndown: "P-A1.1 — redaction/trace-context via agent-spi (R6)",
  },
  // Rows 7–8 — `nodes` fat straddler (law 2: tool crate on capabilities + runtime).
  ArchLatent {
    from: "agentflow-nodes",
    to: "agentflow-llm",
    becomes: "law 2 tool→capability",
    burndown: "P-A0.5 — split capability-backed nodes out (R3)",
  },
  ArchLatent {
    from: "agentflow-nodes",
    to: "agentflow-rag",
    becomes: "law 2 tool→capability",
    burndown: "P-A0.5 — split capability-backed nodes out (R3)",
  },
  ArchLatent {
    from: "agentflow-nodes",
    to: "agentflow-mcp",
    becomes: "law 2 tool→tool",
    burndown: "P-A0.5 — MCPNode moves with the decomposition (R3)",
  },
  ArchLatent {
    from: "agentflow-nodes",
    to: "agentflow-core",
    becomes: "law 2 tool→runtime",
    burndown: "P-A1.3 — IR-only edge; becomes nodes→graph",
  },
  // Row 9 — `skills` capability depends on the `agents` runtime (law 3 inversion).
  ArchLatent {
    from: "agentflow-skills",
    to: "agentflow-agents",
    becomes: "law 3 capability→runtime",
    burndown: "P-A4.3 — Capability::lower; surface wires the runtime",
  },
  // Row 10 — `memory` capability→capability (law 3).
  ArchLatent {
    from: "agentflow-memory",
    to: "agentflow-rag",
    becomes: "law 3 capability→capability",
    burndown: "P-A1.2 — EmbeddingProvider via store-spi (R6)",
  },
  // Row 11 — `mcp` tool→ops (law 2), traceparent ambient only.
  ArchLatent {
    from: "agentflow-mcp",
    to: "agentflow-tracing",
    becomes: "law 2 tool→ops",
    burndown: "P-A1.1 — trace-context contract via agent-spi/value (R6)",
  },
  // Extra — `tracing` ops→runtime for the workflow event types.
  ArchLatent {
    from: "agentflow-tracing",
    to: "agentflow-core",
    becomes: "ops→runtime",
    burndown: "P-A1.1/P-A1.5 — depend on agent-spi + value, not core",
  },
];

/// Return the law a `from -> to` internal edge breaks, or `None` when the edge
/// is allowed. Pure over the supplied tier sets so it is unit-testable with
/// synthetic crate names.
fn classify_arch_edge(
  from: &str,
  to: &str,
  runtimes: &[&str],
  surfaces: &[&str],
) -> Option<&'static str> {
  let member = |set: &[&str], c: &str| set.contains(&c);
  if member(runtimes, from) && member(runtimes, to) {
    return Some(LAW_RUNTIME_ISOLATION);
  }
  if member(surfaces, from) && member(surfaces, to) {
    return Some(LAW_SURFACE_ISOLATION);
  }
  None
}

/// Outcome of evaluating the architecture laws over a set of edges.
struct ArchEval {
  /// Violating edges recorded in the allowlist (tolerated debt).
  tracked: Vec<(String, String, &'static str)>,
  /// Violating edges NOT in the allowlist (new regressions).
  new: Vec<(String, String, &'static str)>,
  /// Allowlist `(from, to)` pairs whose edge is gone or no longer violates.
  stale: Vec<(String, String)>,
}

/// Pure evaluator: classify every edge, split into tracked vs new violations,
/// and flag stale allowlist entries. No filesystem access, so it is unit-
/// tested directly with synthetic inputs.
fn evaluate_arch(
  edges: &[(String, String)],
  runtimes: &[&str],
  surfaces: &[&str],
  allowlist: &[(&str, &str)],
) -> ArchEval {
  let allow: BTreeSet<(&str, &str)> = allowlist.iter().copied().collect();
  let edge_set: BTreeSet<(&str, &str)> = edges
    .iter()
    .map(|(a, b)| (a.as_str(), b.as_str()))
    .collect();

  let mut tracked = Vec::new();
  let mut new = Vec::new();
  for (from, to) in edges {
    if let Some(law) = classify_arch_edge(from, to, runtimes, surfaces) {
      if allow.contains(&(from.as_str(), to.as_str())) {
        tracked.push((from.clone(), to.clone(), law));
      } else {
        new.push((from.clone(), to.clone(), law));
      }
    }
  }

  let mut stale = Vec::new();
  for (from, to) in allowlist {
    let present = edge_set.contains(&(*from, *to));
    let violates = classify_arch_edge(from, to, runtimes, surfaces).is_some();
    if !present || !violates {
      stale.push((from.to_string(), to.to_string()));
    }
  }

  ArchEval {
    tracked,
    new,
    stale,
  }
}

/// Outcome of evaluating the latent target-state edge map (`ARCH_LATENT_EDGES`).
struct LatentEval {
  /// Latent edges that still exist and are not yet active violations (expected).
  present: Vec<(String, String, &'static str)>,
  /// Latent entries whose edge is gone — paid down; prune from the list.
  resolved: Vec<(String, String)>,
  /// Latent entries whose edge now breaks an *active* law — move to ARCH_ALLOWLIST.
  misfiled: Vec<(String, String, &'static str)>,
}

/// Pure evaluator for the latent edge map. Pure over its inputs so it is
/// unit-tested directly with synthetic crate names. A latent entry is healthy
/// while its edge exists and is not yet classified as an active violation;
/// `resolved` (edge gone) and `misfiled` (edge now actively violates) both force
/// the list to be updated, so the map can only stay truthful or shrink.
fn evaluate_latent(
  edges: &[(String, String)],
  latent: &[(&str, &str, &'static str)],
  runtimes: &[&str],
  surfaces: &[&str],
) -> LatentEval {
  let edge_set: BTreeSet<(&str, &str)> = edges
    .iter()
    .map(|(a, b)| (a.as_str(), b.as_str()))
    .collect();
  let mut present = Vec::new();
  let mut resolved = Vec::new();
  let mut misfiled = Vec::new();
  for (from, to, becomes) in latent {
    if !edge_set.contains(&(*from, *to)) {
      resolved.push((from.to_string(), to.to_string()));
    } else if let Some(law) = classify_arch_edge(from, to, runtimes, surfaces) {
      misfiled.push((from.to_string(), to.to_string(), law));
    } else {
      present.push((from.to_string(), to.to_string(), *becomes));
    }
  }
  LatentEval {
    present,
    resolved,
    misfiled,
  }
}

/// Read the internal (workspace-member) dependencies declared by `manifest`.
/// Considers `[dependencies]` + `[build-dependencies]`; resolves renamed deps
/// via their `package = "..."` key. `[dev-dependencies]` are excluded by
/// design — they are test-only and do not shape the shipped graph.
fn read_internal_deps(manifest: &Path, members: &BTreeSet<String>) -> Result<Vec<String>> {
  let content = std::fs::read_to_string(manifest)
    .with_context(|| format!("Failed to read {}", manifest.display()))?;
  let parsed: toml::Value =
    toml::from_str(&content).with_context(|| format!("Failed to parse {}", manifest.display()))?;
  let mut deps: BTreeSet<String> = BTreeSet::new();
  for table in ["dependencies", "build-dependencies"] {
    let Some(tbl) = parsed.get(table).and_then(|t| t.as_table()) else {
      continue;
    };
    for (key, value) in tbl {
      // `foo = { package = "agentflow-x" }` renames resolve to the real crate.
      let crate_name = value
        .as_table()
        .and_then(|t| t.get("package"))
        .and_then(|p| p.as_str())
        .unwrap_or(key.as_str());
      if members.contains(crate_name) {
        deps.insert(crate_name.to_string());
      }
    }
  }
  Ok(deps.into_iter().collect())
}

/// Build the internal dependency edge list for the whole workspace.
fn collect_arch_edges(workspace_root: &Path) -> Result<Vec<(String, String)>> {
  let members = read_workspace_members(workspace_root)?;
  let member_set: BTreeSet<String> = members.iter().cloned().collect();
  let mut edges: Vec<(String, String)> = Vec::new();
  for member in &members {
    let manifest = workspace_root.join(member).join("Cargo.toml");
    if !manifest.exists() {
      continue;
    }
    for dep in read_internal_deps(&manifest, &member_set)? {
      edges.push((member.clone(), dep));
    }
  }
  edges.sort();
  edges.dedup();
  Ok(edges)
}

/// Run the architecture-law gate against `workspace_root` and report through
/// the caller-supplied sinks. Returns `Ok(())` only when there are zero new
/// violations and zero stale allowlist entries.
fn check_arch_at(workspace_root: &Path, out: &mut impl Write, err: &mut impl Write) -> Result<()> {
  let members = read_workspace_members(workspace_root)?;
  let edges = collect_arch_edges(workspace_root)?;

  let allow_pairs: Vec<(&str, &str)> = ARCH_ALLOWLIST.iter().map(|a| (a.from, a.to)).collect();
  let eval = evaluate_arch(
    &edges,
    ARCH_RUNTIME_CRATES,
    ARCH_SURFACE_CRATES,
    &allow_pairs,
  );

  let latent_pairs: Vec<(&str, &str, &'static str)> = ARCH_LATENT_EDGES
    .iter()
    .map(|l| (l.from, l.to, l.becomes))
    .collect();
  let latent = evaluate_latent(
    &edges,
    &latent_pairs,
    ARCH_RUNTIME_CRATES,
    ARCH_SURFACE_CRATES,
  );

  writeln!(
    out,
    "check-arch: {} member(s), {} internal edge(s), 2 active law(s)",
    members.len(),
    edges.len()
  )?;
  writeln!(
    out,
    "check-arch: {} tracked (allowlisted), {} new, {} stale allowlist entr(ies)",
    eval.tracked.len(),
    eval.new.len(),
    eval.stale.len()
  )?;
  for (from, to, law) in &eval.tracked {
    writeln!(out, "  · tracked: {from} -> {to} breaks {law}")?;
  }

  // Latent target-state map (P-A0.4): informational until each contract-tier
  // law is activated; the repoint checklist for the kernel migration.
  writeln!(
    out,
    "check-arch: {} latent target-state edge(s) (not yet enforced; see docs/ARCHITECTURE_EVALUATION_2026-06-20.md §2)",
    latent.present.len()
  )?;
  for (from, to, becomes) in &latent.present {
    writeln!(out, "  ◦ latent: {from} -> {to} will break {becomes}")?;
  }

  if eval.new.is_empty()
    && eval.stale.is_empty()
    && latent.resolved.is_empty()
    && latent.misfiled.is_empty()
  {
    writeln!(out, "check-arch: OK")?;
    return Ok(());
  }

  writeln!(err, "check-arch: FAIL")?;
  for (from, to, law) in &eval.new {
    writeln!(
      err,
      "  ✗ NEW violation: {from} -> {to} breaks {law} — fix it or add to ARCH_ALLOWLIST with a burndown task"
    )?;
  }
  for (from, to) in &eval.stale {
    let note = ARCH_ALLOWLIST
      .iter()
      .find(|a| a.from == from && a.to == to)
      .map(|a| a.burndown)
      .unwrap_or("(no burndown recorded)");
    writeln!(
      err,
      "  ✗ STALE allowlist: {from} -> {to} no longer violates — remove it from ARCH_ALLOWLIST (burndown: {note})"
    )?;
  }
  for (from, to) in &latent.resolved {
    let note = ARCH_LATENT_EDGES
      .iter()
      .find(|l| l.from == from && l.to == to)
      .map(|l| l.burndown)
      .unwrap_or("(no burndown recorded)");
    writeln!(
      err,
      "  ✗ RESOLVED latent: {from} -> {to} edge is gone — remove it from ARCH_LATENT_EDGES (paid down: {note})"
    )?;
  }
  for (from, to, law) in &latent.misfiled {
    writeln!(
      err,
      "  ✗ MISFILED latent: {from} -> {to} now breaks {law} — move it from ARCH_LATENT_EDGES to ARCH_ALLOWLIST"
    )?;
  }
  bail!(
    "{} new, {} stale allowlist, {} resolved latent, {} misfiled latent",
    eval.new.len(),
    eval.stale.len(),
    latent.resolved.len(),
    latent.misfiled.len()
  );
}

#[cfg(test)]
mod arch_tests {
  use super::*;

  fn edges(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
      .iter()
      .map(|(a, b)| (a.to_string(), b.to_string()))
      .collect()
  }

  #[test]
  fn runtime_to_runtime_is_a_new_violation() {
    let e = edges(&[("r-a", "r-b")]);
    let eval = evaluate_arch(&e, &["r-a", "r-b"], &[], &[]);
    assert_eq!(eval.new.len(), 1);
    assert_eq!(eval.tracked.len(), 0);
    assert_eq!(eval.stale.len(), 0);
    assert_eq!(eval.new[0].2, LAW_RUNTIME_ISOLATION);
  }

  #[test]
  fn allowlisted_violation_is_tracked_not_new() {
    let e = edges(&[("r-a", "r-b")]);
    let eval = evaluate_arch(&e, &["r-a", "r-b"], &[], &[("r-a", "r-b")]);
    assert_eq!(eval.new.len(), 0);
    assert_eq!(eval.tracked.len(), 1);
    assert_eq!(eval.stale.len(), 0);
  }

  #[test]
  fn surface_to_surface_is_flagged() {
    let e = edges(&[("s-a", "s-b")]);
    let eval = evaluate_arch(&e, &[], &["s-a", "s-b"], &[]);
    assert_eq!(eval.new.len(), 1);
    assert_eq!(eval.new[0].2, LAW_SURFACE_ISOLATION);
  }

  #[test]
  fn non_tier_edges_are_allowed() {
    let e = edges(&[("cap", "tool")]);
    let eval = evaluate_arch(&e, &["r-a"], &["s-a"], &[]);
    assert!(eval.new.is_empty() && eval.tracked.is_empty() && eval.stale.is_empty());
  }

  #[test]
  fn stale_allowlist_when_edge_removed() {
    // The allowlisted edge is no longer in the graph → it must be pruned.
    let eval = evaluate_arch(&[], &["r-a", "r-b"], &[], &[("r-a", "r-b")]);
    assert_eq!(eval.stale, vec![("r-a".to_string(), "r-b".to_string())]);
  }

  #[test]
  fn stale_allowlist_when_edge_no_longer_violates() {
    // Edge still present but neither endpoint is a runtime/surface → no law
    // broken, so the allowlist entry is pointless and flagged stale.
    let e = edges(&[("plain-a", "plain-b")]);
    let eval = evaluate_arch(&e, &["r-a"], &[], &[("plain-a", "plain-b")]);
    assert_eq!(eval.stale.len(), 1);
    assert_eq!(eval.new.len(), 0);
  }

  #[test]
  fn latent_edge_present_is_reported_not_failed() {
    // A latent edge that exists and breaks no *active* law is healthy: it shows
    // up in `present` and never fails the gate.
    let e = edges(&[("nodes", "llm")]);
    let l = evaluate_latent(
      &e,
      &[("nodes", "llm", "law 2 tool→capability")],
      &["r-a"],
      &["s-a"],
    );
    assert_eq!(l.present.len(), 1);
    assert!(l.resolved.is_empty() && l.misfiled.is_empty());
    assert_eq!(l.present[0].2, "law 2 tool→capability");
  }

  #[test]
  fn latent_edge_gone_is_resolved() {
    // The latent edge was paid down (dep removed) → must be pruned from the list.
    let l = evaluate_latent(&[], &[("nodes", "llm", "law 2")], &["r-a"], &["s-a"]);
    assert_eq!(l.resolved, vec![("nodes".to_string(), "llm".to_string())]);
    assert!(l.present.is_empty() && l.misfiled.is_empty());
  }

  #[test]
  fn latent_edge_that_now_violates_active_law_is_misfiled() {
    // The edge still exists but now breaks an *active* law (both endpoints are
    // runtimes) → it belongs in ARCH_ALLOWLIST, not the latent list.
    let e = edges(&[("r-a", "r-b")]);
    let l = evaluate_latent(
      &e,
      &[("r-a", "r-b", "law 4 runtime→impl")],
      &["r-a", "r-b"],
      &[],
    );
    assert_eq!(l.misfiled.len(), 1);
    assert_eq!(l.misfiled[0].2, LAW_RUNTIME_ISOLATION);
    assert!(l.present.is_empty() && l.resolved.is_empty());
  }

  #[test]
  fn read_internal_deps_resolves_members_and_excludes_dev_deps() {
    let dir = tempfile::tempdir().expect("tempdir");
    let manifest = dir.path().join("Cargo.toml");
    std::fs::write(
      &manifest,
      "[package]\nname = \"x\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n\
       [dependencies]\n\
       agentflow-core = { path = \"../agentflow-core\" }\n\
       aliased = { package = \"agentflow-tools\" }\n\
       serde = \"1\"\n\n\
       [dev-dependencies]\n\
       agentflow-llm = { path = \"../agentflow-llm\" }\n",
    )
    .expect("write manifest");
    let members: BTreeSet<String> = ["agentflow-core", "agentflow-tools", "agentflow-llm"]
      .iter()
      .map(|s| s.to_string())
      .collect();
    let deps = read_internal_deps(&manifest, &members).expect("read deps");
    assert!(deps.contains(&"agentflow-core".to_string()));
    assert!(
      deps.contains(&"agentflow-tools".to_string()),
      "rename via package= must resolve"
    );
    assert!(
      !deps.contains(&"agentflow-llm".to_string()),
      "dev-dependencies must be excluded"
    );
    assert_eq!(deps.len(), 2);
  }

  #[test]
  fn real_workspace_passes_with_current_allowlist() {
    // Self-consistency guard: the real workspace must be clean under the gate
    // with exactly the seeded allowlist. Fails CI when someone adds a NEW
    // runtime/surface cross-edge, or FIXES one without pruning the allowlist.
    let root = workspace_root();
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let result = check_arch_at(&root, &mut out, &mut err);
    assert!(
      result.is_ok(),
      "real workspace failed check-arch:\nstdout:\n{}\nstderr:\n{}",
      String::from_utf8_lossy(&out),
      String::from_utf8_lossy(&err),
    );
    let stdout = String::from_utf8(out).expect("utf8 stdout");
    assert!(stdout.contains("check-arch: OK"), "stdout:\n{stdout}");
    assert!(
      stdout.contains("0 tracked"),
      "expected ZERO tracked violations — the entire P-A runtime/surface-isolation \
       allowlist is burned down (agents->core, harness->agents, server->cli, \
       worker->server); got:\n{stdout}"
    );
    assert!(
      stdout.contains("latent target-state edge(s)"),
      "expected the latent target-state map to be reported; got:\n{stdout}"
    );
  }

  #[test]
  fn latent_map_entries_are_unique_and_distinct_from_allowlist() {
    // Guard against a latent edge being listed twice, or being duplicated in
    // both ARCH_LATENT_EDGES and ARCH_ALLOWLIST (the two lists must partition
    // the target-state edge map, not overlap).
    let mut seen: BTreeSet<(&str, &str)> = BTreeSet::new();
    for l in ARCH_LATENT_EDGES {
      assert!(
        seen.insert((l.from, l.to)),
        "duplicate latent edge: {} -> {}",
        l.from,
        l.to
      );
      assert!(
        !ARCH_ALLOWLIST
          .iter()
          .any(|a| a.from == l.from && a.to == l.to),
        "{} -> {} is in BOTH ARCH_LATENT_EDGES and ARCH_ALLOWLIST",
        l.from,
        l.to
      );
    }
  }
}

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
///   --baseline <path>   override the default baseline file
///   --threshold <ratio> override the regression ratio (default 1.25)
///   --allow-missing     don't fail when a baseline entry has no
///                       matching Criterion result (useful for CI runs
///                       that intentionally only ran a subset of benches)
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
  let baseline_text = std::fs::read_to_string(baseline_path)
    .with_context(|| format!("failed to read baseline file '{}'", baseline_path.display()))?;
  let baseline: BaselineFile = serde_json::from_str(&baseline_text).with_context(|| {
    format!(
      "baseline file '{}' is not valid bench-gate JSON",
      baseline_path.display()
    )
  })?;

  let criterion_root = pick_criterion_root(workspace_root);
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
    let result = bench_gate_at(root.path(), &baseline, 1.25, false, &mut out, &mut err);
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
    let result = bench_gate_at(root.path(), &baseline, 1.25, false, &mut out, &mut err);
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
    let _criterion = synth_workspace_for_gate(root.path());
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
    let result = bench_gate_at(root.path(), &baseline, 1.25, false, &mut out, &mut err);
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
    let _criterion = synth_workspace_for_gate(root.path());
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
    let result = bench_gate_at(root.path(), &baseline, 1.25, true, &mut out, &mut err);
    assert!(
      result.is_ok(),
      "--allow-missing should not fail: {:?}",
      result.err()
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

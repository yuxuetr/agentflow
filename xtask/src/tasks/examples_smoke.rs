//! `examples-smoke` — compile and run each SDK example from the
//! canonical matrix (`examples/README.md`) with a per-example wall-
//! clock cap; fail the workspace if any example panics or exceeds the
//! cap. Backs the P3.2 / P3.10 / P7.3 CI gate.
//!
//! The smoke list is intentionally explicit (not a filesystem walk) so a new
//! example doesn't silently enter the CI gate. Adding a row here is a
//! deliberate one-line PR; removing one likewise. Each row carries:
//!   - package: the workspace member that owns the example
//!   - example: the `<name>` to pass to `cargo run --example`
//!   - features: extra `--features` flag (empty when the default set
//!     covers it)
//!   - timeout: per-example wall-clock cap. Most demos finish well under
//!     5 s, but mock-LLM ReAct loops can spend 1-2 s per turn so a
//!     generous 30 s default is the floor.

use anyhow::{Result, bail};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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

#[cfg(test)]
mod tests {
  use super::*;
  use std::collections::BTreeSet;

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
}

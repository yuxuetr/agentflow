//! `agentflow eval run` — execute an agent eval dataset.
//!
//! See `docs/AGENT_EVAL_FORMAT.md` for the dataset format and the JSON
//! envelope this command emits. Slice 3 of `P4.4`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;

use agentflow_agents::eval::{
  AgentRuntimeFactory, CaseStatus, Dataset, EvalCase, EvalReport, EvalRunner, EvalRunnerError,
  PricingTable,
};
use agentflow_agents::react::{ReActAgent, ReActConfig};
use agentflow_agents::runtime::AgentRuntime;
use agentflow_llm::AgentFlow;
use agentflow_memory::SessionMemory;
use agentflow_skills::{SkillBuilder, SkillLoader};
use agentflow_tools::ToolRegistry;

/// `agentflow eval run <dataset>` entry point.
#[allow(clippy::too_many_arguments)]
pub async fn execute(
  dataset_dir: String,
  format: String,
  filter: Option<String>,
  fail_on_status: String,
) -> Result<()> {
  let format = parse_format(&format)?;
  let fail_threshold = parse_fail_on_status(&fail_on_status)?;

  let dataset = Dataset::load_from_dir(&dataset_dir)
    .with_context(|| format!("failed to load eval dataset from '{dataset_dir}'"))?;

  AgentFlow::init()
    .await
    .context("failed to initialise AgentFlow — is your LLM provider config set up?")?;

  let factory: Box<dyn AgentRuntimeFactory> = Box::new(ReActAgentFactory::new());
  let mut runner = EvalRunner::new(&dataset, factory.as_ref());
  if let Some(pattern) = filter.clone() {
    runner = runner.with_filter(move |case| glob_match(&pattern, &case.id));
  }
  let pricing = load_pricing_table()?;
  runner = runner.with_pricing(pricing);
  let report = runner.run().await;

  match format {
    OutputFormat::Text => print_text_report(&report),
    OutputFormat::Json => {
      let json = serde_json::to_string_pretty(&report)?;
      println!("{json}");
    }
  }

  if exceeds_fail_threshold(&report, fail_threshold) {
    // Exit 1 = at least one failure; exit 2 is reserved for dataset /
    // config errors, which are handled by anyhow's bubble-up above.
    std::process::exit(1);
  }
  Ok(())
}

#[derive(Debug, Clone, Copy)]
enum OutputFormat {
  Text,
  Json,
}

fn parse_format(value: &str) -> Result<OutputFormat> {
  match value {
    "text" => Ok(OutputFormat::Text),
    "json" => Ok(OutputFormat::Json),
    other => bail!("unsupported --format '{other}', expected 'text' or 'json'"),
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailThreshold {
  /// Exit nonzero only when at least one case is `failed` (default).
  Failed,
  /// Never exit nonzero from per-case status (CI orchestration mode).
  Never,
}

fn parse_fail_on_status(value: &str) -> Result<FailThreshold> {
  match value {
    "failed" => Ok(FailThreshold::Failed),
    "never" => Ok(FailThreshold::Never),
    other => bail!("unsupported --fail-on-status '{other}', expected 'failed' or 'never'"),
  }
}

fn exceeds_fail_threshold(report: &EvalReport, threshold: FailThreshold) -> bool {
  match threshold {
    FailThreshold::Never => false,
    FailThreshold::Failed => report.has_failures(),
  }
}

/// Glob matcher with `*` and `?` wildcards only. Mirrors the simple
/// glob semantics operators expect from `--filter` flags elsewhere in
/// the CLI (e.g. `agentflow workflow run --filter "rust-expert-*"`).
fn glob_match(pattern: &str, candidate: &str) -> bool {
  let pat: Vec<char> = pattern.chars().collect();
  let cand: Vec<char> = candidate.chars().collect();
  glob_match_inner(&pat, &cand)
}

fn glob_match_inner(pattern: &[char], candidate: &[char]) -> bool {
  // Iterative two-pointer with a star-backtrack memo. Linear-ish.
  let (mut pi, mut ci) = (0usize, 0usize);
  let (mut star, mut match_start) = (None::<usize>, 0usize);
  while ci < candidate.len() {
    if pi < pattern.len() && (pattern[pi] == '?' || pattern[pi] == candidate[ci]) {
      pi += 1;
      ci += 1;
    } else if pi < pattern.len() && pattern[pi] == '*' {
      star = Some(pi);
      match_start = ci;
      pi += 1;
    } else if let Some(sp) = star {
      pi = sp + 1;
      match_start += 1;
      ci = match_start;
    } else {
      return false;
    }
  }
  while pi < pattern.len() && pattern[pi] == '*' {
    pi += 1;
  }
  pi == pattern.len()
}

/// Resolve a [`PricingTable`] from the operator's environment.
///
/// Lookup order (first match wins):
///   1. `AGENTFLOW_PRICING_TABLE=/path/to/pricing.yml` — explicit override.
///   2. `~/.agentflow/pricing.yml` — per-host default.
///   3. Empty table — every model costs $0.
///
/// Missing files are not an error: an unconfigured host should still
/// run evals, just with cost_usd_actual = 0 across the board.
/// Malformed YAML *is* an error and short-circuits the run with a
/// structured anyhow message so operators don't silently lose cost
/// tracking they expected to have.
fn load_pricing_table() -> Result<PricingTable> {
  if let Ok(path) = std::env::var("AGENTFLOW_PRICING_TABLE") {
    let path = PathBuf::from(path);
    return PricingTable::load_from_yaml(&path)
      .with_context(|| format!("failed to load pricing table from {}", path.display()));
  }
  if let Some(home) = dirs::home_dir() {
    let default_path = home.join(".agentflow").join("pricing.yml");
    if default_path.exists() {
      return PricingTable::load_from_yaml(&default_path).with_context(|| {
        format!(
          "failed to load default pricing table from {}",
          default_path.display()
        )
      });
    }
  }
  Ok(PricingTable::empty())
}

/// Default factory: spin up a fresh bare ReActAgent per case using the
/// case-declared model. No tool registry, no skill loading in this
/// slice. Suitable for the CI mock-provider fixture; richer factories
/// (skill loading, tool admission via P3.5 flags) land alongside the
/// agent eval slot in P4.7+.
struct ReActAgentFactory;

impl ReActAgentFactory {
  fn new() -> Self {
    Self
  }
}

#[async_trait]
impl AgentRuntimeFactory for ReActAgentFactory {
  async fn build(&self, case: &EvalCase) -> Result<Box<dyn AgentRuntime>, EvalRunnerError> {
    if let Some(skill_dir) = case.skill.as_deref() {
      build_skill_agent(case, skill_dir).await
    } else {
      build_bare_agent(case)
    }
  }
}

fn build_bare_agent(case: &EvalCase) -> Result<Box<dyn AgentRuntime>, EvalRunnerError> {
  let model = case
    .model
    .clone()
    .ok_or_else(|| EvalRunnerError::FactoryFailed {
      case_id: case.id.clone(),
      message: "case has no model and dataset has no default model".to_string(),
    })?;
  let config = match case.max_steps {
    Some(n) => ReActConfig::new(&model).with_max_iterations(n),
    None => ReActConfig::new(&model),
  };
  let agent = ReActAgent::new(
    config,
    Box::new(SessionMemory::default_window()),
    Arc::new(ToolRegistry::new()),
  );
  Ok(Box::new(agent))
}

async fn build_skill_agent(
  case: &EvalCase,
  skill_dir: &str,
) -> Result<Box<dyn AgentRuntime>, EvalRunnerError> {
  let dir = Path::new(skill_dir);
  let mut manifest = SkillLoader::load(dir).map_err(|e| EvalRunnerError::FactoryFailed {
    case_id: case.id.clone(),
    message: format!("failed to load skill '{skill_dir}': {e}"),
  })?;
  if let Some(model) = case.model.clone() {
    manifest.model.name = Some(model);
  }
  let _warnings =
    SkillLoader::validate(&manifest, dir).map_err(|e| EvalRunnerError::FactoryFailed {
      case_id: case.id.clone(),
      message: format!("skill validation failed for '{skill_dir}': {e}"),
    })?;

  let admit = admission_for_case(case);
  let agent = SkillBuilder::build_with_admission(&manifest, dir, admit)
    .await
    .map_err(|e| EvalRunnerError::FactoryFailed {
      case_id: case.id.clone(),
      message: format!("failed to build skill agent for '{skill_dir}': {e}"),
    })?;
  // Note: `case.max_steps` is enforced by the runner via
  // `RuntimeLimits::max_steps` threaded into `AgentContext`, not by the
  // agent's own iteration cap. Both knobs apply; the lower one wins.
  Ok(Box::new(agent))
}

/// Build the per-case admission closure.
///
/// Precedence mirrors the P3.5 / P1.9 CLI override layer applied at the
/// case scope:
///
/// - Denied tools always lose.
/// - When `tools_allowed` is non-empty, only its members are admitted.
/// - When `tools_allowed` is empty, every non-denied tool the skill
///   declared is admitted (default open).
fn admission_for_case(case: &EvalCase) -> impl Fn(&str) -> bool + '_ {
  let denied: HashSet<&str> = case.tools_denied.iter().map(String::as_str).collect();
  let allowed: HashSet<&str> = case.tools_allowed.iter().map(String::as_str).collect();
  move |name: &str| -> bool {
    if denied.contains(name) {
      return false;
    }
    if allowed.is_empty() {
      return true;
    }
    allowed.contains(name)
  }
}

fn print_text_report(report: &EvalReport) {
  println!("Dataset: {} v{}", report.dataset, report.dataset_version);
  println!(
    "Started:  {}\nFinished: {}",
    report.started_at, report.finished_at
  );
  let s = &report.summary;
  println!(
    "Summary:  {} total, {} passed, {} failed, {} skipped",
    s.total, s.passed, s.failed, s.skipped
  );
  println!(
    "Latency:  p50 {} ms, p95 {} ms",
    s.latency_ms_p50, s.latency_ms_p95
  );
  println!("Cost:     ${:.4} total", s.cost_usd_total);
  println!();
  for case in &report.cases {
    println!(
      "  {symbol} {id}  [{status}]  {dur} ms  ({stop})",
      symbol = status_symbol(case.status),
      id = case.id,
      status = case.status.as_str(),
      dur = case.duration_ms,
      stop = case.stop_reason
    );
    if let Some(trace) = &case.trace_id {
      println!("       trace: {trace}");
    }
    if matches!(case.status, CaseStatus::Failed) {
      for outcome in case.assertions.iter().filter(|o| !o.passed) {
        let reason = outcome
          .reason
          .clone()
          .unwrap_or_else(|| "(no reason recorded)".to_string());
        println!("       ✗ {}", reason);
      }
      if let Some(err) = &case.runtime_error {
        println!("       runtime_error: {err}");
      }
    }
  }
}

fn status_symbol(s: CaseStatus) -> char {
  match s {
    CaseStatus::Passed => '✓',
    CaseStatus::Failed => '✗',
    CaseStatus::Skipped => '·',
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn glob_match_handles_star_prefix_and_suffix() {
    assert!(glob_match("rust-*", "rust-expert-001"));
    assert!(glob_match("*-001", "rust-expert-001"));
    assert!(glob_match("*expert*", "rust-expert-001"));
    assert!(!glob_match("python-*", "rust-expert-001"));
  }

  #[test]
  fn glob_match_handles_question_mark() {
    assert!(glob_match("c?se", "case"));
    assert!(!glob_match("c?se", "cse"));
  }

  #[test]
  fn glob_match_exact_string_no_wildcards() {
    assert!(glob_match("exact", "exact"));
    assert!(!glob_match("exact", "other"));
  }

  #[test]
  fn fail_threshold_failed_trips_only_when_failures_present() {
    let mut report = EvalReport {
      schema_version: 1,
      dataset: "d".to_string(),
      dataset_version: "0".to_string(),
      started_at: chrono::Utc::now(),
      finished_at: chrono::Utc::now(),
      summary: Default::default(),
      cases: Vec::new(),
    };
    assert!(!exceeds_fail_threshold(&report, FailThreshold::Failed));
    report.summary.failed = 1;
    assert!(exceeds_fail_threshold(&report, FailThreshold::Failed));
    assert!(!exceeds_fail_threshold(&report, FailThreshold::Never));
  }

  #[test]
  fn parse_format_round_trip() {
    assert!(matches!(parse_format("text"), Ok(OutputFormat::Text)));
    assert!(matches!(parse_format("json"), Ok(OutputFormat::Json)));
    assert!(parse_format("xml").is_err());
  }
}

//! `agentflow rag eval` — run the RAG eval harness over a dataset directory.
//!
//! The CLI is deliberately scoped to the offline BM25 retriever for the v0.4.0
//! milestone: it requires no external services, runs on every CI host, and
//! produces a deterministic report. Vector / hybrid retrievers can plug in
//! later via additional `--retriever` values.

use agentflow_rag::eval::{
  Bm25Eval, ComparisonReport, Dataset, EvalConfig, EvalReport, compare, evaluate,
};
use anyhow::{Context, Result, bail};
use colored::Colorize;
use serde_json::{Value, json};
use std::path::PathBuf;

/// Default thresholds used by the regression gate when `--compare-baseline`
/// is supplied. Both must trip together to flag a regression — matches the
/// P4.2 spec (`p < 0.05 AND ≥3% absolute drop in Recall@5`).
const DEFAULT_REGRESSION_RECALL_DROP: f64 = 0.03;
const DEFAULT_REGRESSION_P_VALUE: f64 = 0.05;
const REGRESSION_RECALL_K: usize = 5;

/// Execute the `rag eval` command.
///
/// `k_values` defaults to `[1, 3, 5, 10]` when empty. `retriever` controls the
/// backend (currently only `"bm25"`). When `compare_to` is `Some`, the CLI runs
/// a second BM25 eval with custom params parsed from the form `k1=1.5,b=0.6`
/// and emits a paired comparison report.
#[allow(clippy::too_many_arguments)]
pub async fn execute(
  dataset_dir: PathBuf,
  retriever: String,
  k_values: Vec<usize>,
  compare_to: Option<String>,
  compare_baseline: Option<PathBuf>,
  regression_recall_threshold: Option<f64>,
  regression_p_value: Option<f64>,
  output: Option<PathBuf>,
  format: String,
) -> Result<()> {
  let is_envelope = format == "json-envelope";

  if compare_to.is_some() && compare_baseline.is_some() {
    bail!("--compare-to and --compare-baseline are mutually exclusive — pick one");
  }

  let dataset = Dataset::load_from_dir(&dataset_dir)
    .with_context(|| format!("loading dataset from {}", dataset_dir.display()))?;

  if !is_envelope {
    println!(
      "{}",
      format!("Loaded dataset: {}", dataset_dir.display())
        .bold()
        .blue()
    );
    if let Some(name) = &dataset.manifest.name {
      println!("  manifest: name={}", name);
    }
    println!(
      "  corpus={} queries={} judgments={}",
      dataset.corpus.len(),
      dataset.queries.len(),
      dataset.judgments.len()
    );
  }

  let k_values = if k_values.is_empty() {
    vec![1usize, 3, 5, 10]
  } else {
    k_values
  };

  let baseline_report = run_eval(&dataset, &retriever, &k_values, "baseline")?;
  if !is_envelope {
    println!();
    println!("{}", baseline_report.render_table());
  }

  let mut comparison: Option<ComparisonReport> = None;
  let mut candidate_report: Option<EvalReport> = None;
  let mut regression_decision: Option<RegressionDecision> = None;
  if let Some(spec) = &compare_to {
    let candidate = run_compare_candidate(&dataset, spec, &k_values)?;
    let cmp = compare(&baseline_report, &candidate);
    if !is_envelope {
      println!();
      println!("{}", candidate.render_table());
      println!();
      println!("{}", cmp.render_table().bold());
    }
    candidate_report = Some(candidate);
    comparison = Some(cmp);
  } else if let Some(path) = &compare_baseline {
    // `--compare-baseline <path>` swaps the roles: the on-disk
    // snapshot is the baseline; the freshly-computed report is the
    // candidate. We then check the regression criteria.
    let stored_baseline = load_baseline_from_path(path)?;
    let candidate = baseline_report.clone();
    let cmp = compare(&stored_baseline, &candidate);
    let decision = evaluate_regression(
      &cmp,
      regression_recall_threshold.unwrap_or(DEFAULT_REGRESSION_RECALL_DROP),
      regression_p_value.unwrap_or(DEFAULT_REGRESSION_P_VALUE),
    );
    if !is_envelope {
      println!();
      println!(
        "{}",
        format!("Comparing against baseline: {}", path.display())
          .bold()
          .yellow()
      );
      println!();
      println!("{}", cmp.render_table().bold());
      print_regression_decision(&decision);
    }
    candidate_report = Some(candidate);
    comparison = Some(cmp);
    regression_decision = Some(decision);
  }

  // Build the payload once so file + envelope outputs share the same body.
  let regression_json: Value = match &regression_decision {
    Some(d) => json!({
      "regression_detected": d.regression_detected,
      "reason": d.reason,
      "recall_at_k_drop": d.recall_at_k_drop,
      "p_value": d.p_value,
      "threshold_recall_drop": d.threshold_recall_drop,
      "threshold_p_value": d.threshold_p_value,
    }),
    None => Value::Null,
  };
  let report_payload = json!({
    "dataset": {
      "path": dataset_dir.display().to_string(),
      "manifest": {
        "name": dataset.manifest.name,
        "version": dataset.manifest.version,
        "source": dataset.manifest.source,
        "license": dataset.manifest.license,
      },
      "corpus_size": dataset.corpus.len(),
      "queries": dataset.queries.len(),
      "judgments": dataset.judgments.len(),
    },
    "baseline": baseline_report,
    "candidate": candidate_report,
    "comparison": comparison,
    "regression": regression_json,
  });

  if let Some(path) = output {
    std::fs::write(&path, serde_json::to_string_pretty(&report_payload)?)
      .with_context(|| format!("writing report to {}", path.display()))?;
    if !is_envelope {
      println!(
        "{}",
        format!("Report written to {}", path.display()).green()
      );
    }
  }

  if is_envelope {
    // P3.3 migration: wrap the same payload `--output` writes in
    // the canonical envelope. Regression detection surfaces via
    // `errors[]` so shell consumers can `jq '.errors[]'` without
    // walking `result.regression.regression_detected`.
    let errors: Vec<String> = match &regression_decision {
      Some(d) if d.regression_detected => vec![format!(
        "regression detected: {} (recall_drop={:?}, p_value={:?})",
        d.reason, d.recall_at_k_drop, d.p_value
      )],
      _ => Vec::new(),
    };
    let envelope = crate::json_envelope::CliJsonEnvelope::with_errors(
      "rag eval",
      &report_payload,
      errors,
    );
    println!("{}", serde_json::to_string_pretty(&envelope)?);
  }

  // Exit nonzero when the regression gate flagged a real regression so
  // CI fails the release gate. Other comparison outcomes (e.g.
  // candidate wins, inconclusive) keep exit 0.
  if let Some(decision) = &regression_decision
    && decision.regression_detected
  {
    std::process::exit(1);
  }
  Ok(())
}

#[derive(Debug, Clone)]
struct RegressionDecision {
  regression_detected: bool,
  reason: String,
  recall_at_k_drop: Option<f64>,
  p_value: Option<f64>,
  threshold_recall_drop: f64,
  threshold_p_value: f64,
}

/// Load a stored `EvalReport` (baseline snapshot) from disk. The file
/// is expected to be the same JSON shape `--output` writes inside its
/// `"baseline"` block — produced once by an earlier run and checked in
/// under `agentflow-rag/eval_baselines/<dataset>/<retriever>.json`.
fn load_baseline_from_path(path: &PathBuf) -> Result<EvalReport> {
  let raw = std::fs::read_to_string(path)
    .with_context(|| format!("reading baseline snapshot at {}", path.display()))?;
  let report: EvalReport = serde_json::from_str(&raw)
    .with_context(|| format!("parsing baseline snapshot at {}", path.display()))?;
  Ok(report)
}

/// Apply the regression criteria: BOTH the recall-at-K absolute drop
/// AND the paired sign-test p-value must trip together. Quality
/// signals each on their own (one big drop, or a slight statistical
/// dip) are informative but not release-blocking — only the joint
/// hit is.
fn evaluate_regression(
  cmp: &ComparisonReport,
  threshold_recall_drop: f64,
  threshold_p_value: f64,
) -> RegressionDecision {
  let metric_key = format!("Recall@{REGRESSION_RECALL_K}");
  let recall_at_k_drop = cmp
    .deltas
    .iter()
    .find(|d| d.metric == metric_key)
    .map(|d| -d.abs_delta); // positive = candidate dropped vs baseline

  let p_value = cmp.paired_sign_p_value;

  let recall_tripped = recall_at_k_drop.is_some_and(|drop| drop >= threshold_recall_drop);
  let p_tripped = p_value.is_some_and(|p| p < threshold_p_value);
  let regression_detected = recall_tripped && p_tripped;

  let reason = if regression_detected {
    format!(
      "regression: Recall@{REGRESSION_RECALL_K} dropped by {drop:.4} (≥{threshold_recall_drop:.4}) AND p-value {p:.4} < {threshold_p_value:.4}",
      drop = recall_at_k_drop.unwrap_or(0.0),
      p = p_value.unwrap_or(0.0),
    )
  } else if recall_tripped {
    format!(
      "recall drop ≥ threshold but p-value {p:?} not significant",
      p = p_value
    )
  } else if p_tripped {
    format!(
      "p-value significant but recall drop {drop:?} below threshold",
      drop = recall_at_k_drop
    )
  } else {
    "no regression: neither recall nor p-value crossed the threshold".to_string()
  };

  RegressionDecision {
    regression_detected,
    reason,
    recall_at_k_drop,
    p_value,
    threshold_recall_drop,
    threshold_p_value,
  }
}

fn print_regression_decision(decision: &RegressionDecision) {
  let line = format!(
    "Regression gate ({}): {}",
    if decision.regression_detected {
      "FAIL"
    } else {
      "PASS"
    },
    decision.reason
  );
  if decision.regression_detected {
    println!("{}", line.red().bold());
  } else {
    println!("{}", line.green().bold());
  }
}

fn run_eval(
  dataset: &Dataset,
  retriever_kind: &str,
  k_values: &[usize],
  label: &str,
) -> Result<EvalReport> {
  let config = EvalConfig {
    k_values: k_values.to_vec(),
    label: label.to_string(),
  };
  match retriever_kind {
    "bm25" => {
      let retriever = Bm25Eval::from_dataset(dataset);
      evaluate(&retriever, dataset, &config).map_err(anyhow::Error::from)
    }
    other => bail!(
      "unsupported retriever `{}`. Supported: bm25 (vector/hybrid pending)",
      other
    ),
  }
}

fn run_compare_candidate(dataset: &Dataset, spec: &str, k_values: &[usize]) -> Result<EvalReport> {
  let (k1, b) = parse_bm25_params(spec)?;
  let config = EvalConfig {
    k_values: k_values.to_vec(),
    label: format!("bm25(k1={},b={})", k1, b),
  };
  let retriever = Bm25Eval::with_params(dataset, k1, b);
  evaluate(&retriever, dataset, &config).map_err(anyhow::Error::from)
}

/// Parse `--compare-to "k1=1.5,b=0.6"` into `(k1, b)`. Both keys are required;
/// missing or unrecognized keys produce a clear error rather than silently
/// defaulting.
fn parse_bm25_params(spec: &str) -> Result<(f32, f32)> {
  let mut k1: Option<f32> = None;
  let mut b: Option<f32> = None;
  for part in spec.split(',') {
    let part = part.trim();
    if part.is_empty() {
      continue;
    }
    let (key, value) = part
      .split_once('=')
      .with_context(|| format!("expected key=value, got `{}`", part))?;
    let key = key.trim();
    let value: f32 = value
      .trim()
      .parse()
      .with_context(|| format!("invalid number for `{}`: `{}`", key, value))?;
    match key {
      "k1" => k1 = Some(value),
      "b" => b = Some(value),
      other => bail!("unknown BM25 param `{}` (expected k1 or b)", other),
    }
  }
  match (k1, b) {
    (Some(k1), Some(b)) => Ok((k1, b)),
    _ => bail!("--compare-to expects both k1 and b, e.g. \"k1=1.5,b=0.6\""),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse_bm25_params_basic() {
    let (k1, b) = parse_bm25_params("k1=1.8,b=0.6").unwrap();
    assert!((k1 - 1.8).abs() < 1e-6);
    assert!((b - 0.6).abs() < 1e-6);
  }

  #[test]
  fn parse_bm25_params_rejects_missing_key() {
    assert!(parse_bm25_params("k1=1.5").is_err());
  }

  #[test]
  fn parse_bm25_params_rejects_unknown_key() {
    assert!(parse_bm25_params("k1=1.5,b=0.5,xyz=1").is_err());
  }

  #[test]
  fn parse_bm25_params_rejects_non_numeric() {
    assert!(parse_bm25_params("k1=oops,b=0.5").is_err());
  }

  // ── evaluate_regression unit tests (P4.2 gate logic) ────────────────

  use agentflow_rag::eval::compare::MetricDelta;
  use agentflow_rag::eval::{ComparisonReport, Verdict};

  fn comparison_with_recall_drop(drop_at_5: f64, p_value: Option<f64>) -> ComparisonReport {
    ComparisonReport {
      baseline_label: "baseline".to_string(),
      candidate_label: "candidate".to_string(),
      deltas: vec![MetricDelta {
        metric: "Recall@5".to_string(),
        baseline: 1.0,
        candidate: 1.0 - drop_at_5,
        abs_delta: -drop_at_5,
        rel_delta: Some(-drop_at_5),
      }],
      paired_wins: 0,
      paired_losses: 0,
      paired_ties: 0,
      verdict: Verdict::Inconclusive,
      verdict_reason: "fixture".to_string(),
      paired_sign_p_value: p_value,
    }
  }

  #[test]
  fn evaluate_regression_flags_both_recall_drop_and_p_value() {
    // 5% drop + p = 0.01 → both criteria trip → regression
    let cmp = comparison_with_recall_drop(0.05, Some(0.01));
    let d = evaluate_regression(&cmp, 0.03, 0.05);
    assert!(d.regression_detected, "reason: {}", d.reason);
    assert!(d.reason.contains("regression"));
  }

  #[test]
  fn evaluate_regression_skips_when_only_recall_dropped_but_not_significant() {
    // 5% drop, p = 0.20 → recall trips but p doesn't → no regression
    let cmp = comparison_with_recall_drop(0.05, Some(0.20));
    let d = evaluate_regression(&cmp, 0.03, 0.05);
    assert!(!d.regression_detected);
    assert!(d.reason.contains("not significant"), "reason: {}", d.reason);
  }

  #[test]
  fn evaluate_regression_skips_when_only_p_value_significant_but_recall_unchanged() {
    // Tiny drop (0.005 < 0.03), p = 0.01 → p trips but recall
    // doesn't → no regression
    let cmp = comparison_with_recall_drop(0.005, Some(0.01));
    let d = evaluate_regression(&cmp, 0.03, 0.05);
    assert!(!d.regression_detected);
    assert!(d.reason.contains("below threshold"), "reason: {}", d.reason);
  }

  #[test]
  fn evaluate_regression_skips_when_neither_criterion_trips() {
    let cmp = comparison_with_recall_drop(0.0, Some(0.50));
    let d = evaluate_regression(&cmp, 0.03, 0.05);
    assert!(!d.regression_detected);
    assert!(d.reason.contains("no regression"));
  }

  #[test]
  fn evaluate_regression_skips_when_p_value_missing() {
    let cmp = comparison_with_recall_drop(0.05, None);
    let d = evaluate_regression(&cmp, 0.03, 0.05);
    assert!(!d.regression_detected, "no p-value should not auto-fail");
  }

  #[test]
  fn evaluate_regression_skips_when_recall_metric_missing_from_comparison() {
    // ComparisonReport with no Recall@5 delta — happens when the
    // baseline EvalReport doesn't include k=5 in per_k. Gate should
    // not trip.
    let cmp = ComparisonReport {
      baseline_label: "baseline".to_string(),
      candidate_label: "candidate".to_string(),
      deltas: vec![],
      paired_wins: 0,
      paired_losses: 10,
      paired_ties: 0,
      verdict: Verdict::BaselineWins,
      verdict_reason: "fixture".to_string(),
      paired_sign_p_value: Some(0.001),
    };
    let d = evaluate_regression(&cmp, 0.03, 0.05);
    assert!(!d.regression_detected);
  }

  #[test]
  fn evaluate_regression_honors_custom_thresholds() {
    // 4% drop + p = 0.04. Default thresholds (3% / 0.05) → trips.
    // Stricter thresholds (5% / 0.01) → doesn't trip.
    let cmp = comparison_with_recall_drop(0.04, Some(0.04));
    assert!(evaluate_regression(&cmp, 0.03, 0.05).regression_detected);
    assert!(!evaluate_regression(&cmp, 0.05, 0.01).regression_detected);
  }
}

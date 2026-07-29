//! Baseline regression comparison for agent eval reports (T2.1).
//!
//! Mirrors the `agentflow rag eval --compare-baseline` CI gate
//! (`.github/workflows/quality.yml::rag-eval-smoke`) at the shape level
//! — run a small, cheap dataset, compare summary metrics against a
//! checked-in baseline, fail on regression — but with a tolerance-based
//! comparison rather than RAG eval's paired significance test. RAG
//! eval's `p < 0.05 AND ≥3% recall drop` gate exists because Recall@K is
//! a per-query distribution large enough to test for statistical
//! significance; a smoke-scale agent eval dataset (a handful of cases)
//! doesn't have that per-query granularity, so a direct tolerance
//! comparison is the appropriate — and simpler — tool here.

use serde::{Deserialize, Serialize};

use super::runner::{CaseStatus, EvalReport};

/// Summary metrics checked into the repo alongside a dataset, consumed
/// by `agentflow eval run --compare-baseline <path>`. Regenerate via
/// `agentflow eval run <dataset> --dump-baseline <path>` after a
/// deliberate change to the dataset, prompts, or agent behavior.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EvalBaseline {
  /// `summary.passed / summary.total` (0.0 when `total == 0`).
  pub success_rate: f64,
  /// Mean `step_count` over non-skipped cases (0.0 when there are none).
  pub avg_step_count: f64,
  /// Mean `tool_call_count` over non-skipped cases.
  pub avg_tool_call_count: f64,
}

impl EvalBaseline {
  /// Compute the baseline metrics from a fresh [`EvalReport`]. All
  /// three metrics — including `success_rate` — are computed over
  /// non-skipped cases only, so an invocation with a `--filter` applied
  /// (or a dataset that grows a new, still-`Skipped`-by-default case)
  /// doesn't dilute the rate with cases that were never actually run.
  pub fn from_report(report: &EvalReport) -> Self {
    let counted: Vec<&super::runner::CaseReport> = report
      .cases
      .iter()
      .filter(|case| case.status != CaseStatus::Skipped)
      .collect();
    if counted.is_empty() {
      return Self {
        success_rate: 0.0,
        avg_step_count: 0.0,
        avg_tool_call_count: 0.0,
      };
    }
    let n = counted.len() as f64;
    let passed = counted
      .iter()
      .filter(|c| c.status == CaseStatus::Passed)
      .count();
    Self {
      success_rate: passed as f64 / n,
      avg_step_count: counted.iter().map(|c| c.step_count as f64).sum::<f64>() / n,
      avg_tool_call_count: counted
        .iter()
        .map(|c| c.tool_call_count as f64)
        .sum::<f64>()
        / n,
    }
  }
}

/// Tolerance applied when comparing a fresh report against a checked-in
/// baseline. Every check is regression-only: a metric moving in the
/// *favorable* direction (higher success rate, fewer steps/tool calls)
/// never trips the gate, no matter by how much.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BaselineTolerance {
  /// Maximum allowed absolute drop in success rate.
  pub success_rate_drop: f64,
  /// Maximum allowed absolute increase in average step count.
  pub avg_step_count_increase: f64,
  /// Maximum allowed absolute increase in average tool-call count.
  pub avg_tool_call_count_increase: f64,
}

impl Default for BaselineTolerance {
  /// Zero tolerance on success rate (a smoke dataset this small should
  /// be fully deterministic against a pinned mock provider — any drop
  /// is a real regression, not noise); a small allowance on step/
  /// tool-call counts absorbs incidental +/-1 changes from harmless
  /// prompt-formatting tweaks without masking a genuine behavior change.
  fn default() -> Self {
    Self {
      success_rate_drop: 0.0,
      avg_step_count_increase: 1.0,
      avg_tool_call_count_increase: 1.0,
    }
  }
}

/// Result of comparing a fresh report's metrics against a baseline.
#[derive(Debug, Clone, PartialEq)]
pub struct BaselineComparison {
  pub baseline: EvalBaseline,
  pub actual: EvalBaseline,
  /// Human-readable description of each metric that regressed beyond
  /// tolerance. Empty means no regression.
  pub violations: Vec<String>,
}

impl BaselineComparison {
  /// True when at least one metric regressed beyond its tolerance.
  pub fn regressed(&self) -> bool {
    !self.violations.is_empty()
  }
}

/// Compare `report`'s metrics against `baseline` under `tolerance`.
pub fn compare_against_baseline(
  report: &EvalReport,
  baseline: EvalBaseline,
  tolerance: BaselineTolerance,
) -> BaselineComparison {
  let actual = EvalBaseline::from_report(report);
  let mut violations = Vec::new();

  let success_rate_drop = baseline.success_rate - actual.success_rate;
  if success_rate_drop > tolerance.success_rate_drop {
    violations.push(format!(
      "success_rate regressed: baseline={:.3} actual={:.3} (drop {:.3} exceeds tolerance {:.3})",
      baseline.success_rate, actual.success_rate, success_rate_drop, tolerance.success_rate_drop
    ));
  }

  let step_increase = actual.avg_step_count - baseline.avg_step_count;
  if step_increase > tolerance.avg_step_count_increase {
    violations.push(format!(
      "avg_step_count regressed: baseline={:.2} actual={:.2} (increase {:.2} exceeds tolerance {:.2})",
      baseline.avg_step_count, actual.avg_step_count, step_increase, tolerance.avg_step_count_increase
    ));
  }

  let tool_call_increase = actual.avg_tool_call_count - baseline.avg_tool_call_count;
  if tool_call_increase > tolerance.avg_tool_call_count_increase {
    violations.push(format!(
      "avg_tool_call_count regressed: baseline={:.2} actual={:.2} (increase {:.2} exceeds tolerance {:.2})",
      baseline.avg_tool_call_count,
      actual.avg_tool_call_count,
      tool_call_increase,
      tolerance.avg_tool_call_count_increase
    ));
  }

  BaselineComparison {
    baseline,
    actual,
    violations,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::eval::runner::{CaseReport, EvalSummary};
  use chrono::Utc;

  fn case(status: CaseStatus, step_count: usize, tool_call_count: usize) -> CaseReport {
    let now = Utc::now();
    CaseReport {
      id: "c".to_string(),
      status,
      trace_id: None,
      started_at: now,
      finished_at: now,
      duration_ms: 0,
      cost_usd_actual: 0.0,
      stop_reason: "final_answer".to_string(),
      step_count,
      tool_call_count,
      assertions: Vec::new(),
      notes: None,
      runtime_error: None,
    }
  }

  fn report(cases: Vec<CaseReport>) -> EvalReport {
    let passed = cases
      .iter()
      .filter(|c| c.status == CaseStatus::Passed)
      .count();
    let failed = cases
      .iter()
      .filter(|c| c.status == CaseStatus::Failed)
      .count();
    let skipped = cases
      .iter()
      .filter(|c| c.status == CaseStatus::Skipped)
      .count();
    let now = Utc::now();
    EvalReport {
      schema_version: 1,
      dataset: "test".to_string(),
      dataset_version: "0.0.1".to_string(),
      started_at: now,
      finished_at: now,
      summary: EvalSummary {
        total: cases.len(),
        passed,
        failed,
        skipped,
        cost_usd_total: 0.0,
        latency_ms_p50: 0,
        latency_ms_p95: 0,
      },
      cases,
    }
  }

  #[test]
  fn from_report_computes_success_rate_and_averages_over_non_skipped_cases() {
    let r = report(vec![
      case(CaseStatus::Passed, 2, 0),
      case(CaseStatus::Passed, 4, 2),
      case(CaseStatus::Skipped, 100, 100), // must not pollute the averages
    ]);
    let baseline = EvalBaseline::from_report(&r);
    assert_eq!(baseline.success_rate, 1.0);
    assert_eq!(baseline.avg_step_count, 3.0);
    assert_eq!(baseline.avg_tool_call_count, 1.0);
  }

  #[test]
  fn from_report_handles_empty_report_without_dividing_by_zero() {
    let r = report(vec![]);
    let baseline = EvalBaseline::from_report(&r);
    assert_eq!(baseline.success_rate, 0.0);
    assert_eq!(baseline.avg_step_count, 0.0);
    assert_eq!(baseline.avg_tool_call_count, 0.0);
  }

  #[test]
  fn compare_passes_when_actual_matches_baseline_exactly() {
    let r = report(vec![case(CaseStatus::Passed, 3, 1)]);
    let baseline = EvalBaseline::from_report(&r);
    let cmp = compare_against_baseline(&r, baseline, BaselineTolerance::default());
    assert!(!cmp.regressed(), "violations: {:?}", cmp.violations);
  }

  #[test]
  fn compare_flags_success_rate_regression_with_zero_tolerance() {
    let baseline = EvalBaseline {
      success_rate: 1.0,
      avg_step_count: 3.0,
      avg_tool_call_count: 1.0,
    };
    let r = report(vec![
      case(CaseStatus::Passed, 3, 1),
      case(CaseStatus::Failed, 3, 1),
    ]);
    let cmp = compare_against_baseline(&r, baseline, BaselineTolerance::default());
    assert!(cmp.regressed());
    assert!(cmp.violations.iter().any(|v| v.contains("success_rate")));
  }

  #[test]
  fn compare_ignores_improvements_no_matter_how_large() {
    let baseline = EvalBaseline {
      success_rate: 0.5,
      avg_step_count: 10.0,
      avg_tool_call_count: 5.0,
    };
    // Actual is strictly better on every metric.
    let r = report(vec![
      case(CaseStatus::Passed, 1, 0),
      case(CaseStatus::Passed, 1, 0),
    ]);
    let cmp = compare_against_baseline(&r, baseline, BaselineTolerance::default());
    assert!(!cmp.regressed(), "violations: {:?}", cmp.violations);
  }

  #[test]
  fn compare_flags_step_count_regression_beyond_tolerance() {
    let baseline = EvalBaseline {
      success_rate: 1.0,
      avg_step_count: 3.0,
      avg_tool_call_count: 1.0,
    };
    // +2 steps exceeds the default 1.0 tolerance.
    let r = report(vec![case(CaseStatus::Passed, 5, 1)]);
    let cmp = compare_against_baseline(&r, baseline, BaselineTolerance::default());
    assert!(cmp.regressed());
    assert!(cmp.violations.iter().any(|v| v.contains("avg_step_count")));
  }

  #[test]
  fn compare_allows_step_count_drift_within_tolerance() {
    let baseline = EvalBaseline {
      success_rate: 1.0,
      avg_step_count: 3.0,
      avg_tool_call_count: 1.0,
    };
    // +1 step is within the default 1.0 tolerance (not strictly greater).
    let r = report(vec![case(CaseStatus::Passed, 4, 1)]);
    let cmp = compare_against_baseline(&r, baseline, BaselineTolerance::default());
    assert!(!cmp.regressed(), "violations: {:?}", cmp.violations);
  }
}

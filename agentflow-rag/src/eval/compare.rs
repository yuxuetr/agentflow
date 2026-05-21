//! Baseline comparison between two [`EvalReport`]s.
//!
//! Compares a *candidate* run against a *baseline* run on the **same dataset**
//! and emits per-metric deltas plus a directional verdict. The verdict uses a
//! lightweight sign test on per-query reciprocal rank — full statistical
//! testing is out of scope for the harness, but a sign test is enough to
//! catch the obvious case of "candidate beats baseline on more queries than
//! it loses on" and warns when paired data is missing.

use super::runner::{EvalReport, PerQueryRow};
use serde::{Deserialize, Serialize};

/// Per-metric absolute and relative delta between candidate and baseline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDelta {
  pub metric: String,
  pub baseline: f64,
  pub candidate: f64,
  pub abs_delta: f64,
  /// `(candidate - baseline) / baseline`. `None` when baseline is 0.
  pub rel_delta: Option<f64>,
}

/// Directional verdict from the paired sign test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
  /// Candidate strictly beats baseline on the chosen tiebreak metric.
  CandidateWins,
  /// Baseline strictly beats candidate.
  BaselineWins,
  /// No clear winner from the sign test (either tied or below threshold).
  Inconclusive,
  /// Reports could not be compared (different datasets, missing per-query
  /// data, …).
  NotComparable { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonReport {
  pub baseline_label: String,
  pub candidate_label: String,
  pub deltas: Vec<MetricDelta>,
  pub paired_wins: usize,
  pub paired_losses: usize,
  pub paired_ties: usize,
  pub verdict: Verdict,
  pub verdict_reason: String,
  /// One-tailed paired sign-test p-value for the hypothesis
  /// "candidate is worse than baseline". Computed as
  /// `P(X ≤ paired_wins | X ~ Binomial(paired_wins + paired_losses, 0.5))`
  /// — a small value means the candidate's per-query loss rate is
  /// unlikely under the null of "no difference", i.e. a regression.
  ///
  /// `None` when no paired-query data was available (Verdict ==
  /// NotComparable) or both sides tied on every query
  /// (`paired_wins + paired_losses == 0`).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub paired_sign_p_value: Option<f64>,
}

impl ComparisonReport {
  pub fn render_table(&self) -> String {
    let mut out = String::new();
    out.push_str(&format!(
      "Baseline:  {}\nCandidate: {}\n\n",
      self.baseline_label, self.candidate_label
    ));
    out.push_str(&format!(
      "{:<14} {:>10} {:>10} {:>10} {:>10}\n",
      "Metric", "Baseline", "Candidate", "Δ abs", "Δ rel"
    ));
    out.push_str("-------------- ---------- ---------- ---------- ----------\n");
    for d in &self.deltas {
      let rel = d
        .rel_delta
        .map(|x| format!("{:+.2}%", x * 100.0))
        .unwrap_or_else(|| "n/a".to_string());
      out.push_str(&format!(
        "{:<14} {:>10.4} {:>10.4} {:>+10.4} {:>10}\n",
        d.metric, d.baseline, d.candidate, d.abs_delta, rel
      ));
    }
    out.push_str(&format!(
      "\nPaired sign test (per-query reciprocal rank):\n  wins={}  losses={}  ties={}\n",
      self.paired_wins, self.paired_losses, self.paired_ties
    ));
    if let Some(p) = self.paired_sign_p_value {
      out.push_str(&format!(
        "  p-value (one-tailed, candidate worse): {:.4}\n",
        p
      ));
    }
    out.push_str(&format!(
      "Verdict:   {} — {}\n",
      verdict_label(&self.verdict),
      self.verdict_reason
    ));
    out
  }
}

fn verdict_label(verdict: &Verdict) -> &'static str {
  match verdict {
    Verdict::CandidateWins => "candidate wins",
    Verdict::BaselineWins => "baseline wins",
    Verdict::Inconclusive => "inconclusive",
    Verdict::NotComparable { .. } => "not comparable",
  }
}

/// Compare `candidate` against `baseline`. Both reports must come from the
/// same dataset (same query ids in the same order); otherwise the verdict
/// is `NotComparable`.
pub fn compare(baseline: &EvalReport, candidate: &EvalReport) -> ComparisonReport {
  let mut deltas: Vec<MetricDelta> = Vec::new();

  // Per-K metric deltas. Only emit rows for K values that exist in both reports.
  let baseline_ks: std::collections::HashSet<usize> = baseline.per_k.iter().map(|r| r.k).collect();
  let candidate_ks: std::collections::HashSet<usize> =
    candidate.per_k.iter().map(|r| r.k).collect();
  let common_ks: Vec<usize> = {
    let mut intersect: Vec<usize> = baseline_ks.intersection(&candidate_ks).copied().collect();
    intersect.sort_unstable();
    intersect
  };

  for k in &common_ks {
    let b = baseline.per_k.iter().find(|r| r.k == *k).unwrap();
    let c = candidate.per_k.iter().find(|r| r.k == *k).unwrap();
    deltas.push(metric_delta(&format!("Recall@{}", k), b.recall, c.recall));
    deltas.push(metric_delta(&format!("nDCG@{}", k), b.ndcg, c.ndcg));
  }
  deltas.push(metric_delta("MRR", baseline.mrr, candidate.mrr));
  deltas.push(metric_delta(
    "Latency (mean ms)",
    baseline.latency.mean_ms,
    candidate.latency.mean_ms,
  ));

  let pairing = pair_per_query(&baseline.per_query, &candidate.per_query);
  let (paired_wins, paired_losses, paired_ties, verdict, reason) = match pairing {
    Pairing::Mismatch(reason) => (
      0,
      0,
      0,
      Verdict::NotComparable {
        reason: reason.clone(),
      },
      reason,
    ),
    Pairing::Paired(rows) => {
      let mut wins = 0usize;
      let mut losses = 0usize;
      let mut ties = 0usize;
      for (b, c) in &rows {
        if c.reciprocal_rank > b.reciprocal_rank + 1e-9 {
          wins += 1;
        } else if c.reciprocal_rank + 1e-9 < b.reciprocal_rank {
          losses += 1;
        } else {
          ties += 1;
        }
      }
      let total_decisive = wins + losses;
      let (verdict, reason) = if total_decisive == 0 {
        (
          Verdict::Inconclusive,
          "all paired queries tied on reciprocal rank".to_string(),
        )
      } else if wins > losses && (wins as f64) / total_decisive as f64 >= 0.6 {
        (
          Verdict::CandidateWins,
          format!(
            "candidate wins on {}/{} decisive queries (≥60% threshold)",
            wins, total_decisive
          ),
        )
      } else if losses > wins && (losses as f64) / total_decisive as f64 >= 0.6 {
        (
          Verdict::BaselineWins,
          format!(
            "baseline wins on {}/{} decisive queries (≥60% threshold)",
            losses, total_decisive
          ),
        )
      } else {
        (
          Verdict::Inconclusive,
          format!(
            "win-rate {}/{} below 60% threshold; needs more queries or larger gap",
            wins, total_decisive
          ),
        )
      };
      (wins, losses, ties, verdict, reason)
    }
  };

  let paired_sign_p_value = if matches!(verdict, Verdict::NotComparable { .. }) {
    None
  } else {
    paired_sign_lower_tail_p_value(paired_wins, paired_losses)
  };

  ComparisonReport {
    baseline_label: format_label(baseline),
    candidate_label: format_label(candidate),
    deltas,
    paired_wins,
    paired_losses,
    paired_ties,
    verdict,
    verdict_reason: reason,
    paired_sign_p_value,
  }
}

/// One-tailed binomial p-value for the paired sign test asking
/// "is the candidate worse than the baseline?".
///
/// Returns `P(X ≤ wins)` where `X ~ Binomial(wins + losses, 0.5)`.
/// `None` when `wins + losses == 0` (no decisive paired queries to
/// score). All math is in log-space so the result is well-behaved
/// for `n` up to several thousand without overflow.
pub fn paired_sign_lower_tail_p_value(wins: usize, losses: usize) -> Option<f64> {
  let n = wins + losses;
  if n == 0 {
    return None;
  }
  // CDF for Binomial(n, 0.5) up to and including `wins`.
  // P(X = k) = C(n, k) * 0.5^n. We accumulate in log space, then sum
  // exp() of each term. For n ≤ a few thousand, this is fast and
  // numerically stable enough for the regression gate's purposes.
  let log_half_n = (n as f64) * (0.5f64).ln();
  let mut sum = 0.0_f64;
  for k in 0..=wins {
    let log_term = log_choose(n, k) + log_half_n;
    sum += log_term.exp();
  }
  // Clamp into [0, 1] to absorb tiny numeric drift near the
  // boundaries (e.g. wins = n).
  Some(sum.clamp(0.0, 1.0))
}

fn log_choose(n: usize, k: usize) -> f64 {
  // log(C(n, k)) via lgamma. lgamma(x + 1) = log(x!) for non-negative
  // integers; std exposes it as f64::ln_gamma_1p? No — use a
  // hand-rolled Stirling-friendly via lgamma. Rust std doesn't
  // expose lgamma; implement directly using libm-style series.
  if k > n {
    return f64::NEG_INFINITY;
  }
  // Cap k at n - k to halve the work.
  let k = k.min(n - k);
  let mut log_c = 0.0_f64;
  for i in 0..k {
    // log(n - i) - log(i + 1)
    log_c += ((n - i) as f64).ln() - ((i + 1) as f64).ln();
  }
  log_c
}

fn format_label(report: &EvalReport) -> String {
  if report.label.is_empty() {
    report.retriever.clone()
  } else {
    format!("{} [{}]", report.retriever, report.label)
  }
}

fn metric_delta(name: &str, baseline: f64, candidate: f64) -> MetricDelta {
  let abs_delta = candidate - baseline;
  let rel_delta = if baseline.abs() < f64::EPSILON {
    None
  } else {
    Some(abs_delta / baseline)
  };
  MetricDelta {
    metric: name.to_string(),
    baseline,
    candidate,
    abs_delta,
    rel_delta,
  }
}

enum Pairing<'a> {
  Paired(Vec<(&'a PerQueryRow, &'a PerQueryRow)>),
  Mismatch(String),
}

fn pair_per_query<'a>(baseline: &'a [PerQueryRow], candidate: &'a [PerQueryRow]) -> Pairing<'a> {
  if baseline.len() != candidate.len() {
    return Pairing::Mismatch(format!(
      "per-query row counts differ: baseline={} candidate={}",
      baseline.len(),
      candidate.len()
    ));
  }
  if baseline.is_empty() {
    return Pairing::Mismatch("no per-query rows to pair".to_string());
  }
  // Build a map from candidate query_id → row, then pair against baseline order.
  let by_id: std::collections::HashMap<&str, &PerQueryRow> = candidate
    .iter()
    .map(|row| (row.query_id.as_str(), row))
    .collect();
  let mut paired: Vec<(&PerQueryRow, &PerQueryRow)> = Vec::with_capacity(baseline.len());
  for b in baseline {
    let Some(c) = by_id.get(b.query_id.as_str()) else {
      return Pairing::Mismatch(format!(
        "candidate report missing query_id `{}`",
        b.query_id
      ));
    };
    paired.push((b, *c));
  }
  Pairing::Paired(paired)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::eval::metrics::LatencyAggregate;
  use crate::eval::runner::{EvalReport, PerKMetrics, PerQueryRow};

  fn make_report(label: &str, mrr: f64, per_query: Vec<(&str, f64)>) -> EvalReport {
    let per_query = per_query
      .into_iter()
      .map(|(qid, rr)| PerQueryRow {
        query_id: qid.to_string(),
        query_text: qid.to_string(),
        recall_at_k: vec![(5, if rr > 0.0 { 1.0 } else { 0.0 })],
        ndcg_at_k: vec![(5, if rr > 0.0 { 1.0 } else { 0.0 })],
        reciprocal_rank: rr,
        latency_ms: 1.0,
      })
      .collect::<Vec<_>>();
    EvalReport {
      retriever: label.to_string(),
      label: label.to_string(),
      per_k: vec![PerKMetrics {
        k: 5,
        recall: 0.5,
        ndcg: 0.5,
      }],
      mrr,
      latency: LatencyAggregate {
        mean_ms: 1.0,
        p50_ms: 1.0,
        p95_ms: 1.0,
      },
      num_queries: per_query.len(),
      queries_with_relevant: per_query.len(),
      per_query,
      chunk_size: None,
    }
  }

  #[test]
  fn compare_candidate_wins_supermajority() {
    let baseline = make_report(
      "baseline",
      0.4,
      vec![
        ("q1", 0.0),
        ("q2", 0.0),
        ("q3", 0.5),
        ("q4", 0.0),
        ("q5", 0.0),
      ],
    );
    let candidate = make_report(
      "candidate",
      0.8,
      vec![
        ("q1", 1.0),
        ("q2", 1.0),
        ("q3", 1.0),
        ("q4", 1.0),
        ("q5", 0.0),
      ],
    );
    let cmp = compare(&baseline, &candidate);
    assert_eq!(cmp.verdict, Verdict::CandidateWins);
    assert_eq!(cmp.paired_wins, 4);
  }

  #[test]
  fn compare_baseline_wins() {
    let baseline = make_report("baseline", 0.9, vec![("q1", 1.0), ("q2", 1.0), ("q3", 1.0)]);
    let candidate = make_report(
      "candidate",
      0.0,
      vec![("q1", 0.0), ("q2", 0.0), ("q3", 0.0)],
    );
    let cmp = compare(&baseline, &candidate);
    assert_eq!(cmp.verdict, Verdict::BaselineWins);
  }

  #[test]
  fn compare_inconclusive_when_close() {
    let baseline = make_report("baseline", 0.5, vec![("q1", 0.5), ("q2", 0.5)]);
    let candidate = make_report("candidate", 0.5, vec![("q1", 1.0), ("q2", 0.0)]);
    let cmp = compare(&baseline, &candidate);
    assert_eq!(cmp.verdict, Verdict::Inconclusive);
  }

  #[test]
  fn compare_not_comparable_when_query_ids_differ() {
    let baseline = make_report("baseline", 0.5, vec![("q1", 0.5)]);
    let candidate = make_report("candidate", 0.5, vec![("qX", 0.5)]);
    let cmp = compare(&baseline, &candidate);
    assert!(matches!(cmp.verdict, Verdict::NotComparable { .. }));
  }

  #[test]
  fn compare_emits_metric_deltas() {
    let baseline = make_report("baseline", 0.5, vec![("q1", 0.5)]);
    let candidate = make_report("candidate", 0.8, vec![("q1", 1.0)]);
    let cmp = compare(&baseline, &candidate);
    let mrr = cmp.deltas.iter().find(|d| d.metric == "MRR").unwrap();
    assert!((mrr.abs_delta - 0.3).abs() < 1e-9);
    assert!(mrr.rel_delta.is_some());
  }

  #[test]
  fn render_table_smoke() {
    let baseline = make_report("baseline", 0.5, vec![("q1", 0.5)]);
    let candidate = make_report("candidate", 0.8, vec![("q1", 1.0)]);
    let cmp = compare(&baseline, &candidate);
    let text = cmp.render_table();
    assert!(text.contains("MRR"));
    assert!(text.contains("Verdict"));
  }

  // ── Paired sign test p-value ────────────────────────────────────────

  #[test]
  fn paired_sign_p_value_is_none_when_all_paired_queries_tied() {
    assert_eq!(paired_sign_lower_tail_p_value(0, 0), None);
  }

  #[test]
  fn paired_sign_p_value_05_when_wins_equal_losses() {
    // n = 10, wins = 5. P(X ≤ 5 | Binomial(10, 0.5)) ≈ 0.6230 (the
    // median + a touch). The point of this test is that the helper
    // returns a value greater than 0.5 when wins equal losses, since
    // P(X ≤ median) of a symmetric distribution is just over 0.5.
    let p = paired_sign_lower_tail_p_value(5, 5).unwrap();
    assert!((p - 0.6230).abs() < 1e-3, "p = {p}");
  }

  #[test]
  fn paired_sign_p_value_small_when_losses_dominate() {
    // n = 10, wins = 1 (so 9 losses). P(X ≤ 1) = (1 + 10) / 2^10
    // = 11 / 1024 ≈ 0.01074. Solidly below 0.05; CI gate should
    // flag this as a regression.
    let p = paired_sign_lower_tail_p_value(1, 9).unwrap();
    assert!(p < 0.05, "p = {p} should be below 0.05");
    assert!((p - (11.0 / 1024.0)).abs() < 1e-6, "p = {p}");
  }

  #[test]
  fn paired_sign_p_value_near_one_when_wins_dominate() {
    // All wins → P(X ≤ n) = 1 exactly. The helper should clamp to
    // 1.0 even with tiny numeric drift.
    let p = paired_sign_lower_tail_p_value(20, 0).unwrap();
    assert!((p - 1.0).abs() < 1e-9, "p = {p}");
  }

  #[test]
  fn paired_sign_p_value_borderline_at_two_out_of_ten() {
    // n = 10, wins = 2. P(X ≤ 2) = (1 + 10 + 45) / 1024
    // = 56/1024 ≈ 0.0547 — just barely above 0.05. The classic
    // "almost significant" cutoff that proves the gate's threshold
    // is meaningful.
    let p = paired_sign_lower_tail_p_value(2, 8).unwrap();
    assert!(p > 0.05, "p = {p} should be just above 0.05");
    assert!((p - (56.0 / 1024.0)).abs() < 1e-6, "p = {p}");
  }

  #[test]
  fn compare_emits_p_value_field_when_paired_data_present() {
    let baseline = make_report("baseline", 0.5, vec![("q1", 0.5), ("q2", 0.5)]);
    let candidate = make_report("candidate", 0.5, vec![("q1", 1.0), ("q2", 1.0)]);
    let cmp = compare(&baseline, &candidate);
    assert!(
      cmp.paired_sign_p_value.is_some(),
      "p-value should be set when paired data is present"
    );
  }

  #[test]
  fn compare_omits_p_value_when_not_comparable() {
    let baseline = make_report("baseline", 0.5, vec![("q1", 0.5)]);
    let candidate = make_report("candidate", 0.5, vec![("qX", 0.5)]);
    let cmp = compare(&baseline, &candidate);
    assert!(cmp.paired_sign_p_value.is_none());
  }
}

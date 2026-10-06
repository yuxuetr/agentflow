//! Retrieval-quality metrics for the RAG eval harness.
//!
//! Metrics implemented:
//!
//! - **Recall@K**: fraction of relevant documents that appear in the top-K results.
//! - **MRR (Mean Reciprocal Rank)**: 1 / rank of the first relevant document
//!   (averaged over queries; missing → 0).
//! - **nDCG@K**: normalized Discounted Cumulative Gain. Supports graded relevance
//!   when the dataset attaches per-doc scores; defaults to binary relevance.
//!
//! Latency aggregation (mean / p50 / p95) is exposed via [`LatencyAggregate`]
//! and computed by the runner from per-query timings.

use serde::{Deserialize, Serialize};

use super::dataset::{Judgment, RelevanceScore};

/// Stable, machine-readable list of supported metric kinds. Useful when
/// drilling into a `MetricsReport` from CLI / reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricKind {
  Recall,
  Mrr,
  Ndcg,
}

/// One query's evaluation outcome — the raw inputs into per-metric averaging.
///
/// `retrieved_ids` is the ranked top-K list returned by the retriever; index 0
/// is the highest-ranked result. `judgment` is the gold annotation for this
/// query.
#[derive(Debug, Clone)]
pub struct QueryEvaluation<'a> {
  pub judgment: &'a Judgment,
  pub retrieved_ids: &'a [String],
  /// Latency in milliseconds for this single query (retrieval-only).
  pub latency_ms: f64,
}

/// Compute Recall@K for one query. K is taken from `retrieved_ids.len()` —
/// the caller is responsible for slicing the retrieval to the requested K
/// before calling this function.
///
/// Returns 0.0 when the query has no relevant docs (which is also the only
/// well-defined value — this case is excluded from the macro-average).
pub fn recall_at_k(eval: &QueryEvaluation<'_>) -> f64 {
  let total_relevant = eval.judgment.relevant_ids().count();
  if total_relevant == 0 {
    return 0.0;
  }
  let mut hits = 0usize;
  for id in eval.retrieved_ids {
    if eval.judgment.is_relevant(id) {
      hits += 1;
    }
  }
  hits as f64 / total_relevant as f64
}

/// Reciprocal Rank for one query. Returns 0.0 when no relevant doc was
/// retrieved within `retrieved_ids`.
pub fn reciprocal_rank(eval: &QueryEvaluation<'_>) -> f64 {
  for (idx, id) in eval.retrieved_ids.iter().enumerate() {
    if eval.judgment.is_relevant(id) {
      return 1.0 / (idx as f64 + 1.0);
    }
  }
  0.0
}

/// nDCG@K for one query. Uses the standard formulation:
///
/// ```text
/// DCG@K = Σ_{i=1..K} (2^rel_i - 1) / log2(i + 1)
/// ```
///
/// IDCG is computed against the ideal ranking of the judgment's relevant doc
/// set (sorted by relevance desc). Returns 0.0 when there are no relevant
/// docs (avoids 0/0).
pub fn ndcg_at_k(eval: &QueryEvaluation<'_>) -> f64 {
  let dcg = dcg(eval.retrieved_ids, eval.judgment);
  let idcg = ideal_dcg(eval.judgment, eval.retrieved_ids.len());
  if idcg == 0.0 {
    return 0.0;
  }
  dcg / idcg
}

fn dcg(retrieved_ids: &[String], judgment: &Judgment) -> f64 {
  retrieved_ids
    .iter()
    .enumerate()
    .map(|(idx, id)| {
      let rel = judgment.relevance(id) as f64;
      // 2^rel - 1 keeps binary relevance (0/1) → (0/1), but rewards graded
      // relevance scores >1 super-linearly which matches IR conventions.
      let gain = (2f64.powf(rel)) - 1.0;
      let discount = ((idx as f64) + 2.0).log2();
      gain / discount
    })
    .sum()
}

fn ideal_dcg(judgment: &Judgment, k: usize) -> f64 {
  let mut grades: Vec<RelevanceScore> = judgment.relevances().collect();
  grades.sort_unstable_by(|a, b| b.cmp(a));
  grades
    .into_iter()
    .take(k)
    .enumerate()
    .map(|(idx, rel)| {
      let gain = (2f64.powf(rel as f64)) - 1.0;
      let discount = ((idx as f64) + 2.0).log2();
      gain / discount
    })
    .sum()
}

/// Mean / median / p95 latency in milliseconds for a slice of timings.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct LatencyAggregate {
  pub mean_ms: f64,
  pub p50_ms: f64,
  pub p95_ms: f64,
}

impl LatencyAggregate {
  pub fn from_samples(samples: &[f64]) -> Self {
    if samples.is_empty() {
      return Self::default();
    }
    let mean_ms = samples.iter().sum::<f64>() / samples.len() as f64;
    let mut sorted: Vec<f64> = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p50_ms = percentile(&sorted, 0.50);
    let p95_ms = percentile(&sorted, 0.95);
    Self {
      mean_ms,
      p50_ms,
      p95_ms,
    }
  }
}

/// Linear-interpolation percentile that matches NumPy's default `linear` mode.
/// `pct` must be in [0.0, 1.0]; assumes `sorted` is already ascending.
fn percentile(sorted: &[f64], pct: f64) -> f64 {
  if sorted.is_empty() {
    return 0.0;
  }
  if sorted.len() == 1 {
    return sorted[0];
  }
  let rank = pct * (sorted.len() as f64 - 1.0);
  let lo = rank.floor() as usize;
  let hi = rank.ceil() as usize;
  if lo == hi {
    return sorted[lo];
  }
  let frac = rank - lo as f64;
  sorted[lo] * (1.0 - frac) + sorted[hi] * frac
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::eval::dataset::Judgment;
  use std::collections::HashMap;

  fn judgment_from(query_id: &str, relevant: &[(&str, RelevanceScore)]) -> Judgment {
    let mut relevances: HashMap<String, RelevanceScore> = HashMap::new();
    for (doc_id, score) in relevant {
      relevances.insert((*doc_id).to_string(), *score);
    }
    Judgment {
      query_id: query_id.to_string(),
      relevances,
      notes: None,
    }
  }

  fn eval_with(_judgment: &Judgment, retrieved: &[&str]) -> Vec<String> {
    retrieved.iter().map(|s| (*s).to_string()).collect()
  }

  #[test]
  fn recall_at_k_perfect_hit() {
    let j = judgment_from("q1", &[("d1", 1), ("d2", 1)]);
    let retrieved = eval_with(&j, &["d1", "d2", "d3"]);
    let eval = QueryEvaluation {
      judgment: &j,
      retrieved_ids: &retrieved,
      latency_ms: 0.0,
    };
    assert_eq!(recall_at_k(&eval), 1.0);
  }

  #[test]
  fn recall_at_k_partial_hit() {
    let j = judgment_from("q1", &[("d1", 1), ("d2", 1)]);
    let retrieved = eval_with(&j, &["d3", "d1", "d4"]);
    let eval = QueryEvaluation {
      judgment: &j,
      retrieved_ids: &retrieved,
      latency_ms: 0.0,
    };
    assert!((recall_at_k(&eval) - 0.5).abs() < 1e-9);
  }

  #[test]
  fn recall_at_k_no_relevant_returns_zero() {
    let j = judgment_from("q1", &[]);
    let retrieved = eval_with(&j, &["d1"]);
    let eval = QueryEvaluation {
      judgment: &j,
      retrieved_ids: &retrieved,
      latency_ms: 0.0,
    };
    assert_eq!(recall_at_k(&eval), 0.0);
  }

  #[test]
  fn reciprocal_rank_first_position() {
    let j = judgment_from("q1", &[("d1", 1)]);
    let retrieved = eval_with(&j, &["d1", "d2", "d3"]);
    let eval = QueryEvaluation {
      judgment: &j,
      retrieved_ids: &retrieved,
      latency_ms: 0.0,
    };
    assert_eq!(reciprocal_rank(&eval), 1.0);
  }

  #[test]
  fn reciprocal_rank_third_position() {
    let j = judgment_from("q1", &[("d3", 1)]);
    let retrieved = eval_with(&j, &["d1", "d2", "d3"]);
    let eval = QueryEvaluation {
      judgment: &j,
      retrieved_ids: &retrieved,
      latency_ms: 0.0,
    };
    assert!((reciprocal_rank(&eval) - 1.0 / 3.0).abs() < 1e-9);
  }

  #[test]
  fn reciprocal_rank_no_hit() {
    let j = judgment_from("q1", &[("d99", 1)]);
    let retrieved = eval_with(&j, &["d1", "d2", "d3"]);
    let eval = QueryEvaluation {
      judgment: &j,
      retrieved_ids: &retrieved,
      latency_ms: 0.0,
    };
    assert_eq!(reciprocal_rank(&eval), 0.0);
  }

  #[test]
  fn ndcg_perfect_ranking_is_one() {
    let j = judgment_from("q1", &[("d1", 3), ("d2", 2), ("d3", 1)]);
    let retrieved = eval_with(&j, &["d1", "d2", "d3"]);
    let eval = QueryEvaluation {
      judgment: &j,
      retrieved_ids: &retrieved,
      latency_ms: 0.0,
    };
    assert!((ndcg_at_k(&eval) - 1.0).abs() < 1e-9);
  }

  #[test]
  fn ndcg_reversed_ranking_lower_than_perfect() {
    let j = judgment_from("q1", &[("d1", 3), ("d2", 2), ("d3", 1)]);
    let retrieved = eval_with(&j, &["d3", "d2", "d1"]);
    let eval = QueryEvaluation {
      judgment: &j,
      retrieved_ids: &retrieved,
      latency_ms: 0.0,
    };
    let v = ndcg_at_k(&eval);
    assert!(v > 0.0);
    assert!(v < 1.0);
  }

  #[test]
  fn ndcg_no_relevant_zero() {
    let j = judgment_from("q1", &[]);
    let retrieved = eval_with(&j, &["d1"]);
    let eval = QueryEvaluation {
      judgment: &j,
      retrieved_ids: &retrieved,
      latency_ms: 0.0,
    };
    assert_eq!(ndcg_at_k(&eval), 0.0);
  }

  #[test]
  fn percentile_basic() {
    let mut sorted = [10.0, 20.0, 30.0, 40.0, 50.0];
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!((percentile(&sorted, 0.50) - 30.0).abs() < 1e-9);
    assert!((percentile(&sorted, 0.0) - 10.0).abs() < 1e-9);
    assert!((percentile(&sorted, 1.0) - 50.0).abs() < 1e-9);
  }

  #[test]
  fn latency_aggregate_basic() {
    let agg = LatencyAggregate::from_samples(&[10.0, 20.0, 30.0, 40.0, 50.0]);
    assert!((agg.mean_ms - 30.0).abs() < 1e-9);
    assert!((agg.p50_ms - 30.0).abs() < 1e-9);
    assert!(agg.p95_ms > agg.p50_ms);
  }

  #[test]
  fn latency_aggregate_empty() {
    let agg = LatencyAggregate::from_samples(&[]);
    assert_eq!(agg.mean_ms, 0.0);
    assert_eq!(agg.p50_ms, 0.0);
    assert_eq!(agg.p95_ms, 0.0);
  }
}

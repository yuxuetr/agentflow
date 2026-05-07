//! Eval runner: drives a [`Retriever`] over a [`Dataset`] and produces a
//! [`EvalReport`].
//!
//! The runner is intentionally **retriever-agnostic**: it does not know or
//! care whether the underlying implementation uses BM25, vector similarity,
//! or any hybrid. Concrete adapters live in [`super::retrievers`].

use super::dataset::{Dataset, Judgment};
use super::metrics::{
  LatencyAggregate, QueryEvaluation, ndcg_at_k, recall_at_k, reciprocal_rank,
};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Synchronous retriever interface used by the eval runner.
///
/// Eval runs are batched and offline, so we deliberately keep this sync to
/// avoid forcing every backend into an async wrapper. The Qdrant /
/// embedding-API path can still be wrapped via [`adapters::AsyncRetriever`]
/// if needed.
pub trait Retriever: Send + Sync {
  /// Backend label for the report (e.g. `"bm25"`, `"vector:openai"`).
  fn name(&self) -> &str;
  /// Return the top-`k` doc ids ranked best-first.
  fn search(&self, query: &str, k: usize) -> Result<Vec<String>>;
}

/// Eval configuration. The runner computes Recall@K / nDCG@K for every K in
/// `k_values`, plus MRR (single value, not K-dependent).
#[derive(Debug, Clone)]
pub struct EvalConfig {
  pub k_values: Vec<usize>,
  /// Optional human-readable label shown in reports.
  pub label: String,
}

impl Default for EvalConfig {
  fn default() -> Self {
    Self {
      k_values: vec![1, 3, 5, 10],
      label: String::new(),
    }
  }
}

/// One row in the report: per-K metric averages.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerKMetrics {
  pub k: usize,
  pub recall: f64,
  pub ndcg: f64,
}

/// Per-query breakdown — handy for spotting outliers and for paired
/// significance comparisons.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerQueryRow {
  pub query_id: String,
  pub query_text: String,
  /// Recall keyed by K.
  pub recall_at_k: Vec<(usize, f64)>,
  /// nDCG keyed by K.
  pub ndcg_at_k: Vec<(usize, f64)>,
  pub reciprocal_rank: f64,
  pub latency_ms: f64,
}

/// Aggregate eval report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalReport {
  /// Backend label (from `Retriever::name`).
  pub retriever: String,
  /// Caller-supplied config label (see `EvalConfig::label`).
  pub label: String,
  /// Macro-averaged Recall / nDCG for each requested K.
  pub per_k: Vec<PerKMetrics>,
  /// MRR averaged over queries.
  pub mrr: f64,
  pub latency: LatencyAggregate,
  pub num_queries: usize,
  /// Subset of queries whose judgment had at least one relevant doc — these
  /// are the only ones counted in MRR / Recall / nDCG averages.
  pub queries_with_relevant: usize,
  pub per_query: Vec<PerQueryRow>,
}

impl EvalReport {
  /// Pretty-print as a CLI-friendly fixed-width table.
  pub fn render_table(&self) -> String {
    let mut out = String::new();
    out.push_str(&format!(
      "Retriever: {}\nLabel:     {}\nQueries:   {} ({} with relevant)\n\n",
      self.retriever, self.label, self.num_queries, self.queries_with_relevant
    ));
    out.push_str(&format!(
      "{:<6} {:>10} {:>10}\n",
      "K", "Recall", "nDCG"
    ));
    out.push_str("------ ---------- ----------\n");
    for row in &self.per_k {
      out.push_str(&format!(
        "{:<6} {:>10.4} {:>10.4}\n",
        row.k, row.recall, row.ndcg
      ));
    }
    out.push_str(&format!("\nMRR:       {:.4}\n", self.mrr));
    out.push_str(&format!(
      "Latency:   mean={:.2}ms p50={:.2}ms p95={:.2}ms\n",
      self.latency.mean_ms, self.latency.p50_ms, self.latency.p95_ms
    ));
    out
  }
}

/// Run a single retriever over `dataset` and return its report.
pub fn evaluate(
  retriever: &dyn Retriever,
  dataset: &Dataset,
  config: &EvalConfig,
) -> Result<EvalReport> {
  if config.k_values.is_empty() {
    return Err(crate::error::RAGError::invalid_input(
      "EvalConfig::k_values must contain at least one K".to_string(),
    ));
  }
  let max_k = *config.k_values.iter().max().unwrap();

  // Pair queries with judgments. Skip queries whose judgment is missing —
  // the dataset validator already complains if judgments reference unknown
  // queries, but the reverse direction (query without judgment) is allowed
  // so the harness can score a partial annotation set.
  let pairs: Vec<(&super::dataset::Query, &Judgment)> = dataset
    .queries
    .iter()
    .filter_map(|q| dataset.judgment_for(&q.id).map(|j| (q, j)))
    .collect();

  let mut latencies: Vec<f64> = Vec::with_capacity(pairs.len());
  let mut per_query: Vec<PerQueryRow> = Vec::with_capacity(pairs.len());
  let mut queries_with_relevant: usize = 0;
  let mut sum_mrr: f64 = 0.0;

  // Per-K accumulators (kept parallel to `config.k_values`).
  let mut per_k_recall: Vec<f64> = vec![0.0; config.k_values.len()];
  let mut per_k_ndcg: Vec<f64> = vec![0.0; config.k_values.len()];

  for (query, judgment) in &pairs {
    let start = Instant::now();
    let retrieved = retriever.search(&query.text, max_k)?;
    let latency_ms = duration_ms(start);
    latencies.push(latency_ms);

    let has_relevant = judgment.relevant_ids().next().is_some();
    if has_relevant {
      queries_with_relevant += 1;
    }

    let mut row = PerQueryRow {
      query_id: query.id.clone(),
      query_text: query.text.clone(),
      recall_at_k: Vec::with_capacity(config.k_values.len()),
      ndcg_at_k: Vec::with_capacity(config.k_values.len()),
      reciprocal_rank: 0.0,
      latency_ms,
    };

    for (idx, k) in config.k_values.iter().enumerate() {
      let top_k = take_top_k(&retrieved, *k);
      let eval = QueryEvaluation {
        judgment,
        retrieved_ids: &top_k,
        latency_ms,
      };
      let recall = recall_at_k(&eval);
      let ndcg = ndcg_at_k(&eval);
      row.recall_at_k.push((*k, recall));
      row.ndcg_at_k.push((*k, ndcg));
      if has_relevant {
        per_k_recall[idx] += recall;
        per_k_ndcg[idx] += ndcg;
      }
    }

    if has_relevant {
      let eval = QueryEvaluation {
        judgment,
        retrieved_ids: &retrieved,
        latency_ms,
      };
      let rr = reciprocal_rank(&eval);
      row.reciprocal_rank = rr;
      sum_mrr += rr;
    }

    per_query.push(row);
  }

  let denom = queries_with_relevant.max(1) as f64;
  let per_k = config
    .k_values
    .iter()
    .enumerate()
    .map(|(idx, k)| PerKMetrics {
      k: *k,
      recall: per_k_recall[idx] / denom,
      ndcg: per_k_ndcg[idx] / denom,
    })
    .collect();

  Ok(EvalReport {
    retriever: retriever.name().to_string(),
    label: config.label.clone(),
    per_k,
    mrr: if queries_with_relevant == 0 {
      0.0
    } else {
      sum_mrr / queries_with_relevant as f64
    },
    latency: LatencyAggregate::from_samples(&latencies),
    num_queries: pairs.len(),
    queries_with_relevant,
    per_query,
  })
}

fn take_top_k(retrieved: &[String], k: usize) -> Vec<String> {
  retrieved.iter().take(k).cloned().collect()
}

fn duration_ms(start: Instant) -> f64 {
  let elapsed = start.elapsed();
  elapsed.as_secs() as f64 * 1000.0 + elapsed.subsec_nanos() as f64 / 1_000_000.0
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::eval::dataset::{CorpusDoc, Judgment, Query};
  use std::collections::HashMap;

  /// Trivial retriever for tests — returns ranked ids verbatim from a map.
  struct ScriptedRetriever {
    name: String,
    answers: HashMap<String, Vec<String>>,
  }

  impl Retriever for ScriptedRetriever {
    fn name(&self) -> &str {
      &self.name
    }
    fn search(&self, query: &str, _k: usize) -> Result<Vec<String>> {
      Ok(self.answers.get(query).cloned().unwrap_or_default())
    }
  }

  fn tiny_dataset() -> Dataset {
    let corpus = vec![
      CorpusDoc {
        id: "d1".into(),
        text: "the quick brown fox".into(),
        title: None,
      },
      CorpusDoc {
        id: "d2".into(),
        text: "the lazy dog".into(),
        title: None,
      },
      CorpusDoc {
        id: "d3".into(),
        text: "machine learning".into(),
        title: None,
      },
    ];
    let queries = vec![
      Query {
        id: "q1".into(),
        text: "brown fox".into(),
        notes: None,
      },
      Query {
        id: "q2".into(),
        text: "machine".into(),
        notes: None,
      },
    ];
    let mut q1_rel = HashMap::new();
    q1_rel.insert("d1".to_string(), 1u8);
    let mut q2_rel = HashMap::new();
    q2_rel.insert("d3".to_string(), 1u8);
    let judgments = vec![
      Judgment {
        query_id: "q1".into(),
        relevances: q1_rel,
        notes: None,
      },
      Judgment {
        query_id: "q2".into(),
        relevances: q2_rel,
        notes: None,
      },
    ];
    Dataset::new(corpus, queries, judgments)
  }

  #[test]
  fn evaluate_perfect_retriever() {
    let mut answers = HashMap::new();
    answers.insert("brown fox".to_string(), vec!["d1".into(), "d2".into()]);
    answers.insert("machine".to_string(), vec!["d3".into()]);
    let retriever = ScriptedRetriever {
      name: "scripted".into(),
      answers,
    };
    let dataset = tiny_dataset();
    let config = EvalConfig {
      k_values: vec![1, 3],
      label: "perfect".into(),
    };
    let report = evaluate(&retriever, &dataset, &config).unwrap();
    assert_eq!(report.retriever, "scripted");
    assert_eq!(report.num_queries, 2);
    assert_eq!(report.queries_with_relevant, 2);
    assert!((report.mrr - 1.0).abs() < 1e-9);
    let recall_1 = report.per_k.iter().find(|r| r.k == 1).unwrap();
    assert!((recall_1.recall - 1.0).abs() < 1e-9);
  }

  #[test]
  fn evaluate_partial_retriever() {
    let mut answers = HashMap::new();
    answers.insert("brown fox".to_string(), vec!["d2".into(), "d1".into()]); // hit at rank 2
    answers.insert("machine".to_string(), vec!["d2".into()]); // miss
    let retriever = ScriptedRetriever {
      name: "scripted".into(),
      answers,
    };
    let dataset = tiny_dataset();
    let config = EvalConfig {
      k_values: vec![1, 3],
      label: "partial".into(),
    };
    let report = evaluate(&retriever, &dataset, &config).unwrap();
    let recall_1 = report.per_k.iter().find(|r| r.k == 1).unwrap();
    let recall_3 = report.per_k.iter().find(|r| r.k == 3).unwrap();
    assert!((recall_1.recall - 0.0).abs() < 1e-9); // q1 misses at K=1, q2 misses entirely
    assert!((recall_3.recall - 0.5).abs() < 1e-9); // q1 hits at K=3, q2 still miss
    assert!((report.mrr - 0.25).abs() < 1e-9); // (1/2 + 0) / 2
  }

  #[test]
  fn evaluate_rejects_empty_k() {
    let retriever = ScriptedRetriever {
      name: "x".into(),
      answers: HashMap::new(),
    };
    let dataset = tiny_dataset();
    let config = EvalConfig {
      k_values: vec![],
      label: "".into(),
    };
    assert!(evaluate(&retriever, &dataset, &config).is_err());
  }

  #[test]
  fn render_table_contains_metrics() {
    let report = EvalReport {
      retriever: "demo".into(),
      label: "test".into(),
      per_k: vec![PerKMetrics {
        k: 5,
        recall: 0.8,
        ndcg: 0.7,
      }],
      mrr: 0.5,
      latency: LatencyAggregate {
        mean_ms: 1.0,
        p50_ms: 1.0,
        p95_ms: 1.0,
      },
      num_queries: 10,
      queries_with_relevant: 10,
      per_query: vec![],
    };
    let text = report.render_table();
    assert!(text.contains("Recall"));
    assert!(text.contains("0.8"));
    assert!(text.contains("MRR"));
  }
}

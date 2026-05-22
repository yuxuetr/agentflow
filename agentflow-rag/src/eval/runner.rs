//! Eval runner: drives a [`Retriever`] over a [`Dataset`] and produces a
//! [`EvalReport`].
//!
//! The runner is intentionally **retriever-agnostic**: it does not know or
//! care whether the underlying implementation uses BM25, vector similarity,
//! or any hybrid. Concrete adapters live in [`super::retrievers`].

use super::chunking_eval::remap_chunks_to_doc_ids;
use super::dataset::{Dataset, Judgment};
use super::metrics::{LatencyAggregate, QueryEvaluation, ndcg_at_k, recall_at_k, reciprocal_rank};
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
  /// P10.6.3: optional chunking dimension. `Some(n)` when the eval
  /// ran over a fixed-size-chunked corpus (chunk character size = n).
  /// `None` (the default) when the eval ran over the un-chunked
  /// corpus (current behaviour, every pre-P10.6.3 baseline). Persists
  /// through the baseline JSON file so a future `--compare-baseline`
  /// run can detect chunking-strategy drift and warn.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub chunk_size: Option<usize>,
}

impl EvalReport {
  /// Pretty-print as a CLI-friendly fixed-width table.
  pub fn render_table(&self) -> String {
    let mut out = String::new();
    out.push_str(&format!(
      "Retriever: {}\nLabel:     {}\nQueries:   {} ({} with relevant)\n\n",
      self.retriever, self.label, self.num_queries, self.queries_with_relevant
    ));
    out.push_str(&format!("{:<6} {:>10} {:>10}\n", "K", "Recall", "nDCG"));
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
    if let Some(n) = self.chunk_size {
      // P10.6.3: surface the chunking dimension next to the latency
      // block so an operator scanning the report knows whether the
      // numbers came from chunked or un-chunked indexing.
      out.push_str(&format!("Chunk size: {n} (fixed-size, overlap=0)\n"));
    }
    out
  }
}

/// Run a single retriever over `dataset` and return its report.
///
/// Equivalent to `evaluate_with_remapping(retriever, dataset, &None,
/// config)`; kept for callers that don't use chunked corpora.
pub fn evaluate(
  retriever: &dyn Retriever,
  dataset: &Dataset,
  config: &EvalConfig,
) -> Result<EvalReport> {
  evaluate_with_remapping(retriever, dataset, &None, config)
}

/// Run a single retriever over `dataset` with an optional chunk-id →
/// source-doc-id remapping applied to every retrieval result before
/// qrels scoring (P10.6.3).
///
/// When `chunk_to_doc` is `None`, behaviour is identical to
/// [`evaluate`]. When `Some`, each retrieved id is looked up in the
/// map (falling through unchanged when missing) and duplicates from
/// multiple chunks of the same source doc are deduped within the
/// top-K window. This keeps Recall@K / MRR / nDCG@K comparable
/// across chunked vs un-chunked runs of the same dataset.
///
/// The returned [`EvalReport::chunk_size`] is **NOT** populated by
/// this function — the caller is the source of truth for which
/// chunking config produced `dataset`, so they set the field
/// afterward. Keeps the runner agnostic of the chunker's
/// configuration.
pub fn evaluate_with_remapping(
  retriever: &dyn Retriever,
  dataset: &Dataset,
  chunk_to_doc: &Option<HashMap<String, String>>,
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
    // When the retriever's index holds chunked docs, we need to
    // over-fetch from the retriever (so the dedupe step still gives
    // us K *distinct source docs* at the end). Asking for max_k * 8
    // is a heuristic that covers the common case where each source
    // doc produced <= 8 chunks; degenerate cases (a 1-MB doc chunked
    // at 64 chars) might still drop a few candidates, which the
    // operator can mitigate with a larger chunk_size.
    let fetch_k = if chunk_to_doc.is_some() {
      max_k.saturating_mul(8)
    } else {
      max_k
    };
    let raw_retrieved = retriever.search(&query.text, fetch_k)?;
    let retrieved = match chunk_to_doc {
      Some(map) => remap_chunks_to_doc_ids(&raw_retrieved, map),
      None => raw_retrieved,
    };
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
    chunk_size: None,
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
      chunk_size: None,
    };
    let text = report.render_table();
    assert!(text.contains("Recall"));
    assert!(text.contains("0.8"));
    assert!(text.contains("MRR"));
    // chunk_size: None must NOT render — keeps the un-chunked
    // output byte-identical to pre-P10.6.3.
    assert!(!text.contains("Chunk size"));
  }

  #[test]
  fn render_table_includes_chunk_size_line_when_present() {
    // P10.6.3: chunked-eval reports surface the chunk-size dimension
    // next to the latency block so an operator scanning the report
    // can confirm at a glance which index shape the numbers came
    // from.
    let report = EvalReport {
      retriever: "demo".into(),
      label: "test".into(),
      per_k: vec![],
      mrr: 0.5,
      latency: LatencyAggregate {
        mean_ms: 1.0,
        p50_ms: 1.0,
        p95_ms: 1.0,
      },
      num_queries: 0,
      queries_with_relevant: 0,
      per_query: vec![],
      chunk_size: Some(256),
    };
    let text = report.render_table();
    assert!(
      text.contains("Chunk size: 256 (fixed-size, overlap=0)"),
      "expected chunk-size line; got:\n{text}"
    );
  }

  #[test]
  fn eval_report_serde_chunk_size_round_trips_and_is_optional() {
    // Schema-stability pin: pre-P10.6.3 baselines (without the
    // `chunk_size` key) must continue to parse as `None`. New
    // baselines with `Some(n)` round-trip cleanly.
    let json_without = serde_json::json!({
      "retriever": "bm25",
      "label": "",
      "per_k": [],
      "mrr": 0.0,
      "latency": { "mean_ms": 0.0, "p50_ms": 0.0, "p95_ms": 0.0 },
      "num_queries": 0,
      "queries_with_relevant": 0,
      "per_query": []
    });
    let parsed: EvalReport = serde_json::from_value(json_without).unwrap();
    assert_eq!(parsed.chunk_size, None);

    let report = EvalReport {
      retriever: "bm25".into(),
      label: String::new(),
      per_k: vec![],
      mrr: 0.0,
      latency: LatencyAggregate {
        mean_ms: 0.0,
        p50_ms: 0.0,
        p95_ms: 0.0,
      },
      num_queries: 0,
      queries_with_relevant: 0,
      per_query: vec![],
      chunk_size: Some(512),
    };
    let encoded = serde_json::to_value(&report).unwrap();
    assert_eq!(encoded["chunk_size"], 512);
    let decoded: EvalReport = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded.chunk_size, Some(512));
  }
}

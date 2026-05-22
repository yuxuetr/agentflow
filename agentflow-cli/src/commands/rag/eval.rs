//! `agentflow rag eval` — run the RAG eval harness over a dataset directory.
//!
//! Supported retriever backends (P10.6.1):
//! - `bm25` — offline lexical BM25; no external dependencies.
//! - `dense` — in-memory cosine similarity over OpenAI embeddings;
//!   requires `OPENAI_API_KEY` at run time.
//! - `hybrid` — Reciprocal Rank Fusion combining BM25 + dense; also
//!   requires `OPENAI_API_KEY`.
//!
//! Eval-scale corpora (<100k docs) fit in memory, so the dense path
//! does NOT require a vector store (Qdrant) — that's a deployment-
//! scale concern, not an eval-harness concern.

use agentflow_rag::embeddings::{EmbeddingProvider, OpenAIEmbedding};
use agentflow_rag::eval::{
  Bm25Eval, ChunkedDataset, ComparisonReport, Dataset, DenseEval, EvalConfig, EvalReport,
  HybridEval, chunk_dataset, compare, evaluate_with_remapping,
};
use anyhow::{Context, Result, bail};
use colored::Colorize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;

/// Default thresholds used by the regression gate when `--compare-baseline`
/// is supplied. Both must trip together to flag a regression — matches the
/// P4.2 spec (`p < 0.05 AND ≥3% absolute drop in Recall@5`).
const DEFAULT_REGRESSION_RECALL_DROP: f64 = 0.03;
const DEFAULT_REGRESSION_P_VALUE: f64 = 0.05;
const REGRESSION_RECALL_K: usize = 5;

/// Execute the `rag eval` command.
///
/// `k_values` defaults to `[1, 3, 5, 10]` when empty. `retriever`
/// chooses the backend (`bm25` / `dense` / `hybrid`).
/// `embedding_model` is only consulted when the dense or hybrid
/// path is selected (ignored otherwise). When `compare_to` is
/// `Some`, the CLI runs a second BM25 eval with custom params
/// parsed from the form `k1=1.5,b=0.6` and emits a paired
/// comparison report.
#[allow(clippy::too_many_arguments)]
pub async fn execute(
  dataset_dir: PathBuf,
  retriever: String,
  embedding_model: String,
  k_values: Vec<usize>,
  compare_to: Option<String>,
  compare_baseline: Option<PathBuf>,
  regression_recall_threshold: Option<f64>,
  regression_p_value: Option<f64>,
  output: Option<PathBuf>,
  format: String,
  chunk_size: Option<usize>,
) -> Result<()> {
  let is_envelope = format == "json-envelope";

  if compare_to.is_some() && compare_baseline.is_some() {
    bail!("--compare-to and --compare-baseline are mutually exclusive — pick one");
  }

  let dataset = Dataset::load_from_dir(&dataset_dir)
    .with_context(|| format!("loading dataset from {}", dataset_dir.display()))?;

  // P10.6.3: when `--chunk-size N` is set, re-chunk the corpus once
  // up front. Both the baseline and (optional) `--compare-to`
  // candidate share the same chunked corpus so the comparison stays
  // apples-to-apples.
  let chunked = match chunk_size {
    Some(n) => Some(
      chunk_dataset(&dataset, n, 0)
        .with_context(|| format!("chunking corpus at chunk_size={n}"))?,
    ),
    None => None,
  };
  if let Some(ref c) = chunked
    && !is_envelope
  {
    println!(
      "  chunked: corpus_size={} (chunk_size={} overlap={})",
      c.corpus.len(),
      c.chunk_size,
      c.overlap
    );
  }

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

  let baseline_report = run_eval(
    &dataset,
    chunked.as_ref(),
    &retriever,
    &embedding_model,
    &k_values,
    "baseline",
  )
  .await?;
  if !is_envelope {
    println!();
    println!("{}", baseline_report.render_table());
  }

  let mut comparison: Option<ComparisonReport> = None;
  let mut candidate_report: Option<EvalReport> = None;
  let mut regression_decision: Option<RegressionDecision> = None;
  if let Some(spec) = &compare_to {
    let candidate = run_compare_candidate(&dataset, chunked.as_ref(), spec, &k_values)?;
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
    // P10.6.3: warn (don't fail) when the stored baseline's
    // chunk_size differs from the current run's. Cross-chunk-size
    // comparisons can still be useful for "did the chunking change
    // hurt recall?" investigations — but the operator should know
    // they're not apples-to-apples.
    if stored_baseline.chunk_size != baseline_report.chunk_size && !is_envelope {
      eprintln!(
        "warning: baseline chunk_size={:?} differs from current chunk_size={:?}; \
         metric deltas may reflect chunking-strategy change, not pure retriever drift",
        stored_baseline.chunk_size, baseline_report.chunk_size
      );
    }
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
    let envelope =
      crate::json_envelope::CliJsonEnvelope::with_errors("rag eval", &report_payload, errors);
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

/// Load a stored `EvalReport` (baseline snapshot) from disk.
///
/// Accepts two on-disk shapes:
///
/// 1. **Bare `EvalReport`** — the historical convention used by
///    `agentflow-rag/eval_baselines/ci_offline/bm25.json` (one
///    object with `label`, `per_k`, `mrr`, etc. at the top level).
/// 2. **Envelope form** — the shape `--output <file>` writes
///    today: `{ dataset, baseline, candidate, comparison,
///    regression }`. The reader extracts the `baseline` field.
///
/// Accepting both shapes (P10.6.2) means `agentflow rag eval
/// --retriever dense --output <path>` produces a file that can be
/// fed back via `--compare-baseline <path>` on a later run without
/// the operator having to hand-extract the `.baseline` field. The
/// pre-P10.6.2 path required that manual extraction.
fn load_baseline_from_path(path: &PathBuf) -> Result<EvalReport> {
  let raw = std::fs::read_to_string(path)
    .with_context(|| format!("reading baseline snapshot at {}", path.display()))?;
  // Try the bare shape first: it's what bm25.json uses and what
  // `serde::from_str::<EvalReport>` decodes cleanly. If that
  // fails, fall through to the envelope path so operators with a
  // `--output`-generated file get a clear error only when BOTH
  // shapes fail.
  if let Ok(report) = serde_json::from_str::<EvalReport>(&raw) {
    return Ok(report);
  }
  let envelope: Value = serde_json::from_str(&raw)
    .with_context(|| format!("parsing baseline snapshot at {}", path.display()))?;
  let baseline = envelope.get("baseline").ok_or_else(|| {
    anyhow::anyhow!(
      "baseline snapshot at {} is neither a bare EvalReport nor an envelope with a \
       `baseline` field; regenerate with `agentflow rag eval --output <path>` to fix",
      path.display()
    )
  })?;
  let report: EvalReport = serde_json::from_value(baseline.clone()).with_context(|| {
    format!(
      "extracting `baseline` field from envelope at {}",
      path.display()
    )
  })?;
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

async fn run_eval(
  dataset: &Dataset,
  chunked: Option<&ChunkedDataset>,
  retriever_kind: &str,
  embedding_model: &str,
  k_values: &[usize],
  label: &str,
) -> Result<EvalReport> {
  let config = EvalConfig {
    k_values: k_values.to_vec(),
    label: label.to_string(),
  };
  // When chunked, the retriever is built against the chunked corpus
  // and the runner remaps chunk-ids → source-doc-ids before qrels
  // scoring. When un-chunked, the retriever sees the original
  // dataset and no remap fires — pre-P10.6.3 behaviour.
  let (index_dataset, remap) = match chunked {
    Some(c) => (c.as_dataset(), Some(c.chunk_to_doc.clone())),
    None => (dataset.clone(), None),
  };
  // The runner evaluates Recall/MRR/nDCG against the *judgments*
  // from the source dataset (qrels reference source doc ids).
  // The chunked variant of the dataset only carries the corpus +
  // chunked ids; we still pass the source dataset's judgments by
  // reusing `dataset` for the scoring half. Conceptually:
  //   retriever's index   ← chunked corpus
  //   judgments + queries ← original dataset (passed in as `dataset`)
  // The runner pairs queries↔judgments off `dataset`, then calls
  // retriever.search() against the chunked index, then remaps
  // results back to source ids. Pass `index_dataset` as the
  // retriever's source; pass the source `dataset` to the runner's
  // scoring pass — but the runner currently uses one Dataset for
  // both. Resolution: build retriever from `index_dataset`, but
  // evaluate against `dataset`. Since judgments + queries are
  // identical across the two (we only mutated `corpus`), we can
  // pass either as the second arg; passing `dataset` keeps the
  // scoring intent explicit.
  let mut report = match retriever_kind {
    "bm25" => {
      let retriever = Bm25Eval::from_dataset(&index_dataset);
      evaluate_with_remapping(&retriever, dataset, &remap, &config).map_err(anyhow::Error::from)?
    }
    "dense" => {
      let retriever = build_dense_retriever(&index_dataset, embedding_model).await?;
      evaluate_with_remapping(&retriever, dataset, &remap, &config).map_err(anyhow::Error::from)?
    }
    "hybrid" => {
      // Hybrid wraps trait objects, so both BM25 and Dense must
      // outlive the HybridEval. `Box`ing here keeps lifetimes
      // simple — the HybridEval owns both backends for the
      // duration of the eval.
      let bm25 = Bm25Eval::from_dataset(&index_dataset);
      let dense = build_dense_retriever(&index_dataset, embedding_model).await?;
      let hybrid = HybridEval::new(
        format!("hybrid:bm25+dense:{embedding_model}"),
        Box::new(bm25),
        Box::new(dense),
      );
      evaluate_with_remapping(&hybrid, dataset, &remap, &config).map_err(anyhow::Error::from)?
    }
    other => bail!(
      "unsupported retriever `{}`. Supported: bm25, dense, hybrid",
      other
    ),
  };
  // P10.6.3: stamp the chunking dimension on the report. The
  // runner is config-agnostic (see `evaluate_with_remapping` doc);
  // the CLI is the source of truth for the chunker's settings.
  if let Some(c) = chunked {
    report.chunk_size = Some(c.chunk_size);
  }
  Ok(report)
}

/// Build a [`DenseEval`] by embedding the dataset's corpus and queries
/// via the OpenAI embeddings API. The same embedding call happens at
/// CLI time so the sync `Retriever::search` path inside the eval
/// runner doesn't need an async runtime context.
///
/// **Requires `OPENAI_API_KEY` at run time.** The error path names
/// the missing env var explicitly so fresh-host operators can act on
/// the message in one read.
async fn build_dense_retriever(dataset: &Dataset, embedding_model: &str) -> Result<DenseEval> {
  if std::env::var("OPENAI_API_KEY").is_err() {
    bail!(
      "--retriever dense (or hybrid) requires OPENAI_API_KEY to be set in the environment. \
       Either export it directly or add it to ~/.agentflow/.env. The CLI embeds the corpus + \
       queries once via the `{embedding_model}` model before scoring; no network call happens \
       again during the eval itself."
    );
  }
  let provider = OpenAIEmbedding::new(embedding_model.to_string())
    .with_context(|| format!("creating OpenAI embedding provider for model `{embedding_model}`"))?;

  // Build the corpus body the same way `Bm25Eval` does — title +
  // body — so dense and bm25 see equivalent text. Keeps the eval
  // comparison apples-to-apples.
  let corpus_texts: Vec<String> = dataset
    .corpus
    .iter()
    .map(|doc| match &doc.title {
      Some(t) if !t.is_empty() => format!("{}\n{}", t, doc.text),
      _ => doc.text.clone(),
    })
    .collect();
  let corpus_text_refs: Vec<&str> = corpus_texts.iter().map(|s| s.as_str()).collect();
  let corpus_vectors = provider
    .embed_batch(corpus_text_refs)
    .await
    .with_context(|| {
      format!(
        "embedding {} corpus documents via OpenAI model `{embedding_model}`",
        dataset.corpus.len()
      )
    })?;
  let corpus_pairs: Vec<(String, Vec<f32>)> = dataset
    .corpus
    .iter()
    .zip(corpus_vectors)
    .map(|(doc, vec)| (doc.id.clone(), vec))
    .collect();

  // Queries are deduped on text to avoid paying for re-embedding
  // the same string. The DenseEval lookup is keyed by query text,
  // so a single embedding can serve every query with that text.
  let mut unique_queries: HashMap<String, ()> = HashMap::new();
  for query in &dataset.queries {
    unique_queries.entry(query.text.clone()).or_insert(());
  }
  let unique_query_texts: Vec<String> = unique_queries.into_keys().collect();
  let query_text_refs: Vec<&str> = unique_query_texts.iter().map(|s| s.as_str()).collect();
  let query_vectors = provider
    .embed_batch(query_text_refs)
    .await
    .with_context(|| {
      format!(
        "embedding {} unique queries via OpenAI model `{embedding_model}`",
        unique_query_texts.len()
      )
    })?;
  let query_map: HashMap<String, Vec<f32>> =
    unique_query_texts.into_iter().zip(query_vectors).collect();

  DenseEval::new(format!("dense:{embedding_model}"), corpus_pairs, query_map)
    .map_err(anyhow::Error::from)
}

fn run_compare_candidate(
  dataset: &Dataset,
  chunked: Option<&ChunkedDataset>,
  spec: &str,
  k_values: &[usize],
) -> Result<EvalReport> {
  let (k1, b) = parse_bm25_params(spec)?;
  let config = EvalConfig {
    k_values: k_values.to_vec(),
    label: format!("bm25(k1={},b={})", k1, b),
  };
  // P10.6.3: the candidate shares the chunked index with the
  // baseline so the comparison stays apples-to-apples. When no
  // chunk-size flag is set, both run on the un-chunked corpus.
  let (index_dataset, remap) = match chunked {
    Some(c) => (c.as_dataset(), Some(c.chunk_to_doc.clone())),
    None => (dataset.clone(), None),
  };
  let retriever = Bm25Eval::with_params(&index_dataset, k1, b);
  let mut report =
    evaluate_with_remapping(&retriever, dataset, &remap, &config).map_err(anyhow::Error::from)?;
  if let Some(c) = chunked {
    report.chunk_size = Some(c.chunk_size);
  }
  Ok(report)
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

  /// P10.6.1: `--retriever dense` without `OPENAI_API_KEY` must
  /// produce a single-line actionable error message naming the
  /// missing env var. The build step never proceeds to embedding,
  /// so no network call happens — the test is hermetic.
  ///
  /// **Test isolation**: this temporarily clears `OPENAI_API_KEY`
  /// (and the `OPENAI_KEY` fallback that `OpenAIEmbedding` checks
  /// via the AgentFlow LLM key precedence). Snapshot + restore
  /// keeps a dev environment with the key set from breaking other
  /// parallel unit tests.
  #[tokio::test]
  async fn build_dense_retriever_errors_without_openai_api_key() {
    use agentflow_rag::eval::dataset::{CorpusDoc, Query};

    let snapshot = std::env::var("OPENAI_API_KEY").ok();
    // SAFETY: dedicated test process; restore at end.
    unsafe {
      std::env::remove_var("OPENAI_API_KEY");
    }

    let dataset = Dataset::new(
      vec![CorpusDoc {
        id: "d1".into(),
        text: "anything".into(),
        title: None,
      }],
      vec![Query {
        id: "q1".into(),
        text: "anything".into(),
        notes: None,
      }],
      vec![],
    );
    let result = build_dense_retriever(&dataset, "text-embedding-3-small").await;

    // SAFETY: restore before asserting so a panic doesn't pollute env.
    unsafe {
      if let Some(value) = snapshot {
        std::env::set_var("OPENAI_API_KEY", value);
      }
    }

    match result {
      Err(err) => {
        let msg = err.to_string();
        assert!(
          msg.contains("OPENAI_API_KEY"),
          "error must name the missing env var: {msg}"
        );
        assert!(
          msg.contains("--retriever dense") || msg.contains("hybrid"),
          "error must reference the retriever flag: {msg}"
        );
      }
      Ok(_) => panic!("must error when OPENAI_API_KEY is unset"),
    }
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

  // ── load_baseline_from_path dual-shape parser (P10.6.2) ─────────────

  /// Helper: minimal `EvalReport` for the round-trip tests.
  fn fixture_report() -> EvalReport {
    use agentflow_rag::eval::PerKMetrics;
    use agentflow_rag::eval::metrics::LatencyAggregate;
    EvalReport {
      retriever: "fixture".into(),
      label: "baseline".into(),
      per_k: vec![PerKMetrics {
        k: 5,
        recall: 1.0,
        ndcg: 1.0,
      }],
      mrr: 1.0,
      latency: LatencyAggregate {
        mean_ms: 0.1,
        p50_ms: 0.1,
        p95_ms: 0.1,
      },
      num_queries: 10,
      queries_with_relevant: 10,
      per_query: vec![],
      chunk_size: None,
    }
  }

  #[test]
  fn load_baseline_reads_bare_eval_report() {
    // The bm25.json convention: a bare EvalReport at the top level.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bare.json");
    std::fs::write(
      &path,
      serde_json::to_string_pretty(&fixture_report()).unwrap(),
    )
    .unwrap();
    let loaded = load_baseline_from_path(&path).expect("bare shape must load");
    assert_eq!(loaded.retriever, "fixture");
    assert_eq!(loaded.per_k.len(), 1);
  }

  /// P10.6.2: the reader must also accept the envelope shape that
  /// `--output <path>` writes (`{ dataset, baseline, candidate, ... }`).
  /// Without this, an operator can't feed back their own `--output`
  /// file via `--compare-baseline` without hand-extracting the
  /// `.baseline` field — the exact frustration this commit closes.
  #[test]
  fn load_baseline_reads_envelope_with_baseline_field() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("envelope.json");
    let envelope = json!({
      "dataset": { "path": "/tmp/x", "corpus_size": 20 },
      "baseline": fixture_report(),
      "candidate": Value::Null,
      "comparison": Value::Null,
      "regression": Value::Null,
    });
    std::fs::write(&path, serde_json::to_string_pretty(&envelope).unwrap()).unwrap();
    let loaded = load_baseline_from_path(&path).expect("envelope shape must load");
    assert_eq!(loaded.retriever, "fixture");
  }

  /// Neither shape → clear error naming the recovery path. Catches a
  /// future refactor that loosens the parser to silently accept an
  /// EvalReport-without-required-fields and then trip down-stream.
  #[test]
  fn load_baseline_errors_when_neither_shape_matches() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("garbage.json");
    std::fs::write(&path, r#"{"random_key": 42}"#).unwrap();
    let err = load_baseline_from_path(&path).expect_err("must err");
    let msg = format!("{err:#}");
    // The two-shape diagnostic + the regeneration hint are both
    // actionable; pin both so the message doesn't degrade.
    assert!(
      msg.contains("neither a bare EvalReport nor an envelope"),
      "{msg}"
    );
    assert!(msg.contains("agentflow rag eval --output"), "{msg}");
  }
}

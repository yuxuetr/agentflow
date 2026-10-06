//! Built-in retriever adapters for the eval harness.
//!
//! These are deliberately offline / dependency-free so the harness can run in
//! CI without Qdrant or external embedding APIs. Production runs may plug a
//! custom [`Retriever`] in directly.
//!
//! ## Available backends
//!
//! - [`Bm25Eval`] — lexical BM25 over the dataset corpus. No external
//!   dependencies. Default for offline / CI usage.
//! - [`DenseEval`] — in-memory cosine similarity over pre-embedded corpus
//!   plus queries. The CLI embeds via
//!   [`crate::embeddings::EmbeddingProvider`] once before evaluation; the
//!   sync `search()` then needs no async runtime. The eval-scale corpus
//!   (under 100k docs) fits in RAM, so a vector store is not required —
//!   production-scale retrieval uses [`crate::vectorstore::VectorStore`]
//!   directly, which is a different surface from the eval harness.
//! - [`HybridEval`] — Reciprocal Rank Fusion across any two
//!   [`Retriever`] trait objects. Default `k = 60` matches the standard
//!   recipe from Cormack, Clarke & Buettcher 2009.

use super::dataset::Dataset;
use super::runner::Retriever;
use crate::error::{RAGError, Result};
use crate::retrieval::bm25::BM25Retriever;
use std::collections::HashMap;

/// BM25 retriever over the dataset corpus. Default for the CLI smoke test
/// because it has no external dependencies and is deterministic.
pub struct Bm25Eval {
  retriever: BM25Retriever,
}

impl Bm25Eval {
  pub fn from_dataset(dataset: &Dataset) -> Self {
    let mut retriever = BM25Retriever::new();
    for doc in &dataset.corpus {
      // Concatenate title + body when title is present — common BEIR
      // convention. Avoids losing keyword signal that lives in titles.
      let body = match &doc.title {
        Some(t) if !t.is_empty() => format!("{}\n{}", t, doc.text),
        _ => doc.text.clone(),
      };
      retriever.add_document(doc.id.clone(), body);
    }
    Self { retriever }
  }

  pub fn with_params(dataset: &Dataset, k1: f32, b: f32) -> Self {
    let mut retriever = BM25Retriever::with_params(k1, b);
    for doc in &dataset.corpus {
      let body = match &doc.title {
        Some(t) if !t.is_empty() => format!("{}\n{}", t, doc.text),
        _ => doc.text.clone(),
      };
      retriever.add_document(doc.id.clone(), body);
    }
    Self { retriever }
  }
}

impl Retriever for Bm25Eval {
  fn name(&self) -> &str {
    "bm25"
  }

  fn search(&self, query: &str, k: usize) -> Result<Vec<String>> {
    let results = self.retriever.search(query, k);
    Ok(results.into_iter().map(|r| r.id).collect())
  }
}

/// Dense (vector) retriever that scores via cosine similarity over a
/// pre-embedded corpus + query set.
///
/// Eval scale (<100k docs) keeps the full corpus matrix in RAM — that's
/// cheaper and more deterministic than round-tripping to a vector store.
/// The CLI driver embeds the corpus + queries once via the existing
/// [`crate::embeddings::EmbeddingProvider`] trait, then constructs a
/// `DenseEval` for the eval runner. The runner's sync `search()` can
/// then run without an async runtime context.
///
/// Queries that weren't pre-embedded (i.e. not present in
/// `query_vectors`) return an empty result. This is the same effect as
/// "search for an unknown query" in any retriever; the runner treats
/// it as zero recall, which is the desired behaviour for a partial
/// embedding cache.
pub struct DenseEval {
  /// Backend label used in eval reports.
  label: String,
  /// Corpus vectors keyed by doc id, paired with the L2 norm so the
  /// hot inner-product loop doesn't re-sqrt on every query.
  corpus: Vec<(String, Vec<f32>, f32)>,
  /// Query text → embedding. Lookup keys match `Query::text` (NOT
  /// `Query::id`) because the [`Retriever`] trait's `search()`
  /// receives the raw query text.
  query_vectors: HashMap<String, Vec<f32>>,
}

impl DenseEval {
  /// Construct a dense retriever from pre-computed embeddings.
  ///
  /// `label` is shown in eval reports (e.g.
  /// `"dense:text-embedding-3-small"`). Both `corpus_vectors` and
  /// `query_vectors` are consumed by value to avoid copying.
  ///
  /// Returns `Err(RAGError::InvalidInput)` if any corpus vector has
  /// dimension 0 or if vectors disagree on dimension — the eval
  /// harness can't recover from a corrupt embedding cache silently.
  pub fn new(
    label: impl Into<String>,
    corpus_vectors: Vec<(String, Vec<f32>)>,
    query_vectors: HashMap<String, Vec<f32>>,
  ) -> Result<Self> {
    if corpus_vectors.is_empty() {
      return Err(RAGError::invalid_input(
        "DenseEval requires at least one corpus vector".to_string(),
      ));
    }
    let dim = corpus_vectors[0].1.len();
    if dim == 0 {
      return Err(RAGError::invalid_input(
        "DenseEval corpus vectors must be non-empty".to_string(),
      ));
    }
    let mut corpus = Vec::with_capacity(corpus_vectors.len());
    for (id, vec) in corpus_vectors {
      if vec.len() != dim {
        return Err(RAGError::invalid_input(format!(
          "DenseEval corpus dimension mismatch: doc '{}' has dim {} but expected {}",
          id,
          vec.len(),
          dim,
        )));
      }
      let norm = l2_norm(&vec);
      corpus.push((id, vec, norm));
    }
    for (query_text, vec) in &query_vectors {
      if vec.len() != dim {
        return Err(RAGError::invalid_input(format!(
          "DenseEval query dimension mismatch: '{}' has dim {} but corpus expected {}",
          query_text,
          vec.len(),
          dim,
        )));
      }
    }
    Ok(Self {
      label: label.into(),
      corpus,
      query_vectors,
    })
  }
}

impl Retriever for DenseEval {
  fn name(&self) -> &str {
    &self.label
  }

  fn search(&self, query: &str, k: usize) -> Result<Vec<String>> {
    let Some(qvec) = self.query_vectors.get(query) else {
      // Unknown query → empty result. The runner treats this as zero
      // recall, which is the right behaviour for a partial embedding
      // cache; surfacing it as `Err` would abort the whole eval.
      return Ok(Vec::new());
    };
    let q_norm = l2_norm(qvec);
    if q_norm == 0.0 {
      return Ok(Vec::new());
    }
    let mut scored: Vec<(f32, &str)> = self
      .corpus
      .iter()
      .map(|(id, vec, norm)| {
        let denom = q_norm * norm;
        let score = if denom == 0.0 {
          0.0
        } else {
          dot(qvec, vec) / denom
        };
        (score, id.as_str())
      })
      .collect();
    // Top-k by score descending. `sort_by` is stable, which keeps
    // ties deterministic across runs (important for paired sign-test
    // comparisons in the eval harness).
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    Ok(
      scored
        .into_iter()
        .take(k)
        .map(|(_, id)| id.to_string())
        .collect(),
    )
  }
}

fn l2_norm(v: &[f32]) -> f32 {
  v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
  // Both slices are the same length by `DenseEval::new`'s dim check.
  // `zip` over the shorter is a defensive coder against future API
  // changes that loosen that invariant.
  a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Hybrid retriever combining any two [`Retriever`] trait objects via
/// Reciprocal Rank Fusion (RRF). The fusion formula is:
///
/// ```text
/// score(d) = Σ_i 1 / (k + rank_i(d))
/// ```
///
/// where `rank_i(d)` is the 1-indexed rank of doc `d` in retriever
/// `i`'s output, and `k` is a smoothing constant (default 60, the
/// standard recipe from Cormack-Clarke-Buettcher 2009).
///
/// Inputs from each backend are fetched at `inner_k` (often 2-3×
/// the requested `k`) so that mid-rank docs from one backend still
/// have a chance to win on the fusion score.
pub struct HybridEval {
  label: String,
  primary: Box<dyn Retriever>,
  secondary: Box<dyn Retriever>,
  rrf_k: f32,
  /// How many docs to fetch from each backend before fusing. Set to
  /// 0 to make it default to `2 * eval_k` at query time.
  inner_k_multiplier: usize,
}

impl HybridEval {
  /// Default RRF smoothing constant per Cormack et al. (2009).
  pub const DEFAULT_RRF_K: f32 = 60.0;
  /// Default inner-k multiplier. Each backend is queried for
  /// `inner_k_multiplier * eval_k` candidates before fusion.
  pub const DEFAULT_INNER_K_MULTIPLIER: usize = 3;

  /// Build a hybrid retriever with the default RRF constant
  /// (`k = 60`) and inner-k multiplier (`3 × eval_k`).
  pub fn new(
    label: impl Into<String>,
    primary: Box<dyn Retriever>,
    secondary: Box<dyn Retriever>,
  ) -> Self {
    Self::with_params(
      label,
      primary,
      secondary,
      Self::DEFAULT_RRF_K,
      Self::DEFAULT_INNER_K_MULTIPLIER,
    )
  }

  /// Build a hybrid retriever with custom RRF parameters.
  ///
  /// `rrf_k` smooths the contribution of low-ranked docs; values
  /// 10–100 are common, 60 is the default that performs well across
  /// most BEIR benchmarks. `inner_k_multiplier` controls how many
  /// candidates each backend supplies — 0 falls back to the default
  /// of 3× the requested eval `k`.
  pub fn with_params(
    label: impl Into<String>,
    primary: Box<dyn Retriever>,
    secondary: Box<dyn Retriever>,
    rrf_k: f32,
    inner_k_multiplier: usize,
  ) -> Self {
    let multiplier = if inner_k_multiplier == 0 {
      Self::DEFAULT_INNER_K_MULTIPLIER
    } else {
      inner_k_multiplier
    };
    Self {
      label: label.into(),
      primary,
      secondary,
      rrf_k,
      inner_k_multiplier: multiplier,
    }
  }
}

impl Retriever for HybridEval {
  fn name(&self) -> &str {
    &self.label
  }

  fn search(&self, query: &str, k: usize) -> Result<Vec<String>> {
    if k == 0 {
      return Ok(Vec::new());
    }
    let inner_k = k.saturating_mul(self.inner_k_multiplier).max(k);
    let primary_results = self.primary.search(query, inner_k)?;
    let secondary_results = self.secondary.search(query, inner_k)?;

    let mut rrf_scores: HashMap<String, f32> = HashMap::new();
    for (rank0, id) in primary_results.iter().enumerate() {
      // RRF rank is 1-indexed.
      let contribution = 1.0 / (self.rrf_k + (rank0 + 1) as f32);
      *rrf_scores.entry(id.clone()).or_insert(0.0) += contribution;
    }
    for (rank0, id) in secondary_results.iter().enumerate() {
      let contribution = 1.0 / (self.rrf_k + (rank0 + 1) as f32);
      *rrf_scores.entry(id.clone()).or_insert(0.0) += contribution;
    }

    let mut scored: Vec<(String, f32)> = rrf_scores.into_iter().collect();
    // Sort by score desc; break ties by id asc so the output is
    // deterministic (HashMap iteration order is randomised, which
    // would otherwise leak into the report).
    scored.sort_by(|a, b| {
      b.1
        .partial_cmp(&a.1)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| a.0.cmp(&b.0))
    });
    Ok(scored.into_iter().take(k).map(|(id, _)| id).collect())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::eval::dataset::{CorpusDoc, Judgment, Query};
  use crate::eval::runner::{EvalConfig, evaluate};
  use std::collections::HashMap;

  fn dataset_for_bm25() -> Dataset {
    let corpus = vec![
      CorpusDoc {
        id: "d1".into(),
        text: "machine learning improves predictions".into(),
        title: None,
      },
      CorpusDoc {
        id: "d2".into(),
        text: "the quick brown fox jumps over the lazy dog".into(),
        title: None,
      },
      CorpusDoc {
        id: "d3".into(),
        text: "deep learning neural networks".into(),
        title: None,
      },
    ];
    let queries = vec![Query {
      id: "q1".into(),
      text: "deep learning".into(),
      notes: None,
    }];
    let mut rel = HashMap::new();
    rel.insert("d3".to_string(), 1u8);
    let judgments = vec![Judgment {
      query_id: "q1".into(),
      relevances: rel,
      notes: None,
    }];
    Dataset::new(corpus, queries, judgments)
  }

  #[test]
  fn bm25_eval_finds_obvious_match() {
    let dataset = dataset_for_bm25();
    let retriever = Bm25Eval::from_dataset(&dataset);
    let config = EvalConfig {
      k_values: vec![1, 3],
      label: "bm25".into(),
    };
    let report = evaluate(&retriever, &dataset, &config).unwrap();
    let recall_1 = report.per_k.iter().find(|r| r.k == 1).unwrap();
    assert!((recall_1.recall - 1.0).abs() < 1e-9);
    assert_eq!(report.retriever, "bm25");
  }

  #[test]
  fn bm25_eval_with_custom_params() {
    let dataset = dataset_for_bm25();
    let retriever = Bm25Eval::with_params(&dataset, 1.5, 0.5);
    let result = retriever.search("deep learning", 1).unwrap();
    assert_eq!(result.first().map(|s| s.as_str()), Some("d3"));
  }

  #[test]
  fn bm25_eval_uses_title_when_present() {
    let dataset = Dataset::new(
      vec![CorpusDoc {
        id: "d1".into(),
        text: "body".into(),
        title: Some("rare-keyword".into()),
      }],
      vec![Query {
        id: "q1".into(),
        text: "rare-keyword".into(),
        notes: None,
      }],
      vec![],
    );
    let retriever = Bm25Eval::from_dataset(&dataset);
    let result = retriever.search("rare-keyword", 1).unwrap();
    assert_eq!(result.first().map(|s| s.as_str()), Some("d1"));
  }

  // ── DenseEval tests ─────────────────────────────────────────────────
  //
  // All mock vectors; no embedding API needed. The vectors are picked
  // so the closest neighbours are obvious by inspection.

  fn dense_query_vectors(pairs: &[(&str, [f32; 2])]) -> HashMap<String, Vec<f32>> {
    pairs
      .iter()
      .map(|(text, v)| ((*text).to_string(), v.to_vec()))
      .collect()
  }

  #[test]
  fn dense_eval_ranks_by_cosine_similarity() {
    // Query vector points along (1, 0). doc1 is colinear, doc2 is
    // orthogonal. Top-1 must be doc1.
    let corpus = vec![
      ("doc1".to_string(), vec![1.0_f32, 0.0]),
      ("doc2".to_string(), vec![0.0_f32, 1.0]),
    ];
    let queries = dense_query_vectors(&[("east", [1.0, 0.0])]);
    let retriever = DenseEval::new("dense:mock", corpus, queries).unwrap();
    let top1 = retriever.search("east", 1).unwrap();
    assert_eq!(top1, vec!["doc1".to_string()]);
    // Top-2 must include both, doc1 first.
    let top2 = retriever.search("east", 2).unwrap();
    assert_eq!(top2, vec!["doc1".to_string(), "doc2".to_string()]);
  }

  #[test]
  fn dense_eval_unknown_query_returns_empty() {
    // The runner treats unknown queries as zero recall rather than
    // aborting the eval — covers partial embedding caches.
    let corpus = vec![("doc1".to_string(), vec![1.0_f32, 0.0])];
    let queries = dense_query_vectors(&[("known", [1.0, 0.0])]);
    let retriever = DenseEval::new("dense:mock", corpus, queries).unwrap();
    let out = retriever.search("never-embedded", 5).unwrap();
    assert!(out.is_empty());
  }

  #[test]
  fn dense_eval_rejects_dimension_mismatch() {
    let corpus = vec![
      ("doc1".to_string(), vec![1.0_f32, 0.0]),
      ("doc2".to_string(), vec![1.0_f32, 0.0, 0.0]), // wrong dim
    ];
    let queries = HashMap::new();
    // Avoid `.expect_err`: `DenseEval` is non-Debug. `match` keeps
    // the assertion focused on the error message.
    match DenseEval::new("dense:mock", corpus, queries) {
      Err(err) => assert!(
        format!("{err:?}").contains("dimension mismatch"),
        "wrong error: {err:?}",
      ),
      Ok(_) => panic!("must reject mismatched dims"),
    }
  }

  #[test]
  fn dense_eval_rejects_empty_corpus() {
    match DenseEval::new("dense:mock", vec![], HashMap::new()) {
      Err(err) => assert!(
        format!("{err:?}").contains("at least one corpus vector"),
        "wrong error: {err:?}",
      ),
      Ok(_) => panic!("must reject empty corpus"),
    }
  }

  #[test]
  fn dense_eval_zero_query_vector_returns_empty() {
    // A zero-norm query vector can't produce a meaningful score.
    // Returning empty matches the unknown-query path so the runner
    // surfaces it as zero recall rather than NaN scores.
    let corpus = vec![("doc1".to_string(), vec![1.0_f32, 0.0])];
    let queries = dense_query_vectors(&[("blank", [0.0, 0.0])]);
    let retriever = DenseEval::new("dense:mock", corpus, queries).unwrap();
    let out = retriever.search("blank", 5).unwrap();
    assert!(out.is_empty());
  }

  // ── HybridEval (RRF) tests ──────────────────────────────────────────

  /// Scripted retriever for hybrid tests — returns a fixed ranking
  /// regardless of query. Lets us pin the RRF arithmetic without
  /// coupling to BM25 or dense scoring details.
  struct ScriptedFixed {
    name: String,
    ranking: Vec<String>,
  }

  impl Retriever for ScriptedFixed {
    fn name(&self) -> &str {
      &self.name
    }
    fn search(&self, _query: &str, k: usize) -> Result<Vec<String>> {
      Ok(self.ranking.iter().take(k).cloned().collect())
    }
  }

  fn scripted(name: &str, ids: &[&str]) -> Box<dyn Retriever> {
    Box::new(ScriptedFixed {
      name: name.to_string(),
      ranking: ids.iter().map(|s| s.to_string()).collect(),
    })
  }

  #[test]
  fn hybrid_eval_promotes_doc_ranked_high_by_both_backends() {
    // doc_a is rank 1 in primary AND rank 1 in secondary. doc_b is
    // rank 2 in primary, never appears in secondary. RRF must rank
    // doc_a first because it gets a contribution from BOTH backends.
    let primary = scripted("p", &["doc_a", "doc_b"]);
    let secondary = scripted("s", &["doc_a", "doc_c"]);
    let hybrid = HybridEval::new("hybrid", primary, secondary);
    let result = hybrid.search("anything", 3).unwrap();
    assert_eq!(result.first().map(|s| s.as_str()), Some("doc_a"));
  }

  #[test]
  fn hybrid_eval_handles_disjoint_results_via_rrf_smoothing() {
    // Primary returns only doc_p; secondary returns only doc_s.
    // Both have rank 1 in their respective lists → tied RRF scores
    // → tie-break is by id ascending (deterministic), so doc_p
    // comes before doc_s.
    let primary = scripted("p", &["doc_p"]);
    let secondary = scripted("s", &["doc_s"]);
    let hybrid = HybridEval::new("hybrid", primary, secondary);
    let result = hybrid.search("anything", 2).unwrap();
    assert_eq!(
      result,
      vec!["doc_p".to_string(), "doc_s".to_string()],
      "tie-break is alphabetical for deterministic output"
    );
  }

  #[test]
  fn hybrid_eval_respects_k_cap() {
    let primary = scripted("p", &["a", "b", "c", "d", "e"]);
    let secondary = scripted("s", &["a", "b", "c", "d", "e"]);
    let hybrid = HybridEval::new("hybrid", primary, secondary);
    let result = hybrid.search("q", 2).unwrap();
    assert_eq!(result.len(), 2);
  }

  #[test]
  fn hybrid_eval_zero_k_returns_empty() {
    let primary = scripted("p", &["a"]);
    let secondary = scripted("s", &["a"]);
    let hybrid = HybridEval::new("hybrid", primary, secondary);
    assert!(hybrid.search("q", 0).unwrap().is_empty());
  }

  #[test]
  fn hybrid_eval_low_ranked_doc_with_two_hits_beats_top_doc_with_one_hit() {
    // doc_top: primary rank 1 only → RRF = 1/(60+1) ≈ 0.01639
    // doc_low: primary rank 5 + secondary rank 5 → RRF = 2/(60+5) ≈ 0.03077
    // doc_low must win. This is the canonical RRF property: two
    // moderate ranks beat one strong rank.
    let primary = scripted("p", &["doc_top", "p2", "p3", "p4", "doc_low"]);
    let secondary = scripted("s", &["s1", "s2", "s3", "s4", "doc_low"]);
    let hybrid = HybridEval::new("hybrid", primary, secondary);
    let result = hybrid.search("q", 10).unwrap();
    let top_doc = result.first().map(|s| s.as_str());
    assert_eq!(
      top_doc,
      Some("doc_low"),
      "RRF: two moderate ranks should beat one top rank, got {result:?}",
    );
  }

  #[test]
  fn hybrid_eval_custom_inner_k_multiplier_widens_candidate_pool() {
    // With multiplier = 5 and eval k = 2, each backend is queried
    // for 10 candidates. Verifies the multiplier flows through.
    let primary = scripted("p", &["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"]);
    let secondary = scripted("s", &["j", "i", "h", "g", "f", "e", "d", "c", "b", "a"]);
    let hybrid =
      HybridEval::with_params("hybrid", primary, secondary, HybridEval::DEFAULT_RRF_K, 5);
    let result = hybrid.search("q", 2).unwrap();
    assert_eq!(result.len(), 2);
    // The pair (a, j) both appear at extreme ranks in each list —
    // their RRF scores should be equal; tie-break by id ascending
    // means `a` wins. The structural assertion is that 2 distinct
    // docs come back; the specific identity locks the determinism.
    assert!(
      result.contains(&"a".to_string()),
      "expected 'a' (rank 1 in primary, rank 10 in secondary) in top-2: {result:?}",
    );
  }
}

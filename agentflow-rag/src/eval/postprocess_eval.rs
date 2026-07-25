//! Eval-harness integration for the L4.2 post-processor chain — the
//! mechanism `rag eval` uses to prove a rerank/compress/filter chain
//! actually helps before it's allowed to merge ("no gain, no merge").
//!
//! The eval harness's [`Retriever`] trait is deliberately synchronous
//! (`agentflow-rag/src/eval/runner.rs`'s module docs explain why: eval runs
//! are batched/offline and every query is scored in one process, so forcing
//! an async runtime onto every backend isn't worth it). A
//! [`crate::postprocess::PostProcessor`] is async (an LLM-backed
//! [`crate::postprocess::RelevanceScorer`] needs to be).
//! [`build_post_processed_retriever`] bridges the two by running the
//! chain once, up front, over every query in the dataset, and baking the
//! result into a [`PrecomputedRetriever`] — a trivial sync lookup — so the
//! existing `evaluate()` / `compare()` machinery needs no changes at all.

use std::collections::HashMap;

use agentflow_store_spi::KnowledgeChunk;

use super::dataset::Dataset;
use super::runner::Retriever;
use crate::error::Result;
use crate::postprocess::PostProcessorChain;

/// A [`Retriever`] backed by a fixed `query text -> ranked doc ids` map,
/// computed ahead of time (typically by [`build_post_processed_retriever`]).
pub struct PrecomputedRetriever {
  name: String,
  results: HashMap<String, Vec<String>>,
}

impl PrecomputedRetriever {
  pub fn new(name: impl Into<String>, results: HashMap<String, Vec<String>>) -> Self {
    Self {
      name: name.into(),
      results,
    }
  }
}

impl Retriever for PrecomputedRetriever {
  fn name(&self) -> &str {
    &self.name
  }

  fn search(&self, query: &str, k: usize) -> Result<Vec<String>> {
    Ok(
      self
        .results
        .get(query)
        .map(|ids| ids.iter().take(k).cloned().collect())
        .unwrap_or_default(),
    )
  }
}

/// Run `base`'s top-`fetch_k` results for every query in `dataset` through
/// `chain`, and bake the post-processed ranking into a
/// [`PrecomputedRetriever`] labeled `label`.
///
/// `base.search()` returns bare ids (no score, no content — the
/// [`Retriever`] contract predates chunk-level metadata), so this
/// synthesizes a [`KnowledgeChunk`] per result: `content` from the
/// dataset's corpus (falls back to an empty string for an id the corpus
/// doesn't have — this happens on a chunked dataset the caller passed the
/// *un-chunked* `dataset` argument for by mistake; the post-processor still
/// runs, just scoring/filtering an empty passage) and a synthetic
/// `score = 1.0 / (rank + 1)` — the same reciprocal-rank convention
/// [`crate::eval::retrievers::HybridEval`]'s RRF fusion already uses,
/// standing in for a real retrieval score since [`Retriever`] doesn't carry
/// one. This is exactly what lets [`crate::postprocess::ScoreRelevanceScorer`]
/// (or any rank-sensitive scorer) reason about the base ranking without the
/// eval harness needing to grow a richer `Retriever::search` return type.
pub async fn build_post_processed_retriever(
  base: &dyn Retriever,
  dataset: &Dataset,
  fetch_k: usize,
  chain: &PostProcessorChain,
  label: impl Into<String>,
) -> Result<PrecomputedRetriever> {
  let corpus_text: HashMap<&str, &str> = dataset
    .corpus
    .iter()
    .map(|doc| (doc.id.as_str(), doc.text.as_str()))
    .collect();

  let mut results: HashMap<String, Vec<String>> = HashMap::with_capacity(dataset.queries.len());
  for query in &dataset.queries {
    let ranked_ids = base.search(&query.text, fetch_k)?;
    let chunks: Vec<KnowledgeChunk> = ranked_ids
      .iter()
      .enumerate()
      .map(|(rank, id)| KnowledgeChunk {
        id: id.clone(),
        content: corpus_text
          .get(id.as_str())
          .copied()
          .unwrap_or("")
          .to_string(),
        score: 1.0 / (rank as f32 + 1.0),
        source: None,
        metadata: HashMap::new(),
      })
      .collect();
    let processed = chain.run(&query.text, chunks).await?;
    let ids: Vec<String> = processed.into_iter().map(|c| c.id).collect();
    results.insert(query.text.clone(), ids);
  }

  Ok(PrecomputedRetriever::new(label, results))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::eval::dataset::{CorpusDoc, Judgment, Query};
  use crate::eval::runner::{EvalConfig, evaluate};
  use crate::eval::{ComparisonReport, compare, requires_gain};
  use crate::postprocess::{RelevanceFilterProcessor, RelevanceScorer, RerankProcessor};
  use async_trait::async_trait;
  use std::sync::Arc;

  fn dataset_with_a_decoy() -> Dataset {
    // d1 is the truly relevant doc but BM25 (or any lexical baseline)
    // ranks the keyword-stuffed decoy d2 first. A relevance-filtering
    // post-processor that knows d2 is off-topic should recover d1 at
    // rank 1, which is exactly the kind of gain L4.2's eval hook needs
    // to be able to prove.
    let corpus = vec![
      CorpusDoc {
        id: "d1".into(),
        text: "rust ownership prevents data races".into(),
        title: None,
      },
      CorpusDoc {
        id: "d2".into(),
        text: "rust rust rust rust rust rust rust rust".into(),
        title: None,
      },
    ];
    let queries = vec![Query {
      id: "q1".into(),
      text: "rust".into(),
      notes: None,
    }];
    let mut rel = HashMap::new();
    rel.insert("d1".to_string(), 1u8);
    let judgments = vec![Judgment {
      query_id: "q1".into(),
      relevances: rel,
      notes: None,
    }];
    Dataset::new(corpus, queries, judgments)
  }

  /// Scripted base retriever: always returns the decoy first, exactly
  /// like a real lexical retriever would on this fixture.
  struct DecoyFirstRetriever;

  impl Retriever for DecoyFirstRetriever {
    fn name(&self) -> &str {
      "decoy-first"
    }

    fn search(&self, _query: &str, k: usize) -> Result<Vec<String>> {
      Ok(
        vec!["d2".to_string(), "d1".to_string()]
          .into_iter()
          .take(k)
          .collect(),
      )
    }
  }

  /// A scorer that knows d2 is keyword-stuffed noise — the stand-in for
  /// an LLM-backed [`RelevanceScorer`] a caller with an LLM dependency
  /// would supply in production.
  struct KnowsTheDecoyScorer;

  #[async_trait]
  impl RelevanceScorer for KnowsTheDecoyScorer {
    async fn score(&self, _query: &str, chunks: &[KnowledgeChunk]) -> Result<Vec<f32>> {
      Ok(
        chunks
          .iter()
          .map(|c| if c.id == "d2" { 0.0 } else { 1.0 })
          .collect(),
      )
    }
    fn name(&self) -> &str {
      "knows_the_decoy"
    }
  }

  #[tokio::test]
  async fn build_post_processed_retriever_reflects_chain_output() {
    let dataset = dataset_with_a_decoy();
    let base = DecoyFirstRetriever;
    let scorer: Arc<dyn RelevanceScorer> = Arc::new(KnowsTheDecoyScorer);
    let chain = PostProcessorChain::new(vec![Arc::new(RerankProcessor::new(scorer))]);

    let candidate = build_post_processed_retriever(&base, &dataset, 5, &chain, "reranked")
      .await
      .expect("build ok");
    assert_eq!(candidate.name(), "reranked");
    let ranked = candidate.search("rust", 2).expect("search ok");
    assert_eq!(
      ranked,
      vec!["d1".to_string(), "d2".to_string()],
      "rerank must promote the truly relevant doc above the decoy"
    );
  }

  #[tokio::test]
  async fn post_processed_retriever_proves_a_recall_gain_over_the_baseline() {
    // End-to-end: baseline (decoy-first) EvalReport vs. candidate
    // (post-processed) EvalReport, compared via the SAME compare()
    // used for chunking/retriever regressions elsewhere in this
    // harness — proving L4.2's "no gain, no merge" bar is checkable
    // with existing machinery, not bespoke plumbing.
    let dataset = dataset_with_a_decoy();
    let base = DecoyFirstRetriever;
    let config = EvalConfig {
      k_values: vec![1],
      label: "baseline".into(),
    };
    let baseline_report = evaluate(&base, &dataset, &config).expect("baseline eval ok");
    // Baseline misses at K=1 because the decoy is ranked first.
    let baseline_recall_1 = baseline_report.per_k[0].recall;
    assert!(
      baseline_recall_1 < 1.0,
      "fixture must actually decoy the baseline"
    );

    let scorer: Arc<dyn RelevanceScorer> = Arc::new(KnowsTheDecoyScorer);
    let chain = PostProcessorChain::new(vec![Arc::new(RerankProcessor::new(scorer))]);
    let candidate_retriever =
      build_post_processed_retriever(&base, &dataset, 5, &chain, "reranked")
        .await
        .expect("build ok");
    let candidate_config = EvalConfig {
      k_values: vec![1],
      label: "candidate".into(),
    };
    let candidate_report =
      evaluate(&candidate_retriever, &dataset, &candidate_config).expect("candidate eval ok");
    assert!((candidate_report.per_k[0].recall - 1.0).abs() < 1e-9);

    let cmp: ComparisonReport = compare(&baseline_report, &candidate_report);
    let recall_gain = cmp
      .deltas
      .iter()
      .find(|d| d.metric == "Recall@1")
      .map(|d| d.abs_delta)
      .expect("Recall@1 delta present");
    assert!(
      recall_gain > 0.0,
      "post-processing must show a measurable recall gain"
    );

    // Single-query fixture: wins=1, losses=0 → p-value is exactly 0.5
    // (P(X<=0 | Binomial(1,0.5))). Use a threshold just above that so the
    // strict `<` comparison in `requires_gain` trips.
    let decision = requires_gain(&cmp, "Recall@1", 0.1, 0.51);
    assert!(
      decision.gain_confirmed,
      "single-query fixture: gain must be confirmed — {decision:?}"
    );
  }

  #[tokio::test]
  async fn no_gain_chain_is_not_confirmed() {
    // A relevance filter with a threshold so low it filters nothing
    // changes nothing about the ranking — proving the harness doesn't
    // rubber-stamp a no-op chain as a "gain."
    let dataset = dataset_with_a_decoy();
    let base = DecoyFirstRetriever;
    let config = EvalConfig {
      k_values: vec![1],
      label: "baseline".into(),
    };
    let baseline_report = evaluate(&base, &dataset, &config).expect("baseline eval ok");

    let noop_scorer: Arc<dyn RelevanceScorer> = Arc::new(crate::postprocess::ScoreRelevanceScorer);
    let noop_chain = PostProcessorChain::new(vec![Arc::new(RelevanceFilterProcessor::new(
      noop_scorer,
      -1.0, // below every possible synthetic score, so nothing is dropped
    ))]);
    let candidate_retriever =
      build_post_processed_retriever(&base, &dataset, 5, &noop_chain, "noop")
        .await
        .expect("build ok");
    let candidate_report =
      evaluate(&candidate_retriever, &dataset, &config).expect("candidate eval ok");

    let cmp = compare(&baseline_report, &candidate_report);
    let decision = requires_gain(&cmp, "Recall@1", 0.1, 0.5);
    assert!(
      !decision.gain_confirmed,
      "a no-op post-processing chain must not be confirmed as a gain — {decision:?}"
    );
  }
}

//! Eval-harness integration for L4.3 query rewrite / decomposition — lets
//! `rag eval` build a before/after comparison group for a
//! [`crate::rewrite::QueryRewriter`], the same way
//! [`super::postprocess_eval`] does for L4.2's post-processor chain.
//!
//! Bridges the eval harness's sync [`Retriever`] and the async
//! [`QueryRewriter`] the same way [`super::postprocess_eval`] bridges
//! [`Retriever`] and the async `PostProcessor` chain: rewrite + fan-out +
//! RRF-fuse every query up front, bake the result into a
//! [`super::postprocess_eval::PrecomputedRetriever`], then score it with
//! the existing `evaluate()` / `compare()` / `requires_gain()` machinery —
//! no changes needed to the harness itself.

use std::collections::HashMap;

use agentflow_store_spi::KnowledgeChunk;

use super::dataset::Dataset;
use super::postprocess_eval::PrecomputedRetriever;
use super::runner::Retriever;
use crate::error::Result;
use crate::rewrite::{QueryRewriter, fuse_multi_query_results};

/// Build a [`PrecomputedRetriever`] labeled `label` by, for every query in
/// `dataset`: rewriting it via `rewriter`, running `base.search()` for each
/// rewritten query at `fetch_k`, and RRF-fusing the per-query id lists
/// (via synthetic `1/(rank+1)` scores — the same convention
/// [`super::postprocess_eval::build_post_processed_retriever`] uses, since
/// [`Retriever::search`] doesn't carry real scores).
pub async fn build_multi_query_retriever(
  base: &dyn Retriever,
  dataset: &Dataset,
  fetch_k: usize,
  rewriter: &dyn QueryRewriter,
  rrf_k: f32,
  label: impl Into<String>,
) -> Result<PrecomputedRetriever> {
  let mut results: HashMap<String, Vec<String>> = HashMap::with_capacity(dataset.queries.len());
  for query in &dataset.queries {
    let rewritten = rewriter.rewrite(&query.text).await?;
    let mut per_query: Vec<Vec<KnowledgeChunk>> = Vec::with_capacity(rewritten.len());
    for rewritten_query in &rewritten {
      let ranked_ids = base.search(rewritten_query, fetch_k)?;
      let chunks: Vec<KnowledgeChunk> = ranked_ids
        .into_iter()
        .enumerate()
        .map(|(rank, id)| KnowledgeChunk {
          id,
          content: String::new(),
          score: 1.0 / (rank as f32 + 1.0),
          source: None,
          metadata: HashMap::new(),
        })
        .collect();
      per_query.push(chunks);
    }
    let fused = fuse_multi_query_results(per_query, rrf_k, fetch_k);
    results.insert(
      query.text.clone(),
      fused.into_iter().map(|c| c.id).collect(),
    );
  }
  Ok(PrecomputedRetriever::new(label, results))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::eval::dataset::{CorpusDoc, Judgment, Query};
  use crate::eval::runner::{EvalConfig, evaluate};
  use crate::eval::{compare, requires_gain};
  use crate::rewrite::SplitQueryRewriter;

  fn dataset_needing_decomposition() -> Dataset {
    // A lexical baseline searching the raw compound query "ownership and
    // borrowing" scores worse than searching the two halves separately,
    // because each corpus doc only contains ONE of the two keywords —
    // the compound query's terms are split across documents, exactly the
    // scenario query decomposition is meant to fix.
    let corpus = vec![
      CorpusDoc {
        id: "d_ownership".into(),
        text: "ownership rules in rust".into(),
        title: None,
      },
      CorpusDoc {
        id: "d_borrowing".into(),
        text: "borrowing and references explained".into(),
        title: None,
      },
      CorpusDoc {
        id: "d_unrelated".into(),
        text: "python list comprehensions".into(),
        title: None,
      },
    ];
    let queries = vec![Query {
      id: "q1".into(),
      text: "ownership and borrowing".into(),
      notes: None,
    }];
    let mut rel = HashMap::new();
    rel.insert("d_ownership".to_string(), 1u8);
    rel.insert("d_borrowing".to_string(), 1u8);
    let judgments = vec![Judgment {
      query_id: "q1".into(),
      relevances: rel,
      notes: None,
    }];
    Dataset::new(corpus, queries, judgments)
  }

  /// Scripted retriever: exact substring match against the query,
  /// standing in for a real lexical retriever without pulling in BM25 —
  /// keeps the fixture's "compound query splits across two docs"
  /// property exact and easy to reason about.
  struct SubstringRetriever {
    corpus: Vec<(String, String)>,
  }

  impl Retriever for SubstringRetriever {
    fn name(&self) -> &str {
      "substring"
    }

    fn search(&self, query: &str, k: usize) -> Result<Vec<String>> {
      let query_lower = query.to_ascii_lowercase();
      let mut hits: Vec<String> = self
        .corpus
        .iter()
        .filter(|(_, text)| {
          query_lower
            .split_whitespace()
            .all(|term| text.to_ascii_lowercase().contains(term))
        })
        .map(|(id, _)| id.clone())
        .collect();
      hits.sort();
      hits.truncate(k);
      Ok(hits)
    }
  }

  fn substring_retriever(dataset: &Dataset) -> SubstringRetriever {
    SubstringRetriever {
      corpus: dataset
        .corpus
        .iter()
        .map(|d| (d.id.clone(), d.text.clone()))
        .collect(),
    }
  }

  #[tokio::test]
  async fn build_multi_query_retriever_recovers_docs_split_across_sub_queries() {
    let dataset = dataset_needing_decomposition();
    let base = substring_retriever(&dataset);

    // Baseline: the raw compound query matches nothing (no single doc
    // contains every term).
    let baseline_hits = base.search("ownership and borrowing", 5).unwrap();
    assert!(
      baseline_hits.is_empty(),
      "fixture must actually stump the raw-query baseline"
    );

    let candidate = build_multi_query_retriever(
      &base,
      &dataset,
      5,
      &SplitQueryRewriter,
      60.0,
      "split-rewrite",
    )
    .await
    .expect("build ok");
    let ranked = candidate
      .search("ownership and borrowing", 5)
      .expect("search ok");
    assert!(ranked.contains(&"d_ownership".to_string()));
    assert!(ranked.contains(&"d_borrowing".to_string()));
  }

  #[tokio::test]
  async fn query_rewrite_shows_a_confirmed_recall_gain_over_the_raw_query_baseline() {
    let dataset = dataset_needing_decomposition();
    let base = substring_retriever(&dataset);
    let config = EvalConfig {
      k_values: vec![2],
      label: "baseline".into(),
    };
    let baseline_report = evaluate(&base, &dataset, &config).expect("baseline eval ok");
    assert!(
      baseline_report.per_k[0].recall < 1.0,
      "fixture must actually stump the raw-query baseline"
    );

    let candidate_retriever = build_multi_query_retriever(
      &base,
      &dataset,
      5,
      &SplitQueryRewriter,
      60.0,
      "split-rewrite",
    )
    .await
    .expect("build ok");
    let candidate_report =
      evaluate(&candidate_retriever, &dataset, &config).expect("candidate eval ok");
    assert!((candidate_report.per_k[0].recall - 1.0).abs() < 1e-9);

    let cmp = compare(&baseline_report, &candidate_report);
    let decision = requires_gain(&cmp, "Recall@2", 0.1, 0.51);
    assert!(
      decision.gain_confirmed,
      "query decomposition must show a confirmed recall gain — {decision:?}"
    );
  }
}

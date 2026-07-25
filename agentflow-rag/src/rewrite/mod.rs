//! Query rewrite / decomposition (L4.3).
//!
//! An optional step in front of a [`agentflow_store_spi::KnowledgeBackend`]
//! search: replace the caller's query with one or more rewritten queries
//! (a paraphrase, a split into sub-queries, or both), fan out a search per
//! rewritten query, and fuse the per-query rankings via Reciprocal Rank
//! Fusion (RRF) — the same fusion recipe [`crate::eval::retrievers::HybridEval`]
//! already uses to combine two retrievers, generalized here to combine N
//! query variants of one retriever instead.
//!
//! `agentflow-rag` has no dependency on `agentflow-llm` (see
//! `docs/RFC_CRATE_ARCHITECTURE.md` — capability crates stay off each
//! other; also explained in `crate::postprocess`'s module docs for the
//! L4.2 sibling feature). [`QueryRewriter`] is the injection point: a
//! caller that already depends on `agentflow-llm` supplies an LLM-backed
//! rewriter for true paraphrase / semantic decomposition.
//! [`SplitQueryRewriter`] is the dependency-free fallback that decomposes a
//! compound query on conjunction words / punctuation — a real, testable
//! "拆分子查询" (split into sub-queries) implementation, just a
//! syntactic one rather than a semantic one.

use std::collections::HashMap;
use std::sync::Arc;

use agentflow_store_spi::{KnowledgeBackend, KnowledgeChunk, KnowledgeError};
use async_trait::async_trait;

use crate::error::Result;
use crate::knowledge::knowledge_error;

/// Rewrites a query into one or more queries to search with.
#[async_trait]
pub trait QueryRewriter: Send + Sync {
  /// Returns at least one query. `vec![query.to_string()]` is a valid
  /// (identity) response.
  async fn rewrite(&self, query: &str) -> Result<Vec<String>>;

  fn name(&self) -> &str;
}

/// No-op rewriter: returns the original query unchanged. The default when
/// no smarter rewriter is configured.
pub struct IdentityQueryRewriter;

#[async_trait]
impl QueryRewriter for IdentityQueryRewriter {
  async fn rewrite(&self, query: &str) -> Result<Vec<String>> {
    Ok(vec![query.to_string()])
  }

  fn name(&self) -> &str {
    "identity"
  }
}

/// Deterministic, dependency-free decomposition: splits a compound query on
/// conjunction words (" and ", " or ") and `,`/`;`, trimming and dropping
/// empty pieces. A query with no split points decomposes to itself
/// unchanged (never returns zero sub-queries).
///
/// Case-insensitive on the conjunction words so `"X And Y"` splits the same
/// as `"x and y"`.
pub struct SplitQueryRewriter;

#[async_trait]
impl QueryRewriter for SplitQueryRewriter {
  async fn rewrite(&self, query: &str) -> Result<Vec<String>> {
    let normalized = query.replace([',', ';'], " , ");
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in normalized.split_whitespace() {
      let lower = word.to_ascii_lowercase();
      if lower == "and" || lower == "or" || word == "," {
        if !current.trim().is_empty() {
          parts.push(current.trim().to_string());
        }
        current.clear();
      } else {
        if !current.is_empty() {
          current.push(' ');
        }
        current.push_str(word);
      }
    }
    if !current.trim().is_empty() {
      parts.push(current.trim().to_string());
    }
    if parts.is_empty() {
      parts.push(query.trim().to_string());
    }
    Ok(parts)
  }

  fn name(&self) -> &str {
    "split"
  }
}

/// RRF-fuse per-rewritten-query result lists into one ranking, deduping by
/// chunk id and overwriting each survivor's `.score` with its fused RRF
/// score (so a downstream [`crate::postprocess::PostProcessor`] sees a
/// meaningful post-fusion score rather than a stale single-query one).
/// Ties break by id ascending for determinism.
pub(crate) fn fuse_multi_query_results(
  per_query_results: Vec<Vec<KnowledgeChunk>>,
  rrf_k: f32,
  top_k: usize,
) -> Vec<KnowledgeChunk> {
  let mut fused: HashMap<String, (f32, KnowledgeChunk)> = HashMap::new();
  for results in per_query_results {
    for (rank0, chunk) in results.into_iter().enumerate() {
      let contribution = 1.0 / (rrf_k + (rank0 + 1) as f32);
      fused
        .entry(chunk.id.clone())
        .and_modify(|(score, _)| *score += contribution)
        .or_insert_with(|| (contribution, chunk));
    }
  }
  let mut scored: Vec<(f32, KnowledgeChunk)> = fused.into_values().collect();
  scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
  scored
    .into_iter()
    .take(top_k)
    .map(|(fused_score, mut chunk)| {
      chunk.score = fused_score;
      chunk
    })
    .collect()
}

/// Wraps any [`KnowledgeBackend`] with a [`QueryRewriter`] applied before
/// every search: rewrite the query, fan out a search per rewritten query,
/// RRF-fuse the results. Backend-agnostic (operates on the shared
/// [`KnowledgeChunk`] SPI type), same design shape as
/// [`crate::postprocess::PostProcessedKnowledgeBackend`].
pub struct MultiQueryKnowledgeBackend {
  inner: Arc<dyn KnowledgeBackend>,
  rewriter: Arc<dyn QueryRewriter>,
  rrf_k: f32,
  name: String,
}

impl MultiQueryKnowledgeBackend {
  /// Default RRF smoothing constant, matching
  /// [`crate::eval::retrievers::HybridEval::DEFAULT_RRF_K`].
  pub const DEFAULT_RRF_K: f32 = 60.0;

  pub fn new(inner: Arc<dyn KnowledgeBackend>, rewriter: Arc<dyn QueryRewriter>) -> Self {
    let name = inner.name().to_string();
    Self {
      inner,
      rewriter,
      rrf_k: Self::DEFAULT_RRF_K,
      name,
    }
  }

  pub fn with_rrf_k(mut self, rrf_k: f32) -> Self {
    self.rrf_k = rrf_k;
    self
  }
}

#[async_trait]
impl KnowledgeBackend for MultiQueryKnowledgeBackend {
  async fn search(
    &self,
    query: &str,
    top_k: usize,
  ) -> std::result::Result<Vec<KnowledgeChunk>, KnowledgeError> {
    let rewritten = self
      .rewriter
      .rewrite(query)
      .await
      .map_err(knowledge_error)?;
    if rewritten.is_empty() {
      return Err(KnowledgeError::InvalidQuery(format!(
        "QueryRewriter `{}` returned zero queries for `{}`",
        self.rewriter.name(),
        query
      )));
    }
    let mut per_query = Vec::with_capacity(rewritten.len());
    for rewritten_query in &rewritten {
      per_query.push(self.inner.search(rewritten_query, top_k).await?);
    }
    Ok(fuse_multi_query_results(per_query, self.rrf_k, top_k))
  }

  fn name(&self) -> &str {
    &self.name
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::collections::HashMap as StdHashMap;

  fn chunk(id: &str, score: f32) -> KnowledgeChunk {
    KnowledgeChunk {
      id: id.to_string(),
      content: id.to_string(),
      score,
      source: None,
      metadata: StdHashMap::new(),
    }
  }

  #[tokio::test]
  async fn identity_rewriter_returns_query_unchanged() {
    let out = IdentityQueryRewriter
      .rewrite("what is rust")
      .await
      .expect("ok");
    assert_eq!(out, vec!["what is rust".to_string()]);
  }

  #[tokio::test]
  async fn split_rewriter_decomposes_on_and() {
    let out = SplitQueryRewriter
      .rewrite("what is ownership and how does borrowing work")
      .await
      .expect("ok");
    assert_eq!(
      out,
      vec![
        "what is ownership".to_string(),
        "how does borrowing work".to_string()
      ]
    );
  }

  #[tokio::test]
  async fn split_rewriter_decomposes_on_commas_and_is_case_insensitive() {
    let out = SplitQueryRewriter
      .rewrite("rust ownership, borrowing, And lifetimes")
      .await
      .expect("ok");
    assert_eq!(
      out,
      vec![
        "rust ownership".to_string(),
        "borrowing".to_string(),
        "lifetimes".to_string()
      ]
    );
  }

  #[tokio::test]
  async fn split_rewriter_with_no_split_points_returns_query_unchanged() {
    let out = SplitQueryRewriter
      .rewrite("simple query")
      .await
      .expect("ok");
    assert_eq!(out, vec!["simple query".to_string()]);
  }

  #[tokio::test]
  async fn split_rewriter_never_returns_empty() {
    let out = SplitQueryRewriter.rewrite("and or").await.expect("ok");
    assert!(!out.is_empty());
  }

  #[test]
  fn fuse_promotes_doc_ranked_high_in_multiple_sub_queries() {
    let results = vec![
      vec![chunk("a", 0.0), chunk("b", 0.0)],
      vec![chunk("a", 0.0), chunk("c", 0.0)],
    ];
    let fused = fuse_multi_query_results(results, 60.0, 3);
    assert_eq!(fused[0].id, "a", "a appears in both sub-query result sets");
  }

  #[test]
  fn fuse_sets_score_to_the_fused_rrf_value_not_a_stale_per_query_score() {
    let results = vec![vec![chunk("a", 999.0)]];
    let fused = fuse_multi_query_results(results, 60.0, 5);
    assert!(
      (fused[0].score - 1.0 / 61.0).abs() < 1e-6,
      "score must be overwritten with the RRF value: {}",
      fused[0].score
    );
  }

  #[test]
  fn fuse_respects_top_k() {
    let results = vec![vec![chunk("a", 0.0), chunk("b", 0.0), chunk("c", 0.0)]];
    let fused = fuse_multi_query_results(results, 60.0, 2);
    assert_eq!(fused.len(), 2);
  }

  struct StaticBackend {
    answers: StdHashMap<String, Vec<KnowledgeChunk>>,
  }

  #[async_trait]
  impl KnowledgeBackend for StaticBackend {
    async fn search(
      &self,
      query: &str,
      _top_k: usize,
    ) -> std::result::Result<Vec<KnowledgeChunk>, KnowledgeError> {
      Ok(self.answers.get(query).cloned().unwrap_or_default())
    }

    fn name(&self) -> &str {
      "static"
    }
  }

  #[tokio::test]
  async fn multi_query_backend_fans_out_and_fuses() {
    let inner = Arc::new(StaticBackend {
      answers: StdHashMap::from([
        ("ownership".to_string(), vec![chunk("doc_ownership", 0.0)]),
        ("borrowing".to_string(), vec![chunk("doc_borrowing", 0.0)]),
      ]),
    });
    let backend = MultiQueryKnowledgeBackend::new(inner, Arc::new(SplitQueryRewriter));
    let results = backend
      .search("ownership and borrowing", 5)
      .await
      .expect("search ok");
    let ids: Vec<&str> = results.iter().map(|c| c.id.as_str()).collect();
    assert!(ids.contains(&"doc_ownership"));
    assert!(ids.contains(&"doc_borrowing"));
    assert_eq!(
      backend.name(),
      "static",
      "must forward the inner backend's name"
    );
  }

  #[tokio::test]
  async fn multi_query_backend_with_identity_rewriter_matches_single_search() {
    let inner = Arc::new(StaticBackend {
      answers: StdHashMap::from([("q".to_string(), vec![chunk("d1", 0.0), chunk("d2", 0.0)])]),
    });
    let backend = MultiQueryKnowledgeBackend::new(inner, Arc::new(IdentityQueryRewriter));
    let results = backend.search("q", 5).await.expect("search ok");
    let ids: Vec<&str> = results.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec!["d1", "d2"]);
  }
}

//! Composable retrieval post-processor chain (L4.2).
//!
//! Sits after retrieval (a `KnowledgeBackend::search()` call, whether backed
//! by [`crate::knowledge::Bm25KnowledgeBackend`] or
//! [`crate::knowledge::VectorStoreKnowledgeBackend`]) and before the caller
//! (typically [`crate::tool::RagSearchTool`]) sees the results. Three legs,
//! all built on the shared [`RelevanceScorer`] injection point:
//!
//! - **rerank** ([`RerankProcessor`]) — reorder chunks best-first by a
//!   scorer's judgment.
//! - **evidence-question relevance filtering** ([`RelevanceFilterProcessor`])
//!   — drop chunks a scorer judges irrelevant.
//! - **context compression** ([`TruncateCompressor`]) — shrink each chunk's
//!   content to a character budget; deterministic, no scorer needed.
//!
//! `agentflow-rag` deliberately has no dependency on `agentflow-llm` (see
//! `docs/RFC_CRATE_ARCHITECTURE.md` — capability crates stay off each
//! other), so LLM-based reranking / relevance judging (the TODO's stated
//! target; cross-encoder scoring is explicitly deferred) is not implemented
//! here. Instead [`RelevanceScorer`] is the injection point: a caller that
//! already depends on `agentflow-llm` (e.g. `agentflow-cli`) supplies an
//! LLM-backed scorer; [`ScoreRelevanceScorer`] is the dependency-free
//! fallback (reuses each chunk's own retrieval score) used when no smarter
//! scorer is configured, and in tests.

use std::sync::Arc;

use agentflow_store_spi::{KnowledgeBackend, KnowledgeChunk, KnowledgeError};
use async_trait::async_trait;

use crate::error::{RAGError, Result};
use crate::knowledge::knowledge_error;

/// One step in a post-retrieval processing chain: reorders, filters, or
/// rewrites the chunks a search produced.
#[async_trait]
pub trait PostProcessor: Send + Sync {
  async fn process(&self, query: &str, chunks: Vec<KnowledgeChunk>) -> Result<Vec<KnowledgeChunk>>;

  fn name(&self) -> &str;
}

/// Runs a fixed sequence of [`PostProcessor`]s, each seeing the previous
/// step's output. An empty chain is a no-op passthrough.
pub struct PostProcessorChain {
  steps: Vec<Arc<dyn PostProcessor>>,
}

impl PostProcessorChain {
  pub fn new(steps: Vec<Arc<dyn PostProcessor>>) -> Self {
    Self { steps }
  }

  pub async fn run(&self, query: &str, chunks: Vec<KnowledgeChunk>) -> Result<Vec<KnowledgeChunk>> {
    let mut chunks = chunks;
    for step in &self.steps {
      chunks = step.process(query, chunks).await?;
    }
    Ok(chunks)
  }
}

/// Wraps any [`KnowledgeBackend`] with a [`PostProcessorChain`] applied to
/// every search's results before they're returned. Operates on the shared
/// [`KnowledgeChunk`] SPI type rather than on
/// [`crate::retrieval::RetrievalStrategy`] (which only
/// `VectorStoreKnowledgeBackend` uses), so the same chain composes over any
/// backend — BM25 or vector.
pub struct PostProcessedKnowledgeBackend {
  inner: Arc<dyn KnowledgeBackend>,
  chain: PostProcessorChain,
  name: String,
}

impl PostProcessedKnowledgeBackend {
  pub fn new(inner: Arc<dyn KnowledgeBackend>, steps: Vec<Arc<dyn PostProcessor>>) -> Self {
    let name = inner.name().to_string();
    Self {
      inner,
      chain: PostProcessorChain::new(steps),
      name,
    }
  }
}

#[async_trait]
impl KnowledgeBackend for PostProcessedKnowledgeBackend {
  async fn search(
    &self,
    query: &str,
    top_k: usize,
  ) -> std::result::Result<Vec<KnowledgeChunk>, KnowledgeError> {
    let chunks = self.inner.search(query, top_k).await?;
    self.chain.run(query, chunks).await.map_err(knowledge_error)
  }

  fn name(&self) -> &str {
    &self.name
  }
}

/// Scores each chunk's relevance to `query`. The injection point for
/// smarter-than-deterministic (LLM-based, later cross-encoder-based)
/// relevance judging — see the module docs for why the implementation
/// doesn't live in this crate.
#[async_trait]
pub trait RelevanceScorer: Send + Sync {
  /// Returns one score per input chunk, same order, same length as `chunks`.
  async fn score(&self, query: &str, chunks: &[KnowledgeChunk]) -> Result<Vec<f32>>;

  fn name(&self) -> &str;
}

/// Deterministic, dependency-free scorer: reuses each chunk's own retrieval
/// score. The default when no smarter scorer is configured, and the
/// fixture used in this crate's own tests / the eval-harness "does a
/// post-processor chain actually change the ranking" plumbing.
pub struct ScoreRelevanceScorer;

#[async_trait]
impl RelevanceScorer for ScoreRelevanceScorer {
  async fn score(&self, _query: &str, chunks: &[KnowledgeChunk]) -> Result<Vec<f32>> {
    Ok(chunks.iter().map(|c| c.score).collect())
  }

  fn name(&self) -> &str {
    "score"
  }
}

fn scored_chunks(
  scorer: &Arc<dyn RelevanceScorer>,
  scores: Vec<f32>,
  chunks: Vec<KnowledgeChunk>,
) -> Result<Vec<(f32, KnowledgeChunk)>> {
  if scores.len() != chunks.len() {
    return Err(RAGError::retrieval(format!(
      "RelevanceScorer `{}` returned {} scores for {} chunks",
      scorer.name(),
      scores.len(),
      chunks.len()
    )));
  }
  Ok(scores.into_iter().zip(chunks).collect())
}

/// Reorders chunks best-first by `scorer`'s output — the "rerank" leg.
pub struct RerankProcessor {
  scorer: Arc<dyn RelevanceScorer>,
}

impl RerankProcessor {
  pub fn new(scorer: Arc<dyn RelevanceScorer>) -> Self {
    Self { scorer }
  }
}

#[async_trait]
impl PostProcessor for RerankProcessor {
  async fn process(&self, query: &str, chunks: Vec<KnowledgeChunk>) -> Result<Vec<KnowledgeChunk>> {
    let scores = self.scorer.score(query, &chunks).await?;
    let mut scored = scored_chunks(&self.scorer, scores, chunks)?;
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    Ok(scored.into_iter().map(|(_, c)| c).collect())
  }

  fn name(&self) -> &str {
    "rerank"
  }
}

/// Drops chunks whose `scorer` output falls below `min_score` — the
/// "evidence-question relevance filtering" leg.
pub struct RelevanceFilterProcessor {
  scorer: Arc<dyn RelevanceScorer>,
  min_score: f32,
}

impl RelevanceFilterProcessor {
  pub fn new(scorer: Arc<dyn RelevanceScorer>, min_score: f32) -> Self {
    Self { scorer, min_score }
  }
}

#[async_trait]
impl PostProcessor for RelevanceFilterProcessor {
  async fn process(&self, query: &str, chunks: Vec<KnowledgeChunk>) -> Result<Vec<KnowledgeChunk>> {
    let scores = self.scorer.score(query, &chunks).await?;
    let scored = scored_chunks(&self.scorer, scores, chunks)?;
    Ok(
      scored
        .into_iter()
        .filter(|(score, _)| *score >= self.min_score)
        .map(|(_, c)| c)
        .collect(),
    )
  }

  fn name(&self) -> &str {
    "relevance_filter"
  }
}

/// Deterministic context compression: truncates each chunk's `content` to
/// at most `max_chars` characters, appending a truncation marker so a
/// caller can tell the passage was cut. A first, LLM-free cut at the
/// "context compression" leg — LLM-based summarizing compression is a
/// natural follow-up once a scorer-shaped injection point is needed here
/// too, but a hard character budget alone already bounds prompt cost.
pub struct TruncateCompressor {
  max_chars: usize,
}

impl TruncateCompressor {
  pub fn new(max_chars: usize) -> Self {
    Self {
      max_chars: max_chars.max(1),
    }
  }
}

#[async_trait]
impl PostProcessor for TruncateCompressor {
  async fn process(
    &self,
    _query: &str,
    chunks: Vec<KnowledgeChunk>,
  ) -> Result<Vec<KnowledgeChunk>> {
    Ok(
      chunks
        .into_iter()
        .map(|mut chunk| {
          if chunk.content.chars().count() > self.max_chars {
            let truncated: String = chunk.content.chars().take(self.max_chars).collect();
            chunk.content = format!("{truncated}… [truncated]");
          }
          chunk
        })
        .collect(),
    )
  }

  fn name(&self) -> &str {
    "truncate_compressor"
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::collections::HashMap;

  fn chunk(id: &str, content: &str, score: f32) -> KnowledgeChunk {
    KnowledgeChunk {
      id: id.to_string(),
      content: content.to_string(),
      score,
      source: None,
      metadata: HashMap::new(),
    }
  }

  struct StaticBackend {
    chunks: Vec<KnowledgeChunk>,
  }

  #[async_trait]
  impl KnowledgeBackend for StaticBackend {
    async fn search(
      &self,
      query: &str,
      _top_k: usize,
    ) -> std::result::Result<Vec<KnowledgeChunk>, KnowledgeError> {
      if query.trim().is_empty() {
        return Err(KnowledgeError::InvalidQuery("empty".to_string()));
      }
      Ok(self.chunks.clone())
    }

    fn name(&self) -> &str {
      "static"
    }
  }

  /// A scorer that hands back caller-supplied scores keyed by chunk id, so
  /// tests can script "the middle result is actually the most relevant."
  struct ScriptedScorer {
    scores: HashMap<String, f32>,
  }

  #[async_trait]
  impl RelevanceScorer for ScriptedScorer {
    async fn score(&self, _query: &str, chunks: &[KnowledgeChunk]) -> Result<Vec<f32>> {
      Ok(
        chunks
          .iter()
          .map(|c| *self.scores.get(&c.id).unwrap_or(&0.0))
          .collect(),
      )
    }

    fn name(&self) -> &str {
      "scripted"
    }
  }

  #[tokio::test]
  async fn rerank_processor_reorders_best_first() {
    let scorer: Arc<dyn RelevanceScorer> = Arc::new(ScriptedScorer {
      scores: HashMap::from([
        ("a".to_string(), 0.1),
        ("b".to_string(), 0.9),
        ("c".to_string(), 0.5),
      ]),
    });
    let processor = RerankProcessor::new(scorer);
    let chunks = vec![
      chunk("a", "A", 0.0),
      chunk("b", "B", 0.0),
      chunk("c", "C", 0.0),
    ];
    let reordered = processor.process("q", chunks).await.expect("rerank ok");
    let ids: Vec<&str> = reordered.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec!["b", "c", "a"]);
  }

  #[tokio::test]
  async fn relevance_filter_drops_below_threshold() {
    let scorer: Arc<dyn RelevanceScorer> = Arc::new(ScriptedScorer {
      scores: HashMap::from([("a".to_string(), 0.9), ("b".to_string(), 0.1)]),
    });
    let processor = RelevanceFilterProcessor::new(scorer, 0.5);
    let chunks = vec![chunk("a", "A", 0.0), chunk("b", "B", 0.0)];
    let filtered = processor.process("q", chunks).await.expect("filter ok");
    let ids: Vec<&str> = filtered.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec!["a"]);
  }

  #[tokio::test]
  async fn truncate_compressor_shrinks_long_content_only() {
    let compressor = TruncateCompressor::new(5);
    let chunks = vec![
      chunk("a", "short", 0.0),
      chunk("b", "this is way too long", 0.0),
    ];
    let compressed = compressor.process("q", chunks).await.expect("compress ok");
    assert_eq!(
      compressed[0].content, "short",
      "content within budget is untouched"
    );
    assert!(compressed[1].content.starts_with("this "));
    assert!(compressed[1].content.ends_with("[truncated]"));
    assert!(compressed[1].content.chars().count() < "this is way too long".chars().count());
  }

  #[tokio::test]
  async fn scorer_length_mismatch_is_a_loud_error() {
    struct WrongLengthScorer;
    #[async_trait]
    impl RelevanceScorer for WrongLengthScorer {
      async fn score(&self, _query: &str, _chunks: &[KnowledgeChunk]) -> Result<Vec<f32>> {
        Ok(vec![1.0]) // always returns one score, regardless of input length
      }
      fn name(&self) -> &str {
        "wrong_length"
      }
    }
    let processor = RerankProcessor::new(Arc::new(WrongLengthScorer));
    let chunks = vec![chunk("a", "A", 0.0), chunk("b", "B", 0.0)];
    let err = processor
      .process("q", chunks)
      .await
      .expect_err("mismatched score count must error, not silently misalign");
    assert!(matches!(err, RAGError::RetrievalError { .. }));
  }

  #[tokio::test]
  async fn chain_runs_steps_in_order() {
    let scorer: Arc<dyn RelevanceScorer> = Arc::new(ScriptedScorer {
      scores: HashMap::from([
        ("a".to_string(), 0.2),
        ("b".to_string(), 0.9),
        ("c".to_string(), 0.05),
      ]),
    });
    let chain = PostProcessorChain::new(vec![
      Arc::new(RelevanceFilterProcessor::new(scorer.clone(), 0.1)),
      Arc::new(RerankProcessor::new(scorer)),
    ]);
    let chunks = vec![
      chunk("a", "A", 0.0),
      chunk("b", "B", 0.0),
      chunk("c", "C", 0.0),
    ];
    let out = chain.run("q", chunks).await.expect("chain ok");
    let ids: Vec<&str> = out.iter().map(|c| c.id.as_str()).collect();
    // "c" is filtered out by the 0.1 threshold before rerank ever sees it.
    assert_eq!(ids, vec!["b", "a"]);
  }

  #[tokio::test]
  async fn empty_chain_is_a_passthrough() {
    let chain = PostProcessorChain::new(vec![]);
    let chunks = vec![chunk("a", "A", 0.0)];
    let out = chain.run("q", chunks.clone()).await.expect("chain ok");
    assert_eq!(out.len(), chunks.len());
    assert_eq!(out[0].id, chunks[0].id);
  }

  #[tokio::test]
  async fn post_processed_backend_applies_chain_and_forwards_backend_name() {
    let inner = Arc::new(StaticBackend {
      chunks: vec![chunk("a", "A", 0.2), chunk("b", "B", 0.9)],
    });
    let backend = PostProcessedKnowledgeBackend::new(
      inner,
      vec![Arc::new(RerankProcessor::new(Arc::new(
        ScoreRelevanceScorer,
      )))],
    );
    assert_eq!(backend.name(), "static");
    let out = backend.search("q", 5).await.expect("search ok");
    let ids: Vec<&str> = out.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(
      ids,
      vec!["b", "a"],
      "rerank by score must reorder best-first"
    );
  }

  #[tokio::test]
  async fn post_processed_backend_propagates_backend_errors() {
    let inner = Arc::new(StaticBackend { chunks: vec![] });
    let backend = PostProcessedKnowledgeBackend::new(inner, vec![]);
    let err = backend
      .search("   ", 5)
      .await
      .expect_err("empty query must still be rejected by the wrapped backend");
    assert!(matches!(err, KnowledgeError::InvalidQuery(_)));
  }
}

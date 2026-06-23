//! [`KnowledgeBackend`] implementations (RFC §9 — RAG repositioning).
//!
//! Two backends are provided, both implementing the kernel
//! [`agentflow_store_spi::KnowledgeBackend`] SPI so a Skill's `knowledge:`
//! declaration can pick whichever fits its corpus:
//!
//! - [`Bm25KnowledgeBackend`] — an in-memory BM25 keyword index. No network,
//!   no embedding model; ideal for the bundled-files tier (small / static
//!   knowledge shipped inside a skill) and fully unit-testable.
//! - [`VectorStoreKnowledgeBackend`] — semantic retrieval over any
//!   [`VectorStore`] (e.g. Qdrant) via a [`RetrievalStrategy`]. The tier that
//!   fires when bundled-file navigation is insufficient (large / dynamic /
//!   multi-tenant corpora).

use std::sync::Arc;

use agentflow_store_spi::{KnowledgeBackend, KnowledgeChunk, KnowledgeError};
use async_trait::async_trait;

use crate::error::RAGError;
use crate::retrieval::bm25::BM25Retriever;
use crate::retrieval::{RetrievalStrategy, SimilarityRetrieval};
use crate::types::{MetadataValue, SearchResult};
use crate::vectorstore::VectorStore;

/// Render a `MetadataValue` to the flat `String` the kernel chunk carries.
fn metadata_to_string(value: &MetadataValue) -> String {
  match value {
    MetadataValue::String(s) => s.clone(),
    MetadataValue::Integer(i) => i.to_string(),
    MetadataValue::Float(f) => f.to_string(),
    MetadataValue::Boolean(b) => b.to_string(),
    MetadataValue::Array(a) => a.join(","),
  }
}

/// Map a RAG `SearchResult` onto the backend-agnostic kernel [`KnowledgeChunk`].
///
/// The conventional `source` metadata key (set by the loaders / indexers) is
/// surfaced as the chunk's `source`; the full metadata map is flattened to
/// `String` values so downstream consumers never need the RAG types.
fn chunk_from_result(result: SearchResult) -> KnowledgeChunk {
  let source = result.metadata.get("source").map(metadata_to_string);
  let metadata = result
    .metadata
    .iter()
    .map(|(k, v)| (k.clone(), metadata_to_string(v)))
    .collect();
  KnowledgeChunk {
    id: result.id,
    content: result.content,
    score: result.score,
    source,
    metadata,
  }
}

/// Translate a RAG-layer error into the kernel knowledge error so the SPI
/// surface stays free of RAG types.
fn knowledge_error(err: RAGError) -> KnowledgeError {
  match err {
    RAGError::EmbeddingError { message } => KnowledgeError::Embedding(message),
    other => KnowledgeError::Backend(other.to_string()),
  }
}

/// In-memory BM25 keyword backend.
///
/// Build it from a corpus of `(id, content)` pairs once; every
/// [`search`](KnowledgeBackend::search) ranks the corpus with BM25. Because it
/// holds the whole corpus in memory it is meant for the *bundled-files* tier,
/// not million-document collections — reach for [`VectorStoreKnowledgeBackend`]
/// there.
pub struct Bm25KnowledgeBackend {
  retriever: BM25Retriever,
  name: String,
}

impl Bm25KnowledgeBackend {
  /// Build a backend from an iterator of `(id, content)` documents.
  pub fn from_documents<I, S1, S2>(docs: I) -> Self
  where
    I: IntoIterator<Item = (S1, S2)>,
    S1: Into<String>,
    S2: Into<String>,
  {
    let mut retriever = BM25Retriever::new();
    for (id, content) in docs {
      retriever.add_document(id, content);
    }
    retriever.finalize();
    Self {
      retriever,
      name: "bm25".to_string(),
    }
  }

  /// Override the diagnostic name reported by [`KnowledgeBackend::name`].
  pub fn with_name(mut self, name: impl Into<String>) -> Self {
    self.name = name.into();
    self
  }
}

#[async_trait]
impl KnowledgeBackend for Bm25KnowledgeBackend {
  async fn search(&self, query: &str, top_k: usize) -> Result<Vec<KnowledgeChunk>, KnowledgeError> {
    if query.trim().is_empty() {
      return Err(KnowledgeError::InvalidQuery("query is empty".to_string()));
    }
    let chunks = self
      .retriever
      .search(query, top_k)
      .into_iter()
      .map(chunk_from_result)
      .collect();
    Ok(chunks)
  }

  fn name(&self) -> &str {
    &self.name
  }
}

/// Semantic retrieval backend over any [`VectorStore`].
///
/// Wraps a store + collection + [`RetrievalStrategy`] (default
/// [`SimilarityRetrieval`]); each [`search`](KnowledgeBackend::search) delegates
/// to the strategy and maps the results onto kernel chunks.
pub struct VectorStoreKnowledgeBackend {
  store: Arc<dyn VectorStore>,
  collection: String,
  strategy: Arc<dyn RetrievalStrategy>,
}

impl VectorStoreKnowledgeBackend {
  /// Build a backend that runs plain similarity search against `collection`.
  pub fn new(store: Arc<dyn VectorStore>, collection: impl Into<String>) -> Self {
    Self {
      store,
      collection: collection.into(),
      strategy: Arc::new(SimilarityRetrieval),
    }
  }

  /// Override the retrieval strategy (e.g. a hybrid / reranking strategy).
  pub fn with_strategy(mut self, strategy: Arc<dyn RetrievalStrategy>) -> Self {
    self.strategy = strategy;
    self
  }
}

#[async_trait]
impl KnowledgeBackend for VectorStoreKnowledgeBackend {
  async fn search(&self, query: &str, top_k: usize) -> Result<Vec<KnowledgeChunk>, KnowledgeError> {
    if query.trim().is_empty() {
      return Err(KnowledgeError::InvalidQuery("query is empty".to_string()));
    }
    let results = self
      .strategy
      .retrieve(self.store.as_ref(), &self.collection, query, top_k, None)
      .await
      .map_err(knowledge_error)?;
    Ok(results.into_iter().map(chunk_from_result).collect())
  }

  fn name(&self) -> &str {
    "vector"
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn sample_backend() -> Bm25KnowledgeBackend {
    Bm25KnowledgeBackend::from_documents([
      (
        "doc-rust",
        "Rust is a systems programming language focused on safety",
      ),
      ("doc-python", "Python is a high level scripting language"),
      (
        "doc-gc",
        "Garbage collection frees memory automatically at runtime",
      ),
    ])
  }

  #[tokio::test]
  async fn bm25_backend_ranks_relevant_document_first() {
    let backend = sample_backend();
    let chunks = backend.search("rust safety", 3).await.expect("search ok");
    assert!(!chunks.is_empty(), "expected at least one hit");
    assert_eq!(
      chunks[0].id, "doc-rust",
      "most relevant doc should rank first"
    );
    // Scores are sorted best-first.
    for w in chunks.windows(2) {
      assert!(
        w[0].score >= w[1].score,
        "results must be ranked best-first"
      );
    }
  }

  #[tokio::test]
  async fn bm25_backend_respects_top_k() {
    let backend = sample_backend();
    let chunks = backend.search("language", 1).await.expect("search ok");
    assert!(chunks.len() <= 1, "top_k must bound result count");
  }

  #[tokio::test]
  async fn bm25_backend_empty_query_is_invalid() {
    let backend = sample_backend();
    let err = backend
      .search("   ", 3)
      .await
      .expect_err("empty query rejected");
    assert!(matches!(err, KnowledgeError::InvalidQuery(_)));
  }

  #[tokio::test]
  async fn bm25_backend_no_match_returns_empty_not_error() {
    let backend = sample_backend();
    let chunks = backend
      .search("zzzznonexistentterm", 3)
      .await
      .expect("no-match is Ok(empty), not an error");
    assert!(chunks.is_empty());
  }

  #[test]
  fn metadata_value_renders_each_variant() {
    assert_eq!(metadata_to_string(&MetadataValue::String("s".into())), "s");
    assert_eq!(metadata_to_string(&MetadataValue::Integer(7)), "7");
    assert_eq!(metadata_to_string(&MetadataValue::Boolean(true)), "true");
    assert_eq!(
      metadata_to_string(&MetadataValue::Array(vec!["a".into(), "b".into()])),
      "a,b"
    );
  }
}

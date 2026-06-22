//! Knowledge retrieval contract (RFC §9 — RAG repositioning).
//!
//! [`KnowledgeBackend`] is the SPI behind a Skill's `knowledge:` declaration:
//! given a natural-language query, return the most relevant passages. The
//! concrete implementations live in `agentflow-rag` (an in-memory BM25 backend
//! plus a vector-store backend) so this kernel crate stays free of the
//! RAG / embedding machinery — exactly the `MemoryStore` ⟷ `agentflow-memory`
//! split applied to the retrieval axis.
//!
//! Why it lives here: both `agentflow-skills` (which consumes a backend behind
//! `knowledge:`) and `agentflow-rag` (which implements it) must agree on the
//! contract without `skills` depending on the `rag` implementation crate.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// A single retrieved passage with its relevance score.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeChunk {
  /// Stable identifier of the source chunk (document id, file path + offset…).
  pub id: String,

  /// The passage text.
  pub content: String,

  /// Relevance score; higher is more relevant. Each backend normalises to its
  /// own scale (BM25 raw scores, cosine similarity in `0.0..=1.0`, …), so
  /// scores are only comparable *within* a single backend's result set.
  pub score: f32,

  /// Optional human-readable provenance (filename, URL, collection…).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub source: Option<String>,

  /// Backend-specific metadata carried through from the source document.
  #[serde(default, skip_serializing_if = "HashMap::is_empty")]
  pub metadata: HashMap<String, String>,
}

impl KnowledgeChunk {
  /// Construct a chunk with no source / metadata.
  pub fn new(id: impl Into<String>, content: impl Into<String>, score: f32) -> Self {
    Self {
      id: id.into(),
      content: content.into(),
      score,
      source: None,
      metadata: HashMap::new(),
    }
  }

  /// Builder: attach a provenance label.
  pub fn with_source(mut self, source: impl Into<String>) -> Self {
    self.source = Some(source.into());
    self
  }

  /// Builder: attach a metadata key/value.
  pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
    self.metadata.insert(key.into(), value.into());
    self
  }
}

/// Errors surfaced by a [`KnowledgeBackend`].
///
/// `#[non_exhaustive]` per the RFC §2 modeling rule (closed-but-extensible
/// error set): callers match via `Display` / `?` / a `_` arm, so adding a
/// variant later is not a breaking change.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum KnowledgeError {
  /// The backend could not service the query (vector store unreachable,
  /// collection missing, transport failure…).
  #[error("Knowledge backend error: {0}")]
  Backend(String),

  /// Embedding the query failed (missing API key, model load failure…).
  #[error("Embedding error: {0}")]
  Embedding(String),

  /// The query was malformed or empty.
  #[error("Invalid query: {0}")]
  InvalidQuery(String),
}

/// Retrieval SPI behind a Skill's `knowledge:` declaration.
///
/// Implementations rank their corpus against `query` and return up to `top_k`
/// passages, most-relevant first. The trait is object-safe so a skill can hold
/// an `Arc<dyn KnowledgeBackend>` chosen at assembly time (BM25 over bundled
/// files, or vector retrieval over a large corpus).
#[async_trait]
pub trait KnowledgeBackend: Send + Sync {
  /// Retrieve up to `top_k` passages relevant to `query`, ranked best-first.
  ///
  /// An empty corpus or a query with no matches returns `Ok(vec![])` — not an
  /// error. [`KnowledgeError`] is reserved for genuine backend failures.
  async fn search(&self, query: &str, top_k: usize) -> Result<Vec<KnowledgeChunk>, KnowledgeError>;

  /// Short label for diagnostics and prompt headers. Defaults to `"knowledge"`.
  fn name(&self) -> &str {
    "knowledge"
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn chunk_builders_compose() {
    let c = KnowledgeChunk::new("doc-1", "hello world", 0.5)
      .with_source("readme.md")
      .with_metadata("section", "intro");
    assert_eq!(c.source.as_deref(), Some("readme.md"));
    assert_eq!(c.metadata.get("section").map(String::as_str), Some("intro"));
  }

  #[test]
  fn chunk_roundtrips_through_json_without_empty_optionals() {
    let c = KnowledgeChunk::new("id", "body", 1.0);
    let json = serde_json::to_string(&c).expect("serialize");
    // `source` / `metadata` are skipped when empty so the wire stays compact.
    assert!(!json.contains("source"), "empty source must be skipped: {json}");
    assert!(!json.contains("metadata"), "empty metadata must be skipped: {json}");
    let back: KnowledgeChunk = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, c);
  }

  // The trait must stay object-safe — a skill holds `Arc<dyn KnowledgeBackend>`.
  #[allow(dead_code)]
  fn assert_object_safe(_: &dyn KnowledgeBackend) {}
}

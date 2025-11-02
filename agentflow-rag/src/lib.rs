//! # AgentFlow RAG System
//!
//! Retrieval-Augmented Generation (RAG) system for AgentFlow workflows.
//!
//! This crate provides comprehensive RAG capabilities including:
//! - Vector store abstractions (Qdrant, Chroma, and more)
//! - Embedding generation (OpenAI, local models)
//! - Document processing and chunking
//! - Semantic search and retrieval
//! - Re-ranking and filtering
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use agentflow_rag::{
//!     vectorstore::{VectorStore, QdrantStore},
//!     embeddings::{EmbeddingProvider, OpenAIEmbedding},
//!     types::{Document, CollectionConfig, DistanceMetric},
//! };
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // 1. Connect to vector store
//! let store = QdrantStore::new("http://localhost:6334").await?;
//!
//! // 2. Create collection
//! store.create_collection("docs", CollectionConfig {
//!     dimension: 1536,
//!     distance: DistanceMetric::Cosine,
//!     index_config: None,
//! }).await?;
//!
//! // 3. Index documents
//! let doc = Document::new("AgentFlow is a workflow orchestration platform")
//!     .with_metadata("source".into(), "readme".into());
//!
//! store.add_documents("docs", vec![doc]).await?;
//!
//! // 4. Search
//! let results = store.similarity_search("docs", "workflow platform", 5, None).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Features
//!
//! - `qdrant` - Qdrant vector database support (default)
//! - `local-embeddings` - Local embedding models via ONNX
//! - `pdf` - PDF document processing
//! - `html` - HTML document processing

// Public modules
pub mod chunking;
pub mod embeddings;
pub mod error;
pub mod indexing;
pub mod reranking;
pub mod retrieval;
pub mod sources;
pub mod types;
pub mod vectorstore;

// Re-exports for convenience
pub use error::{RAGError, Result};
pub use types::{
  ChunkingConfig, ChunkingStrategy, CollectionConfig, Condition, DistanceMetric, Document,
  EmbeddingConfig, Filter, IndexingStats, MetadataValue, SearchResult, TextChunk,
};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Check if a feature is enabled
pub fn has_feature(feature: &str) -> bool {
  match feature {
    "qdrant" => cfg!(feature = "qdrant"),
    "local-embeddings" => cfg!(feature = "local-embeddings"),
    "pdf" => cfg!(feature = "pdf"),
    "html" => cfg!(feature = "html"),
    _ => false,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_version() {
    assert!(!VERSION.is_empty());
  }

  #[test]
  fn test_default_features() {
    // qdrant should be enabled by default
    assert!(has_feature("qdrant"));
  }
}

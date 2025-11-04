//! Retrieval strategies for semantic and keyword search

pub mod bm25;
pub mod hybrid;

use crate::{
  error::Result,
  types::{Filter, SearchResult},
  vectorstore::VectorStore,
};

/// Retrieval strategy for querying vector stores
#[async_trait::async_trait]
pub trait RetrievalStrategy: Send + Sync {
  /// Retrieve documents based on query
  async fn retrieve(
    &self,
    store: &dyn VectorStore,
    collection: &str,
    query: &str,
    top_k: usize,
    filter: Option<Filter>,
  ) -> Result<Vec<SearchResult>>;
}

/// Simple similarity-based retrieval
pub struct SimilarityRetrieval;

#[async_trait::async_trait]
impl RetrievalStrategy for SimilarityRetrieval {
  async fn retrieve(
    &self,
    store: &dyn VectorStore,
    collection: &str,
    query: &str,
    top_k: usize,
    filter: Option<Filter>,
  ) -> Result<Vec<SearchResult>> {
    store.similarity_search(collection, query, top_k, filter).await
  }
}

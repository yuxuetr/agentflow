//! Qdrant vector store implementation (stub)

use crate::{
  error::{RAGError, Result},
  types::{CollectionConfig, Document, Filter, SearchResult},
  vectorstore::{CollectionStats, VectorStore},
};
use async_trait::async_trait;

pub struct QdrantStore {
  url: String,
}

impl QdrantStore {
  pub async fn new(url: impl Into<String>) -> Result<Self> {
    let url = url.into();
    tracing::info!("Connecting to Qdrant at: {}", url);
    Ok(Self { url })
  }
}

#[async_trait]
impl VectorStore for QdrantStore {
  async fn create_collection(&self, _name: &str, _config: CollectionConfig) -> Result<()> {
    Err(RAGError::vector_store("Not yet implemented"))
  }

  async fn delete_collection(&self, _name: &str) -> Result<()> {
    Err(RAGError::vector_store("Not yet implemented"))
  }

  async fn collection_exists(&self, _name: &str) -> Result<bool> {
    Err(RAGError::vector_store("Not yet implemented"))
  }

  async fn list_collections(&self) -> Result<Vec<String>> {
    Err(RAGError::vector_store("Not yet implemented"))
  }

  async fn add_documents(&self, _collection: &str, _docs: Vec<Document>) -> Result<Vec<String>> {
    Err(RAGError::vector_store("Not yet implemented"))
  }

  async fn delete_documents(&self, _collection: &str, _ids: Vec<String>) -> Result<()> {
    Err(RAGError::vector_store("Not yet implemented"))
  }

  async fn similarity_search(
    &self,
    _collection: &str,
    _query: &str,
    _top_k: usize,
    _filter: Option<Filter>,
  ) -> Result<Vec<SearchResult>> {
    Err(RAGError::vector_store("Not yet implemented"))
  }

  async fn similarity_search_by_vector(
    &self,
    _collection: &str,
    _vector: Vec<f32>,
    _top_k: usize,
    _filter: Option<Filter>,
  ) -> Result<Vec<SearchResult>> {
    Err(RAGError::vector_store("Not yet implemented"))
  }

  async fn get_collection_stats(&self, _collection: &str) -> Result<CollectionStats> {
    Err(RAGError::vector_store("Not yet implemented"))
  }
}

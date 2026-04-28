//! Core types for the RAG system

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A document to be indexed in the vector store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
  /// Unique document identifier
  pub id: String,

  /// Document content (text)
  pub content: String,

  /// Document metadata
  pub metadata: HashMap<String, MetadataValue>,

  /// Optional embedding vector (pre-computed)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub embedding: Option<Vec<f32>>,
}

impl Document {
  /// Create a new document with generated UUID
  pub fn new<S: Into<String>>(content: S) -> Self {
    Self {
      id: Uuid::new_v4().to_string(),
      content: content.into(),
      metadata: HashMap::new(),
      embedding: None,
    }
  }

  /// Create a document with a specific ID
  pub fn with_id<S1: Into<String>, S2: Into<String>>(id: S1, content: S2) -> Self {
    Self {
      id: id.into(),
      content: content.into(),
      metadata: HashMap::new(),
      embedding: None,
    }
  }

  /// Add metadata to the document
  pub fn with_metadata(mut self, key: String, value: MetadataValue) -> Self {
    self.metadata.insert(key, value);
    self
  }

  /// Set pre-computed embedding
  pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
    self.embedding = Some(embedding);
    self
  }
}

/// A text chunk from document chunking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextChunk {
  /// Chunk content
  pub content: String,

  /// Start position in original document
  pub start_idx: usize,

  /// End position in original document
  pub end_idx: usize,

  /// Metadata inherited from parent document
  pub metadata: HashMap<String, MetadataValue>,

  /// Chunk index in document
  pub chunk_index: usize,

  /// Total chunks in document
  pub total_chunks: usize,
}

/// Metadata value types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum MetadataValue {
  String(String),
  Integer(i64),
  Float(f64),
  Boolean(bool),
  Array(Vec<String>),
}

impl From<String> for MetadataValue {
  fn from(s: String) -> Self {
    MetadataValue::String(s)
  }
}

impl From<&str> for MetadataValue {
  fn from(s: &str) -> Self {
    MetadataValue::String(s.to_string())
  }
}

impl From<i64> for MetadataValue {
  fn from(i: i64) -> Self {
    MetadataValue::Integer(i)
  }
}

impl From<f64> for MetadataValue {
  fn from(f: f64) -> Self {
    MetadataValue::Float(f)
  }
}

impl From<bool> for MetadataValue {
  fn from(b: bool) -> Self {
    MetadataValue::Boolean(b)
  }
}

/// Search result from vector store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
  /// Document ID
  pub id: String,

  /// Document content
  pub content: String,

  /// Similarity score (0.0 to 1.0, higher is better)
  pub score: f32,

  /// Document metadata
  pub metadata: HashMap<String, MetadataValue>,
}

/// Filter for search queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
  /// Must match all conditions (AND)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub must: Option<Vec<Condition>>,

  /// Must match at least one condition (OR)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub should: Option<Vec<Condition>>,

  /// Must not match any condition (NOT)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub must_not: Option<Vec<Condition>>,
}

/// Filter condition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Condition {
  /// Exact match
  #[serde(rename = "match")]
  Match { field: String, value: MetadataValue },

  /// Range condition
  #[serde(rename = "range")]
  Range {
    field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    gte: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lte: Option<f64>,
  },

  /// Contains (for arrays or strings)
  #[serde(rename = "contains")]
  Contains { field: String, value: String },
}

/// Collection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionConfig {
  /// Embedding dimension
  pub dimension: usize,

  /// Distance metric
  pub distance: DistanceMetric,

  /// Optional index configuration
  #[serde(skip_serializing_if = "Option::is_none")]
  pub index_config: Option<IndexConfig>,
}

/// Distance metric for vector similarity
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DistanceMetric {
  /// Cosine similarity (normalized dot product)
  Cosine,

  /// Euclidean distance (L2)
  Euclidean,

  /// Dot product
  Dot,
}

/// Index configuration for vector store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexConfig {
  /// HNSW parameters
  #[serde(skip_serializing_if = "Option::is_none")]
  pub hnsw: Option<HNSWConfig>,
}

/// HNSW (Hierarchical Navigable Small World) configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HNSWConfig {
  /// Number of edges per node
  pub m: usize,

  /// Size of the dynamic candidate list
  pub ef_construct: usize,
}

impl Default for HNSWConfig {
  fn default() -> Self {
    Self {
      m: 16,
      ef_construct: 100,
    }
  }
}

/// Chunking configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkingConfig {
  /// Chunk size in characters
  pub chunk_size: usize,

  /// Overlap between chunks in characters
  pub overlap: usize,

  /// Chunking strategy
  pub strategy: ChunkingStrategy,
}

/// Chunking strategy enum
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChunkingStrategy {
  /// Fixed-size character chunking
  FixedSize,

  /// Sentence-based chunking
  Sentence,

  /// Recursive character chunking
  Recursive,

  /// Semantic chunking (embedding-based)
  Semantic,
}

/// Embedding configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
  /// Provider name (e.g., "openai", "local")
  pub provider: String,

  /// Model name
  pub model: String,

  /// API key (if required)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub api_key: Option<String>,

  /// Batch size for batch embedding
  #[serde(default = "default_batch_size")]
  pub batch_size: usize,
}

fn default_batch_size() -> usize {
  32
}

/// Indexing statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IndexingStats {
  /// Number of documents processed
  pub documents_processed: usize,

  /// Number of chunks created
  pub chunks_created: usize,

  /// Number of embeddings generated
  pub embeddings_generated: usize,

  /// Processing time in milliseconds
  pub processing_time_ms: u64,

  /// Number of errors encountered
  pub errors: usize,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_document_creation() {
    let doc = Document::new("Test content")
      .with_metadata("source".to_string(), "test".into())
      .with_metadata("page".to_string(), 1i64.into());

    assert_eq!(doc.content, "Test content");
    assert_eq!(doc.metadata.len(), 2);
    assert!(doc.embedding.is_none());
  }

  #[test]
  fn test_metadata_value_conversion() {
    let string_val: MetadataValue = "test".into();
    assert!(matches!(string_val, MetadataValue::String(_)));

    let int_val: MetadataValue = 42i64.into();
    assert!(matches!(int_val, MetadataValue::Integer(42)));

    let float_val: MetadataValue = 3.5f64.into();
    assert!(matches!(float_val, MetadataValue::Float(_)));

    let bool_val: MetadataValue = true.into();
    assert!(matches!(bool_val, MetadataValue::Boolean(true)));
  }

  #[test]
  fn test_collection_config() {
    let config = CollectionConfig {
      dimension: 384,
      distance: DistanceMetric::Cosine,
      index_config: Some(IndexConfig {
        hnsw: Some(HNSWConfig::default()),
      }),
    };

    assert_eq!(config.dimension, 384);
    assert_eq!(config.distance, DistanceMetric::Cosine);
  }
}

//! Embedding generation for text vectorization

use crate::error::Result;
use async_trait::async_trait;

pub mod openai;

#[cfg(feature = "local-embeddings")]
pub mod onnx;

pub use openai::{CostTracker, OpenAIEmbedding, OpenAIEmbeddingBuilder};

#[cfg(feature = "local-embeddings")]
pub use onnx::{ONNXEmbedding, ONNXEmbeddingBuilder};

/// Embedding provider trait for generating text embeddings
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
  /// Generate embedding for a single text
  async fn embed_text(&self, text: &str) -> Result<Vec<f32>>;

  /// Generate embeddings for multiple texts in batch
  async fn embed_batch(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>>;

  /// Get the dimension of the embedding vectors
  fn dimension(&self) -> usize;

  /// Get the model name
  fn model_name(&self) -> &str;

  /// Get maximum tokens per request
  fn max_tokens(&self) -> usize {
    8192 // Default for most models
  }

  /// Estimate token count for text (rough estimate)
  fn estimate_tokens(&self, text: &str) -> usize {
    // Rough estimate: 1 token ≈ 4 characters
    text.len() / 4
  }

  /// Check if text is within token limit
  fn is_within_limit(&self, text: &str) -> bool {
    self.estimate_tokens(text) <= self.max_tokens()
  }
}

/// Embedding model information
#[derive(Debug, Clone)]
pub struct EmbeddingModel {
  /// Model identifier
  pub name: String,

  /// Embedding dimension
  pub dimension: usize,

  /// Maximum input tokens
  pub max_tokens: usize,

  /// Cost per token (if applicable)
  pub cost_per_token: Option<f64>,
}

/// Common embedding models
pub mod models {
  use super::EmbeddingModel;

  /// OpenAI text-embedding-3-small
  pub fn text_embedding_3_small() -> EmbeddingModel {
    EmbeddingModel {
      name: "text-embedding-3-small".to_string(),
      dimension: 1536,
      max_tokens: 8191,
      cost_per_token: Some(0.00002 / 1000.0),
    }
  }

  /// OpenAI text-embedding-3-large
  pub fn text_embedding_3_large() -> EmbeddingModel {
    EmbeddingModel {
      name: "text-embedding-3-large".to_string(),
      dimension: 3072,
      max_tokens: 8191,
      cost_per_token: Some(0.00013 / 1000.0),
    }
  }

  /// OpenAI text-embedding-ada-002
  pub fn text_embedding_ada_002() -> EmbeddingModel {
    EmbeddingModel {
      name: "text-embedding-ada-002".to_string(),
      dimension: 1536,
      max_tokens: 8191,
      cost_per_token: Some(0.0001 / 1000.0),
    }
  }
}

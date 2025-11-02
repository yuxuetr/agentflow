//! OpenAI embedding provider

use crate::{
  embeddings::EmbeddingProvider,
  error::{RAGError, Result},
};
use async_trait::async_trait;

/// OpenAI embedding provider
pub struct OpenAIEmbedding {
  model: String,
  api_key: String,
  dimension: usize,
}

impl OpenAIEmbedding {
  pub fn new(model: impl Into<String>) -> Result<Self> {
    let model = model.into();
    let api_key = std::env::var("OPENAI_API_KEY")
      .map_err(|_| RAGError::configuration("OPENAI_API_KEY not set"))?;
    
    let dimension = match model.as_str() {
      "text-embedding-3-small" => 1536,
      "text-embedding-3-large" => 3072,
      "text-embedding-ada-002" => 1536,
      _ => return Err(RAGError::configuration(format!("Unknown model: {}", model))),
    };

    Ok(Self { model, api_key, dimension })
  }
}

#[async_trait]
impl EmbeddingProvider for OpenAIEmbedding {
  async fn embed_text(&self, _text: &str) -> Result<Vec<f32>> {
    // TODO: Implement OpenAI API call
    Err(RAGError::embedding("Not yet implemented"))
  }

  async fn embed_batch(&self, _texts: Vec<&str>) -> Result<Vec<Vec<f32>>> {
    // TODO: Implement batch OpenAI API call
    Err(RAGError::embedding("Not yet implemented"))
  }

  fn dimension(&self) -> usize {
    self.dimension
  }

  fn model_name(&self) -> &str {
    &self.model
  }
}

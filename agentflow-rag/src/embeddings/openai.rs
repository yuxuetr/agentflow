//! OpenAI embedding provider

use crate::{
  embeddings::EmbeddingProvider,
  error::{RAGError, Result},
};
use async_trait::async_trait;
use governor::{clock::DefaultClock, state::InMemoryState, state::NotKeyed, Quota, RateLimiter};
use nonzero_ext::nonzero;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;
use tokio_retry::{strategy::ExponentialBackoff, Retry};

const OPENAI_API_URL: &str = "https://api.openai.com/v1/embeddings";
const MAX_BATCH_SIZE: usize = 2048; // OpenAI limit
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// OpenAI API request structure
#[derive(Debug, Serialize)]
struct EmbeddingRequest {
  model: String,
  input: Vec<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  encoding_format: Option<String>,
}

/// OpenAI API response structure
#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
  data: Vec<EmbeddingData>,
  usage: Usage,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
  embedding: Vec<f32>,
  #[allow(dead_code)]
  index: usize,
}

#[derive(Debug, Deserialize)]
struct Usage {
  #[allow(dead_code)]
  prompt_tokens: usize,
  total_tokens: usize,
}

/// Cost tracking for embedding requests
#[derive(Debug, Clone, Default)]
pub struct CostTracker {
  pub total_tokens: usize,
  pub total_cost: f64,
  pub request_count: usize,
}

/// OpenAI embedding provider with rate limiting and cost tracking
pub struct OpenAIEmbedding {
  model: String,
  api_key: String,
  dimension: usize,
  max_tokens: usize,
  cost_per_token: f64,
  client: Client,
  rate_limiter: Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>,
  cost_tracker: Arc<tokio::sync::Mutex<CostTracker>>,
}

// Manual Debug implementation (RateLimiter doesn't implement Debug)
impl std::fmt::Debug for OpenAIEmbedding {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("OpenAIEmbedding")
      .field("model", &self.model)
      .field("dimension", &self.dimension)
      .field("max_tokens", &self.max_tokens)
      .field("cost_per_token", &self.cost_per_token)
      .finish_non_exhaustive()
  }
}

impl OpenAIEmbedding {
  /// Create a new OpenAI embedding provider
  ///
  /// # Arguments
  /// * `model` - Model name (e.g., "text-embedding-3-small")
  ///
  /// # Environment Variables
  /// * `OPENAI_API_KEY` - Required API key
  ///
  /// # Returns
  /// * `Result<Self>` - New instance or error
  pub fn new(model: impl Into<String>) -> Result<Self> {
    Self::builder(model).build()
  }

  /// Create a builder for more configuration options
  pub fn builder(model: impl Into<String>) -> OpenAIEmbeddingBuilder {
    OpenAIEmbeddingBuilder::new(model)
  }

  /// Get cost tracker statistics
  pub async fn get_cost_stats(&self) -> CostTracker {
    self.cost_tracker.lock().await.clone()
  }

  /// Reset cost tracker
  pub async fn reset_cost_tracker(&self) {
    *self.cost_tracker.lock().await = CostTracker::default();
  }

  /// Internal method to call OpenAI API with retry logic
  async fn call_api(&self, texts: Vec<String>) -> Result<EmbeddingResponse> {
    let request = EmbeddingRequest {
      model: self.model.clone(),
      input: texts,
      encoding_format: Some("float".to_string()),
    };

    // Wait for rate limiter
    self.rate_limiter.until_ready().await;

    // Retry strategy: exponential backoff with max 3 retries
    let retry_strategy = ExponentialBackoff::from_millis(100)
      .max_delay(Duration::from_secs(10))
      .take(3);

    let response = Retry::spawn(retry_strategy, || async {
      self
        .client
        .post(OPENAI_API_URL)
        .header("Authorization", format!("Bearer {}", self.api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .map_err(|e| RAGError::embedding(format!("HTTP request failed: {}", e)))
    })
    .await?;

    // Check status code
    let status = response.status();
    if !status.is_success() {
      let error_text = response
        .text()
        .await
        .unwrap_or_else(|_| "Unknown error".to_string());
      return Err(RAGError::api(
        status.as_u16(),
        format!("OpenAI API error: {}", error_text),
      ));
    }

    // Parse response
    let embedding_response: EmbeddingResponse = response
      .json()
      .await
      .map_err(|e| RAGError::embedding(format!("Failed to parse response: {}", e)))?;

    // Update cost tracker
    let mut tracker = self.cost_tracker.lock().await;
    tracker.total_tokens += embedding_response.usage.total_tokens;
    tracker.total_cost += embedding_response.usage.total_tokens as f64 * self.cost_per_token;
    tracker.request_count += 1;
    drop(tracker);

    tracing::debug!(
      "OpenAI API call successful: {} tokens used, ${:.6} cost",
      embedding_response.usage.total_tokens,
      embedding_response.usage.total_tokens as f64 * self.cost_per_token
    );

    Ok(embedding_response)
  }
}

#[async_trait]
impl EmbeddingProvider for OpenAIEmbedding {
  async fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
    // Validate input
    if text.is_empty() {
      return Err(RAGError::invalid_input("Text cannot be empty"));
    }

    if !self.is_within_limit(text) {
      return Err(RAGError::invalid_input(format!(
        "Text exceeds token limit: estimated {} tokens, max {}",
        self.estimate_tokens(text),
        self.max_tokens
      )));
    }

    tracing::debug!("Embedding single text with {} characters", text.len());

    let response = self.call_api(vec![text.to_string()]).await?;

    // Extract first embedding
    response
      .data
      .into_iter()
      .next()
      .map(|d| d.embedding)
      .ok_or_else(|| RAGError::embedding("No embedding returned from API"))
  }

  async fn embed_batch(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>> {
    if texts.is_empty() {
      return Ok(Vec::new());
    }

    tracing::debug!("Embedding batch of {} texts", texts.len());

    // Split into chunks that respect OpenAI's batch size limit
    let mut all_embeddings = Vec::with_capacity(texts.len());
    let mut current_batch = Vec::new();
    let mut current_tokens = 0;

    for text in texts {
      if text.is_empty() {
        return Err(RAGError::invalid_input("Batch contains empty text"));
      }

      let tokens = self.estimate_tokens(text);

      // Check individual text size
      if tokens > self.max_tokens {
        return Err(RAGError::invalid_input(format!(
          "Text exceeds token limit: estimated {} tokens, max {}",
          tokens,
          self.max_tokens
        )));
      }

      // If adding this text would exceed batch size, process current batch
      if current_tokens + tokens > MAX_BATCH_SIZE || current_batch.len() >= 2048 {
        if !current_batch.is_empty() {
          let response = self.call_api(current_batch.clone()).await?;
          all_embeddings.extend(response.data.into_iter().map(|d| d.embedding));
          current_batch.clear();
          current_tokens = 0;
        }
      }

      current_batch.push(text.to_string());
      current_tokens += tokens;
    }

    // Process remaining batch
    if !current_batch.is_empty() {
      let response = self.call_api(current_batch).await?;
      all_embeddings.extend(response.data.into_iter().map(|d| d.embedding));
    }

    Ok(all_embeddings)
  }

  fn dimension(&self) -> usize {
    self.dimension
  }

  fn model_name(&self) -> &str {
    &self.model
  }

  fn max_tokens(&self) -> usize {
    self.max_tokens
  }
}

/// Builder for OpenAI embedding provider
pub struct OpenAIEmbeddingBuilder {
  model: String,
  api_key: Option<String>,
  requests_per_minute: Option<NonZeroU32>,
  timeout_secs: Option<u64>,
}

impl OpenAIEmbeddingBuilder {
  pub fn new(model: impl Into<String>) -> Self {
    Self {
      model: model.into(),
      api_key: None,
      requests_per_minute: None,
      timeout_secs: None,
    }
  }

  /// Set API key (otherwise uses OPENAI_API_KEY env var)
  pub fn api_key(mut self, key: impl Into<String>) -> Self {
    self.api_key = Some(key.into());
    self
  }

  /// Set rate limit in requests per minute (default: 3500)
  pub fn requests_per_minute(mut self, rpm: u32) -> Self {
    self.requests_per_minute = NonZeroU32::new(rpm);
    self
  }

  /// Set request timeout in seconds (default: 30)
  pub fn timeout_secs(mut self, secs: u64) -> Self {
    self.timeout_secs = Some(secs);
    self
  }

  /// Build the provider
  pub fn build(self) -> Result<OpenAIEmbedding> {
    let api_key = self
      .api_key
      .or_else(|| std::env::var("OPENAI_API_KEY").ok())
      .ok_or_else(|| RAGError::configuration("OPENAI_API_KEY not set"))?;

    // Model configuration
    let (dimension, max_tokens, cost_per_token) = match self.model.as_str() {
      "text-embedding-3-small" => (1536, 8191, 0.00002 / 1000.0),
      "text-embedding-3-large" => (3072, 8191, 0.00013 / 1000.0),
      "text-embedding-ada-002" => (1536, 8191, 0.0001 / 1000.0),
      _ => {
        return Err(RAGError::configuration(format!(
          "Unknown OpenAI model: {}. Supported: text-embedding-3-small, text-embedding-3-large, text-embedding-ada-002",
          self.model
        )))
      }
    };

    // HTTP client
    let client = Client::builder()
      .timeout(Duration::from_secs(
        self.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS),
      ))
      .build()
      .map_err(|e| RAGError::configuration(format!("Failed to create HTTP client: {}", e)))?;

    // Rate limiter (default: 3500 RPM for OpenAI Tier 1)
    let rpm = self.requests_per_minute.unwrap_or(nonzero!(3500u32));
    let quota = Quota::per_minute(rpm);
    let rate_limiter = Arc::new(RateLimiter::direct(quota));

    tracing::info!(
      "Created OpenAI embedding provider: model={}, dimension={}, rate_limit={}rpm",
      self.model,
      dimension,
      rpm
    );

    Ok(OpenAIEmbedding {
      model: self.model,
      api_key,
      dimension,
      max_tokens,
      cost_per_token,
      client,
      rate_limiter,
      cost_tracker: Arc::new(tokio::sync::Mutex::new(CostTracker::default())),
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_builder_pattern() {
    // This will fail without API key, but tests the builder
    let result = OpenAIEmbedding::builder("text-embedding-3-small")
      .api_key("test-key")
      .requests_per_minute(1000)
      .timeout_secs(60)
      .build();

    assert!(result.is_ok());
    let provider = result.unwrap();
    assert_eq!(provider.model_name(), "text-embedding-3-small");
    assert_eq!(provider.dimension(), 1536);
    assert_eq!(provider.max_tokens(), 8191);
  }

  #[test]
  fn test_unknown_model() {
    let result = OpenAIEmbedding::builder("unknown-model")
      .api_key("test-key")
      .build();

    assert!(result.is_err());
    assert!(result
      .unwrap_err()
      .to_string()
      .contains("Unknown OpenAI model"));
  }

  #[test]
  fn test_token_estimation() {
    let provider = OpenAIEmbedding::builder("text-embedding-3-small")
      .api_key("test-key")
      .build()
      .unwrap();

    let text = "Hello, world!";
    let tokens = provider.estimate_tokens(text);
    assert!(tokens > 0);
    assert!(tokens < text.len()); // Should be less than character count
  }

  #[test]
  fn test_is_within_limit() {
    let provider = OpenAIEmbedding::builder("text-embedding-3-small")
      .api_key("test-key")
      .build()
      .unwrap();

    let short_text = "Hello";
    assert!(provider.is_within_limit(short_text));

    // Create a very long text that exceeds limit
    let long_text = "a".repeat(50000);
    assert!(!provider.is_within_limit(&long_text));
  }

  // Integration tests requiring real API key
  #[tokio::test]
  #[ignore]
  async fn test_embed_text_integration() {
    let provider = OpenAIEmbedding::new("text-embedding-3-small").unwrap();
    let result = provider.embed_text("Hello, world!").await;

    assert!(result.is_ok());
    let embedding = result.unwrap();
    assert_eq!(embedding.len(), 1536);
  }

  #[tokio::test]
  #[ignore]
  async fn test_embed_batch_integration() {
    let provider = OpenAIEmbedding::new("text-embedding-3-small").unwrap();
    let texts = vec!["Hello", "World", "Test"];
    let result = provider.embed_batch(texts).await;

    assert!(result.is_ok());
    let embeddings = result.unwrap();
    assert_eq!(embeddings.len(), 3);
    assert_eq!(embeddings[0].len(), 1536);
  }

  #[tokio::test]
  #[ignore]
  async fn test_cost_tracking() {
    let provider = OpenAIEmbedding::new("text-embedding-3-small").unwrap();

    provider.embed_text("Test text").await.unwrap();

    let stats = provider.get_cost_stats().await;
    assert!(stats.total_tokens > 0);
    assert!(stats.total_cost > 0.0);
    assert_eq!(stats.request_count, 1);
  }
}

use crate::{LLMError, Result, StreamingResponse};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

pub mod anthropic;
pub mod google;
pub mod mock;
pub mod moonshot;
pub mod openai;
pub mod stepfun;

pub use anthropic::AnthropicProvider;
pub use google::GoogleProvider;
pub use mock::MockProvider;
pub use moonshot::MoonshotProvider;
pub use openai::OpenAIProvider;
pub use stepfun::StepFunProvider;

/// Request structure for LLM providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderRequest {
  pub model: String,
  pub messages: Vec<Value>,
  pub stream: bool,
  pub parameters: HashMap<String, Value>,
}

/// Content types that can be returned by LLM providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentType {
  /// Plain text content
  Text(String),
  /// Image content with binary data and media type
  Image { data: Vec<u8>, media_type: String },
  /// Audio content with binary data and media type
  Audio { data: Vec<u8>, media_type: String },
  /// Mixed content containing multiple blocks
  Mixed(Vec<ContentBlock>),
}

/// Individual content blocks for mixed content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentBlock {
  /// Text block
  Text(String),
  /// Image block
  Image { data: Vec<u8>, media_type: String },
  /// Audio block
  Audio { data: Vec<u8>, media_type: String },
}

impl ContentType {
  /// Convert content to a plain text string (for backward compatibility)
  pub fn to_string(&self) -> String {
    match self {
      ContentType::Text(text) => text.clone(),
      ContentType::Image { .. } => "[Image]".to_string(),
      ContentType::Audio { .. } => "[Audio]".to_string(),
      ContentType::Mixed(blocks) => blocks
        .iter()
        .map(|block| match block {
          ContentBlock::Text(text) => text.clone(),
          ContentBlock::Image { .. } => "[Image]".to_string(),
          ContentBlock::Audio { .. } => "[Audio]".to_string(),
        })
        .collect::<Vec<_>>()
        .join(" "),
    }
  }

  /// Check if content is purely text
  pub fn is_text(&self) -> bool {
    matches!(self, ContentType::Text(_))
  }

  /// Check if content contains images
  pub fn has_images(&self) -> bool {
    match self {
      ContentType::Image { .. } => true,
      ContentType::Mixed(blocks) => blocks
        .iter()
        .any(|b| matches!(b, ContentBlock::Image { .. })),
      _ => false,
    }
  }

  /// Check if content contains audio
  pub fn has_audio(&self) -> bool {
    match self {
      ContentType::Audio { .. } => true,
      ContentType::Mixed(blocks) => blocks
        .iter()
        .any(|b| matches!(b, ContentBlock::Audio { .. })),
      _ => false,
    }
  }

  /// Get the length of the content (for text, the string length; for binary, the data length)
  pub fn len(&self) -> usize {
    match self {
      ContentType::Text(text) => text.len(),
      ContentType::Image { data, .. } => data.len(),
      ContentType::Audio { data, .. } => data.len(),
      ContentType::Mixed(blocks) => blocks
        .iter()
        .map(|block| match block {
          ContentBlock::Text(text) => text.len(),
          ContentBlock::Image { data, .. } => data.len(),
          ContentBlock::Audio { data, .. } => data.len(),
        })
        .sum(),
    }
  }
}

impl From<String> for ContentType {
  fn from(text: String) -> Self {
    ContentType::Text(text)
  }
}

impl From<&str> for ContentType {
  fn from(text: &str) -> Self {
    ContentType::Text(text.to_string())
  }
}

/// Response structure from LLM providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderResponse {
  pub content: ContentType,
  pub usage: Option<TokenUsage>,
  pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
  pub prompt_tokens: Option<u32>,
  pub completion_tokens: Option<u32>,
  pub total_tokens: Option<u32>,
}

/// Trait that all LLM providers must implement
#[async_trait]
pub trait LLMProvider: Send + Sync {
  /// Get the provider name (e.g., "openai", "anthropic", "google")
  fn name(&self) -> &str;

  /// Execute a non-streaming request
  async fn execute(&self, request: &ProviderRequest) -> Result<ProviderResponse>;

  /// Execute a streaming request
  async fn execute_streaming(
    &self,
    request: &ProviderRequest,
  ) -> Result<Box<dyn StreamingResponse>>;

  /// Validate that the provider is properly configured
  async fn validate_config(&self) -> Result<()>;

  /// Get the base URL for this provider
  fn base_url(&self) -> &str;

  /// Get supported model names for this provider
  fn supported_models(&self) -> Vec<String>;
}

/// Factory function to create providers by name
pub fn create_provider(
  provider_name: &str,
  api_key: &str,
  base_url: Option<String>,
) -> Result<Box<dyn LLMProvider>> {
  match provider_name.to_lowercase().as_str() {
    "mock" => Ok(Box::new(MockProvider::new(api_key, base_url)?)), // Mock provider for testing
    "openai" => Ok(Box::new(OpenAIProvider::new(api_key, base_url)?)),
    "anthropic" => Ok(Box::new(AnthropicProvider::new(api_key, base_url)?)),
    "google" | "gemini" => Ok(Box::new(GoogleProvider::new(api_key, base_url)?)),
    "moonshot" => Ok(Box::new(MoonshotProvider::new(api_key, base_url)?)),
    "stepfun" | "step" => Ok(Box::new(StepFunProvider::new(api_key, base_url)?)), // Use dedicated StepFun provider
    "dashscope" => Ok(Box::new(OpenAIProvider::new(api_key, base_url)?)), // Dashscope is OpenAI-compatible
    _ => Err(LLMError::UnsupportedProvider {
      provider: provider_name.to_string(),
    }),
  }
}

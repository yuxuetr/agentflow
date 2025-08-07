use crate::{LLMError, Result, model_types::{ModelType, ModelCapabilities, InputType}};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::path::Path;

/// Configuration for a specific model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
  /// The vendor/provider name (e.g., "openai", "anthropic", "google")
  pub vendor: String,

  /// The model type (granular classification with input/output types)
  /// New granular types: text, imageunderstand, text2image, image2image, imageedit, tts, asr, etc.
  /// Legacy types: text, multimodal, image, audio (maintained for backward compatibility)
  pub r#type: Option<String>,
  
  /// Detailed model capabilities (computed from type)
  #[serde(skip_serializing_if = "Option::is_none")]
  pub capabilities: Option<ModelCapabilities>,
  
  /// The actual model ID used by the provider API (optional, defaults to the config key)
  pub model_id: Option<String>,
  
  /// Base URL for the API (optional, uses provider default if not specified)
  pub base_url: Option<String>,
  
  /// Default temperature for this model
  pub temperature: Option<f32>,
  
  /// Default top_p for this model
  pub top_p: Option<f32>,
  
  /// Default max_tokens for this model
  pub max_tokens: Option<u32>,
  
  /// Default frequency penalty for this model
  pub frequency_penalty: Option<f32>,
  
  /// Default stop sequences for this model
  pub stop: Option<Vec<String>>,
  
  /// Default number of responses to generate
  pub n: Option<u32>,
  
  /// Whether streaming is supported for this model
  pub supports_streaming: Option<bool>,
  
  /// Whether this model supports function calling/tools
  pub supports_tools: Option<bool>,
  
  /// Whether this model supports multimodal input (images)
  pub supports_multimodal: Option<bool>,
  
  /// Response format configuration (e.g., "json_object")
  pub response_format: Option<String>,
  
  /// Additional model-specific parameters
  #[serde(flatten)]
  pub additional_params: HashMap<String, serde_json::Value>,
}

impl ModelConfig {
  /// Get the model type, defaulting to "text" if not specified
  pub fn model_type(&self) -> &str {
    self.r#type.as_deref().unwrap_or("text")
  }

  /// Get the granular model type enum
  pub fn granular_type(&self) -> ModelType {
    let type_str = self.model_type();
    
    // Handle granular types first
    match type_str {
      "text" => ModelType::Text,
      "imageunderstand" => ModelType::ImageUnderstand,
      "text2image" => ModelType::Text2Image,
      "image2image" => ModelType::Image2Image,
      "imageedit" => ModelType::ImageEdit,
      "tts" => ModelType::Tts,
      "asr" => ModelType::Asr,
      "videounderstand" => ModelType::VideoUnderstand,
      "text2video" => ModelType::Text2Video,
      "codegen" => ModelType::CodeGen,
      "docunderstand" => ModelType::DocUnderstand,
      "embedding" => ModelType::Embedding,
      "functioncalling" => ModelType::FunctionCalling,
      // Legacy type mapping
      _ => ModelType::from(type_str),
    }
  }

  /// Get or compute model capabilities
  pub fn get_capabilities(&self) -> ModelCapabilities {
    if let Some(ref capabilities) = self.capabilities {
      capabilities.clone()
    } else {
      // Compute capabilities from model type and config
      let mut capabilities = ModelCapabilities::from_model_type(self.granular_type());
      
      // Override with explicit config values
      if let Some(streaming) = self.supports_streaming {
        capabilities.supports_streaming = streaming;
      }
      if let Some(tools) = self.supports_tools {
        capabilities.supports_tools = tools;
      }
      if let Some(max_tokens) = self.max_tokens {
        capabilities.max_output_tokens = Some(max_tokens);
      }
      
      capabilities
    }
  }

  /// Check if this is a multimodal model (legacy method for backward compatibility)
  pub fn is_multimodal(&self) -> bool {
    self.get_capabilities().is_multimodal() || self.supports_multimodal.unwrap_or(false)
  }

  /// Check if this is an image generation model
  pub fn is_image_model(&self) -> bool {
    matches!(self.granular_type(), ModelType::Text2Image | ModelType::Image2Image | ModelType::ImageEdit)
  }

  /// Check if this is an audio model
  pub fn is_audio_model(&self) -> bool {
    matches!(self.granular_type(), ModelType::Tts | ModelType::Asr)
  }

  /// Check if this is a text-to-speech model
  pub fn is_tts_model(&self) -> bool {
    matches!(self.granular_type(), ModelType::Tts)
  }

  /// Check if this model supports the given input type
  pub fn supports_input_type(&self, input_type: &InputType) -> bool {
    self.get_capabilities().supports_input(input_type)
  }

  /// Check if this model supports the given content type (legacy method)
  pub fn supports_content_type(&self, content_type: &str) -> bool {
    let input_type = match content_type {
      "text" => InputType::Text,
      "image" => InputType::Image,
      "audio" => InputType::Audio,
      "video" => InputType::Video,
      "document" => InputType::Document,
      _ => return false,
    };
    
    self.supports_input_type(&input_type)
  }

  /// Validate a request against this model's capabilities
  pub fn validate_request(&self, has_text: bool, has_images: bool, has_audio: bool, 
                         has_video: bool, requires_streaming: bool, uses_tools: bool) -> Result<()> {
    let capabilities = self.get_capabilities();
    
    capabilities.validate_request(has_text, has_images, has_audio, has_video, requires_streaming, uses_tools)
      .map_err(|msg| LLMError::InvalidModelConfig { message: msg })
  }

  /// Get supported input types for this model
  pub fn supported_inputs(&self) -> std::collections::HashSet<InputType> {
    self.granular_type().supported_inputs()
  }

  /// Get the primary output type for this model
  pub fn primary_output(&self) -> crate::model_types::OutputType {
    self.granular_type().primary_output()
  }

  /// Check if this model supports streaming
  pub fn supports_streaming_capability(&self) -> bool {
    self.get_capabilities().supports_streaming
  }

  /// Check if this model requires streaming (no non-streaming mode)
  pub fn requires_streaming(&self) -> bool {
    self.get_capabilities().requires_streaming
  }

  /// Check if this model supports tools/function calling
  pub fn supports_tools_capability(&self) -> bool {
    self.get_capabilities().supports_tools
  }

  /// Get a description of what this model does
  pub fn description(&self) -> &'static str {
    self.granular_type().description()
  }

  /// Get typical use cases for this model
  pub fn use_cases(&self) -> Vec<&'static str> {
    self.granular_type().use_cases()
  }
}

/// Configuration for a provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
  /// Environment variable name for the API key
  pub api_key_env: String,
  
  /// Default base URL for this provider
  pub base_url: Option<String>,
  
  /// Default timeout in seconds
  pub timeout_seconds: Option<u64>,
  
  /// Rate limiting configuration
  pub rate_limit: Option<RateLimitConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
  pub requests_per_minute: u32,
  pub tokens_per_minute: Option<u32>,
}

/// Main configuration structure for all LLM models and providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMConfig {
  /// Model configurations keyed by model name
  pub models: HashMap<String, ModelConfig>,
  
  /// Provider configurations keyed by provider name
  #[serde(default)]
  pub providers: HashMap<String, ProviderConfig>,
  
  /// Global defaults
  #[serde(default)]
  pub defaults: GlobalDefaults,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalDefaults {
  pub timeout_seconds: Option<u64>,
  pub max_retries: Option<u32>,
  pub retry_delay_ms: Option<u64>,
}

impl LLMConfig {
  /// Load configuration from a YAML file
  pub async fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
    let content = tokio::fs::read_to_string(path).await.map_err(|e| {
      LLMError::ConfigurationError {
        message: format!("Failed to read config file: {}", e),
      }
    })?;

    Self::from_yaml(&content)
  }

  /// Parse configuration from YAML string
  pub fn from_yaml(yaml_content: &str) -> Result<Self> {
    serde_yaml::from_str(yaml_content).map_err(|e| LLMError::ConfigurationError {
      message: format!("Failed to parse YAML config: {}", e),
    })
  }

  /// Get a model configuration by name
  pub fn get_model(&self, model_name: &str) -> Result<&ModelConfig> {
    self.models.get(model_name).ok_or_else(|| LLMError::ModelNotFound {
      model_name: model_name.to_string(),
    })
  }

  /// Get a provider configuration by name
  pub fn get_provider(&self, provider_name: &str) -> Option<&ProviderConfig> {
    self.providers.get(provider_name)
  }

  /// Get API key for a provider from environment variables
  pub fn get_api_key(&self, provider_name: &str) -> Result<String> {
    // First try provider-specific config
    if let Some(provider_config) = self.get_provider(provider_name) {
      if let Ok(api_key) = env::var(&provider_config.api_key_env) {
        return Ok(api_key);
      }
    }

    // Fallback to common environment variable patterns
    let common_env_vars = match provider_name.to_lowercase().as_str() {
      "openai" => vec!["OPENAI_API_KEY", "OPENAI_KEY"],
      "anthropic" => vec!["ANTHROPIC_API_KEY", "ANTHROPIC_KEY", "CLAUDE_API_KEY"],
      "google" | "gemini" => vec!["GOOGLE_API_KEY", "GEMINI_API_KEY", "GOOGLE_AI_KEY"],
      "moonshot" => vec!["MOONSHOT_API_KEY", "MOONSHOT_KEY"],
      _ => vec![],
    };

    for env_var in common_env_vars {
      if let Ok(api_key) = env::var(env_var) {
        return Ok(api_key);
      }
    }

    Err(LLMError::MissingApiKey {
      provider: provider_name.to_string(),
    })
  }

  /// Validate the configuration against available environment variables
  pub fn validate(&self) -> Result<()> {
    for (model_name, model_config) in &self.models {
      // Check if provider exists
      if !["openai", "anthropic", "google", "gemini", "moonshot", "dashscope", "step"].contains(&model_config.vendor.as_str()) {
        return Err(LLMError::UnsupportedProvider {
          provider: model_config.vendor.clone(),
        });
      }

      // Check if API key is available
      if let Err(_) = self.get_api_key(&model_config.vendor) {
        return Err(LLMError::MissingApiKey {
          provider: model_config.vendor.clone(),
        });
      }

      // Validate model-specific configuration
      if let Some(temp) = model_config.temperature {
        if temp < 0.0 || temp > 2.0 {
          return Err(LLMError::InvalidModelConfig {
            message: format!("Temperature for model '{}' must be between 0.0 and 2.0", model_name),
          });
        }
      }

      if let Some(top_p) = model_config.top_p {
        if top_p < 0.0 || top_p > 1.0 {
          return Err(LLMError::InvalidModelConfig {
            message: format!("top_p for model '{}' must be between 0.0 and 1.0", model_name),
          });
        }
      }

      if let Some(freq_penalty) = model_config.frequency_penalty {
        if freq_penalty < 0.0 || freq_penalty > 2.0 {
          return Err(LLMError::InvalidModelConfig {
            message: format!("frequency_penalty for model '{}' must be between 0.0 and 2.0", model_name),
          });
        }
      }

      if let Some(n) = model_config.n {
        if n == 0 || n > 10 {
          return Err(LLMError::InvalidModelConfig {
            message: format!("n for model '{}' must be between 1 and 10", model_name),
          });
        }
      }
    }

    Ok(())
  }
}

impl Default for LLMConfig {
  fn default() -> Self {
    Self {
      models: HashMap::new(),
      providers: HashMap::new(),
      defaults: GlobalDefaults::default(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::env;

  #[test]
  fn test_model_config_parsing() {
    let yaml = r#"
models:
  gpt-4o:
    vendor: openai
    base_url: "https://api.openai.com/v1/chat/completions"
    temperature: 0.7
    top_p: 0.9
    max_tokens: 4096
    supports_streaming: true
  
  claude-3-sonnet:
    vendor: anthropic
    model_id: "claude-3-sonnet-20240229"
    temperature: 0.5
    max_tokens: 4096

providers:
  openai:
    api_key_env: "OPENAI_API_KEY"
    timeout_seconds: 30
  anthropic:
    api_key_env: "ANTHROPIC_API_KEY"
    timeout_seconds: 60

defaults:
  timeout_seconds: 30
  max_retries: 3
"#;

    let config = LLMConfig::from_yaml(yaml).unwrap();
    
    assert_eq!(config.models.len(), 2);
    assert_eq!(config.providers.len(), 2);
    
    let gpt4_config = config.get_model("gpt-4o").unwrap();
    assert_eq!(gpt4_config.vendor, "openai");
    assert_eq!(gpt4_config.temperature, Some(0.7));
    assert_eq!(gpt4_config.supports_streaming, Some(true));
    
    let claude_config = config.get_model("claude-3-sonnet").unwrap();
    assert_eq!(claude_config.vendor, "anthropic");
    assert_eq!(claude_config.model_id, Some("claude-3-sonnet-20240229".to_string()));
  }

  #[test]
  fn test_api_key_resolution() {
    env::set_var("TEST_OPENAI_KEY", "test-key");
    
    let yaml = r#"
models:
  gpt-4o:
    vendor: openai

providers:
  openai:
    api_key_env: "TEST_OPENAI_KEY"
"#;

    let config = LLMConfig::from_yaml(yaml).unwrap();
    let api_key = config.get_api_key("openai").unwrap();
    assert_eq!(api_key, "test-key");
    
    env::remove_var("TEST_OPENAI_KEY");
  }
}
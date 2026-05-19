use crate::{
  LLMError, Result,
  model_types::{InputType, ModelCapabilities, ModelType},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Configuration for a specific model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
  /// The vendor/provider name (e.g., "openai", "anthropic", "google")
  pub vendor: String,

  /// The model type (granular classification with input/output types).
  ///
  /// Canonical (post P-LLM.0) values:
  ///   - `chat` for all chat-shaped text-reasoning models
  ///     (regardless of whether they accept image / audio / video
  ///     input — that's expressed via `accepts`)
  ///   - `embedding`
  ///   - `text_to_image` / `image_to_image` / `image_edit`
  ///   - `text_to_video`
  ///   - `tts` / `asr`
  ///
  /// Legacy values still parsed for backward compatibility:
  /// `text`, `multimodal`, `imageunderstand`, `videounderstand`,
  /// `docunderstand`, `codegen`, `functioncalling` (all → `chat`),
  /// `generateimage` (→ `text_to_image`),
  /// `image` (→ `image_to_image`),
  /// `editimage` (→ `image_edit`).
  pub r#type: Option<String>,

  /// Input modalities this model accepts (`text` / `image` / `audio` /
  /// `video` / `document`).
  ///
  /// When `Some`, this is the authoritative source for what the model
  /// can ingest — `ModelConfig::accepts()` returns it directly.
  /// When `None`, the value is inferred from `granular_type()` (e.g.,
  /// `Chat` defaults to `[text]`, `Asr` to `[audio]`, etc.).
  ///
  /// Use this to distinguish e.g. GPT-4o (`type: chat,
  /// accepts: [text, image]`) from DeepSeek-Chat (`type: chat,
  /// accepts: [text]`) — both are chat-shaped but only the former
  /// can be wired into vision nodes.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub accepts: Option<Vec<InputType>>,

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

  /// Whether this model supports function calling/tools (any path).
  pub supports_tools: Option<bool>,

  /// Whether this model supports provider-native tool calling
  /// (OpenAI `tool_calls`, Anthropic `tool_use`, Google `functionCall`).
  ///
  /// When `false` or unset, callers fall back to prompt-based ReAct.
  pub native_tool_calling: Option<bool>,

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

  /// Get the granular model type enum.
  ///
  /// Recognises both the canonical post-P-LLM.0 type strings
  /// (`chat`, `text_to_image`, `image_to_image`, `image_edit`,
  /// `text_to_video`, `embedding`, `tts`, `asr`) and the historical
  /// pre-P-LLM.0 strings (`text`, `multimodal`, `imageunderstand`,
  /// `videounderstand`, `docunderstand`, `codegen`, `functioncalling`
  /// → all collapse to `Chat` semantics; `generateimage` →
  /// `Text2Image`; `image` → `Image2Image`; `editimage` → `ImageEdit`).
  pub fn granular_type(&self) -> ModelType {
    let type_str = self.model_type();

    match type_str {
      // Canonical post-P-LLM.0 names (chat-shaped collapsed onto Text variant
      // until Slice 3 renames the variant itself).
      "chat" => ModelType::Text,
      "text_to_image" | "text2image" => ModelType::Text2Image,
      "image_to_image" | "image2image" => ModelType::Image2Image,
      "image_edit" | "imageedit" => ModelType::ImageEdit,
      "text_to_video" | "text2video" => ModelType::Text2Video,
      "tts" => ModelType::Tts,
      "asr" => ModelType::Asr,
      "embedding" => ModelType::Embedding,
      // Legacy chat-shaped aliases (all map to ModelType::Text today;
      // Slice 3 collapses them to ModelType::Chat).
      "text" => ModelType::Text,
      "imageunderstand" => ModelType::ImageUnderstand,
      "videounderstand" => ModelType::VideoUnderstand,
      "codegen" => ModelType::CodeGen,
      "docunderstand" => ModelType::DocUnderstand,
      "functioncalling" => ModelType::FunctionCalling,
      // Legacy image-generation aliases.
      "generateimage" => ModelType::Text2Image,
      "editimage" => ModelType::ImageEdit,
      // Everything else: fall back to the From<&str> mapping
      // (legacy "multimodal" / "image" / "audio" still resolve there).
      _ => ModelType::from(type_str),
    }
  }

  /// Input modalities this model accepts.
  ///
  /// Returns the explicit `accepts` field when set; otherwise derives
  /// the default from `granular_type()` (e.g., a `Chat` model with no
  /// explicit `accepts` returns `[Text]`; an `Asr` model returns
  /// `[Audio]`).
  ///
  /// Callers that need to filter models by input capability — e.g.,
  /// "any chat model that takes image input" — should use this rather
  /// than `granular_type()` directly.
  pub fn accepts(&self) -> std::collections::HashSet<InputType> {
    if let Some(explicit) = self.accepts.as_ref() {
      return explicit.iter().cloned().collect();
    }
    self.granular_type().supported_inputs()
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
      if let Some(native) = self.native_tool_calling {
        capabilities.native_tool_calling = native;
      }
      if let Some(max_tokens) = self.max_tokens {
        capabilities.max_output_tokens = Some(max_tokens);
      }

      capabilities
    }
  }

  /// Whether this model is configured for provider-native tool calling.
  ///
  /// `false` means callers must fall back to prompt-based ReAct.
  pub fn supports_native_tool_calling(&self) -> bool {
    self.get_capabilities().native_tool_calling
  }

  /// Check if this is a multimodal model (legacy method for backward compatibility)
  pub fn is_multimodal(&self) -> bool {
    self.get_capabilities().is_multimodal() || self.supports_multimodal.unwrap_or(false)
  }

  /// Check if this is an image generation model
  pub fn is_image_model(&self) -> bool {
    matches!(
      self.granular_type(),
      ModelType::Text2Image | ModelType::Image2Image | ModelType::ImageEdit
    )
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
  pub fn validate_request(
    &self,
    has_text: bool,
    has_images: bool,
    has_audio: bool,
    has_video: bool,
    requires_streaming: bool,
    uses_tools: bool,
  ) -> Result<()> {
    let capabilities = self.get_capabilities();

    capabilities
      .validate_request(
        has_text,
        has_images,
        has_audio,
        has_video,
        requires_streaming,
        uses_tools,
      )
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

/// Environment variable that overrides the default model configuration path.
pub const MODELS_CONFIG_ENV: &str = "AGENTFLOW_MODELS_CONFIG";

/// Source used when loading the default LLM model configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LLMConfigSource {
  /// Stable source kind for diagnostics and machine-readable output.
  pub kind: LLMConfigSourceKind,

  /// File path for user-provided sources. Built-in defaults have no path.
  pub path: Option<PathBuf>,

  /// Non-fatal diagnostics collected during source resolution.
  pub warnings: Vec<String>,
}

impl LLMConfigSource {
  /// Human-readable description of the selected source.
  pub fn display_path(&self) -> String {
    self
      .path
      .as_ref()
      .map(|path| path.display().to_string())
      .unwrap_or_else(|| "built-in default_models.yml".to_string())
  }

  /// Returns true when the selected source is a file on disk.
  pub fn is_file(&self) -> bool {
    self.path.is_some()
  }
}

/// Stable kind for default model configuration resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LLMConfigSourceKind {
  EnvOverride,
  UserModelsYml,
  UserModelsYaml,
  BuiltInDefault,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalDefaults {
  pub timeout_seconds: Option<u64>,
  pub max_retries: Option<u32>,
  pub retry_delay_ms: Option<u64>,
}

impl LLMConfig {
  /// Resolve the model configuration source using AgentFlow's default priority:
  ///
  /// 1. `AGENTFLOW_MODELS_CONFIG`
  /// 2. `~/.agentflow/models.yml`
  /// 3. `~/.agentflow/models.yaml`
  /// 4. Built-in defaults bundled in the crate
  pub fn resolve_default_source() -> Result<LLMConfigSource> {
    let config_dir = dirs::home_dir().map(|home| home.join(".agentflow"));
    Self::resolve_default_source_from(config_dir.as_deref(), env::var_os(MODELS_CONFIG_ENV))
  }

  /// Resolve a model configuration source from explicit inputs.
  ///
  /// This is public so CLI commands and tests can use the same precedence rules
  /// without mutating process-global environment variables.
  pub fn resolve_default_source_from(
    config_dir: Option<&Path>,
    env_override: Option<OsString>,
  ) -> Result<LLMConfigSource> {
    if let Some(path) = env_override.filter(|value| !value.is_empty()) {
      return Ok(LLMConfigSource {
        kind: LLMConfigSourceKind::EnvOverride,
        path: Some(PathBuf::from(path)),
        warnings: Vec::new(),
      });
    }

    if let Some(config_dir) = config_dir {
      let yml_path = config_dir.join("models.yml");
      let yaml_path = config_dir.join("models.yaml");
      let yml_exists = yml_path.exists();
      let yaml_exists = yaml_path.exists();

      if yml_exists {
        let mut warnings = Vec::new();
        if yaml_exists {
          warnings.push(format!(
            "Both '{}' and '{}' exist; using '{}'",
            yml_path.display(),
            yaml_path.display(),
            yml_path.display()
          ));
        }
        return Ok(LLMConfigSource {
          kind: LLMConfigSourceKind::UserModelsYml,
          path: Some(yml_path),
          warnings,
        });
      }

      if yaml_exists {
        return Ok(LLMConfigSource {
          kind: LLMConfigSourceKind::UserModelsYaml,
          path: Some(yaml_path),
          warnings: Vec::new(),
        });
      }
    }

    Ok(LLMConfigSource {
      kind: LLMConfigSourceKind::BuiltInDefault,
      path: None,
      warnings: Vec::new(),
    })
  }

  /// Load configuration from a YAML file
  pub async fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
    let content =
      tokio::fs::read_to_string(path)
        .await
        .map_err(|e| LLMError::ConfigurationError {
          message: format!("Failed to read config file: {}", e),
        })?;

    Self::from_yaml(&content)
  }

  /// Parse configuration from YAML string
  pub fn from_yaml(yaml_content: &str) -> Result<Self> {
    serde_yaml::from_str(yaml_content).map_err(|e| LLMError::ConfigurationError {
      message: format!("Failed to parse YAML config: {}", e),
    })
  }

  /// Load configuration from the default resolved source.
  pub async fn from_default_source() -> Result<(Self, LLMConfigSource)> {
    let source = Self::resolve_default_source()?;
    let config = match source.path.as_ref() {
      Some(path) => Self::from_file(path).await?,
      None => Self::from_yaml(include_str!("../../templates/default_models.yml"))?,
    };
    Ok((config, source))
  }

  /// Get a model configuration by name
  pub fn get_model(&self, model_name: &str) -> Result<&ModelConfig> {
    self
      .models
      .get(model_name)
      .ok_or_else(|| LLMError::ModelNotFound {
        model_name: model_name.to_string(),
      })
  }

  /// Get a provider configuration by name
  pub fn get_provider(&self, provider_name: &str) -> Option<&ProviderConfig> {
    self.providers.get(provider_name)
  }

  /// Get API key for a provider from environment variables
  pub fn get_api_key(&self, provider_name: &str) -> Result<String> {
    if provider_name.eq_ignore_ascii_case("mock") {
      return Ok("mock".to_string());
    }

    // First try provider-specific config
    if let Some(provider_config) = self.get_provider(provider_name)
      && let Ok(api_key) = env::var(&provider_config.api_key_env)
    {
      return Ok(api_key);
    }

    // Fallback to common environment variable patterns
    let common_env_vars = match provider_name.to_lowercase().as_str() {
      "openai" => vec!["OPENAI_API_KEY", "OPENAI_KEY"],
      "anthropic" => vec!["ANTHROPIC_API_KEY", "ANTHROPIC_KEY", "CLAUDE_API_KEY"],
      "google" | "gemini" => vec!["GOOGLE_API_KEY", "GEMINI_API_KEY", "GOOGLE_AI_KEY"],
      "moonshot" => vec!["MOONSHOT_API_KEY", "MOONSHOT_KEY"],
      "stepfun" | "step" => vec!["STEPFUN_API_KEY", "STEP_API_KEY"],
      "glm" | "bigmodel" | "zhipu" => vec!["GLM_API_KEY", "BIGMODEL_API_KEY", "ZHIPU_API_KEY"],
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
      if ![
        "openai",
        "anthropic",
        "google",
        "gemini",
        "moonshot",
        "dashscope",
        "step",
        "mock",
      ]
      .contains(&model_config.vendor.as_str())
      {
        return Err(LLMError::UnsupportedProvider {
          provider: model_config.vendor.clone(),
        });
      }

      // Check if API key is available
      if self.get_api_key(&model_config.vendor).is_err() {
        return Err(LLMError::MissingApiKey {
          provider: model_config.vendor.clone(),
        });
      }

      // Validate model-specific configuration
      if let Some(temp) = model_config.temperature
        && !(0.0..=2.0).contains(&temp)
      {
        return Err(LLMError::InvalidModelConfig {
          message: format!(
            "Temperature for model '{}' must be between 0.0 and 2.0",
            model_name
          ),
        });
      }

      if let Some(top_p) = model_config.top_p
        && !(0.0..=1.0).contains(&top_p)
      {
        return Err(LLMError::InvalidModelConfig {
          message: format!(
            "top_p for model '{}' must be between 0.0 and 1.0",
            model_name
          ),
        });
      }

      if let Some(freq_penalty) = model_config.frequency_penalty
        && !(0.0..=2.0).contains(&freq_penalty)
      {
        return Err(LLMError::InvalidModelConfig {
          message: format!(
            "frequency_penalty for model '{}' must be between 0.0 and 2.0",
            model_name
          ),
        });
      }

      if let Some(n) = model_config.n
        && (n == 0 || n > 10)
      {
        return Err(LLMError::InvalidModelConfig {
          message: format!("n for model '{}' must be between 1 and 10", model_name),
        });
      }
    }

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::env;
  use tempfile::TempDir;

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
    assert_eq!(
      claude_config.model_id,
      Some("claude-3-sonnet-20240229".to_string())
    );
  }

  #[test]
  fn test_api_key_resolution() {
    // SAFETY: this unit test mutates a dedicated test env var before reading it.
    unsafe {
      env::set_var("TEST_OPENAI_KEY", "test-key");
    }

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

    // SAFETY: cleanup of the dedicated test env var after the test read.
    unsafe {
      env::remove_var("TEST_OPENAI_KEY");
    }
  }

  #[test]
  fn test_mock_provider_needs_no_api_key() {
    let yaml = r#"
models:
  mock-model:
    vendor: mock
    type: text
"#;

    let config = LLMConfig::from_yaml(yaml).unwrap();
    assert_eq!(config.get_api_key("mock").unwrap(), "mock");
    assert!(config.validate().is_ok());
  }

  #[test]
  fn resolves_models_yml_before_legacy_yaml() {
    let temp = TempDir::new().unwrap();
    let config_dir = temp.path();
    std::fs::write(config_dir.join("models.yml"), "models: {}\n").unwrap();
    std::fs::write(config_dir.join("models.yaml"), "models: {}\n").unwrap();

    let source = LLMConfig::resolve_default_source_from(Some(config_dir), None).unwrap();

    assert_eq!(source.kind, LLMConfigSourceKind::UserModelsYml);
    assert_eq!(source.path.as_ref(), Some(&config_dir.join("models.yml")));
    assert_eq!(source.warnings.len(), 1);
  }

  #[test]
  fn resolves_legacy_models_yaml_when_yml_is_absent() {
    let temp = TempDir::new().unwrap();
    let config_dir = temp.path();
    std::fs::write(config_dir.join("models.yaml"), "models: {}\n").unwrap();

    let source = LLMConfig::resolve_default_source_from(Some(config_dir), None).unwrap();

    assert_eq!(source.kind, LLMConfigSourceKind::UserModelsYaml);
    assert_eq!(source.path.as_ref(), Some(&config_dir.join("models.yaml")));
    assert!(source.warnings.is_empty());
  }

  #[test]
  fn resolves_env_override_before_user_config() {
    let temp = TempDir::new().unwrap();
    let config_dir = temp.path();
    let override_path = config_dir.join("override.yml");
    std::fs::write(config_dir.join("models.yml"), "models: {}\n").unwrap();

    let source = LLMConfig::resolve_default_source_from(
      Some(config_dir),
      Some(override_path.clone().into_os_string()),
    )
    .unwrap();

    assert_eq!(source.kind, LLMConfigSourceKind::EnvOverride);
    assert_eq!(source.path.as_ref(), Some(&override_path));
  }

  #[test]
  fn chat_type_string_parses_to_chat_shaped_model_type() {
    let yaml = r#"
models:
  gpt-4o:
    vendor: openai
    type: chat
"#;
    let config = LLMConfig::from_yaml(yaml).unwrap();
    let model = config.get_model("gpt-4o").unwrap();
    // Today `chat` collapses onto ModelType::Text; Slice 3 will rename
    // the variant to `Chat`. The acceptance contract here is that the
    // canonical `chat` type string is recognised at parse time.
    assert_eq!(model.granular_type(), ModelType::Text);
  }

  #[test]
  fn legacy_type_aliases_still_parse() {
    // Pre-P-LLM.0 YAML used three labels for chat-shaped models; all
    // three must keep working so existing model registries don't break.
    let yaml = r#"
models:
  text-model:
    vendor: openai
    type: text
  multimodal-model:
    vendor: openai
    type: multimodal
  vision-model:
    vendor: stepfun
    type: imageunderstand
  legacy-gen-image:
    vendor: stepfun
    type: generateimage
  legacy-edit-image:
    vendor: stepfun
    type: editimage
  legacy-imagen:
    vendor: google
    type: image
"#;
    let config = LLMConfig::from_yaml(yaml).unwrap();
    assert_eq!(
      config.get_model("text-model").unwrap().granular_type(),
      ModelType::Text
    );
    assert_eq!(
      config.get_model("multimodal-model").unwrap().granular_type(),
      ModelType::ImageUnderstand
    );
    assert_eq!(
      config.get_model("vision-model").unwrap().granular_type(),
      ModelType::ImageUnderstand
    );
    assert_eq!(
      config
        .get_model("legacy-gen-image")
        .unwrap()
        .granular_type(),
      ModelType::Text2Image
    );
    assert_eq!(
      config
        .get_model("legacy-edit-image")
        .unwrap()
        .granular_type(),
      ModelType::ImageEdit
    );
    // Historical `type: image` carried the "text → image generation"
    // meaning (Google Imagen entries used it pre-P-LLM.0); keep that
    // mapping stable so existing registries don't silently change
    // dispatch class on upgrade.
    assert_eq!(
      config.get_model("legacy-imagen").unwrap().granular_type(),
      ModelType::Text2Image
    );
  }

  #[test]
  fn accepts_field_overrides_inferred_modalities() {
    // Explicit `accepts` must win over what the type alone would imply.
    // This is the mechanism that lets us encode "GPT-4o is chat-shaped
    // AND accepts images" without inventing a separate type.
    let yaml = r#"
models:
  gpt-4o:
    vendor: openai
    type: chat
    accepts: [text, image]
  claude-text-only:
    vendor: anthropic
    type: chat
    accepts: [text]
"#;
    let config = LLMConfig::from_yaml(yaml).unwrap();

    let gpt4o = config.get_model("gpt-4o").unwrap();
    let gpt4o_accepts = gpt4o.accepts();
    assert!(gpt4o_accepts.contains(&InputType::Text));
    assert!(gpt4o_accepts.contains(&InputType::Image));
    assert_eq!(gpt4o_accepts.len(), 2);

    let claude = config.get_model("claude-text-only").unwrap();
    let claude_accepts = claude.accepts();
    assert!(claude_accepts.contains(&InputType::Text));
    assert!(!claude_accepts.contains(&InputType::Image));
    assert_eq!(claude_accepts.len(), 1);
  }

  #[test]
  fn bundled_default_models_yaml_uses_post_pllm0_schema() {
    // Snapshot test for the P-LLM.0 Slice 2 migration. Locks down the
    // post-migration shape of the bundled `default_models.yml` so a
    // future edit can't accidentally re-introduce the legacy
    // `text` / `multimodal` / `imageunderstand` / `generateimage` /
    // `editimage` / `image` strings.
    let yaml = include_str!("../../templates/default_models.yml");
    let config = LLMConfig::from_yaml(yaml).unwrap();

    let mut counts: std::collections::BTreeMap<&str, usize> =
      std::collections::BTreeMap::new();
    let mut accepts_image_count = 0usize;
    for model in config.models.values() {
      let t = model.model_type();
      *counts.entry(Box::leak(t.to_string().into_boxed_str())).or_default() += 1;
      if model.accepts().contains(&InputType::Image) {
        accepts_image_count += 1;
      }
    }

    // No legacy strings should appear.
    for legacy in ["multimodal", "imageunderstand", "generateimage", "editimage"] {
      assert!(
        !counts.contains_key(legacy),
        "legacy type label '{legacy}' must be migrated out of default_models.yml; \
         still present in {counts:?}"
      );
    }

    // Canonical chat type dominates the registry.
    let chat_count = counts.get("chat").copied().unwrap_or(0);
    assert!(
      chat_count >= 180,
      "expected ≥ 180 chat-shaped entries after migration, got {chat_count}"
    );

    // Generation/editing types use the new canonical names.
    assert!(counts.get("text_to_image").copied().unwrap_or(0) >= 1);
    assert!(counts.get("image_edit").copied().unwrap_or(0) >= 1);

    // At least the 65 image-accepting chat models surface their
    // capability via the explicit `accepts: [text, image]` field.
    assert!(
      accepts_image_count >= 65,
      "expected ≥ 65 entries with image in accepts, got {accepts_image_count}"
    );
  }

  #[test]
  fn accepts_falls_back_to_type_default_when_unset() {
    // The fallback path is the back-compat seam: a YAML entry written
    // before P-LLM.0 has no `accepts` field, so we derive it from the
    // historic `granular_type().supported_inputs()` behaviour.
    let yaml = r#"
models:
  legacy-text:
    vendor: openai
    type: text
  legacy-asr:
    vendor: openai
    type: asr
  legacy-tts:
    vendor: openai
    type: tts
"#;
    let config = LLMConfig::from_yaml(yaml).unwrap();
    let text_accepts = config.get_model("legacy-text").unwrap().accepts();
    assert!(text_accepts.contains(&InputType::Text));
    assert_eq!(text_accepts.len(), 1);

    let asr_accepts = config.get_model("legacy-asr").unwrap().accepts();
    assert!(asr_accepts.contains(&InputType::Audio));
    assert_eq!(asr_accepts.len(), 1);

    let tts_accepts = config.get_model("legacy-tts").unwrap().accepts();
    assert!(tts_accepts.contains(&InputType::Text));
    assert_eq!(tts_accepts.len(), 1);
  }

  #[test]
  fn resolves_builtin_default_when_no_user_config_exists() {
    let temp = TempDir::new().unwrap();

    let source = LLMConfig::resolve_default_source_from(Some(temp.path()), None).unwrap();

    assert_eq!(source.kind, LLMConfigSourceKind::BuiltInDefault);
    assert!(source.path.is_none());
  }
}

use crate::{
  config::{LLMConfig, ModelConfig},
  providers::{create_provider, LLMProvider},
  LLMError, Result,
};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

/// Global model registry that manages model configurations and provider instances
pub struct ModelRegistry {
  config: Arc<RwLock<Option<LLMConfig>>>,
  providers: Arc<RwLock<HashMap<String, Arc<dyn LLMProvider>>>>,
}

impl ModelRegistry {
  /// Create a new empty model registry
  pub fn new() -> Self {
    Self {
      config: Arc::new(RwLock::new(None)),
      providers: Arc::new(RwLock::new(HashMap::new())),
    }
  }

  /// Get the global singleton instance
  pub fn global() -> &'static ModelRegistry {
    static INSTANCE: OnceLock<ModelRegistry> = OnceLock::new();
    INSTANCE.get_or_init(ModelRegistry::new)
  }

  /// Load configuration from a YAML file and initialize providers
  pub async fn load_config(&self, config_path: &str) -> Result<()> {
    let config = LLMConfig::from_file(config_path).await?;
    
    // Validate configuration
    config.validate()?;
    
    // Initialize providers
    self.initialize_providers(&config).await?;
    
    // Store the config
    {
      let mut config_guard = self.config.write().unwrap();
      *config_guard = Some(config);
    }

    Ok(())
  }

  /// Load built-in default configuration
  pub async fn load_builtin_config(&self) -> Result<()> {
    let builtin_yaml = include_str!("../../templates/default_models.yml");
    self.load_config_from_yaml(builtin_yaml).await
  }

  /// Load configuration from YAML string
  pub async fn load_config_from_yaml(&self, yaml_content: &str) -> Result<()> {
    let config = LLMConfig::from_yaml(yaml_content)?;
    
    // Validate configuration
    config.validate()?;
    
    // Initialize providers
    self.initialize_providers(&config).await?;
    
    // Store the config
    {
      let mut config_guard = self.config.write().unwrap();
      *config_guard = Some(config);
    }

    Ok(())
  }

  /// Get a model configuration by name
  pub fn get_model(&self, model_name: &str) -> Result<ModelConfig> {
    let config_guard = self.config.read().unwrap();
    let config = config_guard.as_ref().ok_or_else(|| LLMError::ConfigurationError {
      message: "No configuration loaded. Call load_config() first.".to_string(),
    })?;

    config.get_model(model_name).cloned().map_err(|_| LLMError::ModelNotFound {
      model_name: model_name.to_string(),
    })
  }

  /// Get a provider instance by name
  pub fn get_provider(&self, provider_name: &str) -> Result<Arc<dyn LLMProvider>> {
    let providers_guard = self.providers.read().unwrap();
    providers_guard.get(provider_name).cloned().ok_or_else(|| LLMError::UnsupportedProvider {
      provider: provider_name.to_string(),
    })
  }

  /// List all available model names
  pub fn list_models(&self) -> Vec<String> {
    let config_guard = self.config.read().unwrap();
    if let Some(config) = config_guard.as_ref() {
      config.models.keys().cloned().collect()
    } else {
      Vec::new()
    }
  }

  /// List all available provider names
  pub fn list_providers(&self) -> Vec<String> {
    let providers_guard = self.providers.read().unwrap();
    providers_guard.keys().cloned().collect()
  }

  /// Check if a model is available
  pub fn has_model(&self, model_name: &str) -> bool {
    let config_guard = self.config.read().unwrap();
    if let Some(config) = config_guard.as_ref() {
      config.models.contains_key(model_name)
    } else {
      false
    }
  }

  /// Get the current configuration
  pub async fn get_config(&self) -> Result<LLMConfig> {
    let config_guard = self.config.read().unwrap();
    config_guard.as_ref().cloned().ok_or_else(|| LLMError::ConfigurationError {
      message: "No configuration loaded".to_string(),
    })
  }

  /// Get model information for debugging/inspection
  pub fn get_model_info(&self, model_name: &str) -> Result<ModelInfo> {
    let model_config = self.get_model(model_name)?;
    let provider = self.get_provider(&model_config.vendor)?;

    Ok(ModelInfo {
      name: model_name.to_string(),
      vendor: model_config.vendor.clone(),
      model_id: model_config.model_id.unwrap_or_else(|| model_name.to_string()),
      base_url: model_config.base_url.unwrap_or_else(|| provider.base_url().to_string()),
      temperature: model_config.temperature,
      max_tokens: model_config.max_tokens,
      supports_streaming: model_config.supports_streaming.unwrap_or(true),
    })
  }

  /// Validate all providers are working
  pub async fn validate_all_providers(&self) -> Result<ValidationReport> {
    let mut report = ValidationReport {
      valid_providers: Vec::new(),
      invalid_providers: Vec::new(),
    };

    let providers_guard = self.providers.read().unwrap();
    
    for (provider_name, provider) in providers_guard.iter() {
      match provider.validate_config().await {
        Ok(()) => report.valid_providers.push(provider_name.clone()),
        Err(e) => report.invalid_providers.push((provider_name.clone(), e.to_string())),
      }
    }

    Ok(report)
  }

  async fn initialize_providers(&self, config: &LLMConfig) -> Result<()> {
    let mut providers = HashMap::new();
    let mut unique_providers = std::collections::HashSet::new();

    // Collect all unique providers from model configurations
    for (_, model_config) in &config.models {
      unique_providers.insert(model_config.vendor.clone());
    }

    // Initialize each provider
    for provider_name in unique_providers {
      let api_key = config.get_api_key(&provider_name)?;
      
      let base_url = config
        .get_provider(&provider_name)
        .and_then(|p| p.base_url.clone());

      let provider = create_provider(&provider_name, &api_key, base_url)?;
      providers.insert(provider_name, Arc::from(provider));
    }

    // Store providers
    {
      let mut providers_guard = self.providers.write().unwrap();
      *providers_guard = providers;
    }

    Ok(())
  }
}

impl Default for ModelRegistry {
  fn default() -> Self {
    Self::new()
  }
}

/// Information about a model for debugging/inspection
#[derive(Debug, Clone)]
pub struct ModelInfo {
  pub name: String,
  pub vendor: String,
  pub model_id: String,
  pub base_url: String,
  pub temperature: Option<f32>,
  pub max_tokens: Option<u32>,
  pub supports_streaming: bool,
}

/// Report from provider validation
#[derive(Debug)]
pub struct ValidationReport {
  pub valid_providers: Vec<String>,
  pub invalid_providers: Vec<(String, String)>,
}

impl ValidationReport {
  pub fn is_all_valid(&self) -> bool {
    self.invalid_providers.is_empty()
  }

  pub fn summary(&self) -> String {
    let mut summary = String::new();
    
    if !self.valid_providers.is_empty() {
      summary.push_str("Valid providers:\n");
      for provider in &self.valid_providers {
        summary.push_str(&format!("  ✅ {}\n", provider));
      }
      summary.push('\n');
    }

    if !self.invalid_providers.is_empty() {
      summary.push_str("Invalid providers:\n");
      for (provider, error) in &self.invalid_providers {
        summary.push_str(&format!("  ❌ {}: {}\n", provider, error));
      }
    }

    summary
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::env;

  #[tokio::test]
  async fn test_registry_initialization() {
    let registry = ModelRegistry::new();
    assert_eq!(registry.list_models().len(), 0);
    assert_eq!(registry.list_providers().len(), 0);
  }

  #[tokio::test]
  async fn test_load_config_from_yaml() {
    env::set_var("TEST_OPENAI_API_KEY", "test-key");

    let yaml = r#"
models:
  gpt-4o:
    vendor: openai
    temperature: 0.7
    max_tokens: 4096

providers:
  openai:
    api_key_env: "TEST_OPENAI_API_KEY"
    timeout_seconds: 30
"#;

    let registry = ModelRegistry::new();
    let result = registry.load_config_from_yaml(yaml).await;
    assert!(result.is_ok());

    assert!(registry.has_model("gpt-4o"));
    assert_eq!(registry.list_models(), vec!["gpt-4o"]);
    assert_eq!(registry.list_providers(), vec!["openai"]);

    let model_info = registry.get_model_info("gpt-4o").unwrap();
    assert_eq!(model_info.vendor, "openai");
    assert_eq!(model_info.temperature, Some(0.7));

    env::remove_var("TEST_OPENAI_API_KEY");
  }

  #[tokio::test]
  async fn test_global_registry() {
    let registry1 = ModelRegistry::global();
    let registry2 = ModelRegistry::global();
    
    // Should be the same instance
    assert!(std::ptr::eq(registry1, registry2));
  }

  #[test]
  fn test_model_not_found() {
    let registry = ModelRegistry::new();
    let result = registry.get_model("nonexistent");
    assert!(matches!(result, Err(LLMError::ConfigurationError { .. })));
    
    assert!(!registry.has_model("nonexistent"));
  }
}
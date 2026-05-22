use crate::{
  LLMError, Result,
  config::{LLMConfig, ModelConfig},
  providers::{LLMProvider, create_provider},
};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock, RwLock};

/// Global model registry that manages model configurations and provider instances
pub struct ModelRegistry {
  config: Arc<RwLock<Option<LLMConfig>>>,
  providers: Arc<RwLock<HashMap<String, Arc<dyn LLMProvider>>>>,
  /// Vendors referenced by `config.models` whose API key env var
  /// was unset at `load_config*` time. Populated by
  /// [`Self::initialize_providers`] and consulted by
  /// [`Self::get_provider`] so that a lookup of a model whose vendor
  /// was skipped returns [`LLMError::MissingApiKey`] (actionable)
  /// rather than [`LLMError::UnsupportedProvider`] (misleading —
  /// the provider IS supported, just unauthenticated).
  ///
  /// See P10.3.1: lenient init.
  missing_key_providers: Arc<RwLock<HashSet<String>>>,
}

impl ModelRegistry {
  /// Create a new empty model registry
  pub fn new() -> Self {
    Self {
      config: Arc::new(RwLock::new(None)),
      providers: Arc::new(RwLock::new(HashMap::new())),
      missing_key_providers: Arc::new(RwLock::new(HashSet::new())),
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
      let mut config_guard = self.config.write().map_err(|e| LLMError::InternalError {
        message: format!("Configuration lock poisoned: {}", e),
      })?;
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
      let mut config_guard = self.config.write().map_err(|e| LLMError::InternalError {
        message: format!("Configuration lock poisoned: {}", e),
      })?;
      *config_guard = Some(config);
    }

    Ok(())
  }

  /// Get a model configuration by name
  pub fn get_model(&self, model_name: &str) -> Result<ModelConfig> {
    let config_guard = self.config.read().map_err(|e| LLMError::InternalError {
      message: format!("Configuration lock poisoned: {}", e),
    })?;
    let config = config_guard
      .as_ref()
      .ok_or_else(|| LLMError::ConfigurationError {
        message: "No configuration loaded. Call load_config() first.".to_string(),
      })?;

    config
      .get_model(model_name)
      .cloned()
      .map_err(|_| LLMError::ModelNotFound {
        model_name: model_name.to_string(),
      })
  }

  /// Get a provider instance by name
  pub fn get_provider(&self, provider_name: &str) -> Result<Arc<dyn LLMProvider>> {
    let providers_guard = self.providers.read().map_err(|e| LLMError::InternalError {
      message: format!("Providers lock poisoned: {}", e),
    })?;
    if let Some(provider) = providers_guard.get(provider_name).cloned() {
      return Ok(provider);
    }

    // P10.3.1: when a provider was skipped at init time due to a
    // missing API key, surface that fact instead of the generic
    // "unsupported provider" error — the provider IS supported,
    // it just wasn't authenticated.
    let missing_guard = self
      .missing_key_providers
      .read()
      .map_err(|e| LLMError::InternalError {
        message: format!("missing_key_providers lock poisoned: {}", e),
      })?;
    if missing_guard.contains(provider_name) {
      return Err(LLMError::MissingApiKey {
        provider: provider_name.to_string(),
      });
    }

    Err(LLMError::UnsupportedProvider {
      provider: provider_name.to_string(),
    })
  }

  /// List all available model names
  pub fn list_models(&self) -> Vec<String> {
    // Note: Returns empty vec if lock is poisoned to maintain backward compatibility
    let config_guard = match self.config.read() {
      Ok(guard) => guard,
      Err(_) => return Vec::new(),
    };
    if let Some(config) = config_guard.as_ref() {
      config.models.keys().cloned().collect()
    } else {
      Vec::new()
    }
  }

  /// List all available provider names
  pub fn list_providers(&self) -> Vec<String> {
    // Note: Returns empty vec if lock is poisoned to maintain backward compatibility
    let providers_guard = match self.providers.read() {
      Ok(guard) => guard,
      Err(_) => return Vec::new(),
    };
    providers_guard.keys().cloned().collect()
  }

  /// Check if a model is available
  pub fn has_model(&self, model_name: &str) -> bool {
    // Note: Returns false if lock is poisoned to maintain backward compatibility
    let config_guard = match self.config.read() {
      Ok(guard) => guard,
      Err(_) => return false,
    };
    if let Some(config) = config_guard.as_ref() {
      config.models.contains_key(model_name)
    } else {
      false
    }
  }

  /// Get the current configuration
  pub async fn get_config(&self) -> Result<LLMConfig> {
    let config_guard = self.config.read().map_err(|e| LLMError::InternalError {
      message: format!("Configuration lock poisoned: {}", e),
    })?;
    config_guard
      .as_ref()
      .cloned()
      .ok_or_else(|| LLMError::ConfigurationError {
        message: "No configuration loaded".to_string(),
      })
  }

  /// Get model information for debugging/inspection.
  ///
  /// Read-only by design: must not require a live provider, because
  /// inventory paths like `agentflow llm models` need to enumerate every
  /// declared model even when its provider's API key is unset. Falls back
  /// to the static config's `providers[vendor].base_url` when the model
  /// doesn't override it, and finally to an empty string when neither
  /// is available (rare; the registry is the source of truth for what
  /// base URL a request would use).
  pub fn get_model_info(&self, model_name: &str) -> Result<ModelInfo> {
    let model_config = self.get_model(model_name)?;
    let provider_base_url = {
      let config_guard = self.config.read().map_err(|e| LLMError::InternalError {
        message: format!("Configuration lock poisoned: {}", e),
      })?;
      config_guard
        .as_ref()
        .and_then(|c| c.providers.get(&model_config.vendor))
        .and_then(|p| p.base_url.clone())
    };

    Ok(ModelInfo {
      name: model_name.to_string(),
      vendor: model_config.vendor.clone(),
      model_id: model_config
        .model_id
        .unwrap_or_else(|| model_name.to_string()),
      base_url: model_config
        .base_url
        .or(provider_base_url)
        .unwrap_or_default(),
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

    let providers = {
      let providers_guard = self.providers.read().map_err(|e| LLMError::InternalError {
        message: format!("Providers lock poisoned: {}", e),
      })?;
      providers_guard
        .iter()
        .map(|(name, provider)| (name.clone(), Arc::clone(provider)))
        .collect::<Vec<_>>()
    };

    for (provider_name, provider) in providers {
      match provider.validate_config().await {
        Ok(()) => report.valid_providers.push(provider_name),
        Err(e) => report
          .invalid_providers
          .push((provider_name, e.to_string())),
      }
    }

    Ok(report)
  }

  async fn initialize_providers(&self, config: &LLMConfig) -> Result<()> {
    let mut providers = HashMap::new();
    let mut unique_providers = HashSet::new();
    let mut missing_keys: HashSet<String> = HashSet::new();

    // Collect all unique providers from model configurations
    for model_config in config.models.values() {
      unique_providers.insert(model_config.vendor.clone());
    }

    // P10.3.1: skip providers whose API key env var is unset rather
    // than fail-closing the whole init. The skipped vendor goes into
    // `missing_key_providers` so a later `get_provider()` lookup
    // can return [`LLMError::MissingApiKey`] instead of the generic
    // [`LLMError::UnsupportedProvider`]. `LLMConfig::validate()`
    // already printed a warning naming the affected models, so we
    // stay silent here to avoid double-warning the operator.
    for provider_name in unique_providers {
      let api_key = match config.get_api_key(&provider_name) {
        Ok(key) => key,
        Err(LLMError::MissingApiKey { .. }) => {
          missing_keys.insert(provider_name);
          continue;
        }
        Err(other) => return Err(other),
      };

      let base_url = config
        .get_provider(&provider_name)
        .and_then(|p| p.base_url.clone());

      let provider = create_provider(&provider_name, &api_key, base_url)?;
      providers.insert(provider_name, Arc::from(provider));
    }

    // Store providers
    {
      let mut providers_guard = self
        .providers
        .write()
        .map_err(|e| LLMError::InternalError {
          message: format!("Providers lock poisoned: {}", e),
        })?;
      *providers_guard = providers;
    }

    // Track providers we skipped so lookup-path errors are accurate.
    {
      let mut missing_guard =
        self
          .missing_key_providers
          .write()
          .map_err(|e| LLMError::InternalError {
            message: format!("missing_key_providers lock poisoned: {}", e),
          })?;
      *missing_guard = missing_keys;
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
    // SAFETY: this unit test mutates a dedicated test env var before reading it.
    unsafe {
      env::set_var("TEST_OPENAI_API_KEY", "test-key");
    }

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

    // SAFETY: cleanup of the dedicated test env var after the test read.
    unsafe {
      env::remove_var("TEST_OPENAI_API_KEY");
    }
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

  /// P10.3.1: loading a config that references two providers must
  /// succeed when only one provider's API key is set. The provider
  /// with the missing key is skipped (not initialised), and a
  /// later `get_provider()` lookup of the skipped provider returns
  /// [`LLMError::MissingApiKey`] instead of
  /// [`LLMError::UnsupportedProvider`]. The provider whose key IS
  /// set initialises normally and lookup succeeds.
  ///
  /// This is the core bug the P10.3.1 lenient-init landed to fix:
  /// pre-P10.3.1, fresh users with only one provider key set
  /// couldn't call `AgentFlow::init()` against the bundled
  /// `default_models.yml` (9 providers) without fail-closing on
  /// the 8 unset keys.
  ///
  /// **Test isolation**: `get_api_key` falls back to provider-
  /// specific common env vars (`DEEPSEEK_API_KEY`), so the test
  /// must temporarily clear them to be deterministic. `deepseek`
  /// is chosen for the "missing" side because it has a single
  /// fallback, minimising snapshot+restore noise.
  #[tokio::test]
  async fn load_config_skips_provider_with_missing_key_and_keeps_others() {
    let snapshot_configured_missing = env::var("P10_3_1_REG_DEEPSEEK_KEY").ok();
    let snapshot_fallback_missing = env::var("DEEPSEEK_API_KEY").ok();

    // SAFETY: this unit test mutates dedicated test env vars.
    // It explicitly *unsets* the deepseek key + its single
    // fallback so the assertion is deterministic regardless of
    // what's in the developer's environment.
    unsafe {
      env::set_var("P10_3_1_REG_OPENAI_KEY", "test-key-set");
      env::remove_var("P10_3_1_REG_DEEPSEEK_KEY");
      env::remove_var("DEEPSEEK_API_KEY");
    }

    let yaml = r#"
models:
  gpt-4o:
    vendor: openai
  deepseek-chat:
    vendor: deepseek

providers:
  openai:
    api_key_env: "P10_3_1_REG_OPENAI_KEY"
  deepseek:
    api_key_env: "P10_3_1_REG_DEEPSEEK_KEY"
"#;

    let registry = ModelRegistry::new();
    let load_result = registry.load_config_from_yaml(yaml).await;

    // Verify load succeeded BEFORE we restore env (otherwise a
    // restore-then-panic obscures the actual failure).
    if let Err(err) = &load_result {
      // SAFETY: restore env even on panic path.
      unsafe {
        env::remove_var("P10_3_1_REG_OPENAI_KEY");
        if let Some(value) = snapshot_configured_missing {
          env::set_var("P10_3_1_REG_DEEPSEEK_KEY", value);
        }
        if let Some(value) = snapshot_fallback_missing {
          env::set_var("DEEPSEEK_API_KEY", value);
        }
      }
      panic!("load must succeed even with one provider key missing (P10.3.1); got {err:?}");
    }

    // Both models stay registered (model lookup doesn't care about
    // auth state).
    assert!(registry.has_model("gpt-4o"));
    assert!(registry.has_model("deepseek-chat"));

    // The provider whose key IS set initialised normally.
    let openai_ok = registry.get_provider("openai").is_ok();

    // The provider whose key was UNSET must now surface a
    // [`LLMError::MissingApiKey`] (actionable) — NOT
    // [`LLMError::UnsupportedProvider`] (misleading).
    let deepseek_result = registry.get_provider("deepseek");

    // SAFETY: restore env state before asserting (so any panic
    // doesn't leave the dev's env polluted).
    unsafe {
      env::remove_var("P10_3_1_REG_OPENAI_KEY");
      if let Some(value) = snapshot_configured_missing {
        env::set_var("P10_3_1_REG_DEEPSEEK_KEY", value);
      }
      if let Some(value) = snapshot_fallback_missing {
        env::set_var("DEEPSEEK_API_KEY", value);
      }
    }

    assert!(openai_ok, "openai must initialise when its key is set");
    match deepseek_result {
      Err(LLMError::MissingApiKey { provider }) => {
        assert_eq!(provider, "deepseek");
      }
      Err(other) => panic!("expected MissingApiKey, got {other:?}"),
      Ok(_) => panic!("deepseek provider must not initialise — its key was unset"),
    }
  }
}

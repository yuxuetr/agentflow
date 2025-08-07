//! Configuration updater for adding discovered models to default_models.yml

use super::{DiscoveredModel, ModelFetcher};
use crate::{LLMError, Result, config::{LLMConfig, ModelConfig}};
use std::collections::HashMap;
use std::path::Path;
use tracing::info;

/// Updates configuration files with discovered models
pub struct ConfigUpdater {
  fetcher: ModelFetcher,
}

impl ConfigUpdater {
  /// Create a new ConfigUpdater instance
  pub fn new() -> Result<Self> {
    let fetcher = ModelFetcher::new()?;
    Ok(Self { fetcher })
  }

  /// Fetch models from all vendors and update the default configuration
  pub async fn update_default_models(&self, config_path: &str) -> Result<UpdateResult> {
    info!("Fetching models from all supported vendors...");
    let discovered_models = self.fetcher.fetch_all_models().await;
    
    if discovered_models.is_empty() {
      return Err(LLMError::ConfigurationError {
        message: "No models could be fetched from any vendor".to_string(),
      });
    }

    info!("Fetched models from {} vendors", discovered_models.len());
    
    // Load existing configuration
    let mut config = if Path::new(config_path).exists() {
      LLMConfig::from_file(config_path).await?
    } else {
      info!("Configuration file doesn't exist, creating new one");
      LLMConfig::default()
    };

    let mut stats = UpdateResult::new();
    
    // Add discovered models to configuration
    for (vendor, models) in discovered_models {
      info!("Processing {} models from {}", models.len(), vendor);
      let vendor_stats = self.add_vendor_models(&mut config, &vendor, &models).await;
      stats.merge(vendor_stats);
    }

    // Write updated configuration back to file
    self.write_config(&config, config_path).await?;
    
    info!("Configuration updated successfully: {} new models, {} updated models", 
      stats.added_models, stats.updated_models);
    
    Ok(stats)
  }

  /// Add models from a specific vendor to the configuration
  async fn add_vendor_models(&self, config: &mut LLMConfig, vendor: &str, models: &[DiscoveredModel]) -> UpdateResult {
    let mut stats = UpdateResult::new();
    
    for model in models {
      let model_key = self.generate_model_key(&model.id, vendor);
      let model_config = self.create_model_config(model, vendor);
      
      if config.models.contains_key(&model_key) {
        // Update existing model
        config.models.insert(model_key.clone(), model_config);
        stats.updated_models += 1;
        stats.updated_model_names.push(model_key);
      } else {
        // Add new model
        config.models.insert(model_key.clone(), model_config);
        stats.added_models += 1;
        stats.added_model_names.push(model_key);
      }
    }
    
    stats
  }

  /// Generate a consistent model key for the configuration
  fn generate_model_key(&self, model_id: &str, vendor: &str) -> String {
    // For some vendors, clean up the model ID for better readability
    match vendor {
      "google" => {
        // Remove "models/" prefix from Gemini models
        if model_id.starts_with("models/") {
          model_id.strip_prefix("models/").unwrap_or(model_id).to_string()
        } else {
          model_id.to_string()
        }
      }
      _ => model_id.to_string()
    }
  }

  /// Create a ModelConfig from a discovered model
  fn create_model_config(&self, model: &DiscoveredModel, vendor: &str) -> ModelConfig {
    let mut config = ModelConfig {
      vendor: vendor.to_string(),
      r#type: Some("text".to_string()), // Default type
      model_id: if model.id != self.generate_model_key(&model.id, vendor) {
        Some(model.id.clone())
      } else {
        None
      },
      base_url: None,
      temperature: Some(self.get_default_temperature(vendor)),
      top_p: None,
      max_tokens: Some(self.get_default_max_tokens(vendor)),
      frequency_penalty: None,
      stop: None,
      n: None,
      supports_streaming: Some(true), // Assume streaming support
      supports_tools: Some(self.model_supports_tools(&model.id, vendor)),
      supports_multimodal: Some(self.model_supports_multimodal(&model.id, vendor)),
      response_format: None,
      additional_params: HashMap::new(),
    };

    // Vendor-specific adjustments
    match vendor {
      "anthropic" => {
        config.supports_multimodal = Some(self.is_anthropic_multimodal(&model.id));
        config.supports_tools = Some(true);
      }
      "google" => {
        config.supports_multimodal = Some(self.is_google_multimodal(&model.id));
        config.supports_tools = Some(self.is_google_tools_model(&model.id));
        if model.id.contains("embedding") {
          config.r#type = Some("embedding".to_string());
          config.supports_streaming = Some(false);
          config.supports_tools = Some(false);
          config.supports_multimodal = Some(false);
        }
        if model.id.contains("imagen") {
          config.r#type = Some("image".to_string());
          config.supports_streaming = Some(false);
          config.supports_tools = Some(false);
          config.supports_multimodal = Some(false);
        }
      }
      "dashscope" => {
        config.supports_multimodal = Some(self.is_qwen_multimodal(&model.id));
        if model.id.contains("tts") {
          config.r#type = Some("tts".to_string());
          config.supports_streaming = Some(false);
          config.supports_tools = Some(false);
          config.supports_multimodal = Some(false);
        }
        if model.id.contains("vl") {
          config.r#type = Some("multimodal".to_string());
          config.supports_multimodal = Some(true);
        }
      }
      "moonshot" => {
        config.supports_multimodal = Some(model.id.contains("vision"));
        if model.id.contains("vision") {
          config.r#type = Some("multimodal".to_string());
        }
      }
      "step" => {
        // Determine model type and capabilities
        if model.id.contains("tts") {
          config.r#type = Some("tts".to_string());
          config.supports_streaming = Some(false);
          config.supports_tools = Some(false);
          config.supports_multimodal = Some(false);
        } else if model.id.contains("asr") {
          config.r#type = Some("asr".to_string());
          config.supports_streaming = Some(false);
          config.supports_tools = Some(false);
          config.supports_multimodal = Some(true);
        } else if model.id.contains("audio") {
          config.r#type = Some("audio".to_string());
          config.supports_tools = Some(false);
          config.supports_multimodal = Some(true);
        } else if model.id.contains("v-") || model.id.contains("-v") || model.id.contains("vision") || model.id.starts_with("step-2") {
          config.r#type = Some("multimodal".to_string());
          config.supports_multimodal = Some(true);
        } else {
          config.r#type = Some("text".to_string());
          config.supports_multimodal = Some(false);
        }
      }
      _ => {}
    }

    config
  }

  /// Get default temperature for a vendor
  fn get_default_temperature(&self, vendor: &str) -> f32 {
    match vendor {
      "anthropic" => 0.6,
      "google" => 0.7,
      "moonshot" => 0.7,
      "dashscope" => 0.7,
      "step" => 0.7,
      _ => 0.7,
    }
  }

  /// Get default max tokens for a vendor
  fn get_default_max_tokens(&self, vendor: &str) -> u32 {
    match vendor {
      "anthropic" => 8192,
      "google" => 8192,
      "moonshot" => 4096,
      "dashscope" => 4096,
      "step" => 8192,
      _ => 4096,
    }
  }

  /// Check if a model supports tools/function calling
  fn model_supports_tools(&self, model_id: &str, vendor: &str) -> bool {
    match vendor {
      "anthropic" => true, // Most Claude models support tools
      "google" => !model_id.contains("embedding") && !model_id.contains("imagen") && !model_id.contains("veo"),
      "moonshot" => true,
      "dashscope" => !model_id.contains("tts") && !model_id.contains("vl") && !model_id.contains("embedding"),
      "step" => !model_id.contains("tts") && !model_id.contains("asr") && !model_id.contains("audio"),
      _ => false,
    }
  }

  /// Check if a model supports multimodal input
  fn model_supports_multimodal(&self, model_id: &str, vendor: &str) -> bool {
    match vendor {
      "anthropic" => self.is_anthropic_multimodal(model_id),
      "google" => self.is_google_multimodal(model_id),
      "moonshot" => model_id.contains("vision"),
      "dashscope" => self.is_qwen_multimodal(model_id),
      "step" => model_id.contains("v-") || model_id.contains("-v") || model_id.contains("vision") || model_id.starts_with("step-2") || model_id.contains("audio") || model_id.contains("asr"),
      _ => false,
    }
  }

  fn is_anthropic_multimodal(&self, model_id: &str) -> bool {
    // Claude 3+ models support multimodal
    model_id.contains("claude-3") || model_id.contains("claude-4") || model_id.contains("sonnet") || model_id.contains("haiku")
  }

  fn is_google_multimodal(&self, model_id: &str) -> bool {
    // Gemini models except embedding and generation models
    model_id.contains("gemini") && !model_id.contains("embedding") && !model_id.contains("imagen") && !model_id.contains("veo")
  }

  fn is_google_tools_model(&self, model_id: &str) -> bool {
    // Most Gemini models support tools, except specialized ones
    model_id.contains("gemini") && !model_id.contains("embedding") && !model_id.contains("imagen") && !model_id.contains("veo") && !model_id.contains("tts")
  }

  fn is_qwen_multimodal(&self, model_id: &str) -> bool {
    // Qwen VL (vision-language) models
    model_id.contains("vl") || model_id.contains("vision")
  }

  /// Write configuration to YAML file
  async fn write_config(&self, config: &LLMConfig, path: &str) -> Result<()> {
    let yaml_content = serde_yaml::to_string(config).map_err(|e| LLMError::ConfigurationError {
      message: format!("Failed to serialize config to YAML: {}", e),
    })?;

    // Add header comment
    let full_content = format!(
      "# AgentFlow LLM Configuration\n# Auto-updated with discovered models\n# Last updated: {}\n\n{}",
      chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
      yaml_content
    );

    tokio::fs::write(path, full_content).await.map_err(|e| LLMError::ConfigurationError {
      message: format!("Failed to write config file: {}", e),
    })?;

    Ok(())
  }
}

/// Result of configuration update operation
#[derive(Debug, Clone)]
pub struct UpdateResult {
  pub added_models: usize,
  pub updated_models: usize,
  pub added_model_names: Vec<String>,
  pub updated_model_names: Vec<String>,
  pub failed_vendors: Vec<String>,
}

impl UpdateResult {
  fn new() -> Self {
    Self {
      added_models: 0,
      updated_models: 0,
      added_model_names: Vec::new(),
      updated_model_names: Vec::new(),
      failed_vendors: Vec::new(),
    }
  }

  fn merge(&mut self, other: UpdateResult) {
    self.added_models += other.added_models;
    self.updated_models += other.updated_models;
    self.added_model_names.extend(other.added_model_names);
    self.updated_model_names.extend(other.updated_model_names);
    self.failed_vendors.extend(other.failed_vendors);
  }

  /// Create a summary report of the update
  pub fn create_report(&self) -> String {
    let mut report = String::new();
    
    report.push_str("Model Configuration Update Report\n");
    report.push_str("=================================\n\n");
    
    report.push_str(&format!("✅ Added Models: {}\n", self.added_models));
    if !self.added_model_names.is_empty() {
      for name in &self.added_model_names {
        report.push_str(&format!("  - {}\n", name));
      }
    }
    report.push('\n');
    
    report.push_str(&format!("🔄 Updated Models: {}\n", self.updated_models));
    if !self.updated_model_names.is_empty() {
      for name in &self.updated_model_names {
        report.push_str(&format!("  - {}\n", name));
      }
    }
    report.push('\n');
    
    if !self.failed_vendors.is_empty() {
      report.push_str(&format!("❌ Failed Vendors: {}\n", self.failed_vendors.len()));
      for vendor in &self.failed_vendors {
        report.push_str(&format!("  - {}\n", vendor));
      }
      report.push('\n');
    }
    
    report.push_str(&format!("Total changes: {} models processed\n", 
      self.added_models + self.updated_models));
    
    report
  }
}

impl Default for ConfigUpdater {
  fn default() -> Self {
    Self::new().expect("Failed to create ConfigUpdater")
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_model_key_generation() {
    let updater = ConfigUpdater::new().unwrap();
    
    // Google models should have "models/" prefix removed
    assert_eq!(updater.generate_model_key("models/gemini-1.5-pro", "google"), "gemini-1.5-pro");
    assert_eq!(updater.generate_model_key("gemini-1.5-flash", "google"), "gemini-1.5-flash");
    
    // Other vendors should keep original ID
    assert_eq!(updater.generate_model_key("moonshot-v1-8k", "moonshot"), "moonshot-v1-8k");
    assert_eq!(updater.generate_model_key("claude-3-sonnet", "anthropic"), "claude-3-sonnet");
  }

  #[test]
  fn test_multimodal_detection() {
    let updater = ConfigUpdater::new().unwrap();
    
    // Anthropic
    assert!(updater.is_anthropic_multimodal("claude-3-sonnet"));
    assert!(updater.is_anthropic_multimodal("claude-3-haiku"));
    assert!(!updater.is_anthropic_multimodal("claude-2"));
    
    // Google
    assert!(updater.is_google_multimodal("gemini-1.5-pro"));
    assert!(!updater.is_google_multimodal("text-embedding-004"));
    assert!(!updater.is_google_multimodal("imagen-3.0-generate-002"));
    
    // Moonshot
    assert!(updater.model_supports_multimodal("moonshot-v1-8k-vision-preview", "moonshot"));
    assert!(!updater.model_supports_multimodal("moonshot-v1-8k", "moonshot"));
    
    // Qwen/DashScope
    assert!(updater.is_qwen_multimodal("qwen-vl-max"));
    assert!(!updater.is_qwen_multimodal("qwen-turbo"));
  }

  #[test]
  fn test_update_result_report() {
    let mut result = UpdateResult::new();
    result.added_models = 2;
    result.updated_models = 1;
    result.added_model_names = vec!["model1".to_string(), "model2".to_string()];
    result.updated_model_names = vec!["model3".to_string()];
    result.failed_vendors = vec!["vendor1".to_string()];
    
    let report = result.create_report();
    
    assert!(report.contains("Added Models: 2"));
    assert!(report.contains("Updated Models: 1"));
    assert!(report.contains("Failed Vendors: 1"));
    assert!(report.contains("model1"));
    assert!(report.contains("model2"));
    assert!(report.contains("model3"));
    assert!(report.contains("vendor1"));
  }
}
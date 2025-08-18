//! Model validation functionality for verifying user-specified models

use super::{ModelFetcher, VendorConfig};
use crate::{config::LLMConfig, Result};
use std::collections::HashSet;
use tracing::{info, warn};

/// Result of model validation
#[derive(Debug, Clone)]
pub struct ValidationResult {
  pub valid_models: Vec<String>,
  pub invalid_models: Vec<InvalidModel>,
  pub unavailable_vendors: Vec<String>,
}

/// Information about an invalid model
#[derive(Debug, Clone)]
pub struct InvalidModel {
  pub model_name: String,
  pub vendor: String,
  pub error: String,
}

/// Validator for checking if user-specified models exist in vendor APIs
pub struct ModelValidator {
  fetcher: ModelFetcher,
}

impl ModelValidator {
  /// Create a new ModelValidator instance
  pub fn new() -> Result<Self> {
    let fetcher = ModelFetcher::new()?;
    Ok(Self { fetcher })
  }

  /// Validate all models in a configuration
  pub async fn validate_config(&self, config: &LLMConfig) -> ValidationResult {
    let mut valid_models = Vec::new();
    let mut invalid_models = Vec::new();
    let mut unavailable_vendors = HashSet::new();

    for (model_name, model_config) in &config.models {
      info!(
        "Validating model: {} (vendor: {})",
        model_name, model_config.vendor
      );

      match self.validate_single_model(model_name, model_config).await {
        Ok(is_valid) => {
          if is_valid {
            valid_models.push(model_name.clone());
          } else {
            invalid_models.push(InvalidModel {
              model_name: model_name.clone(),
              vendor: model_config.vendor.clone(),
              error: "Model not found in vendor's model list".to_string(),
            });
          }
        }
        Err(e) => {
          warn!("Failed to validate model {}: {}", model_name, e);
          unavailable_vendors.insert(model_config.vendor.clone());
          invalid_models.push(InvalidModel {
            model_name: model_name.clone(),
            vendor: model_config.vendor.clone(),
            error: format!("Validation failed: {}", e),
          });
        }
      }
    }

    ValidationResult {
      valid_models,
      invalid_models,
      unavailable_vendors: unavailable_vendors.into_iter().collect(),
    }
  }

  /// Validate a single model configuration
  async fn validate_single_model(
    &self,
    model_name: &str,
    model_config: &crate::config::ModelConfig,
  ) -> Result<bool> {
    let vendor_config = VendorConfig::get_by_name(&model_config.vendor);

    // If vendor doesn't support model listing, assume model is valid
    if vendor_config.is_none() || !vendor_config.as_ref().unwrap().supports_model_list {
      info!(
        "Vendor {} doesn't support model listing, skipping validation for {}",
        model_config.vendor, model_name
      );
      return Ok(true);
    }

    // Get the actual model ID to check (use model_id if specified, otherwise use model_name)
    let model_id_to_check = model_config.model_id.as_ref().map_or(model_name, |v| v);

    // Check if model exists in vendor's API
    self
      .fetcher
      .model_exists(&model_config.vendor, model_id_to_check)
      .await
  }

  /// Validate a specific model by name and vendor
  pub async fn validate_model(&self, model_name: &str, vendor: &str) -> Result<bool> {
    self.fetcher.model_exists(vendor, model_name).await
  }

  /// Get available models for a vendor (for suggesting alternatives)
  pub async fn get_available_models(&self, vendor: &str) -> Result<Vec<String>> {
    let models = self.fetcher.fetch_models_by_vendor_name(vendor).await?;
    Ok(models.into_iter().map(|m| m.id).collect())
  }

  /// Find similar model names (basic string matching)
  pub async fn suggest_similar_models(
    &self,
    target_model: &str,
    vendor: &str,
  ) -> Result<Vec<String>> {
    let available = self.get_available_models(vendor).await?;
    let target_lower = target_model.to_lowercase();

    let mut suggestions: Vec<(String, usize)> = available
      .into_iter()
      .map(|model| {
        let model_lower = model.to_lowercase();
        let similarity = self.calculate_similarity(&target_lower, &model_lower);
        (model, similarity)
      })
      .filter(|(_, similarity)| *similarity > 0)
      .collect();

    // Sort by similarity (higher is better)
    suggestions.sort_by(|a, b| b.1.cmp(&a.1));

    // Return top 5 suggestions
    Ok(
      suggestions
        .into_iter()
        .take(5)
        .map(|(model, _)| model)
        .collect(),
    )
  }

  /// Simple string similarity calculation (number of common substrings)
  fn calculate_similarity(&self, a: &str, b: &str) -> usize {
    let mut score = 0;

    // Exact match gets highest score
    if a == b {
      return 1000;
    }

    // Check if one contains the other
    if a.contains(b) || b.contains(a) {
      score += 100;
    }

    // Count common words
    let words_a: HashSet<&str> = a.split(&['-', '_', ' ']).collect();
    let words_b: HashSet<&str> = b.split(&['-', '_', ' ']).collect();
    let common_words = words_a.intersection(&words_b).count();
    score += common_words * 10;

    // Count common characters
    let chars_a: HashSet<char> = a.chars().collect();
    let chars_b: HashSet<char> = b.chars().collect();
    let common_chars = chars_a.intersection(&chars_b).count();
    score += common_chars;

    score
  }

  /// Create a validation report
  pub fn create_report(&self, result: &ValidationResult) -> String {
    let mut report = String::new();

    report.push_str("Model Validation Report\n");
    report.push_str("======================\n\n");

    if !result.valid_models.is_empty() {
      report.push_str(&format!(
        "✅ Valid Models ({}):\n",
        result.valid_models.len()
      ));
      for model in &result.valid_models {
        report.push_str(&format!("  - {}\n", model));
      }
      report.push('\n');
    }

    if !result.invalid_models.is_empty() {
      report.push_str(&format!(
        "❌ Invalid Models ({}):\n",
        result.invalid_models.len()
      ));
      for invalid in &result.invalid_models {
        report.push_str(&format!(
          "  - {} ({}): {}\n",
          invalid.model_name, invalid.vendor, invalid.error
        ));
      }
      report.push('\n');
    }

    if !result.unavailable_vendors.is_empty() {
      report.push_str(&format!(
        "⚠️  Unavailable Vendors ({}):\n",
        result.unavailable_vendors.len()
      ));
      for vendor in &result.unavailable_vendors {
        report.push_str(&format!("  - {}\n", vendor));
      }
      report.push('\n');
    }

    report.push_str(&format!(
      "Summary: {}/{} models validated successfully\n",
      result.valid_models.len(),
      result.valid_models.len() + result.invalid_models.len()
    ));

    report
  }
}

impl Default for ModelValidator {
  fn default() -> Self {
    Self::new().expect("Failed to create ModelValidator")
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::ModelConfig;

  #[test]
  fn test_similarity_calculation() {
    let validator = ModelValidator::new().unwrap();

    // Exact match
    assert!(
      validator.calculate_similarity("gpt-4", "gpt-4")
        > validator.calculate_similarity("gpt-4", "gpt-3")
    );

    // Partial match
    assert!(
      validator.calculate_similarity("gpt-4-turbo", "gpt-4")
        > validator.calculate_similarity("gpt-4-turbo", "claude")
    );

    // No match
    assert_eq!(validator.calculate_similarity("abc", "xyz"), 0);
  }

  #[test]
  fn test_validation_result_display() {
    let result = ValidationResult {
      valid_models: vec!["gpt-4".to_string(), "claude-3".to_string()],
      invalid_models: vec![InvalidModel {
        model_name: "fake-model".to_string(),
        vendor: "openai".to_string(),
        error: "Not found".to_string(),
      }],
      unavailable_vendors: vec!["unavailable-vendor".to_string()],
    };

    let validator = ModelValidator::new().unwrap();
    let report = validator.create_report(&result);

    assert!(report.contains("Valid Models (2)"));
    assert!(report.contains("Invalid Models (1)"));
    assert!(report.contains("Unavailable Vendors (1)"));
    assert!(report.contains("gpt-4"));
    assert!(report.contains("claude-3"));
    assert!(report.contains("fake-model"));
  }

  #[tokio::test]
  #[ignore] // Integration test - requires API keys
  async fn test_validate_known_model() {
    use std::env;

    // Skip if no API key available
    if env::var("MOONSHOT_API_KEY").is_err() {
      return;
    }

    let validator = ModelValidator::new().unwrap();
    let result = validator.validate_model("moonshot-v1-8k", "moonshot").await;

    match result {
      Ok(is_valid) => {
        println!("moonshot-v1-8k validation result: {}", is_valid);
      }
      Err(e) => {
        println!("Validation failed: {}", e);
      }
    }
  }
}

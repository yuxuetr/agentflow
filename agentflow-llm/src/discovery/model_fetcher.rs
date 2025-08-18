//! Model fetcher implementation for retrieving model lists from vendors

use super::{create_http_client, DiscoveredModel, ModelListResponse, VendorConfig};
use crate::{LLMError, Result};
use reqwest::Client;
use std::collections::HashMap;
use std::env;
use tracing::{debug, error, info};

/// Main struct for fetching model lists from various vendors
pub struct ModelFetcher {
  client: Client,
}

impl ModelFetcher {
  /// Create a new ModelFetcher instance
  pub fn new() -> Result<Self> {
    let client = create_http_client()?;
    Ok(Self { client })
  }

  /// Fetch models from all supported vendors
  pub async fn fetch_all_models(&self) -> HashMap<String, Vec<DiscoveredModel>> {
    let mut all_models = HashMap::new();
    let vendors = VendorConfig::vendors_with_model_list();

    for vendor in vendors {
      info!("Fetching models from vendor: {}", vendor.name);
      match self.fetch_models_for_vendor(&vendor).await {
        Ok(models) => {
          info!(
            "Successfully fetched {} models from {}",
            models.len(),
            vendor.name
          );
          all_models.insert(vendor.name.clone(), models);
        }
        Err(e) => {
          error!("Failed to fetch models from {}: {}", vendor.name, e);
          // Continue with other vendors even if one fails
        }
      }
    }

    all_models
  }

  /// Fetch models from a specific vendor
  pub async fn fetch_models_for_vendor(
    &self,
    vendor: &VendorConfig,
  ) -> Result<Vec<DiscoveredModel>> {
    if !vendor.supports_model_list {
      return Err(LLMError::UnsupportedOperation {
        message: format!(
          "Vendor {} does not support model list fetching",
          vendor.name
        ),
      });
    }

    let api_key = self.get_api_key(&vendor.api_key_env)?;
    let response = self.make_request(vendor, &api_key).await?;

    let models = self.parse_response(response, &vendor.name).await?;
    debug!("Parsed {} models from {}", models.len(), vendor.name);

    Ok(models)
  }

  /// Get API key from environment variable
  fn get_api_key(&self, env_var: &str) -> Result<String> {
    env::var(env_var).map_err(|_| LLMError::MissingApiKey {
      provider: env_var.to_string(),
    })
  }

  /// Make HTTP request to vendor's models endpoint
  async fn make_request(&self, vendor: &VendorConfig, api_key: &str) -> Result<reqwest::Response> {
    let mut request = self.client.get(&vendor.models_endpoint);

    // Add authentication header
    let auth_value = match vendor.auth_header.as_str() {
      "Authorization" => format!("Bearer {}", api_key),
      "x-api-key" => api_key.to_string(),
      _ => format!("Bearer {}", api_key),
    };

    request = request.header(&vendor.auth_header, auth_value);

    // Add additional headers
    for (key, value) in &vendor.additional_headers {
      request = request.header(key, value);
    }

    debug!("Making request to: {}", vendor.models_endpoint);

    let response = request.send().await.map_err(|e| LLMError::NetworkError {
      message: format!("Failed to fetch models from {}: {}", vendor.name, e),
    })?;

    if !response.status().is_success() {
      let status = response.status();
      let error_text = response.text().await.unwrap_or_default();
      return Err(LLMError::ApiError {
        provider: vendor.name.clone(),
        status_code: status.as_u16(),
        message: format!("API request failed: {} - {}", status, error_text),
      });
    }

    Ok(response)
  }

  /// Parse response from vendor API
  async fn parse_response(
    &self,
    response: reqwest::Response,
    vendor_name: &str,
  ) -> Result<Vec<DiscoveredModel>> {
    let response_text = response.text().await.map_err(|e| LLMError::NetworkError {
      message: format!("Failed to read response body: {}", e),
    })?;

    debug!(
      "Response from {}: {}",
      vendor_name,
      &response_text[..response_text.len().min(500)]
    );

    let model_response: ModelListResponse =
      serde_json::from_str(&response_text).map_err(|e| LLMError::ParseError {
        message: format!(
          "Failed to parse models response from {}: {}",
          vendor_name, e
        ),
      })?;

    let mut models = Vec::new();
    for model_data in model_response.data {
      let mut model: DiscoveredModel = model_data.into();
      model.vendor = vendor_name.to_string();
      models.push(model);
    }

    Ok(models)
  }

  /// Fetch models from a specific vendor by name
  pub async fn fetch_models_by_vendor_name(
    &self,
    vendor_name: &str,
  ) -> Result<Vec<DiscoveredModel>> {
    let vendor =
      VendorConfig::get_by_name(vendor_name).ok_or_else(|| LLMError::UnsupportedProvider {
        provider: vendor_name.to_string(),
      })?;

    self.fetch_models_for_vendor(&vendor).await
  }

  /// Check if a specific model exists for a vendor
  pub async fn model_exists(&self, vendor_name: &str, model_id: &str) -> Result<bool> {
    let models = self.fetch_models_by_vendor_name(vendor_name).await?;
    Ok(models.iter().any(|m| m.id == model_id))
  }

  /// Get model information if it exists
  pub async fn get_model_info(
    &self,
    vendor_name: &str,
    model_id: &str,
  ) -> Result<Option<DiscoveredModel>> {
    let models = self.fetch_models_by_vendor_name(vendor_name).await?;
    Ok(models.into_iter().find(|m| m.id == model_id))
  }
}

impl Default for ModelFetcher {
  fn default() -> Self {
    Self::new().expect("Failed to create ModelFetcher")
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::env;

  #[tokio::test]
  async fn test_model_fetcher_creation() {
    let fetcher = ModelFetcher::new();
    assert!(fetcher.is_ok());
  }

  #[test]
  fn test_get_api_key_missing() {
    let fetcher = ModelFetcher::new().unwrap();
    let result = fetcher.get_api_key("NONEXISTENT_API_KEY");
    assert!(result.is_err());
  }

  #[test]
  fn test_get_api_key_present() {
    env::set_var("TEST_API_KEY", "test-key-value");
    let fetcher = ModelFetcher::new().unwrap();
    let result = fetcher.get_api_key("TEST_API_KEY");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "test-key-value");
    env::remove_var("TEST_API_KEY");
  }

  // Integration tests require API keys - only run if environment variables are set
  #[tokio::test]
  #[ignore] // Use `cargo test -- --ignored` to run these tests
  async fn test_fetch_moonshot_models() {
    if env::var("MOONSHOT_API_KEY").is_err() {
      return; // Skip test if API key not available
    }

    let fetcher = ModelFetcher::new().unwrap();
    let result = fetcher.fetch_models_by_vendor_name("moonshot").await;

    match result {
      Ok(models) => {
        assert!(!models.is_empty());
        println!("Found {} MoonShot models", models.len());
        for model in models.iter().take(3) {
          println!("  - {}", model.id);
        }
      }
      Err(e) => {
        println!("Failed to fetch MoonShot models: {}", e);
      }
    }
  }
}

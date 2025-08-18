//! Model discovery and validation module
//!
//! This module provides functionality to:
//! - Fetch available model lists from supported vendors
//! - Verify if user-specified models exist
//! - Update configuration with discovered models

use crate::{LLMError, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

pub mod config_updater;
pub mod model_fetcher;
pub mod model_validator;

pub use config_updater::{ConfigUpdater, UpdateResult};
pub use model_fetcher::ModelFetcher;
pub use model_validator::ModelValidator;

/// Represents a model from a vendor's API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredModel {
  pub id: String,
  pub vendor: String,
  pub display_name: Option<String>,
  pub owned_by: Option<String>,
  pub created: Option<u64>,
  pub object: Option<String>,
}

/// Response structure for model list endpoints
#[derive(Debug, Deserialize)]
pub struct ModelListResponse {
  pub object: Option<String>,
  pub data: Vec<ModelData>,
  pub has_more: Option<bool>,
}

/// Individual model data from API responses
#[derive(Debug, Deserialize)]
pub struct ModelData {
  pub id: String,
  pub object: Option<String>,
  pub created: Option<u64>,
  pub owned_by: Option<String>,
  #[serde(alias = "display_name")]
  pub display_name: Option<String>,
  pub r#type: Option<String>,
  pub permission: Option<Vec<serde_json::Value>>,
}

impl From<ModelData> for DiscoveredModel {
  fn from(data: ModelData) -> Self {
    Self {
      id: data.id,
      vendor: String::new(), // Will be set by the caller
      display_name: data.display_name,
      owned_by: data.owned_by,
      created: data.created,
      object: data.object,
    }
  }
}

/// Vendor-specific API endpoints and configurations
#[derive(Debug, Clone)]
pub struct VendorConfig {
  pub name: String,
  pub models_endpoint: String,
  pub api_key_env: String,
  pub auth_header: String,
  pub additional_headers: HashMap<String, String>,
  pub supports_model_list: bool,
}

impl VendorConfig {
  /// Get configurations for all supported vendors
  pub fn all_vendors() -> Vec<Self> {
    vec![
      // MoonShot
      Self {
        name: "moonshot".to_string(),
        models_endpoint: "https://api.moonshot.cn/v1/models".to_string(),
        api_key_env: "MOONSHOT_API_KEY".to_string(),
        auth_header: "Authorization".to_string(),
        additional_headers: HashMap::new(),
        supports_model_list: true,
      },
      // DashScope (Alibaba Qwen)
      Self {
        name: "dashscope".to_string(),
        models_endpoint: "https://dashscope.aliyuncs.com/compatible-mode/v1/models".to_string(),
        api_key_env: "DASHSCOPE_API_KEY".to_string(),
        auth_header: "Authorization".to_string(),
        additional_headers: HashMap::new(),
        supports_model_list: true,
      },
      // Anthropic
      Self {
        name: "anthropic".to_string(),
        models_endpoint: "https://api.anthropic.com/v1/models".to_string(),
        api_key_env: "ANTHROPIC_API_KEY".to_string(),
        auth_header: "x-api-key".to_string(),
        additional_headers: {
          let mut headers = HashMap::new();
          headers.insert("anthropic-version".to_string(), "2023-06-01".to_string());
          headers
        },
        supports_model_list: true,
      },
      // Google Gemini
      Self {
        name: "google".to_string(),
        models_endpoint: "https://generativelanguage.googleapis.com/v1beta/openai/models"
          .to_string(),
        api_key_env: "GEMINI_API_KEY".to_string(),
        auth_header: "Authorization".to_string(),
        additional_headers: HashMap::new(),
        supports_model_list: true,
      },
      // Step
      Self {
        name: "step".to_string(),
        models_endpoint: "https://api.stepfun.com/v1/models".to_string(),
        api_key_env: "STEP_API_KEY".to_string(),
        auth_header: "Authorization".to_string(),
        additional_headers: HashMap::new(),
        supports_model_list: true,
      },
      // OpenAI (placeholder - doesn't support model list endpoint)
      Self {
        name: "openai".to_string(),
        models_endpoint: "".to_string(),
        api_key_env: "OPENAI_API_KEY".to_string(),
        auth_header: "Authorization".to_string(),
        additional_headers: HashMap::new(),
        supports_model_list: false,
      },
    ]
  }

  /// Get vendor config by name
  pub fn get_by_name(name: &str) -> Option<Self> {
    Self::all_vendors()
      .into_iter()
      .find(|v| v.name == name.to_lowercase())
  }

  /// Get all vendors that support model list fetching
  pub fn vendors_with_model_list() -> Vec<Self> {
    Self::all_vendors()
      .into_iter()
      .filter(|v| v.supports_model_list)
      .collect()
  }
}

/// Create an HTTP client configured for API requests
pub fn create_http_client() -> Result<Client> {
  Client::builder()
    .timeout(Duration::from_secs(30))
    .user_agent("agentflow-llm/1.0")
    .build()
    .map_err(|e| LLMError::NetworkError {
      message: format!("Failed to create HTTP client: {}", e),
    })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_vendor_configs() {
    let vendors = VendorConfig::all_vendors();
    assert!(vendors.len() >= 4);

    let moonshot = VendorConfig::get_by_name("moonshot").unwrap();
    assert_eq!(moonshot.name, "moonshot");
    assert!(moonshot.supports_model_list);

    let openai = VendorConfig::get_by_name("openai").unwrap();
    assert_eq!(openai.name, "openai");
    assert!(!openai.supports_model_list);
  }

  #[test]
  fn test_vendors_with_model_list() {
    let vendors = VendorConfig::vendors_with_model_list();
    assert!(vendors.len() >= 4);
    assert!(!vendors.iter().any(|v| v.name == "openai"));
  }
}

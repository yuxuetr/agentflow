//! # AgentFlow LLM Integration Crate
//! 
//! Provides unified interface for multiple LLM providers with streaming support.
//! 
//! ## Quick Start
//! 
//! ### Text-only Models
//! ```rust
//! use agentflow_llm::AgentFlow;
//! 
//! #[tokio::main]
//! async fn main() -> Result<(), agentflow_llm::LLMError> {
//!   // Auto-initialize (tries project config → user config → built-in defaults)
//!   AgentFlow::init().await?;
//!   
//!   // Use any supported model
//!   let response = AgentFlow::model("gpt-4o")
//!     .prompt("Hello, world!")
//!     .temperature(0.7)
//!     .execute().await?;
//!   
//!   println!("Response: {}", response);
//!   Ok(())
//! }
//! ```
//! 
//! ### Multimodal Models (Text + Images)
//! ```rust
//! use agentflow_llm::{AgentFlow, MultimodalMessage};
//! 
//! #[tokio::main]
//! async fn main() -> Result<(), agentflow_llm::LLMError> {
//!   AgentFlow::init().await?;
//!   
//!   // Simple text + image
//!   let response = AgentFlow::model("step-1o-turbo-vision")
//!     .text_and_image("Describe this image", "https://example.com/image.jpg")
//!     .temperature(0.7)
//!     .execute().await?;
//!   
//!   // Or build complex multimodal messages
//!   let message = MultimodalMessage::user()
//!     .add_text("Analyze these images:")
//!     .add_image_url("https://example.com/img1.jpg")
//!     .add_image_url("https://example.com/img2.jpg")
//!     .build();
//!   
//!   let response = AgentFlow::model("step-1o-turbo-vision")
//!     .multimodal_prompt(message)
//!     .execute().await?;
//!   
//!   println!("Analysis: {}", response);
//!   Ok(())
//! }
//! ```
//! 
//! ## Configuration Management
//! 
//! AgentFlow uses a flexible configuration system with the following priority:
//! 
//! 1. **Project-specific**: `./models.yml` (highest priority)
//! 2. **User-specific**: `~/.agentflow/models.yml` (medium priority) 
//! 3. **Built-in defaults**: Bundled in crate (lowest priority)
//! 
//! ### Generate Configuration Files
//! 
//! ```rust
//! // Generate project-specific config
//! AgentFlow::generate_config("models.yml").await?;
//! 
//! // Generate user-specific config
//! AgentFlow::generate_user_config().await?;
//! ```
//! 
//! ### Configuration Options
//! 
//! - **For developers**: Use built-in defaults for immediate prototyping
//! - **For projects**: Generate `models.yml` for project-specific settings
//! - **For users**: Generate `~/.agentflow/models.yml` for global defaults
//! 
//! ## Supported Providers
//! 
//! - **OpenAI**: GPT-4o, GPT-4o-mini, GPT-4-turbo (text-only)
//! - **Anthropic**: Claude-3.5-Sonnet, Claude-3-Haiku (text-only)  
//! - **Google**: Gemini-1.5-Pro, Gemini-1.5-Flash (text-only)
//! - **Moonshot**: Various models (text-only)
//! - **StepFun**: step-1o-turbo-vision, step-2-16k (multimodal support)

pub mod config;
pub mod providers;
pub mod client;
pub mod registry;
pub mod error;
pub mod discovery;
pub mod multimodal;

// Re-export main API components
pub use client::{LLMClient, StreamingResponse, ResponseFormat};
pub use config::{ModelConfig, LLMConfig, VendorConfigManager, LoadingBenchmark, PerformanceComparison};
pub use error::{LLMError, Result};
pub use registry::ModelRegistry;
pub use discovery::{ModelFetcher, ModelValidator, ConfigUpdater};
pub use multimodal::{MultimodalMessage, MessageContent, ImageUrl, ImageData};

// External dependencies for configuration
use dirs;
use dotenvy;

#[cfg(feature = "logging")]
use tracing_subscriber;

// Fluent API entry point
use crate::client::LLMClientBuilder;

/// Main entry point for AgentFlow LLM integration
/// 
/// Example usage:
/// ```rust
/// // Non-streaming request
/// let response = AgentFlow::model("gpt-4o")
///     .prompt("Hello, world!")
///     .temperature(0.8)
///     .max_tokens(100)
///     .execute().await?;
///
/// // Streaming request
/// let stream = AgentFlow::model("claude-3-5-sonnet")
///     .prompt("Tell me a story")
///     .temperature(0.7)
///     .top_p(0.9)
///     .execute_streaming().await?;
///
/// // With tools (future MCP integration)
/// let response = AgentFlow::model("gpt-4o")
///     .prompt("Search for information about Rust")
///     .tools(mcp_tools) // Vec<Value> from agentflow-mcp
///     .execute().await?;
///
/// // JSON mode for structured output
/// let json_response = AgentFlow::model("gpt-4o")
///     .prompt("Return user data as JSON with name, age, email fields")
///     .json_mode()
///     .execute().await?;
///
/// // With logging enabled
/// let response = AgentFlow::model("claude-3-5-sonnet")
///     .prompt("Analyze this data")
///     .enable_logging(true)
///     .execute().await?;
/// ```
pub struct AgentFlow;

impl AgentFlow {
  /// Create a new LLM client builder for the specified model
  pub fn model(model_name: &str) -> LLMClientBuilder {
    LLMClientBuilder::new(model_name)
  }

  /// Initialize the LLM system with a configuration file
  pub async fn init_with_config(config_path: &str) -> Result<()> {
    let registry = ModelRegistry::global();
    registry.load_config(config_path).await?;
    Ok(())
  }

  /// Initialize the LLM system with default configuration
  /// 
  /// Configuration priority (first found wins):
  /// 1. ./models.yml (project-specific)
  /// 2. ~/.agentflow/models.yml (user-specific) 
  /// 3. Built-in defaults (bundled in crate)
  pub async fn init() -> Result<()> {
    // Try project-specific config first
    if std::path::Path::new("models.yml").exists() {
      return Self::init_with_config("models.yml").await;
    }
    
    // Try user-specific config
    if let Some(home_dir) = dirs::home_dir() {
      let user_config = home_dir.join(".agentflow").join("models.yml");
      if user_config.exists() {
        return Self::init_with_config(user_config.to_str().unwrap()).await;
      }
    }
    
    // Fall back to built-in defaults
    Self::init_with_builtin_config().await
  }
  
  /// Initialize with built-in default configuration
  pub async fn init_with_builtin_config() -> Result<()> {
    let registry = ModelRegistry::global();
    registry.load_builtin_config().await?;
    Ok(())
  }
  
  /// Generate a default configuration file at the specified path
  /// 
  /// Examples:
  /// - `AgentFlow::generate_config("models.yml")` - project config
  /// - `AgentFlow::generate_user_config()` - user config in ~/.agentflow/
  pub async fn generate_config<P: AsRef<std::path::Path>>(path: P) -> Result<()> {
    let config_content = include_str!("../templates/default_models.yml");
    
    // Create parent directory if it doesn't exist
    if let Some(parent) = path.as_ref().parent() {
      tokio::fs::create_dir_all(parent).await.map_err(|e| crate::LLMError::ConfigurationError {
        message: format!("Failed to create config directory: {}", e),
      })?;
    }
    
    tokio::fs::write(&path, config_content).await.map_err(|e| crate::LLMError::ConfigurationError {
      message: format!("Failed to write config file: {}", e),
    })?;
    
    println!("✅ Generated configuration file: {}", path.as_ref().display());
    Ok(())
  }
  
  /// Generate user-specific configuration in ~/.agentflow/models.yml
  pub async fn generate_user_config() -> Result<()> {
    let home_dir = dirs::home_dir().ok_or_else(|| crate::LLMError::ConfigurationError {
      message: "Could not determine home directory".to_string(),
    })?;
    
    let config_path = home_dir.join(".agentflow").join("models.yml");
    Self::generate_config(config_path).await
  }
  
  /// Generate environment file (.env) with API key templates
  /// 
  /// Creates a .env file with placeholder API keys and helpful comments.
  /// Also generates a .gitignore template to prevent accidental commits.
  pub async fn generate_env<P: AsRef<std::path::Path>>(path: P) -> Result<()> {
    let env_content = include_str!("../templates/default.env");
    let gitignore_content = include_str!("../templates/example.gitignore");
    
    // Create parent directory if it doesn't exist
    if let Some(parent) = path.as_ref().parent() {
      tokio::fs::create_dir_all(parent).await.map_err(|e| crate::LLMError::ConfigurationError {
        message: format!("Failed to create directory: {}", e),
      })?;
    }
    
    // Write .env file
    tokio::fs::write(&path, env_content).await.map_err(|e| crate::LLMError::ConfigurationError {
      message: format!("Failed to write .env file: {}", e),
    })?;
    
    // Write .gitignore template if it doesn't exist
    let gitignore_path = path.as_ref().parent().unwrap_or(std::path::Path::new(".")).join(".gitignore");
    if !gitignore_path.exists() {
      tokio::fs::write(&gitignore_path, gitignore_content).await.map_err(|e| crate::LLMError::ConfigurationError {
        message: format!("Failed to write .gitignore: {}", e),
      })?;
      println!("✅ Generated .gitignore file: {}", gitignore_path.display());
    }
    
    println!("✅ Generated environment file: {}", path.as_ref().display());
    println!("⚠️  SECURITY: Add your real API keys and ensure .env is in .gitignore!");
    Ok(())
  }
  
  /// Generate .env file in current directory
  pub async fn generate_project_env() -> Result<()> {
    Self::generate_env(".env").await
  }
  
  /// Generate user-specific .env in ~/.agentflow/.env
  pub async fn generate_user_env() -> Result<()> {
    let home_dir = dirs::home_dir().ok_or_else(|| crate::LLMError::ConfigurationError {
      message: "Could not determine home directory".to_string(),
    })?;
    
    let env_path = home_dir.join(".agentflow").join(".env");
    Self::generate_env(env_path).await
  }
  
  /// Initialize with automatic environment loading
  /// 
  /// Loads environment variables from:
  /// 1. System environment variables (highest priority)
  /// 2. ./.env (project-specific)
  /// 3. ~/.agentflow/.env (user-specific) 
  /// 4. Built-in defaults (if no API keys found)
  pub async fn init_with_env() -> Result<()> {
    // Try to load .env files in priority order
    if std::path::Path::new(".env").exists() {
      dotenvy::from_filename(".env").ok();
    } else if let Some(home_dir) = dirs::home_dir() {
      let user_env = home_dir.join(".agentflow").join(".env");
      if user_env.exists() {
        dotenvy::from_path(&user_env).ok();
      }
    }
    
    // Initialize with regular config discovery
    Self::init().await
  }
  
  /// Initialize logging for AgentFlow LLM
  /// 
  /// Sets up structured logging with appropriate levels:
  /// - ERROR: Critical failures
  /// - WARN: Invalid responses, API issues
  /// - INFO: Request/response summaries
  /// - DEBUG: Full request/response content
  #[cfg(feature = "logging")]
  pub fn init_logging() -> Result<()> {
    use tracing_subscriber::{fmt, EnvFilter};
    
    let filter = EnvFilter::try_from_default_env()
      .unwrap_or_else(|_| EnvFilter::new("agentflow_llm=info"));
    
    fmt()
      .with_env_filter(filter)
      .with_target(false)
      .with_thread_ids(false)
      .with_file(false)
      .with_line_number(false)
      .init();
    
    Ok(())
  }
  
  /// Initialize logging (no-op when logging feature is disabled)
  #[cfg(not(feature = "logging"))]
  pub fn init_logging() -> Result<()> {
    println!("[AgentFlow] Logging feature not enabled. Use --features logging to enable.");
    Ok(())
  }

  /// Fetch models from all supported vendors
  /// 
  /// Returns a HashMap where keys are vendor names and values are lists of discovered models.
  /// Only vendors that support model list fetching will be included.
  /// 
  /// Example:
  /// ```rust
  /// let models = AgentFlow::fetch_all_models().await?;
  /// for (vendor, model_list) in models {
  ///   println!("Vendor {}: {} models", vendor, model_list.len());
  /// }
  /// ```
  pub async fn fetch_all_models() -> std::result::Result<std::collections::HashMap<String, Vec<discovery::DiscoveredModel>>, LLMError> {
    let fetcher = discovery::ModelFetcher::new()?;
    Ok(fetcher.fetch_all_models().await)
  }

  /// Fetch models from a specific vendor
  /// 
  /// Example:
  /// ```rust
  /// let moonshot_models = AgentFlow::fetch_vendor_models("moonshot").await?;
  /// println!("Found {} MoonShot models", moonshot_models.len());
  /// ```
  pub async fn fetch_vendor_models(vendor: &str) -> Result<Vec<discovery::DiscoveredModel>> {
    let fetcher = discovery::ModelFetcher::new()?;
    fetcher.fetch_models_by_vendor_name(vendor).await
  }

  /// Validate all models in the current configuration
  /// 
  /// This checks if user-specified models actually exist in the vendor APIs.
  /// Returns a validation result with details about valid/invalid models.
  /// 
  /// Example:
  /// ```rust
  /// AgentFlow::init().await?;
  /// let result = AgentFlow::validate_models().await?;
  /// println!("Validation report:\n{}", result.create_report());
  /// ```
  pub async fn validate_models() -> Result<discovery::model_validator::ValidationResult> {
    let registry = ModelRegistry::global();
    let config = registry.get_config().await?;
    let validator = discovery::ModelValidator::new()?;
    Ok(validator.validate_config(&config).await)
  }

  /// Validate a specific model by name and vendor
  /// 
  /// Example:
  /// ```rust
  /// let is_valid = AgentFlow::validate_model("moonshot-v1-8k", "moonshot").await?;
  /// println!("Model is valid: {}", is_valid);
  /// ```
  pub async fn validate_model(model_name: &str, vendor: &str) -> Result<bool> {
    let validator = discovery::ModelValidator::new()?;
    validator.validate_model(model_name, vendor).await
  }

  /// Update the default models configuration with newly discovered models
  /// 
  /// This fetches models from all supported vendors and updates the specified
  /// configuration file. If the file doesn't exist, it will be created.
  /// 
  /// Example:
  /// ```rust
  /// let result = AgentFlow::update_models_config("templates/default_models.yml").await?;
  /// println!("Update report:\n{}", result.create_report());
  /// ```
  pub async fn update_models_config(config_path: &str) -> Result<discovery::config_updater::UpdateResult> {
    let updater = discovery::ConfigUpdater::new()?;
    updater.update_default_models(config_path).await
  }

  /// Check if a model exists for a vendor
  /// 
  /// Example:
  /// ```rust
  /// let exists = AgentFlow::model_exists("claude-3-5-sonnet-20241022", "anthropic").await?;
  /// println!("Claude 3.5 Sonnet exists: {}", exists);
  /// ```
  pub async fn model_exists(model_id: &str, vendor: &str) -> Result<bool> {
    let fetcher = discovery::ModelFetcher::new()?;
    fetcher.model_exists(vendor, model_id).await
  }

  /// Get information about a specific model if it exists
  /// 
  /// Example:
  /// ```rust
  /// if let Some(model_info) = AgentFlow::get_model_info("gpt-4o", "openai").await? {
  ///   println!("Model: {} owned by {}", model_info.id, model_info.owned_by.unwrap_or_default());
  /// }
  /// ```
  pub async fn get_model_info(model_id: &str, vendor: &str) -> Result<Option<discovery::DiscoveredModel>> {
    let fetcher = discovery::ModelFetcher::new()?;
    fetcher.get_model_info(vendor, model_id).await
  }

  /// Suggest similar models when a requested model is not found
  /// 
  /// Example:
  /// ```rust
  /// let suggestions = AgentFlow::suggest_similar_models("gpt-4-turbo", "openai").await?;
  /// println!("Did you mean one of these? {:?}", suggestions);
  /// ```
  pub async fn suggest_similar_models(target_model: &str, vendor: &str) -> Result<Vec<String>> {
    let validator = discovery::ModelValidator::new()?;
    validator.suggest_similar_models(target_model, vendor).await
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_agentflow_model_builder() {
    let builder = AgentFlow::model("gpt-4o");
    // Just test that the builder is created successfully
    // The actual functionality is tested in integration tests
    drop(builder); // Explicit drop to show the test passed
  }
}
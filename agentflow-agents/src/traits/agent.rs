//! Agent Application Trait
//!
//! Defines the common interface for all AI agent applications

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Common interface for AI agent applications
#[async_trait]
pub trait AgentApplication {
  /// Configuration type for this agent
  type Config: for<'de> Deserialize<'de> + Send + Sync;
  
  /// Result type produced by this agent
  type Result: Serialize + Send + Sync;
  
  /// Initialize the agent with configuration
  async fn initialize(config: Self::Config) -> crate::AgentResult<Self>
  where
    Self: Sized;
  
  /// Execute the agent on a single input
  async fn execute(&self, input: &str) -> crate::AgentResult<Self::Result>;
  
  /// Batch process multiple inputs (default implementation)
  async fn batch_process(&self, inputs: Vec<&str>) -> crate::AgentResult<Vec<Self::Result>> {
    let mut results = Vec::with_capacity(inputs.len());
    for input in inputs {
      let result = self.execute(input).await?;
      results.push(result);
    }
    Ok(results)
  }
  
  /// Get agent name/identifier
  fn name(&self) -> &'static str;
  
  /// Get agent version
  fn version(&self) -> &'static str {
    "0.1.0"
  }
}

/// File-based agent that processes files
#[async_trait]
pub trait FileAgent: AgentApplication {
  /// Process a single file
  async fn process_file<P: AsRef<Path> + Send + Sync>(
    &self, 
    file_path: P
  ) -> crate::AgentResult<Self::Result>;
  
  /// Process multiple files in a directory
  async fn process_directory<P: AsRef<Path> + Send + Sync>(
    &self, 
    directory: P
  ) -> crate::AgentResult<Vec<(std::path::PathBuf, Self::Result)>>;
  
  /// Get supported file extensions
  fn supported_extensions(&self) -> Vec<&'static str>;
}

/// Batch processing capabilities
#[async_trait]
pub trait BatchAgent: AgentApplication {
  /// Batch processing configuration
  type BatchConfig: for<'de> Deserialize<'de> + Send + Sync;
  
  /// Batch result summary
  type BatchResult: Serialize + Send + Sync;
  
  /// Process multiple inputs with batch configuration
  async fn batch_process_with_config(
    &self, 
    inputs: Vec<&str>,
    config: Self::BatchConfig
  ) -> crate::AgentResult<Self::BatchResult>;
}

/// Configuration trait for agents
pub trait AgentConfig: for<'de> Deserialize<'de> + Serialize + Send + Sync {
  /// Validate the configuration
  fn validate(&self) -> crate::AgentResult<()>;
  
  /// Load configuration from file
  fn load_from_file<P: AsRef<Path>>(path: P) -> crate::AgentResult<Self>
  where
    Self: Sized,
  {
    let content = std::fs::read_to_string(path)?;
    let config: Self = serde_yaml::from_str(&content)?;
    config.validate()?;
    Ok(config)
  }
}
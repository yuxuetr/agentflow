//! AgentFlow Nodes - Built-in Node Implementations
//!
//! This crate provides ready-to-use node implementations for AgentFlow workflows,
//! supporting both code-first and configuration-first approaches.

pub mod nodes;

// Factory traits for configuration support
pub mod factory_traits;

// Optional factory implementations for configuration-first workflows
#[cfg(feature = "factories")]
pub mod factories;

// Re-export core types for convenience
pub use agentflow_core::{AgentFlowError, AsyncNode, Result, SharedState};

// Node implementations - feature-gated
#[cfg(feature = "llm")]
pub use nodes::llm::LlmNode;

#[cfg(feature = "http")]
pub use nodes::http::HttpNode;

#[cfg(feature = "file")]
pub use nodes::file::FileNode;

#[cfg(feature = "template")]
pub use nodes::template::TemplateNode;

#[cfg(feature = "batch")]
pub use nodes::batch::BatchNode;

#[cfg(feature = "conditional")]
pub use nodes::conditional::ConditionalNode;

// Factory trait exports
pub use factory_traits::{NodeConfig, NodeFactory, NodeRegistry, ResolvedNodeConfig};

// Factory implementations for configuration support
#[cfg(feature = "factories")]
pub use factories::*;

// Node result type
pub type NodeResult<T> = std::result::Result<T, NodeError>;

/// Error types specific to node operations
#[derive(thiserror::Error, Debug)]
pub enum NodeError {
  #[error("Configuration error: {message}")]
  ConfigurationError { message: String },

  #[error("Execution error: {message}")]
  ExecutionError { message: String },

  #[error("Validation error: {message}")]
  ValidationError { message: String },

  #[error("Core workflow error: {0}")]
  CoreError(#[from] AgentFlowError),

  #[error("I/O error: {0}")]
  IoError(#[from] std::io::Error),

  #[cfg(feature = "http")]
  #[error("HTTP request error: {0}")]
  HttpError(#[from] reqwest::Error),
}

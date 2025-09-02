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

// Text-based AI model nodes
#[cfg(feature = "llm")]
pub use nodes::llm::LlmNode;

// Image AI model nodes - always available
pub use nodes::text_to_image::TextToImageNode;
pub use nodes::image_to_image::ImageToImageNode;
pub use nodes::image_edit::ImageEditNode;
pub use nodes::image_understand::ImageUnderstandNode;

// Audio AI model nodes - always available
pub use nodes::tts::TTSNode;
pub use nodes::asr::ASRNode;

// Utility nodes - feature-gated
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

  #[error("Serialization error: {0}")]
  SerializationError(#[from] serde_json::Error),
}

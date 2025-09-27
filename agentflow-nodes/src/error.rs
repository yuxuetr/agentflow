//! Error types for AgentFlow nodes

use agentflow_core::AgentFlowError;

/// Error types specific to node operations
#[derive(thiserror::Error, Debug)]
pub enum NodeError {
  #[error("Configuration error: {message}")]
  ConfigurationError { message: String },

  #[error("Execution error: {message}")]
  ExecutionError { message: String },

  #[error("Validation error: {message}")]
  ValidationError { message: String },

  #[error("HTTP request error: {message}")]
  HttpError { message: String },

  #[error("File operation error: {message}")]
  FileOperationError { message: String },

  #[error("Core workflow error: {0}")]
  CoreError(#[from] AgentFlowError),

  #[error("I/O error: {0}")]
  IoError(#[from] std::io::Error),

  #[error("Serialization error: {0}")]
  SerializationError(#[from] serde_json::Error),

  #[error("Base64 decode error: {0}")]
  Base64Error(#[from] base64::DecodeError),
}

// Convert NodeError to AgentFlowError for compatibility
impl From<NodeError> for AgentFlowError {
  fn from(err: NodeError) -> Self {
    match err {
      NodeError::ConfigurationError { message } => {
        AgentFlowError::NodeExecutionFailed { message: format!("Configuration error: {}", message) }
      }
      NodeError::ExecutionError { message } => {
        AgentFlowError::AsyncExecutionError { message }
      }
      NodeError::ValidationError { message } => {
        AgentFlowError::NodeExecutionFailed { message: format!("Validation error: {}", message) }
      }
      NodeError::CoreError(core_err) => core_err,
      NodeError::IoError(io_err) => {
        AgentFlowError::AsyncExecutionError {
          message: format!("I/O error: {}", io_err),
        }
      }
      NodeError::SerializationError(ser_err) => {
        AgentFlowError::SerializationError(ser_err.to_string())
      }
      NodeError::HttpError { message } => {
        AgentFlowError::AsyncExecutionError {
          message: format!("HTTP error: {}", message),
        }
      }
      NodeError::FileOperationError { message } => {
        AgentFlowError::AsyncExecutionError {
          message: format!("File operation error: {}", message),
        }
      }
      NodeError::Base64Error(b64_err) => {
        AgentFlowError::AsyncExecutionError {
          message: format!("Base64 decode error: {}", b64_err),
        }
      }
    }
  }
}

/// Node result type
pub type NodeResult<T> = std::result::Result<T, NodeError>;
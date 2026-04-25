use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToolError {
  #[error("Tool not found: {name}")]
  NotFound { name: String },

  #[error("Tool execution failed: {message}")]
  ExecutionFailed { message: String },

  #[error("Invalid parameters: {message}")]
  InvalidParams { message: String },

  #[error("Sandbox violation: {message}")]
  SandboxViolation { message: String },

  #[error("HTTP error: {message}")]
  HttpError { message: String },

  #[error("IO error: {0}")]
  IoError(#[from] std::io::Error),

  #[error("Serialization error: {0}")]
  SerdeError(#[from] serde_json::Error),
}

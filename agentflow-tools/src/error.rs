use thiserror::Error;

#[derive(Debug, Error)]
pub enum ToolError {
  #[error("Tool not found: {name}")]
  NotFound { name: String },

  #[error("Tool execution failed: {message}")]
  ExecutionFailed { message: String },

  #[error("Invalid parameters: {message}")]
  InvalidParams { message: String },

  /// Q2.9.3: the arguments the LLM produced for a tool call failed
  /// JSON-Schema validation against the tool's declared
  /// `parameters_schema`. Distinct from `InvalidParams` (which is
  /// for hand-rolled per-tool checks) so callers can route
  /// schema-violation cases back to the LLM as a self-correction
  /// signal.
  #[error("Schema violation for tool '{tool}': {message}")]
  SchemaViolation { tool: String, message: String },

  #[error("Tool policy denied: {message}")]
  PolicyDenied { message: String },

  #[error("Sandbox violation: {message}")]
  SandboxViolation { message: String },

  #[error("HTTP error: {message}")]
  HttpError { message: String },

  #[error("IO error: {0}")]
  IoError(#[from] std::io::Error),

  #[error("Serialization error: {0}")]
  SerdeError(#[from] serde_json::Error),
}

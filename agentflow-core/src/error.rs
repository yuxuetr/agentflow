use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum AgentFlowError {
    #[error("Node execution failed: {message}")]
    NodeExecutionFailed { message: String },

    #[error("Node input error: {message}")]
    NodeInputError { message: String },

    #[error("Node was skipped due to a condition.")]
    NodeSkipped,

    #[error("Node '{node_id}' was skipped because its dependency '{dependency_id}' was skipped.")]
    DependencyNotMet { node_id: String, dependency_id: String },

    #[error("Retry attempts exhausted after {attempts} attempts")]
    RetryExhausted { attempts: u32 },

    #[error("Flow definition error: {message}")]
    FlowDefinitionError { message: String },

    #[error("Flow execution failed: {message}")]
    FlowExecutionFailed { message: String },

    #[error("Circular flow detected")]
    CircularFlow,

    #[error("Persistence error: {message}")]
    PersistenceError { message: String },

    #[error("Unknown transition: {action}")]
    UnknownTransition { action: String },

    #[error("Shared state error: {message}")]
    SharedStateError { message: String },

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Generic error: {0}")]
    Generic(String),

    #[error("Timeout exceeded after {duration_ms}ms")]
    TimeoutExceeded { duration_ms: u64 },

    #[error("Circuit breaker open for node: {node_id}")]
    CircuitBreakerOpen { node_id: String },

    #[error("Rate limit exceeded: {limit} requests per {window_ms}ms")]
    RateLimitExceeded { limit: u32, window_ms: u64 },

    #[error("Load shed due to high system load")]
    LoadShed,

    #[error("Resource pool exhausted: {resource_type}")]
    ResourcePoolExhausted { resource_type: String },

    #[error("Task cancelled")]
    TaskCancelled,

    #[error("Async execution error: {message}")]
    AsyncExecutionError { message: String },

    #[error("Batch processing failed: {failed_items} of {total_items} items failed")]
    BatchProcessingFailed {
        failed_items: usize,
        total_items: usize,
    },

    #[error("Configuration error: {message}")]
    ConfigurationError { message: String },

    #[error("Monitoring error: {message}")]
    MonitoringError { message: String },
}

pub type Result<T> = std::result::Result<T, AgentFlowError>;

// Conversion from serde_json::Error to keep things clean in other parts of the code.
impl From<serde_json::Error> for AgentFlowError {
    fn from(err: serde_json::Error) -> Self {
        AgentFlowError::SerializationError(err.to_string())
    }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_agentflow_error_creation() {
    let error = AgentFlowError::NodeExecutionFailed {
      message: "Test error".to_string(),
    };
    assert_eq!(error.to_string(), "Node execution failed: Test error");
  }

  #[test]
  fn test_error_chaining() {
    let json_result: std::result::Result<serde_json::Value, serde_json::Error> =
      serde_json::from_str("{invalid");
    let inner_error = json_result.unwrap_err();
    let error = AgentFlowError::from(inner_error);
    assert!(error.to_string().contains("Serialization error"));
  }

  #[test]
  fn test_retry_exhausted_error() {
    let error = AgentFlowError::RetryExhausted { attempts: 3 };
    assert_eq!(
      error.to_string(),
      "Retry attempts exhausted after 3 attempts"
    );
  }
}
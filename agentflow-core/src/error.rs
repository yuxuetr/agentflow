use thiserror::Error;

#[derive(Error, Debug)]
pub enum AgentFlowError {
  #[error("Node execution failed: {message}")]
  NodeExecutionFailed { message: String },
  
  #[error("Retry attempts exhausted after {attempts} attempts")]
  RetryExhausted { attempts: u32 },
  
  #[error("Flow execution failed: {message}")]
  FlowExecutionFailed { message: String },
  
  #[error("Circular flow detected")]
  CircularFlow,
  
  #[error("Unknown transition: {action}")]
  UnknownTransition { action: String },
  
  #[error("Shared state error: {message}")]
  SharedStateError { message: String },
  
  #[error("Serialization error: {0}")]
  SerializationError(#[from] serde_json::Error),
  
  #[error("Generic error: {0}")]
  Generic(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, AgentFlowError>;

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
    // Create a simple JSON parsing error
    let json_result: std::result::Result<serde_json::Value, serde_json::Error> = serde_json::from_str("{invalid");
    let inner_error = json_result.unwrap_err();
    let error = AgentFlowError::SerializationError(inner_error);
    assert!(error.to_string().contains("Serialization error"));
  }

  #[test]
  fn test_retry_exhausted_error() {
    let error = AgentFlowError::RetryExhausted { attempts: 3 };
    assert_eq!(error.to_string(), "Retry attempts exhausted after 3 attempts");
  }
}
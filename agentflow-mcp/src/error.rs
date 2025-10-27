//! Error types for AgentFlow MCP integration
//!
//! This module provides comprehensive error handling for MCP operations,
//! including error context tracking, JSON-RPC error codes, and backtrace support.

use thiserror::Error;

/// Result type alias for MCP operations
pub type MCPResult<T> = Result<T, MCPError>;

/// Standard JSON-RPC 2.0 error codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonRpcErrorCode {
  /// Invalid JSON was received by the server (-32700)
  ParseError = -32700,
  /// The JSON sent is not a valid Request object (-32600)
  InvalidRequest = -32600,
  /// The method does not exist / is not available (-32601)
  MethodNotFound = -32601,
  /// Invalid method parameter(s) (-32602)
  InvalidParams = -32602,
  /// Internal JSON-RPC error (-32603)
  InternalError = -32603,
  /// MCP-specific: Tool not found (-32001)
  ToolNotFound = -32001,
  /// MCP-specific: Tool execution failed (-32002)
  ToolExecutionFailed = -32002,
  /// MCP-specific: Resource not found (-32003)
  ResourceNotFound = -32003,
  /// MCP-specific: Resource access denied (-32004)
  ResourceAccessDenied = -32004,
  /// MCP-specific: Prompt not found (-32005)
  PromptNotFound = -32005,
}

impl JsonRpcErrorCode {
  /// Get the numeric error code
  pub fn code(&self) -> i32 {
    *self as i32
  }

  /// Get a human-readable description of the error code
  pub fn description(&self) -> &'static str {
    match self {
      Self::ParseError => "Parse error",
      Self::InvalidRequest => "Invalid request",
      Self::MethodNotFound => "Method not found",
      Self::InvalidParams => "Invalid params",
      Self::InternalError => "Internal error",
      Self::ToolNotFound => "Tool not found",
      Self::ToolExecutionFailed => "Tool execution failed",
      Self::ResourceNotFound => "Resource not found",
      Self::ResourceAccessDenied => "Resource access denied",
      Self::PromptNotFound => "Prompt not found",
    }
  }
}

/// Main error type for MCP operations
#[derive(Error, Debug)]
pub enum MCPError {
  /// Transport-layer error (connection, I/O, etc.)
  #[error("Transport error: {message}")]
  Transport {
    message: String,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
  },

  /// Protocol-layer error (JSON-RPC, MCP protocol violations)
  #[error("Protocol error: {message} (code: {code})")]
  Protocol {
    message: String,
    code: i32,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
  },

  /// Tool-related errors
  #[error("Tool error: {message}")]
  ToolError {
    message: String,
    tool_name: Option<String>,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
  },

  /// Resource-related errors
  #[error("Resource error: {message}")]
  ResourceError {
    message: String,
    resource_uri: Option<String>,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
  },

  /// Prompt-related errors
  #[error("Prompt error: {message}")]
  PromptError {
    message: String,
    prompt_name: Option<String>,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
  },

  /// Connection errors (failed to connect, disconnected, etc.)
  #[error("Connection error: {message}")]
  Connection {
    message: String,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
  },

  /// Timeout errors
  #[error("Timeout error: {message}")]
  Timeout {
    message: String,
    timeout_ms: Option<u64>,
  },

  /// Validation errors (JSON Schema, input validation, etc.)
  #[error("Validation error: {message}")]
  Validation {
    message: String,
    field: Option<String>,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
  },

  /// Configuration errors
  #[error("Configuration error: {message}")]
  Configuration {
    message: String,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
  },

  /// Serialization/deserialization errors
  #[error("Serialization error: {0}")]
  Serialization(#[from] serde_json::Error),

  /// IO errors
  #[error("IO error: {0}")]
  Io(#[from] std::io::Error),

  /// Other/unknown errors
  #[error("{message}")]
  Other {
    message: String,
    #[source]
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
  },
}

impl MCPError {
  /// Create a transport error with message
  pub fn transport<S: Into<String>>(message: S) -> Self {
    Self::Transport {
      message: message.into(),
      source: None,
    }
  }

  /// Create a protocol error with JSON-RPC error code
  pub fn protocol<S: Into<String>>(message: S, code: JsonRpcErrorCode) -> Self {
    Self::Protocol {
      message: message.into(),
      code: code.code(),
      source: None,
    }
  }

  /// Create a tool error
  pub fn tool<S: Into<String>>(message: S, tool_name: Option<String>) -> Self {
    Self::ToolError {
      message: message.into(),
      tool_name,
      source: None,
    }
  }

  /// Create a resource error
  pub fn resource<S: Into<String>>(message: S, resource_uri: Option<String>) -> Self {
    Self::ResourceError {
      message: message.into(),
      resource_uri,
      source: None,
    }
  }

  /// Create a connection error
  pub fn connection<S: Into<String>>(message: S) -> Self {
    Self::Connection {
      message: message.into(),
      source: None,
    }
  }

  /// Create a timeout error
  pub fn timeout<S: Into<String>>(message: S, timeout_ms: Option<u64>) -> Self {
    Self::Timeout {
      message: message.into(),
      timeout_ms,
    }
  }

  /// Create a validation error
  pub fn validation<S: Into<String>>(message: S, field: Option<String>) -> Self {
    Self::Validation {
      message: message.into(),
      field,
      source: None,
    }
  }

  /// Create a configuration error
  pub fn configuration<S: Into<String>>(message: S) -> Self {
    Self::Configuration {
      message: message.into(),
      source: None,
    }
  }

  /// Add context to an error
  pub fn context<S: Into<String>>(self, context: S) -> Self {
    let ctx = context.into();
    match self {
      Self::Transport { message, source } => Self::Transport {
        message: format!("{}: {}", ctx, message),
        source,
      },
      Self::Protocol {
        message,
        code,
        source,
      } => Self::Protocol {
        message: format!("{}: {}", ctx, message),
        code,
        source,
      },
      Self::ToolError {
        message,
        tool_name,
        source,
      } => Self::ToolError {
        message: format!("{}: {}", ctx, message),
        tool_name,
        source,
      },
      Self::ResourceError {
        message,
        resource_uri,
        source,
      } => Self::ResourceError {
        message: format!("{}: {}", ctx, message),
        resource_uri,
        source,
      },
      Self::PromptError {
        message,
        prompt_name,
        source,
      } => Self::PromptError {
        message: format!("{}: {}", ctx, message),
        prompt_name,
        source,
      },
      Self::Connection { message, source } => Self::Connection {
        message: format!("{}: {}", ctx, message),
        source,
      },
      Self::Timeout {
        message,
        timeout_ms,
      } => Self::Timeout {
        message: format!("{}: {}", ctx, message),
        timeout_ms,
      },
      Self::Validation {
        message,
        field,
        source,
      } => Self::Validation {
        message: format!("{}: {}", ctx, message),
        field,
        source,
      },
      Self::Configuration { message, source } => Self::Configuration {
        message: format!("{}: {}", ctx, message),
        source,
      },
      Self::Serialization(e) => Self::Other {
        message: format!("{}: {}", ctx, e),
        source: Some(Box::new(e)),
      },
      Self::Io(e) => Self::Other {
        message: format!("{}: {}", ctx, e),
        source: Some(Box::new(e)),
      },
      Self::Other { message, source } => Self::Other {
        message: format!("{}: {}", ctx, message),
        source,
      },
    }
  }

  /// Get the JSON-RPC error code for this error (if applicable)
  pub fn json_rpc_code(&self) -> Option<i32> {
    match self {
      Self::Protocol { code, .. } => Some(*code),
      _ => None,
    }
  }

  /// Check if this is a transient error (retryable)
  pub fn is_transient(&self) -> bool {
    matches!(
      self,
      Self::Transport { .. } | Self::Connection { .. } | Self::Timeout { .. }
    )
  }

  /// Check if this is a fatal error (not retryable)
  pub fn is_fatal(&self) -> bool {
    !self.is_transient()
  }
}

// Implement From for common error types
impl From<String> for MCPError {
  fn from(message: String) -> Self {
    Self::Other {
      message,
      source: None,
    }
  }
}

impl From<&str> for MCPError {
  fn from(message: &str) -> Self {
    Self::Other {
      message: message.to_string(),
      source: None,
    }
  }
}

/// Extension trait for adding context to Results
pub trait ResultExt<T> {
  /// Add context to an error result
  fn context<S: Into<String>>(self, context: S) -> MCPResult<T>;

  /// Add context to an error result (lazy evaluation)
  fn with_context<S: Into<String>, F: FnOnce() -> S>(self, f: F) -> MCPResult<T>;
}

impl<T> ResultExt<T> for MCPResult<T> {
  fn context<S: Into<String>>(self, context: S) -> MCPResult<T> {
    self.map_err(|e| e.context(context))
  }

  fn with_context<S: Into<String>, F: FnOnce() -> S>(self, f: F) -> MCPResult<T> {
    self.map_err(|e| e.context(f()))
  }
}

/// Helper macro for creating protocol errors with error codes
#[macro_export]
macro_rules! protocol_error {
  ($code:expr, $($arg:tt)*) => {
    $crate::error::MCPError::protocol(format!($($arg)*), $code)
  };
}

/// Helper macro for creating transport errors
#[macro_export]
macro_rules! transport_error {
  ($($arg:tt)*) => {
    $crate::error::MCPError::transport(format!($($arg)*))
  };
}

/// Helper macro for creating tool errors
#[macro_export]
macro_rules! tool_error {
  ($tool:expr, $($arg:tt)*) => {
    $crate::error::MCPError::tool(format!($($arg)*), Some($tool.to_string()))
  };
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_error_code_values() {
    assert_eq!(JsonRpcErrorCode::ParseError.code(), -32700);
    assert_eq!(JsonRpcErrorCode::InvalidRequest.code(), -32600);
    assert_eq!(JsonRpcErrorCode::MethodNotFound.code(), -32601);
    assert_eq!(JsonRpcErrorCode::ToolNotFound.code(), -32001);
  }

  #[test]
  fn test_error_construction() {
    let err = MCPError::transport("test error");
    assert!(matches!(err, MCPError::Transport { .. }));
    assert_eq!(err.to_string(), "Transport error: test error");
  }

  #[test]
  fn test_error_context() {
    let err = MCPError::transport("connection failed");
    let err_with_context = err.context("while connecting to server");
    assert_eq!(
      err_with_context.to_string(),
      "Transport error: while connecting to server: connection failed"
    );
  }

  #[test]
  fn test_protocol_error_with_code() {
    let err = MCPError::protocol("method not found", JsonRpcErrorCode::MethodNotFound);
    assert_eq!(err.json_rpc_code(), Some(-32601));
  }

  #[test]
  fn test_transient_errors() {
    assert!(MCPError::timeout("timeout", Some(5000)).is_transient());
    assert!(MCPError::connection("disconnected").is_transient());
    assert!(!MCPError::validation("invalid input", None).is_transient());
  }

  #[test]
  fn test_result_ext_context() {
    let result: MCPResult<()> = Err(MCPError::transport("error"));
    let result_with_context = result.context("operation failed");

    assert!(result_with_context.is_err());
    assert!(result_with_context
      .unwrap_err()
      .to_string()
      .contains("operation failed"));
  }

  #[test]
  fn test_tool_error_with_name() {
    let err = MCPError::tool("execution failed", Some("my_tool".to_string()));
    assert!(err.to_string().contains("Tool error"));
  }

  #[test]
  fn test_resource_error_with_uri() {
    let err = MCPError::resource("not found", Some("file:///test.txt".to_string()));
    assert!(err.to_string().contains("Resource error"));
  }
}

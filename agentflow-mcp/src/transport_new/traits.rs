//! Transport trait abstraction for MCP communication
//!
//! This module defines the core Transport trait that all MCP transports
//! must implement, providing a uniform interface for stdio, HTTP, and
//! future transport mechanisms.

use crate::error::MCPResult;
use async_trait::async_trait;
use serde_json::Value;

/// Transport type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
  /// Standard I/O transport (local process)
  Stdio,
  /// HTTP transport (remote server)
  Http,
  /// HTTP with Server-Sent Events for bidirectional communication
  HttpWithSSE,
}

impl std::fmt::Display for TransportType {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Stdio => write!(f, "stdio"),
      Self::Http => write!(f, "http"),
      Self::HttpWithSSE => write!(f, "http+sse"),
    }
  }
}

/// Transport trait for MCP communication
///
/// All MCP transports (stdio, HTTP, etc.) must implement this trait to provide
/// a uniform interface for sending and receiving JSON-RPC messages.
///
/// # Example Implementation
///
/// ```ignore
/// use agentflow_mcp::transport::Transport;
/// use async_trait::async_trait;
///
/// struct MyTransport;
///
/// #[async_trait]
/// impl Transport for MyTransport {
///   async fn connect(&mut self) -> MCPResult<()> {
///     // Connection logic
///     Ok(())
///   }
///
///   async fn send_message(&mut self, request: Value) -> MCPResult<Value> {
///     // Send logic
///     Ok(serde_json::json!({}))
///   }
///
///   // ... other methods
/// }
/// ```
#[async_trait]
pub trait Transport: Send + Sync {
  /// Connect to the MCP server/endpoint
  ///
  /// This method establishes the underlying connection (e.g., spawns a process
  /// for stdio, opens HTTP connection, etc.).
  ///
  /// # Errors
  ///
  /// Returns an error if connection fails.
  async fn connect(&mut self) -> MCPResult<()>;

  /// Send a message and receive the response
  ///
  /// This is the primary method for request-response communication. It sends
  /// a JSON-RPC request and waits for the corresponding response.
  ///
  /// # Arguments
  ///
  /// * `request` - The JSON-RPC request to send
  ///
  /// # Returns
  ///
  /// The JSON-RPC response, which may contain either a result or an error.
  ///
  /// # Errors
  ///
  /// Returns an error if:
  /// - The transport is not connected
  /// - Writing to the transport fails
  /// - Reading from the transport fails
  /// - The response times out
  /// - The response is malformed
  async fn send_message(&mut self, request: Value) -> MCPResult<Value>;

  /// Send a notification (no response expected)
  ///
  /// Notifications are JSON-RPC messages without an `id` field. The server
  /// will not send a response.
  ///
  /// # Arguments
  ///
  /// * `notification` - The JSON-RPC notification to send
  ///
  /// # Errors
  ///
  /// Returns an error if:
  /// - The transport is not connected
  /// - Writing to the transport fails
  async fn send_notification(&mut self, notification: Value) -> MCPResult<()>;

  /// Receive a message (for server-initiated requests)
  ///
  /// This method is used to receive messages initiated by the server, such as
  /// progress notifications or server-initiated tool calls.
  ///
  /// # Returns
  ///
  /// - `Ok(Some(message))` if a message was received
  /// - `Ok(None)` if no message is available (timeout or non-blocking mode)
  /// - `Err(...)` if an error occurred
  ///
  /// # Note
  ///
  /// Not all transports support server-initiated messages. HTTP without SSE
  /// will always return `Ok(None)`.
  async fn receive_message(&mut self) -> MCPResult<Option<Value>>;

  /// Close the connection
  ///
  /// Gracefully closes the underlying connection. For stdio, this terminates
  /// the spawned process. For HTTP, this closes the connection.
  ///
  /// # Errors
  ///
  /// Returns an error if disconnection fails, though the transport should
  /// still be considered disconnected after this call.
  async fn disconnect(&mut self) -> MCPResult<()>;

  /// Check if the transport is currently connected
  ///
  /// # Returns
  ///
  /// `true` if connected and ready to send/receive, `false` otherwise.
  fn is_connected(&self) -> bool;

  /// Get the transport type
  ///
  /// # Returns
  ///
  /// The transport type identifier.
  fn transport_type(&self) -> TransportType;

  /// Check if this transport supports server-initiated messages
  ///
  /// # Returns
  ///
  /// `true` if the transport can receive server-initiated messages (e.g., stdio, HTTP+SSE).
  /// `false` for pure request-response transports (e.g., basic HTTP).
  fn supports_server_messages(&self) -> bool {
    matches!(
      self.transport_type(),
      TransportType::Stdio | TransportType::HttpWithSSE
    )
  }
}

/// Transport configuration trait
///
/// Optional trait for transports that support runtime configuration.
pub trait TransportConfig {
  /// Get the connection timeout in milliseconds
  fn timeout_ms(&self) -> Option<u64> {
    None
  }

  /// Set the connection timeout in milliseconds
  fn set_timeout_ms(&mut self, timeout: u64);

  /// Get the maximum message size in bytes
  fn max_message_size(&self) -> Option<usize> {
    None
  }

  /// Set the maximum message size in bytes
  fn set_max_message_size(&mut self, size: usize);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_transport_type_display() {
    assert_eq!(TransportType::Stdio.to_string(), "stdio");
    assert_eq!(TransportType::Http.to_string(), "http");
    assert_eq!(TransportType::HttpWithSSE.to_string(), "http+sse");
  }

  #[test]
  fn test_transport_type_equality() {
    assert_eq!(TransportType::Stdio, TransportType::Stdio);
    assert_ne!(TransportType::Stdio, TransportType::Http);
  }

  // Note: Actual transport implementations will have their own tests
}

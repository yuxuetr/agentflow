//! Client session management
//!
//! This module handles the MCP session lifecycle including initialization,
//! connection state tracking, and message correlation.

use crate::error::{JsonRpcErrorCode, MCPError, MCPResult, ResultExt};
use crate::protocol::types::{
  Implementation, InitializeParams, InitializeResult, JsonRpcRequest,
  JsonRpcResponse, RequestId,
};
#[cfg(test)]
use crate::protocol::types::ClientCapabilities;
use crate::transport_new::Transport;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use super::builder::ClientConfig;

/// MCP client with session management
///
/// This client handles the complete MCP session lifecycle including:
/// - Connection and initialization
/// - Request/response correlation
/// - Session state tracking
/// - Graceful disconnection
///
/// # Example
///
/// ```no_run
/// use agentflow_mcp::client::ClientBuilder;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut client = ClientBuilder::new()
///   .with_stdio(vec!["node".to_string(), "server.js".to_string()])
///   .build()
///   .await?;
///
/// client.connect().await?;
/// // Use client...
/// client.disconnect().await?;
/// # Ok(())
/// # }
/// ```
pub struct MCPClient {
  /// Transport for communication
  transport: Arc<Mutex<Box<dyn Transport>>>,
  /// Client configuration
  config: ClientConfig,
  /// Session ID
  session_id: String,
  /// Connection state
  connected: Arc<Mutex<bool>>,
  /// Server capabilities (after initialization)
  server_capabilities: Arc<Mutex<Option<Value>>>,
  /// Server info (after initialization)
  server_info: Arc<Mutex<Option<Implementation>>>,
  /// Request ID counter
  request_counter: Arc<AtomicU64>,
}

impl std::fmt::Debug for MCPClient {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("MCPClient")
      .field("session_id", &self.session_id)
      .field("connected", &"<Mutex>")
      .field("server_capabilities", &"<Mutex>")
      .field("server_info", &"<Mutex>")
      .finish_non_exhaustive()
  }
}

/// Session state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
  /// Not connected
  Disconnected,
  /// Connected but not initialized
  Connected,
  /// Fully initialized and ready
  Ready,
}

impl MCPClient {
  /// Create a new MCP client (internal constructor)
  pub(super) fn new(transport: Box<dyn Transport>, config: ClientConfig) -> Self {
    Self {
      transport: Arc::new(Mutex::new(transport)),
      config,
      session_id: Uuid::new_v4().to_string(),
      connected: Arc::new(Mutex::new(false)),
      server_capabilities: Arc::new(Mutex::new(None)),
      server_info: Arc::new(Mutex::new(None)),
      request_counter: Arc::new(AtomicU64::new(1)),
    }
  }

  /// Connect to the MCP server and initialize the session
  ///
  /// This performs the complete initialization handshake:
  /// 1. Connect transport
  /// 2. Send initialize request
  /// 3. Receive server capabilities
  /// 4. Send initialized notification
  ///
  /// # Errors
  ///
  /// Returns an error if:
  /// - Transport connection fails
  /// - Initialization handshake fails
  /// - Server rejects initialization
  ///
  /// # Example
  ///
  /// ```no_run
  /// # use agentflow_mcp::client::ClientBuilder;
  /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
  /// let mut client = ClientBuilder::new()
  ///   .with_stdio(vec!["node".to_string(), "server.js".to_string()])
  ///   .build()
  ///   .await?;
  ///
  /// client.connect().await?;
  /// # Ok(())
  /// # }
  /// ```
  pub async fn connect(&mut self) -> MCPResult<()> {
    // Check if already connected
    let is_connected = *self.connected.lock().await;
    if is_connected {
      return Ok(());
    }

    // Connect transport with timeout
    let timeout = self.config.timeout;
    let connect_result = tokio::time::timeout(
      timeout,
      self.transport.lock().await.connect()
    )
    .await;

    match connect_result {
      Ok(Ok(())) => {
        // Update connection state
        *self.connected.lock().await = true;
      }
      Ok(Err(e)) => return Err(e.context("Failed to connect transport")),
      Err(_) => {
        return Err(MCPError::timeout(
          format!("Connection timeout after {:?}", timeout),
          Some(timeout.as_millis() as u64),
        ))
      }
    }

    // Initialize session (already has retry + timeout via send_request)
    self.initialize().await.context("Failed to initialize MCP session")?;

    Ok(())
  }

  /// Initialize the MCP session
  async fn initialize(&mut self) -> MCPResult<()> {
    // Build initialize request
    let params = InitializeParams::new(
      self.config.capabilities.clone(),
      self.config.client_info.clone(),
    );

    let request = JsonRpcRequest::new(
      self.next_request_id(),
      "initialize",
      Some(serde_json::to_value(&params).map_err(|e| {
        MCPError::from(e).context("Failed to serialize initialize params")
      })?),
    );

    // Send request
    let response = self
      .send_request(request)
      .await
      .context("Failed to send initialize request")?;

    // Parse response
    let response: JsonRpcResponse = serde_json::from_value(response)
      .map_err(|e| MCPError::from(e).context("Failed to parse initialize response"))?;

    // Check for errors
    if let Some(error) = response.error {
      return Err(MCPError::protocol(
        format!("Initialization failed: {} - {}", error.code, error.message),
        JsonRpcErrorCode::InternalError,
      ));
    }

    // Parse result
    let result = response.result.ok_or_else(|| {
      MCPError::protocol(
        "Missing result in initialize response",
        JsonRpcErrorCode::InvalidRequest,
      )
    })?;

    let init_result: InitializeResult = serde_json::from_value(result)
      .map_err(|e| MCPError::from(e).context("Failed to parse initialize result"))?;

    // Store server info and capabilities
    *self.server_capabilities.lock().await = Some(serde_json::to_value(&init_result.capabilities)
      .map_err(|e| MCPError::from(e).context("Failed to serialize server capabilities"))?);
    *self.server_info.lock().await = Some(init_result.server_info);

    // Send initialized notification
    let notification = JsonRpcRequest::notification("notifications/initialized", None);
    self
      .send_notification(notification)
      .await
      .context("Failed to send initialized notification")?;

    Ok(())
  }

  /// Disconnect from the server
  ///
  /// This gracefully closes the connection and cleans up resources.
  ///
  /// # Example
  ///
  /// ```no_run
  /// # use agentflow_mcp::client::ClientBuilder;
  /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
  /// # let mut client = ClientBuilder::new()
  /// #   .with_stdio(vec!["node".to_string(), "server.js".to_string()])
  /// #   .build().await?;
  /// client.disconnect().await?;
  /// # Ok(())
  /// # }
  /// ```
  pub async fn disconnect(&mut self) -> MCPResult<()> {
    // Disconnect transport
    self
      .transport
      .lock()
      .await
      .disconnect()
      .await
      .context("Failed to disconnect transport")?;

    // Update state
    *self.connected.lock().await = false;
    *self.server_capabilities.lock().await = None;
    *self.server_info.lock().await = None;

    Ok(())
  }

  /// Check if client is connected
  ///
  /// # Example
  ///
  /// ```no_run
  /// # use agentflow_mcp::client::ClientBuilder;
  /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
  /// # let client = ClientBuilder::new()
  /// #   .with_stdio(vec!["node".to_string(), "server.js".to_string()])
  /// #   .build().await?;
  /// if client.is_connected().await {
  ///   println!("Client is connected");
  /// }
  /// # Ok(())
  /// # }
  /// ```
  pub async fn is_connected(&self) -> bool {
    *self.connected.lock().await
  }

  /// Get current session state
  pub async fn session_state(&self) -> SessionState {
    let is_connected = *self.connected.lock().await;
    let has_capabilities = self.server_capabilities.lock().await.is_some();

    if !is_connected {
      SessionState::Disconnected
    } else if has_capabilities {
      SessionState::Ready
    } else {
      SessionState::Connected
    }
  }

  /// Get server capabilities (if initialized)
  pub async fn server_capabilities(&self) -> Option<Value> {
    self.server_capabilities.lock().await.clone()
  }

  /// Get server info (if initialized)
  pub async fn server_info(&self) -> Option<Implementation> {
    self.server_info.lock().await.clone()
  }

  /// Get session ID
  pub fn session_id(&self) -> &str {
    &self.session_id
  }

  /// Send a JSON-RPC request and wait for response
  ///
  /// This method applies:
  /// - Retry logic with exponential backoff (for transient errors)
  /// - Timeout enforcement from client configuration
  pub(super) async fn send_request(&mut self, request: JsonRpcRequest) -> MCPResult<Value> {
    use crate::client::retry::{retry_with_backoff, RetryConfig};

    let request_value = serde_json::to_value(&request)
      .map_err(|e| MCPError::from(e).context("Failed to serialize request"))?;

    // Create retry config from client config
    let retry_config = RetryConfig::new(
      self.config.max_retries,
      self.config.retry_backoff_ms,
    );

    // Clone what we need for the retry closure
    let transport = self.transport.clone();
    let timeout = self.config.timeout;

    // Apply retry + timeout wrapper
    let response = retry_with_backoff(&retry_config, || {
      let transport = transport.clone();
      let request_value = request_value.clone();

      async move {
        // Apply timeout to the transport operation
        let result = tokio::time::timeout(
          timeout,
          async {
            transport
              .lock()
              .await
              .send_message(request_value)
              .await
          }
        )
        .await;

        match result {
          Ok(Ok(response)) => Ok(response),
          Ok(Err(e)) => Err(e.context("Failed to send message")),
          Err(_) => Err(MCPError::timeout(
            format!("Request timeout after {:?}", timeout),
            Some(timeout.as_millis() as u64),
          )),
        }
      }
    })
    .await?;

    Ok(response)
  }

  /// Send a JSON-RPC notification (no response expected)
  ///
  /// This method applies timeout enforcement from client configuration.
  /// Notifications typically don't get retried as they don't expect responses.
  pub(super) async fn send_notification(&mut self, notification: JsonRpcRequest) -> MCPResult<()> {
    let notification_value = serde_json::to_value(&notification)
      .map_err(|e| MCPError::from(e).context("Failed to serialize notification"))?;

    // Apply timeout to notification
    let timeout = self.config.timeout;
    let result = tokio::time::timeout(
      timeout,
      self
        .transport
        .lock()
        .await
        .send_notification(notification_value)
    )
    .await;

    match result {
      Ok(Ok(())) => Ok(()),
      Ok(Err(e)) => Err(e.context("Failed to send notification")),
      Err(_) => Err(MCPError::timeout(
        format!("Notification timeout after {:?}", timeout),
        Some(timeout.as_millis() as u64),
      )),
    }
  }

  /// Generate next request ID
  pub(super) fn next_request_id(&self) -> RequestId {
    let id = self.request_counter.fetch_add(1, Ordering::SeqCst);
    RequestId::Number(id as i64)
  }
}

impl Drop for MCPClient {
  fn drop(&mut self) {
    // Best-effort cleanup
    // Note: Can't use async in Drop, so transport cleanup happens in its own Drop
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::transport_new::StdioTransport;

  #[test]
  fn test_session_id_generated() {
    let transport = Box::new(StdioTransport::new(vec!["echo".to_string()]));
    let config = ClientConfig {
      capabilities: ClientCapabilities::default(),
      client_info: Implementation::agentflow(),
      timeout: std::time::Duration::from_secs(30),
      max_retries: 3,
      retry_backoff_ms: 100,
    };
    let client = MCPClient::new(transport, config);
    assert!(!client.session_id.is_empty());
  }

  #[tokio::test]
  async fn test_initial_state_disconnected() {
    let transport = Box::new(StdioTransport::new(vec!["echo".to_string()]));
    let config = ClientConfig {
      capabilities: ClientCapabilities::default(),
      client_info: Implementation::agentflow(),
      timeout: std::time::Duration::from_secs(30),
      max_retries: 3,
      retry_backoff_ms: 100,
    };
    let client = MCPClient::new(transport, config);
    assert_eq!(client.session_state().await, SessionState::Disconnected);
  }

  #[tokio::test]
  async fn test_request_id_increment() {
    let transport = Box::new(StdioTransport::new(vec!["echo".to_string()]));
    let config = ClientConfig {
      capabilities: ClientCapabilities::default(),
      client_info: Implementation::agentflow(),
      timeout: std::time::Duration::from_secs(30),
      max_retries: 3,
      retry_backoff_ms: 100,
    };
    let client = MCPClient::new(transport, config);

    let id1 = client.next_request_id();
    let id2 = client.next_request_id();

    match (id1, id2) {
      (RequestId::Number(n1), RequestId::Number(n2)) => {
        assert_eq!(n2, n1 + 1);
      }
      _ => panic!("Expected numeric request IDs"),
    }
  }
}

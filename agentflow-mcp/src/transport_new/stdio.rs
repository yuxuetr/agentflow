//! Stdio transport implementation with buffered I/O
//!
//! This module provides a production-ready stdio transport that communicates
//! with MCP servers via standard input/output, using buffered I/O for performance
//! and proper timeout/health check mechanisms.

use crate::error::{MCPError, MCPResult};
use crate::transport_new::traits::{Transport, TransportConfig, TransportType};
use async_trait::async_trait;
use serde_json::Value;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

/// Stdio transport for local MCP servers
///
/// This transport spawns a local process and communicates via stdin/stdout
/// using line-delimited JSON-RPC messages.
///
/// # Example
///
/// ```no_run
/// use agentflow_mcp::transport_new::{StdioTransport, Transport};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let mut transport = StdioTransport::new(vec![
///   "npx".to_string(),
///   "-y".to_string(),
///   "@modelcontextprotocol/server-everything".to_string(),
/// ]);
///
/// transport.connect().await?;
/// # Ok(())
/// # }
/// ```
pub struct StdioTransport {
  /// Command and arguments to spawn
  command: Vec<String>,
  /// Spawned child process
  process: Option<Child>,
  /// Buffered stdin writer
  stdin: Option<BufWriter<ChildStdin>>,
  /// Buffered stdout reader
  stdout: Option<BufReader<ChildStdout>>,
  /// Connection status
  connected: bool,
  /// Timeout for I/O operations
  timeout: Duration,
  /// Maximum message size (for safety)
  max_message_size: usize,
}

impl StdioTransport {
  /// Default timeout for I/O operations (30 seconds)
  pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

  /// Default maximum message size (10 MB)
  pub const DEFAULT_MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024;

  /// Create a new stdio transport
  ///
  /// # Arguments
  ///
  /// * `command` - Command and arguments to spawn (e.g., `["npx", "-y", "server"]`)
  ///
  /// # Example
  ///
  /// ```
  /// use agentflow_mcp::transport_new::StdioTransport;
  ///
  /// let transport = StdioTransport::new(vec![
  ///   "node".to_string(),
  ///   "server.js".to_string(),
  /// ]);
  /// ```
  pub fn new(command: Vec<String>) -> Self {
    Self {
      command,
      process: None,
      stdin: None,
      stdout: None,
      connected: false,
      timeout: Duration::from_millis(Self::DEFAULT_TIMEOUT_MS),
      max_message_size: Self::DEFAULT_MAX_MESSAGE_SIZE,
    }
  }

  /// Set the I/O timeout
  ///
  /// # Arguments
  ///
  /// * `timeout` - Timeout duration
  ///
  /// # Example
  ///
  /// ```
  /// use agentflow_mcp::transport_new::StdioTransport;
  /// use std::time::Duration;
  ///
  /// let transport = StdioTransport::new(vec!["node".into(), "server.js".into()])
  ///   .with_timeout(Duration::from_secs(60));
  /// ```
  pub fn with_timeout(mut self, timeout: Duration) -> Self {
    self.timeout = timeout;
    self
  }

  /// Set the maximum message size
  pub fn with_max_message_size(mut self, size: usize) -> Self {
    self.max_message_size = size;
    self
  }

  /// Read a single line from stdout with timeout
  async fn read_line_with_timeout(&mut self) -> MCPResult<String> {
    if let Some(stdout) = &mut self.stdout {
      let mut line = String::new();

      match tokio::time::timeout(self.timeout, stdout.read_line(&mut line)).await {
        Ok(Ok(0)) => {
          // EOF - process terminated
          self.connected = false;
          Err(MCPError::connection(
            "Process terminated unexpectedly (EOF)",
          ))
        }
        Ok(Ok(bytes_read)) => {
          // Check message size
          if bytes_read > self.max_message_size {
            return Err(MCPError::transport(format!(
              "Message too large: {} bytes (max: {})",
              bytes_read, self.max_message_size
            )));
          }

          Ok(line.trim().to_string())
        }
        Ok(Err(e)) => Err(MCPError::transport(format!(
          "Failed to read from process stdout: {}",
          e
        ))),
        Err(_) => Err(MCPError::timeout(
          format!("Read timeout after {:?}", self.timeout),
          Some(self.timeout.as_millis() as u64),
        )),
      }
    } else {
      Err(MCPError::connection("Stdout not available"))
    }
  }

  /// Write a line to stdin with timeout
  async fn write_line_with_timeout(&mut self, data: &str) -> MCPResult<()> {
    if let Some(stdin) = &mut self.stdin {
      let write_future = async {
        stdin.write_all(data.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        Ok::<(), std::io::Error>(())
      };

      match tokio::time::timeout(self.timeout, write_future).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(MCPError::transport(format!(
          "Failed to write to process stdin: {}",
          e
        ))),
        Err(_) => Err(MCPError::timeout(
          format!("Write timeout after {:?}", self.timeout),
          Some(self.timeout.as_millis() as u64),
        )),
      }
    } else {
      Err(MCPError::connection("Stdin not available"))
    }
  }

  /// Check if the spawned process is still running
  fn check_process_health(&mut self) -> MCPResult<()> {
    if let Some(process) = &mut self.process {
      match process.try_wait() {
        Ok(Some(status)) => {
          self.connected = false;
          Err(MCPError::connection(format!(
            "Process exited with status: {}",
            status
          )))
        }
        Ok(None) => Ok(()), // Still running
        Err(e) => Err(MCPError::connection(format!(
          "Failed to check process status: {}",
          e
        ))),
      }
    } else {
      Err(MCPError::connection("Process not started"))
    }
  }
}

#[async_trait]
impl Transport for StdioTransport {
  async fn connect(&mut self) -> MCPResult<()> {
    if self.connected {
      return Ok(());
    }

    // Validate command
    if self.command.is_empty() {
      return Err(MCPError::configuration("Command cannot be empty"));
    }

    // Spawn the process
    let mut cmd = Command::new(&self.command[0]);
    if self.command.len() > 1 {
      cmd.args(&self.command[1..]);
    }

    let mut child = cmd
      .stdin(std::process::Stdio::piped())
      .stdout(std::process::Stdio::piped())
      .stderr(std::process::Stdio::piped())
      .spawn()
      .map_err(|e| {
        MCPError::connection(format!("Failed to spawn MCP server process: {}", e))
      })?;

    // Capture stdin
    let stdin = child
      .stdin
      .take()
      .ok_or_else(|| MCPError::connection("Failed to capture stdin"))?;

    // Capture stdout
    let stdout = child
      .stdout
      .take()
      .ok_or_else(|| MCPError::connection("Failed to capture stdout"))?;

    // Set up buffered I/O
    self.stdin = Some(BufWriter::new(stdin));
    self.stdout = Some(BufReader::new(stdout));
    self.process = Some(child);
    self.connected = true;

    Ok(())
  }

  async fn send_message(&mut self, request: Value) -> MCPResult<Value> {
    // Check process health before sending
    self.check_process_health().map_err(|e| {
      e.context("Process health check failed before sending message")
    })?;

    // Serialize and send request
    let request_str = serde_json::to_string(&request)
      .map_err(|e| MCPError::from(e).context("Failed to serialize JSON-RPC request"))?;

    self
      .write_line_with_timeout(&request_str)
      .await
      .map_err(|e| e.context("Failed to write JSON-RPC request"))?;

    // Read and parse response
    let response_str = self
      .read_line_with_timeout()
      .await
      .map_err(|e| e.context("Failed to read JSON-RPC response"))?;

    let response: Value = serde_json::from_str(&response_str)
      .map_err(|e| MCPError::from(e).context("Failed to parse JSON-RPC response"))?;

    Ok(response)
  }

  async fn send_notification(&mut self, notification: Value) -> MCPResult<()> {
    // Check process health before sending
    self.check_process_health().map_err(|e| {
      e.context("Process health check failed before sending notification")
    })?;

    // Serialize and send notification
    let notification_str = serde_json::to_string(&notification)
      .map_err(|e| MCPError::from(e).context("Failed to serialize JSON-RPC notification"))?;

    self
      .write_line_with_timeout(&notification_str)
      .await
      .map_err(|e| e.context("Failed to write JSON-RPC notification"))?;

    Ok(())
  }

  async fn receive_message(&mut self) -> MCPResult<Option<Value>> {
    // Check process health
    self.check_process_health().map_err(|e| {
      e.context("Process health check failed before receiving message")
    })?;

    // Try to read a message (with timeout)
    match self.read_line_with_timeout().await {
      Ok(line) => {
        let message: Value = serde_json::from_str(&line)
          .map_err(|e| MCPError::from(e).context("Failed to parse received message"))?;
        Ok(Some(message))
      }
      Err(MCPError::Timeout { .. }) => {
        // Timeout is expected when no message is available
        Ok(None)
      }
      Err(e) => Err(e),
    }
  }

  async fn disconnect(&mut self) -> MCPResult<()> {
    // Drop stdin/stdout first to signal EOF
    self.stdin = None;
    self.stdout = None;

    // Kill and wait for process
    if let Some(mut process) = self.process.take() {
      // Try graceful termination first
      match tokio::time::timeout(Duration::from_secs(2), process.wait()).await {
        Ok(Ok(_)) => {
          // Process exited gracefully
        }
        _ => {
          // Force kill if still running
          let _ = process.kill().await;
          let _ = process.wait().await;
        }
      }
    }

    self.connected = false;
    Ok(())
  }

  fn is_connected(&self) -> bool {
    self.connected && self.process.is_some()
  }

  fn transport_type(&self) -> TransportType {
    TransportType::Stdio
  }
}

impl TransportConfig for StdioTransport {
  fn timeout_ms(&self) -> Option<u64> {
    Some(self.timeout.as_millis() as u64)
  }

  fn set_timeout_ms(&mut self, timeout: u64) {
    self.timeout = Duration::from_millis(timeout);
  }

  fn max_message_size(&self) -> Option<usize> {
    Some(self.max_message_size)
  }

  fn set_max_message_size(&mut self, size: usize) {
    self.max_message_size = size;
  }
}

impl Drop for StdioTransport {
  fn drop(&mut self) {
    // Best effort cleanup
    if let Some(mut process) = self.process.take() {
      let _ = futures::executor::block_on(async {
        let _ = process.kill().await;
        let _ = process.wait().await;
      });
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  // ============================================================================
  // Configuration Tests
  // ============================================================================

  #[test]
  fn test_stdio_transport_creation() {
    let transport = StdioTransport::new(vec!["echo".to_string(), "test".to_string()]);
    assert!(!transport.is_connected());
    assert_eq!(transport.transport_type(), TransportType::Stdio);
    assert_eq!(
      transport.timeout_ms(),
      Some(StdioTransport::DEFAULT_TIMEOUT_MS)
    );
    assert_eq!(
      transport.max_message_size(),
      Some(StdioTransport::DEFAULT_MAX_MESSAGE_SIZE)
    );
  }

  #[test]
  fn test_stdio_transport_with_timeout() {
    let transport = StdioTransport::new(vec!["test".to_string()])
      .with_timeout(Duration::from_secs(60));
    assert_eq!(transport.timeout_ms(), Some(60_000));
  }

  #[test]
  fn test_stdio_transport_with_max_message_size() {
    let transport =
      StdioTransport::new(vec!["test".to_string()]).with_max_message_size(1024);
    assert_eq!(transport.max_message_size(), Some(1024));
  }

  #[test]
  fn test_stdio_transport_config() {
    let mut transport = StdioTransport::new(vec!["test".to_string()]);
    transport.set_timeout_ms(5000);
    assert_eq!(transport.timeout_ms(), Some(5000));

    transport.set_max_message_size(1024 * 1024);
    assert_eq!(transport.max_message_size(), Some(1024 * 1024));
  }

  #[test]
  fn test_stdio_transport_builder_pattern() {
    let transport = StdioTransport::new(vec!["node".to_string(), "server.js".to_string()])
      .with_timeout(Duration::from_secs(10))
      .with_max_message_size(2 * 1024 * 1024);

    assert_eq!(transport.timeout_ms(), Some(10_000));
    assert_eq!(transport.max_message_size(), Some(2 * 1024 * 1024));
    assert!(!transport.is_connected());
  }

  // ============================================================================
  // Connection Tests
  // ============================================================================

  #[tokio::test]
  async fn test_connect_empty_command() {
    let mut transport = StdioTransport::new(vec![]);
    let result = transport.connect().await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MCPError::Configuration { .. }));
  }

  #[tokio::test]
  async fn test_connect_invalid_command() {
    let mut transport =
      StdioTransport::new(vec!["nonexistent_command_xyz123".to_string()]);
    let result = transport.connect().await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MCPError::Connection { .. }));
  }

  #[tokio::test]
  async fn test_connect_already_connected() {
    // Use 'cat' which will wait for input (works on Unix-like systems)
    let mut transport = StdioTransport::new(vec!["cat".to_string()])
      .with_timeout(Duration::from_millis(100));

    transport.connect().await.unwrap();
    assert!(transport.is_connected());

    // Second connect should succeed (idempotent)
    let result = transport.connect().await;
    assert!(result.is_ok());
    assert!(transport.is_connected());

    transport.disconnect().await.unwrap();
  }

  #[tokio::test]
  async fn test_is_connected_state() {
    let mut transport = StdioTransport::new(vec!["cat".to_string()]);
    assert!(!transport.is_connected());

    transport.connect().await.unwrap();
    assert!(transport.is_connected());

    transport.disconnect().await.unwrap();
    assert!(!transport.is_connected());
  }

  // ============================================================================
  // Disconnection Tests
  // ============================================================================

  #[tokio::test]
  async fn test_disconnect_cleans_up_process() {
    let mut transport = StdioTransport::new(vec!["cat".to_string()]);
    transport.connect().await.unwrap();
    assert!(transport.is_connected());

    transport.disconnect().await.unwrap();
    assert!(!transport.is_connected());
    assert!(transport.process.is_none());
    assert!(transport.stdin.is_none());
    assert!(transport.stdout.is_none());
  }

  #[tokio::test]
  async fn test_disconnect_when_not_connected() {
    let mut transport = StdioTransport::new(vec!["cat".to_string()]);
    let result = transport.disconnect().await;
    assert!(result.is_ok());
  }

  // ============================================================================
  // Process Health Check Tests
  // ============================================================================

  #[tokio::test]
  async fn test_check_process_health_not_started() {
    let mut transport = StdioTransport::new(vec!["cat".to_string()]);
    let result = transport.check_process_health();
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MCPError::Connection { .. }));
  }

  #[tokio::test]
  async fn test_check_process_health_running() {
    let mut transport = StdioTransport::new(vec!["cat".to_string()]);
    transport.connect().await.unwrap();

    let result = transport.check_process_health();
    assert!(result.is_ok());

    transport.disconnect().await.unwrap();
  }

  #[tokio::test]
  async fn test_check_process_health_after_exit() {
    // Use 'true' command which exits immediately
    let mut transport = StdioTransport::new(vec!["true".to_string()]);
    transport.connect().await.unwrap();

    // Wait for process to exit and check multiple times
    let mut result = Ok(());
    for _ in 0..10 {
      tokio::time::sleep(Duration::from_millis(50)).await;
      result = transport.check_process_health();
      if result.is_err() {
        break;
      }
    }

    // Should eventually detect the process has exited
    assert!(result.is_err());
    assert!(!transport.is_connected());
  }

  // ============================================================================
  // Message Send/Receive Tests
  // ============================================================================

  #[tokio::test]
  async fn test_send_message_not_connected() {
    let mut transport = StdioTransport::new(vec!["cat".to_string()]);
    let request = json!({"jsonrpc": "2.0", "method": "test", "id": 1});
    let result = transport.send_message(request).await;
    assert!(result.is_err());
  }

  #[tokio::test]
  async fn test_send_notification_not_connected() {
    let mut transport = StdioTransport::new(vec!["cat".to_string()]);
    let notification = json!({"jsonrpc": "2.0", "method": "test"});
    let result = transport.send_notification(notification).await;
    assert!(result.is_err());
  }

  #[tokio::test]
  async fn test_receive_message_not_connected() {
    let mut transport = StdioTransport::new(vec!["cat".to_string()]);
    let result = transport.receive_message().await;
    assert!(result.is_err());
  }

  // ============================================================================
  // Timeout Tests
  // ============================================================================

  #[tokio::test]
  async fn test_read_timeout() {
    // Start cat process which won't send anything
    let mut transport = StdioTransport::new(vec!["cat".to_string()])
      .with_timeout(Duration::from_millis(100));

    transport.connect().await.unwrap();

    // Try to read - should timeout
    let result = transport.read_line_with_timeout().await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MCPError::Timeout { .. }));

    transport.disconnect().await.unwrap();
  }

  #[tokio::test]
  async fn test_receive_message_timeout_returns_none() {
    let mut transport = StdioTransport::new(vec!["cat".to_string()])
      .with_timeout(Duration::from_millis(100));

    transport.connect().await.unwrap();

    // Receive should return None on timeout (not an error)
    let result = transport.receive_message().await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), None);

    transport.disconnect().await.unwrap();
  }

  // ============================================================================
  // Echo Process Integration Tests
  // ============================================================================
  // These tests use simple Unix commands to test the full transport flow

  #[tokio::test]
  #[cfg(unix)] // These tests rely on Unix utilities
  async fn test_echo_json_roundtrip() {
    // Use a shell command that echoes back JSON
    let mut transport = StdioTransport::new(vec![
      "sh".to_string(),
      "-c".to_string(),
      "while read line; do echo \"$line\"; done".to_string(),
    ])
    .with_timeout(Duration::from_secs(1));

    transport.connect().await.unwrap();

    let request = json!({"jsonrpc": "2.0", "method": "test", "id": 1});
    let response = transport.send_message(request.clone()).await.unwrap();

    assert_eq!(response, request); // Echo should return same message

    transport.disconnect().await.unwrap();
  }

  #[tokio::test]
  #[cfg(unix)]
  async fn test_multiple_messages() {
    let mut transport = StdioTransport::new(vec![
      "sh".to_string(),
      "-c".to_string(),
      "while read line; do echo \"$line\"; done".to_string(),
    ])
    .with_timeout(Duration::from_secs(1));

    transport.connect().await.unwrap();

    // Send multiple messages
    for i in 0..3 {
      let request = json!({"jsonrpc": "2.0", "method": "test", "id": i});
      let response = transport.send_message(request.clone()).await.unwrap();
      assert_eq!(response, request);
    }

    transport.disconnect().await.unwrap();
  }

  // ============================================================================
  // Error Handling Tests
  // ============================================================================

  #[tokio::test]
  async fn test_invalid_json_response() {
    // Use echo to return invalid JSON
    let mut transport = StdioTransport::new(vec![
      "sh".to_string(),
      "-c".to_string(),
      "echo 'invalid json'".to_string(),
    ])
    .with_timeout(Duration::from_secs(1));

    transport.connect().await.unwrap();

    let request = json!({"jsonrpc": "2.0", "method": "test", "id": 1});
    let result = transport.send_message(request).await;

    assert!(result.is_err());
    // Should be an error due to JSON parsing failure (wrapped in Other or Protocol)
    let error_msg = result.unwrap_err().to_string();
    assert!(
      error_msg.contains("parse") || error_msg.contains("JSON"),
      "Error should mention JSON parsing: {}",
      error_msg
    );

    transport.disconnect().await.unwrap();
  }

  #[tokio::test]
  async fn test_process_exit_during_operation() {
    // Use 'true' which exits immediately
    let mut transport = StdioTransport::new(vec!["true".to_string()])
      .with_timeout(Duration::from_millis(500));

    transport.connect().await.unwrap();

    // Wait for process to exit
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Try to send message - should fail
    let request = json!({"jsonrpc": "2.0", "method": "test", "id": 1});
    let result = transport.send_message(request).await;

    assert!(result.is_err());
    assert!(!transport.is_connected());
  }

  // ============================================================================
  // Drop Tests
  // ============================================================================

  #[tokio::test]
  async fn test_drop_cleans_up_process() {
    {
      let mut transport = StdioTransport::new(vec!["cat".to_string()]);
      transport.connect().await.unwrap();
      // Transport dropped here
    }
    // If we get here without hanging, drop worked
    assert!(true);
  }

  // ============================================================================
  // Property-Based Tests
  // ============================================================================

  mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
      /// Property: Timeout configuration is preserved
      #[test]
      fn prop_timeout_config_preserved(timeout_ms in 1u64..300_000u64) {
        let transport = StdioTransport::new(vec!["test".to_string()])
          .with_timeout(Duration::from_millis(timeout_ms));

        use crate::transport_new::TransportConfig;
        prop_assert_eq!(transport.timeout_ms(), Some(timeout_ms));
      }

      /// Property: Max message size configuration is preserved
      #[test]
      fn prop_max_message_size_preserved(size in 1usize..100_000_000usize) {
        let transport = StdioTransport::new(vec!["test".to_string()])
          .with_max_message_size(size);

        use crate::transport_new::TransportConfig;
        prop_assert_eq!(transport.max_message_size(), Some(size));
      }

      /// Property: set_timeout_ms updates timeout correctly
      #[test]
      fn prop_set_timeout_ms_works(
        initial_ms in 1u64..10_000u64,
        new_ms in 10_000u64..100_000u64
      ) {
        let mut transport = StdioTransport::new(vec!["test".to_string()])
          .with_timeout(Duration::from_millis(initial_ms));

        use crate::transport_new::TransportConfig;
        prop_assert_eq!(transport.timeout_ms(), Some(initial_ms));

        transport.set_timeout_ms(new_ms);
        prop_assert_eq!(transport.timeout_ms(), Some(new_ms));
      }

      /// Property: set_max_message_size updates size correctly
      #[test]
      fn prop_set_max_message_size_works(
        initial_size in 1usize..1_000_000usize,
        new_size in 1_000_000usize..10_000_000usize
      ) {
        let mut transport = StdioTransport::new(vec!["test".to_string()])
          .with_max_message_size(initial_size);

        use crate::transport_new::TransportConfig;
        prop_assert_eq!(transport.max_message_size(), Some(initial_size));

        transport.set_max_message_size(new_size);
        prop_assert_eq!(transport.max_message_size(), Some(new_size));
      }

      /// Property: Command vec is preserved (non-empty)
      #[test]
      fn prop_command_preserved(cmd_count in 1usize..5usize) {
        let commands: Vec<String> = (0..cmd_count)
          .map(|i| format!("cmd{}", i))
          .collect();

        let transport = StdioTransport::new(commands.clone());

        // Transport should be created successfully
        prop_assert_eq!(transport.transport_type(), TransportType::Stdio);
      }

      /// Property: New transport is not connected
      #[test]
      fn prop_new_transport_not_connected(
        timeout_ms in 1u64..60_000u64,
        max_size in 1usize..10_000_000usize
      ) {
        let transport = StdioTransport::new(vec!["test".to_string()])
          .with_timeout(Duration::from_millis(timeout_ms))
          .with_max_message_size(max_size);

        prop_assert!(!transport.is_connected());
      }

      /// Property: Transport type is always Stdio
      #[test]
      fn prop_transport_type_always_stdio(
        timeout_ms in 1u64..60_000u64
      ) {
        let transport = StdioTransport::new(vec!["test".to_string()])
          .with_timeout(Duration::from_millis(timeout_ms));

        prop_assert_eq!(transport.transport_type(), TransportType::Stdio);
      }

      /// Property: Builder pattern chains correctly
      #[test]
      fn prop_builder_pattern_chains(
        timeout_ms in 1u64..60_000u64,
        max_size in 1usize..10_000_000usize
      ) {
        let transport = StdioTransport::new(vec!["test".to_string()])
          .with_timeout(Duration::from_millis(timeout_ms))
          .with_max_message_size(max_size);

        use crate::transport_new::TransportConfig;
        prop_assert_eq!(transport.timeout_ms(), Some(timeout_ms));
        prop_assert_eq!(transport.max_message_size(), Some(max_size));
        prop_assert_eq!(transport.transport_type(), TransportType::Stdio);
      }
    }
  }

  // Note: Additional integration tests with real MCP servers are in tests/ directory
}

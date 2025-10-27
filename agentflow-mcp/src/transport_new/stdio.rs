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

  #[test]
  fn test_stdio_transport_creation() {
    let transport = StdioTransport::new(vec!["echo".to_string(), "test".to_string()]);
    assert!(!transport.is_connected());
    assert_eq!(transport.transport_type(), TransportType::Stdio);
  }

  #[test]
  fn test_stdio_transport_with_timeout() {
    let transport = StdioTransport::new(vec!["test".to_string()])
      .with_timeout(Duration::from_secs(60));
    assert_eq!(transport.timeout_ms(), Some(60_000));
  }

  #[test]
  fn test_stdio_transport_config() {
    let mut transport = StdioTransport::new(vec!["test".to_string()]);
    transport.set_timeout_ms(5000);
    assert_eq!(transport.timeout_ms(), Some(5000));

    transport.set_max_message_size(1024 * 1024);
    assert_eq!(transport.max_message_size(), Some(1024 * 1024));
  }

  // Note: Integration tests with actual process spawning are in tests/ directory
}

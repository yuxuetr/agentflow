//! Stdio transport implementation with buffered I/O
//!
//! This module provides a production-ready stdio transport that communicates
//! with MCP servers via standard input/output, using buffered I/O for performance
//! and proper timeout/health check mechanisms.

use crate::error::{MCPError, MCPResult};
use crate::transport::traits::{Transport, TransportConfig, TransportType};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::task::JoinHandle;

/// Q3.2.1: env vars that are safe to inherit when sandboxing the spawned
/// MCP server. PATH lets it find `node`/`python`/etc.; locale + HOME +
/// USER + SHELL are needed by many CLI tools to behave correctly. Any
/// LC_* var is additionally forwarded by the spawn code. Anything not
/// on this list — API keys, AWS creds, SSH agent sockets, OPENAI_*,
/// ANTHROPIC_*, etc. — is dropped on the floor unless the caller passes
/// it explicitly via `with_env(...)`.
const SAFE_INHERITED_ENV_VARS: &[&str] = &[
  "PATH",
  "HOME",
  "USER",
  "LOGNAME",
  "SHELL",
  "LANG",
  "TZ",
  "TERM",
  "TMPDIR",
  "PWD",
  // Windows equivalents (harmless on unix, no value, just skipped).
  "USERNAME",
  "USERPROFILE",
  "APPDATA",
  "LOCALAPPDATA",
  "SYSTEMROOT",
];

/// Stdio transport for local MCP servers
///
/// This transport spawns a local process and communicates via stdin/stdout
/// using line-delimited JSON-RPC messages.
///
/// # Example
///
/// ```no_run
/// use agentflow_mcp::transport::{StdioTransport, Transport};
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
  /// Environment variables to set on the spawned process
  env: HashMap<String, String>,
  /// Q3.2.1: when `false` (the default), the spawned MCP server inherits
  /// only a hardened, opt-in safe-list of env vars from the parent
  /// process (PATH, locale, HOME, USER, etc.) plus whatever `env`
  /// declares explicitly. The parent's `OPENAI_API_KEY`,
  /// `ANTHROPIC_API_KEY`, AWS creds, SSH keys, etc. **do not** leak
  /// to a third-party MCP server. Operators who genuinely need full
  /// parent inheritance can flip this with `with_inherit_parent_env(true)`.
  inherit_parent_env: bool,
  /// Spawned child process
  process: Option<Child>,
  /// Buffered stdin writer
  stdin: Option<BufWriter<ChildStdin>>,
  /// Buffered stdout reader
  stdout: Option<BufReader<ChildStdout>>,
  /// Q2.6.1: ring buffer of `Value`s that arrived between calls and
  /// did not match a pending request id (typically notifications).
  /// `send_message` drains them via `receive_message` so a
  /// notification arriving before the matching response no longer
  /// permanently desyncs the JSON-RPC session.
  pending_inbox: VecDeque<Value>,
  /// Q2.6.2: handle to the stderr drain task. Set on `connect`,
  /// dropped on `disconnect`. Forwards every stderr line to
  /// `tracing::warn!` so the server's stderr can't fill Linux's
  /// 64 KiB pipe buffer and deadlock the child.
  stderr_drain: Option<JoinHandle<()>>,
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
  /// use agentflow_mcp::transport::StdioTransport;
  ///
  /// let transport = StdioTransport::new(vec![
  ///   "node".to_string(),
  ///   "server.js".to_string(),
  /// ]);
  /// ```
  pub fn new(command: Vec<String>) -> Self {
    Self {
      command,
      env: HashMap::new(),
      inherit_parent_env: false,
      process: None,
      stdin: None,
      stdout: None,
      pending_inbox: VecDeque::new(),
      stderr_drain: None,
      connected: false,
      timeout: Duration::from_millis(Self::DEFAULT_TIMEOUT_MS),
      max_message_size: Self::DEFAULT_MAX_MESSAGE_SIZE,
    }
  }

  /// Set environment variables for the spawned MCP server process.
  pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
    self.env = env;
    self
  }

  /// Q3.2.1: opt back into full parent-environment inheritance for the
  /// spawned MCP server. Default is `false` (sandboxed). Only set this
  /// to `true` for trusted MCP servers where leaking the parent's
  /// secrets (LLM keys, AWS creds, SSH agent socket) is acceptable.
  pub fn with_inherit_parent_env(mut self, inherit: bool) -> Self {
    self.inherit_parent_env = inherit;
    self
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
  /// use agentflow_mcp::transport::StdioTransport;
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

    // Q3.2.1: sandboxed env by default. `Command::new` inherits the
    // parent's full environment, which leaks every API key / secret
    // the host process happens to hold to whatever third-party MCP
    // binary we just launched. Wipe parent env, then re-grant a
    // hardened safe-list of vars the child genuinely needs to run
    // (PATH so it can locate `node` / `python`; locale so its output
    // isn't garbled; HOME / USER so it can find its config). Anything
    // sensitive must be opted-in via `with_env(...)` per call.
    if !self.inherit_parent_env {
      cmd.env_clear();
      for safe_var in SAFE_INHERITED_ENV_VARS {
        if let Ok(value) = std::env::var(safe_var) {
          cmd.env(safe_var, value);
        }
      }
      // Locale vars are prefix-matched (LC_*), preserved by iterating.
      for (key, value) in std::env::vars() {
        if key.starts_with("LC_") {
          cmd.env(key, value);
        }
      }
    }
    if !self.env.is_empty() {
      cmd.envs(&self.env);
    }

    let mut child = cmd
      .stdin(std::process::Stdio::piped())
      .stdout(std::process::Stdio::piped())
      .stderr(std::process::Stdio::piped())
      // Ensures the child is SIGKILLed if the `Child` is dropped without
      // an explicit `disconnect()` first. Without this, the synchronous
      // `Drop` below cannot await `process.kill()` (which needs the tokio
      // reactor) without deadlocking — see the `Drop` impl note.
      .kill_on_drop(true)
      .spawn()
      .map_err(|e| MCPError::connection(format!("Failed to spawn MCP server process: {}", e)))?;

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

    // Q2.6.2: take stderr and spawn a drain task so the server can
    // log to stderr without ever blocking on a full pipe. Forward
    // each line to `tracing::warn!` so operators see the server's
    // diagnostic output through the same observability stack.
    if let Some(stderr) = child.stderr.take() {
      self.stderr_drain = Some(spawn_stderr_drain(stderr));
    }

    // Set up buffered I/O
    self.stdin = Some(BufWriter::new(stdin));
    self.stdout = Some(BufReader::new(stdout));
    self.process = Some(child);
    self.connected = true;

    Ok(())
  }

  async fn send_message(&mut self, request: Value) -> MCPResult<Value> {
    // Check process health before sending
    self
      .check_process_health()
      .map_err(|e| e.context("Process health check failed before sending message"))?;

    // Q2.6.1: snapshot the request id so we can correlate the
    // response on the wire. Pre-fix `send_message` blindly read the
    // next line and assumed it was the response — a notification
    // sent by the server before the response permanently shifted
    // every subsequent request by one read.
    let expected_id = request.get("id").cloned();

    // Serialize and send request
    let request_str = serde_json::to_string(&request)
      .map_err(|e| MCPError::from(e).context("Failed to serialize JSON-RPC request"))?;

    self
      .write_line_with_timeout(&request_str)
      .await
      .map_err(|e| e.context("Failed to write JSON-RPC request"))?;

    // Loop reading lines until we see a message whose `id` matches
    // the request. Anything else is a notification or out-of-band
    // payload — buffer it on `pending_inbox` so a subsequent
    // `receive_message` can deliver it to the caller rather than
    // dropping it on the floor.
    loop {
      let response_str = self
        .read_line_with_timeout()
        .await
        .map_err(|e| e.context("Failed to read JSON-RPC response"))?;

      let response: Value = serde_json::from_str(&response_str)
        .map_err(|e| MCPError::from(e).context("Failed to parse JSON-RPC response"))?;

      let response_id = response.get("id");
      if response_id == expected_id.as_ref() {
        return Ok(response);
      }

      // Out-of-band (notification or stale response). Queue it for
      // `receive_message` consumers — losing the message would
      // surprise a caller that legitimately subscribed to server
      // notifications via the receive surface.
      tracing::debug!(
        target = "agentflow_mcp::stdio",
        expected_id = ?expected_id,
        got_id = ?response_id,
        "queued out-of-band stdio message while waiting for matching response"
      );
      self.pending_inbox.push_back(response);
    }
  }

  async fn send_notification(&mut self, notification: Value) -> MCPResult<()> {
    // Check process health before sending
    self
      .check_process_health()
      .map_err(|e| e.context("Process health check failed before sending notification"))?;

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
    // Q2.6.1: drain any out-of-band messages that `send_message`
    // queued while it was waiting for a matching response id. This
    // is the single way callers can observe server notifications
    // that arrived interleaved with request responses.
    if let Some(buffered) = self.pending_inbox.pop_front() {
      return Ok(Some(buffered));
    }

    // Check process health
    self
      .check_process_health()
      .map_err(|e| e.context("Process health check failed before receiving message"))?;

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
    self.pending_inbox.clear();
    // Q2.6.2: stop the stderr drain. The process exit will close
    // the pipe so the task would unblock anyway, but aborting
    // explicitly keeps shutdown deterministic for tests.
    if let Some(handle) = self.stderr_drain.take() {
      handle.abort();
    }

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
    // Best effort cleanup. We deliberately do NOT spin up our own executor
    // here — running `futures::executor::block_on(process.kill().await)`
    // inside a tokio runtime context deadlocks, because the inner
    // `tokio::process::Child::kill()` future needs the tokio reactor that
    // is currently parked under the outer `block_on`. The CI hang in
    // `test_drop_cleans_up_process` was exactly this.
    //
    // Cleanup happens via two paths instead:
    //   1. Callers `await disconnect()` for the graceful path (preferred).
    //   2. `Command::kill_on_drop(true)` (set at spawn time) makes tokio
    //      SIGKILL the child when the `Child` is dropped, as long as the
    //      runtime is still alive.
    if let Some(handle) = self.stderr_drain.take() {
      handle.abort();
    }
    drop(self.process.take());
    drop(self.stdin.take());
    drop(self.stdout.take());
  }
}

/// Q2.6.2: stream the child's stderr into `tracing::warn!` so it
/// never blocks on a full pipe. We deliberately ignore I/O errors
/// — once stderr closes (child exits / disconnect aborts the task)
/// the loop terminates.
fn spawn_stderr_drain(stderr: ChildStderr) -> JoinHandle<()> {
  tokio::spawn(async move {
    let mut reader = BufReader::new(stderr);
    let mut line = String::new();
    loop {
      line.clear();
      match reader.read_line(&mut line).await {
        Ok(0) => break, // EOF
        Ok(_) => {
          let trimmed = line.trim_end_matches(['\r', '\n']);
          if !trimmed.is_empty() {
            tracing::warn!(target = "agentflow_mcp::stdio::stderr", "{trimmed}");
          }
        }
        Err(_) => break,
      }
    }
  })
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
    let transport =
      StdioTransport::new(vec!["test".to_string()]).with_timeout(Duration::from_secs(60));
    assert_eq!(transport.timeout_ms(), Some(60_000));
  }

  #[test]
  fn test_stdio_transport_with_max_message_size() {
    let transport = StdioTransport::new(vec!["test".to_string()]).with_max_message_size(1024);
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

  // Q3.2.1: by default a third-party MCP server must NOT see the parent's
  // secrets. We can't trivially observe the spawned child's env from the
  // parent process, but we can verify the spawn-sandbox path *runs* (no
  // env vars escape the construction stage) and that the opt-in flag
  // flips back to inherit-mode without panicking.
  #[cfg(unix)]
  #[tokio::test]
  async fn spawn_sandboxes_parent_env_by_default() {
    // Set a secret in the parent that the child would inherit by
    // default if we hadn't sandboxed. Use a unique key so we can detect
    // leakage even in parallel test runs.
    let secret_key = "Q3_2_1_LEAK_TEST_OPENAI_API_KEY";
    let secret_value = "sk-must-not-leak";
    // SAFETY: test single-threaded w.r.t. this var; no other thread reads it.
    unsafe {
      std::env::set_var(secret_key, secret_value);
    }

    // The child writes its full environment to stdout, so we can grep
    // for the secret.
    let mut transport = StdioTransport::new(vec![
      "/usr/bin/env".to_string(),
    ])
    .with_timeout(Duration::from_secs(5));

    let _ = transport.connect().await; // env exits with code 0; transport may then EOF.
    // Read whatever the child wrote on stdout before exiting.
    let mut stdout_collected = String::new();
    if let Some(stdout) = transport.stdout.as_mut() {
      use tokio::io::AsyncReadExt;
      // Bounded read so we never hang.
      let mut buf = [0u8; 8192];
      while let Ok(Ok(n)) =
        tokio::time::timeout(Duration::from_millis(500), stdout.read(&mut buf)).await
      {
        if n == 0 {
          break;
        }
        stdout_collected.push_str(&String::from_utf8_lossy(&buf[..n]));
      }
    }
    let _ = transport.disconnect().await;

    // SAFETY: test cleanup; var was set by us in the same test.
    unsafe {
      std::env::remove_var(secret_key);
    }

    assert!(
      !stdout_collected.contains(secret_value),
      "parent secret leaked to spawned MCP child by default; got env:\n{stdout_collected}"
    );
  }

  #[cfg(unix)]
  #[tokio::test]
  async fn spawn_inherits_parent_env_when_explicitly_opted_in() {
    let secret_key = "Q3_2_1_OPTIN_TEST_VAR";
    let secret_value = "opted-in-on-purpose";
    // SAFETY: test single-threaded w.r.t. this var; no other thread reads it.
    unsafe {
      std::env::set_var(secret_key, secret_value);
    }

    let mut transport = StdioTransport::new(vec![
      "/usr/bin/env".to_string(),
    ])
    .with_timeout(Duration::from_secs(5))
    .with_inherit_parent_env(true);

    let _ = transport.connect().await;
    let mut stdout_collected = String::new();
    if let Some(stdout) = transport.stdout.as_mut() {
      use tokio::io::AsyncReadExt;
      let mut buf = [0u8; 8192];
      while let Ok(Ok(n)) =
        tokio::time::timeout(Duration::from_millis(500), stdout.read(&mut buf)).await
      {
        if n == 0 {
          break;
        }
        stdout_collected.push_str(&String::from_utf8_lossy(&buf[..n]));
      }
    }
    let _ = transport.disconnect().await;

    // SAFETY: test cleanup; var was set by us in the same test.
    unsafe {
      std::env::remove_var(secret_key);
    }

    assert!(
      stdout_collected.contains(secret_value),
      "with_inherit_parent_env(true) must restore parent env; got:\n{stdout_collected}"
    );
  }

  // ============================================================================
  // Connection Tests
  // ============================================================================

  #[tokio::test]
  async fn test_connect_empty_command() {
    let mut transport = StdioTransport::new(vec![]);
    let result = transport.connect().await;
    assert!(result.is_err());
    assert!(matches!(
      result.unwrap_err(),
      MCPError::Configuration { .. }
    ));
  }

  #[tokio::test]
  async fn test_connect_invalid_command() {
    let mut transport = StdioTransport::new(vec!["nonexistent_command_xyz123".to_string()]);
    let result = transport.connect().await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MCPError::Connection { .. }));
  }

  #[tokio::test]
  async fn test_connect_already_connected() {
    // Use 'cat' which will wait for input (works on Unix-like systems)
    let mut transport =
      StdioTransport::new(vec!["cat".to_string()]).with_timeout(Duration::from_millis(100));

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
    let mut transport =
      StdioTransport::new(vec!["cat".to_string()]).with_timeout(Duration::from_millis(100));

    transport.connect().await.unwrap();

    // Try to read - should timeout
    let result = transport.read_line_with_timeout().await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), MCPError::Timeout { .. }));

    transport.disconnect().await.unwrap();
  }

  #[tokio::test]
  async fn test_receive_message_timeout_returns_none() {
    let mut transport =
      StdioTransport::new(vec!["cat".to_string()]).with_timeout(Duration::from_millis(100));

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

  /// Q2.6.1 regression: a server that emits a notification before
  /// the matching response no longer permanently desyncs the
  /// session. Pre-fix `send_message` returned the notification as
  /// the response and the actual response had to be consumed via a
  /// separate read; here we craft a shell session that ships
  /// `notif → response` for every line, send a request, and verify
  /// the returned message has the matching id.
  #[tokio::test]
  #[cfg(unix)]
  async fn send_message_skips_notifications_until_matching_id() {
    // The shell script:
    //  - reads one line (the request)
    //  - emits a notification (no id)
    //  - echoes the request back unchanged as the response
    // Without Q2.6.1, the client would read the notification line
    // and treat it as the response.
    let mut transport = StdioTransport::new(vec![
      "sh".to_string(),
      "-c".to_string(),
      "while read line; do \
         echo '{\"jsonrpc\":\"2.0\",\"method\":\"server/notify\",\"params\":{\"hi\":1}}'; \
         echo \"$line\"; \
       done"
        .to_string(),
    ])
    .with_timeout(Duration::from_secs(2));
    transport.connect().await.unwrap();

    let request = json!({"jsonrpc": "2.0", "method": "ping", "id": 42});
    let response = transport.send_message(request.clone()).await.unwrap();
    assert_eq!(response.get("id"), Some(&json!(42)));
    assert_eq!(response, request, "response payload must round-trip");

    // The notification we buffered must be observable via
    // `receive_message`.
    let buffered = transport.receive_message().await.unwrap().unwrap();
    assert_eq!(
      buffered.get("method").and_then(|v| v.as_str()),
      Some("server/notify"),
      "queued notification not delivered: {buffered:?}"
    );

    transport.disconnect().await.unwrap();
  }

  /// Q2.6.2 regression: a server writing > 64 KiB of stderr stays
  /// healthy because the drain task keeps the pipe empty. Without
  /// the drain, the child would block in `write(2)` to stderr the
  /// moment the pipe fills and never send a response.
  #[tokio::test]
  #[cfg(unix)]
  async fn stderr_does_not_deadlock_when_server_floods_it() {
    // Print 128 KiB of stderr in 1 KiB chunks before echoing each
    // request. 128 KiB is comfortably above Linux's default 64 KiB
    // pipe buffer, so an undrained pipe would deadlock here.
    let mut transport = StdioTransport::new(vec![
      "sh".to_string(),
      "-c".to_string(),
      "for i in $(seq 1 128); do \
         printf 'stderr noise %d %s\\n' \"$i\" \"$(printf '%.0sx' {1..980})\" >&2; \
       done; \
       while read line; do echo \"$line\"; done"
        .to_string(),
    ])
    .with_timeout(Duration::from_secs(5));
    transport.connect().await.unwrap();

    // Give the script a beat to push the stderr noise.
    tokio::time::sleep(Duration::from_millis(150)).await;

    let request = json!({"jsonrpc": "2.0", "method": "ping", "id": 7});
    let response = transport.send_message(request.clone()).await.unwrap();
    assert_eq!(response, request);

    transport.disconnect().await.unwrap();
  }

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
    // Use echo to return invalid JSON. `sleep 5` keeps the child alive past
    // `send_message`'s `check_process_health` probe — without it, Linux's
    // tighter sh-exit timing races the read loop and the test surfaces
    // "Process exited with status: 0" before the parse-error path runs,
    // which is what we actually want to cover.
    let mut transport = StdioTransport::new(vec![
      "sh".to_string(),
      "-c".to_string(),
      "echo 'invalid json'; sleep 5".to_string(),
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
    let mut transport =
      StdioTransport::new(vec!["true".to_string()]).with_timeout(Duration::from_millis(500));

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
    // If we get here without hanging, drop worked.
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

        use crate::transport::TransportConfig;
        prop_assert_eq!(transport.timeout_ms(), Some(timeout_ms));
      }

      /// Property: Max message size configuration is preserved
      #[test]
      fn prop_max_message_size_preserved(size in 1usize..100_000_000usize) {
        let transport = StdioTransport::new(vec!["test".to_string()])
          .with_max_message_size(size);

        use crate::transport::TransportConfig;
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

        use crate::transport::TransportConfig;
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

        use crate::transport::TransportConfig;
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

        use crate::transport::TransportConfig;
        prop_assert_eq!(transport.timeout_ms(), Some(timeout_ms));
        prop_assert_eq!(transport.max_message_size(), Some(max_size));
        prop_assert_eq!(transport.transport_type(), TransportType::Stdio);
      }
    }
  }

  // Note: Additional integration tests with real MCP servers are in tests/ directory
}

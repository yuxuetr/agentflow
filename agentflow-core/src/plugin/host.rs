//! `PluginHost` — the host side of the subprocess plugin protocol.
//!
//! Spawns a plugin executable as a child process, performs the
//! `plugin/initialize` handshake, and dispatches `node/execute` calls
//! over newline-delimited JSON-RPC on stdio. See `docs/PLUGIN_DESIGN.md`
//! §6 for the architecture.

use crate::error::AgentFlowError;
use crate::plugin::manifest::{ManifestError, PluginManifest, SUPPORTED_PROTOCOL_VERSION};
use crate::plugin::protocol::{
  ExecuteParams, ExecuteResult, InitializeParams, InitializeResult, JSONRPC_VERSION, JsonRpcError,
  JsonRpcRequest, JsonRpcResponse, error_codes, methods,
};
use crate::value::FlowValue;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::timeout;

const HOST_VERSION: &str = env!("CARGO_PKG_VERSION");
const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(10);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(60);
const OUTBOUND_QUEUE_DEPTH: usize = 64;

/// Host-side errors. Mapped into `AgentFlowError::AsyncExecutionError` when a
/// plugin call surfaces through `AsyncNode::execute`.
#[derive(Debug, Error)]
pub enum PluginError {
  #[error("manifest error: {0}")]
  Manifest(#[from] ManifestError),
  #[error("io error: {0}")]
  Io(#[from] std::io::Error),
  #[error("serde error: {0}")]
  Serde(#[from] serde_json::Error),
  #[error("plugin '{name}' rejected initialize: {reason}")]
  InitializeFailed { name: String, reason: String },
  #[error("plugin call timed out after {timeout_ms}ms (method='{method}')")]
  Timeout { method: String, timeout_ms: u64 },
  #[error("plugin process exited unexpectedly: {0}")]
  Exited(String),
  #[error("plugin returned error code {code}: {message}")]
  RemoteError { code: i32, message: String },
  #[error("plugin host has been shut down")]
  ShutDown,
  #[error("plugin '{plugin}' already registered a node type '{node_type}'")]
  DuplicateNodeType { plugin: String, node_type: String },
  #[error("no plugin registered for node type '{0}'")]
  UnknownNodeType(String),
  #[error("command preparer rejected plugin '{plugin}': {reason}")]
  PreparerRejected { plugin: String, reason: String },
}

/// Hook that lets a higher layer modify the plugin's spawn command before
/// the child is created.
///
/// The primary use case is wrapping the command in an OS sandbox (macOS
/// `sandbox-exec`, Linux seccomp). The preparer receives the [`Command`]
/// already configured with `stdin`/`stdout`/`stderr` piped, plus the
/// parsed [`PluginManifest`] so it can derive an enforcement scope from
/// `[plugin.capabilities]`.
///
/// `agentflow-core` itself ships no enforcing implementation — it stays
/// neutral and depends on no platform sandbox crate. The CLI binds an
/// adapter that bridges into `agentflow-tools::sandbox`. Plugin authors
/// embedding the host directly can supply their own preparer or leave
/// the default (no-op), in which case behaviour is identical to v0.3
/// PoC.
pub trait CommandPreparer: Send + Sync + std::fmt::Debug {
  /// Stable name (`"sandbox-exec"`, `"seccomp"`, `"noop-plugin"`, ...) for trace.
  fn name(&self) -> &str;
  /// Modify `command` in place so that, when spawned, the child runs
  /// inside whatever enforcement layer the preparer provides. The
  /// `manifest_dir` is the directory holding the plugin's `plugin.toml`
  /// — backends that need a persistent profile path can scope their
  /// temp files to this directory if they want.
  fn prepare(
    &self,
    command: &mut Command,
    manifest: &PluginManifest,
    manifest_dir: &Path,
  ) -> Result<(), PluginError>;
}

/// Default no-op preparer. Used when callers don't opt into sandboxing,
/// preserving v0.3 PoC behaviour: bare `tokio::process::Command::spawn`.
#[derive(Debug, Default)]
pub struct NoopCommandPreparer;

impl CommandPreparer for NoopCommandPreparer {
  fn name(&self) -> &str {
    "noop-plugin"
  }
  fn prepare(
    &self,
    _command: &mut Command,
    _manifest: &PluginManifest,
    _manifest_dir: &Path,
  ) -> Result<(), PluginError> {
    Ok(())
  }
}

impl From<PluginError> for AgentFlowError {
  fn from(err: PluginError) -> Self {
    AgentFlowError::AsyncExecutionError {
      message: err.to_string(),
    }
  }
}

#[derive(Debug, Default)]
struct PendingTable {
  inner: Mutex<HashMap<u64, oneshot::Sender<std::result::Result<Value, JsonRpcError>>>>,
}

impl PendingTable {
  async fn insert(&self, id: u64, tx: oneshot::Sender<std::result::Result<Value, JsonRpcError>>) {
    self.inner.lock().await.insert(id, tx);
  }

  async fn complete(&self, id: u64, result: std::result::Result<Value, JsonRpcError>) {
    if let Some(tx) = self.inner.lock().await.remove(&id) {
      let _ = tx.send(result);
    }
  }

  async fn fail_all(&self, err: &JsonRpcError) {
    let drained: Vec<_> = {
      let mut guard = self.inner.lock().await;
      guard.drain().collect()
    };
    for (_, tx) in drained {
      let _ = tx.send(Err(err.clone()));
    }
  }
}

/// IO state of a live plugin connection. Created by `Connection::spawn`,
/// retired by `Connection::shutdown`.
#[derive(Debug)]
struct Connection {
  next_id: AtomicU64,
  pending: Arc<PendingTable>,
  outbound: Mutex<Option<mpsc::Sender<JsonRpcRequest>>>,
  reader_handle: Mutex<Option<JoinHandle<()>>>,
  writer_handle: Mutex<Option<JoinHandle<()>>>,
  child: Mutex<Option<Child>>,
}

impl Connection {
  async fn spawn(
    entrypoint: &Path,
    plugin_name: &str,
    manifest: &PluginManifest,
    manifest_dir: &Path,
    preparer: &dyn CommandPreparer,
  ) -> Result<Self, PluginError> {
    let mut command = Command::new(entrypoint);
    command
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .kill_on_drop(true);

    preparer.prepare(&mut command, manifest, manifest_dir)?;

    let mut child = command.spawn()?;
    let stdin = child.stdin.take().ok_or_else(|| {
      PluginError::Io(std::io::Error::new(
        std::io::ErrorKind::BrokenPipe,
        "plugin stdin missing",
      ))
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
      PluginError::Io(std::io::Error::new(
        std::io::ErrorKind::BrokenPipe,
        "plugin stdout missing",
      ))
    })?;
    let stderr = child.stderr.take();

    let pending = Arc::new(PendingTable::default());
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<JsonRpcRequest>(OUTBOUND_QUEUE_DEPTH);

    // Writer task: pulls from the outbound channel and writes one JSON line
    // per message to the plugin's stdin.
    let writer_pending = pending.clone();
    let writer_handle = {
      let mut stdin = stdin;
      tokio::spawn(async move {
        while let Some(req) = outbound_rx.recv().await {
          let json = match serde_json::to_string(&req) {
            Ok(s) => s,
            Err(e) => {
              writer_pending
                .complete(
                  req.id,
                  Err(JsonRpcError {
                    code: error_codes::INTERNAL_ERROR,
                    message: format!("host failed to serialize request: {e}"),
                    data: None,
                  }),
                )
                .await;
              continue;
            }
          };
          let line = format!("{json}\n");
          if let Err(e) = stdin.write_all(line.as_bytes()).await {
            writer_pending
              .complete(
                req.id,
                Err(JsonRpcError {
                  code: error_codes::PLUGIN_EXECUTION_ERROR,
                  message: format!("host failed to write to plugin stdin: {e}"),
                  data: None,
                }),
              )
              .await;
            break;
          }
          if let Err(e) = stdin.flush().await {
            writer_pending
              .complete(
                req.id,
                Err(JsonRpcError {
                  code: error_codes::PLUGIN_EXECUTION_ERROR,
                  message: format!("host failed to flush plugin stdin: {e}"),
                  data: None,
                }),
              )
              .await;
            break;
          }
        }
      })
    };

    // Reader task: parses each line of stdout. Responses are matched against
    // the pending table; notifications (no `id`) are logged through stderr.
    let reader_pending = pending.clone();
    let plugin_name_for_reader = plugin_name.to_string();
    let reader_handle = tokio::spawn(async move {
      let reader = BufReader::new(stdout);
      let mut lines = reader.lines();
      loop {
        match lines.next_line().await {
          Ok(Some(line)) if line.trim().is_empty() => continue,
          Ok(Some(line)) => {
            handle_inbound_line(&plugin_name_for_reader, &line, &reader_pending).await;
          }
          Ok(None) => {
            reader_pending
              .fail_all(&JsonRpcError {
                code: error_codes::PLUGIN_EXECUTION_ERROR,
                message: "plugin stdout closed".to_string(),
                data: None,
              })
              .await;
            break;
          }
          Err(e) => {
            reader_pending
              .fail_all(&JsonRpcError {
                code: error_codes::PLUGIN_EXECUTION_ERROR,
                message: format!("plugin stdout read error: {e}"),
                data: None,
              })
              .await;
            break;
          }
        }
      }
    });

    // Stderr forwarder (best-effort; plugin authors may use stderr for logs).
    if let Some(stderr) = stderr {
      let plugin_name = plugin_name.to_string();
      tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
          eprintln!("[plugin::{plugin_name}] {line}");
        }
      });
    }

    Ok(Connection {
      next_id: AtomicU64::new(1),
      pending,
      outbound: Mutex::new(Some(outbound_tx)),
      reader_handle: Mutex::new(Some(reader_handle)),
      writer_handle: Mutex::new(Some(writer_handle)),
      child: Mutex::new(Some(child)),
    })
  }

  async fn call(
    &self,
    method: &str,
    params: Option<Value>,
    timeout_dur: Duration,
  ) -> Result<Value, PluginError> {
    let id = self.next_id.fetch_add(1, Ordering::SeqCst);
    let request = JsonRpcRequest {
      jsonrpc: JSONRPC_VERSION.to_string(),
      id,
      method: method.to_string(),
      params,
    };

    let (tx, rx) = oneshot::channel();
    self.pending.insert(id, tx).await;

    let send_outcome = {
      let guard = self.outbound.lock().await;
      match guard.as_ref() {
        Some(sender) => sender.send(request).await.map_err(|_| ()),
        None => Err(()),
      }
    };
    if send_outcome.is_err() {
      self
        .pending
        .complete(
          id,
          Err(JsonRpcError {
            code: error_codes::PLUGIN_EXECUTION_ERROR,
            message: "plugin host outbound closed".to_string(),
            data: None,
          }),
        )
        .await;
    }

    match timeout(timeout_dur, rx).await {
      Ok(Ok(Ok(value))) => Ok(value),
      Ok(Ok(Err(rpc_err))) => Err(PluginError::RemoteError {
        code: rpc_err.code,
        message: rpc_err.message,
      }),
      Ok(Err(_canceled)) => Err(PluginError::Exited(format!(
        "response channel canceled for method '{method}'"
      ))),
      Err(_elapsed) => Err(PluginError::Timeout {
        method: method.to_string(),
        timeout_ms: timeout_dur.as_millis() as u64,
      }),
    }
  }

  async fn shutdown(&self) -> Result<(), PluginError> {
    // Best-effort shutdown RPC. We ignore the response since the plugin may
    // exit before completing the round-trip.
    let _ = self
      .call(
        methods::PLUGIN_SHUTDOWN,
        Some(serde_json::json!({})),
        SHUTDOWN_TIMEOUT,
      )
      .await;

    // Drop the outbound sender → writer task observes channel close and
    // exits → plugin's stdin closes → plugin sees EOF.
    {
      let mut guard = self.outbound.lock().await;
      *guard = None;
    }

    if let Some(handle) = self.writer_handle.lock().await.take() {
      let _ = timeout(SHUTDOWN_TIMEOUT, handle).await;
    }
    if let Some(handle) = self.reader_handle.lock().await.take() {
      handle.abort();
    }

    if let Some(mut child) = self.child.lock().await.take() {
      // Give the plugin a moment to exit gracefully; otherwise force it.
      match timeout(SHUTDOWN_TIMEOUT, child.wait()).await {
        Ok(_) => {}
        Err(_) => {
          let _ = child.start_kill();
          let _ = child.wait().await;
        }
      }
    }

    Ok(())
  }
}

async fn handle_inbound_line(plugin_name: &str, line: &str, pending: &PendingTable) {
  let value: Value = match serde_json::from_str(line) {
    Ok(v) => v,
    Err(e) => {
      eprintln!("[plugin::{plugin_name}] non-JSON line on stdout: {e}");
      return;
    }
  };

  if let Some(id) = value.get("id").and_then(|v| v.as_u64()) {
    let response: JsonRpcResponse = match serde_json::from_value(value) {
      Ok(r) => r,
      Err(e) => {
        eprintln!("[plugin::{plugin_name}] malformed JSON-RPC response: {e}");
        return;
      }
    };
    let result = match (response.result, response.error) {
      (Some(value), None) => Ok(value),
      (None, Some(err)) => Err(err),
      (Some(value), Some(_)) => Ok(value),
      (None, None) => Err(JsonRpcError {
        code: error_codes::INTERNAL_ERROR,
        message: "plugin response had neither result nor error".to_string(),
        data: None,
      }),
    };
    pending.complete(id, result).await;
  } else if let Some(method) = value.get("method").and_then(|v| v.as_str())
    && method == methods::PLUGIN_LOG
  {
    let msg = value
      .get("params")
      .and_then(|p| p.get("message"))
      .and_then(|m| m.as_str())
      .unwrap_or("(empty)");
    let level = value
      .get("params")
      .and_then(|p| p.get("level"))
      .and_then(|l| l.as_str())
      .unwrap_or("info");
    eprintln!("[plugin::{plugin_name}] [{level}] {msg}");
  }
}

/// A loaded, initialized plugin. Use [`PluginHost::execute_node`] to invoke
/// a declared node type, and [`PluginHost::shutdown`] to terminate the
/// underlying process.
#[derive(Debug)]
pub struct PluginHost {
  manifest: PluginManifest,
  initialize_result: InitializeResult,
  conn: Connection,
}

/// Builder for [`PluginHost::load`] that lets callers attach a
/// [`CommandPreparer`] before the plugin process is spawned.
///
/// Use [`PluginHost::builder`] to obtain one. The default preparer is
/// [`NoopCommandPreparer`], which preserves v0.3 PoC behaviour (bare
/// `Command::spawn` with no OS sandbox).
pub struct PluginHostBuilder {
  preparer: Arc<dyn CommandPreparer>,
}

impl Default for PluginHostBuilder {
  fn default() -> Self {
    Self {
      preparer: Arc::new(NoopCommandPreparer),
    }
  }
}

impl PluginHostBuilder {
  pub fn new() -> Self {
    Self::default()
  }

  /// Attach a [`CommandPreparer`] (typically an OS-sandbox bridge).
  pub fn with_command_preparer(mut self, preparer: Arc<dyn CommandPreparer>) -> Self {
    self.preparer = preparer;
    self
  }

  /// Load the plugin at `manifest_path` using the configured preparer.
  pub async fn load(self, manifest_path: &Path) -> Result<PluginHost, PluginError> {
    PluginHost::load_with_preparer(manifest_path, self.preparer.as_ref()).await
  }
}

impl PluginHost {
  /// Load a plugin from its `plugin.toml` manifest path. Spawns the plugin
  /// executable and performs the initialize handshake.
  ///
  /// Equivalent to `PluginHost::builder().load(manifest_path)`.
  pub async fn load(manifest_path: &Path) -> Result<Self, PluginError> {
    Self::load_with_preparer(manifest_path, &NoopCommandPreparer).await
  }

  /// Construct a new [`PluginHostBuilder`].
  pub fn builder() -> PluginHostBuilder {
    PluginHostBuilder::new()
  }

  async fn load_with_preparer(
    manifest_path: &Path,
    preparer: &dyn CommandPreparer,
  ) -> Result<Self, PluginError> {
    let (manifest, manifest_dir) = PluginManifest::load_from_path(manifest_path)?;
    manifest.validate()?;
    let entrypoint = manifest.resolve_entrypoint(&manifest_dir);
    let conn = Connection::spawn(
      &entrypoint,
      &manifest.plugin.name,
      &manifest,
      &manifest_dir,
      preparer,
    )
    .await?;

    let init_value = conn
      .call(
        methods::PLUGIN_INITIALIZE,
        Some(serde_json::to_value(InitializeParams {
          host_version: HOST_VERSION.to_string(),
          protocol_version: SUPPORTED_PROTOCOL_VERSION.to_string(),
        })?),
        INITIALIZE_TIMEOUT,
      )
      .await
      .map_err(|err| match err {
        PluginError::RemoteError { code, message } => PluginError::InitializeFailed {
          name: manifest.plugin.name.clone(),
          reason: format!("code {code}: {message}"),
        },
        other => other,
      })?;

    let initialize_result: InitializeResult = serde_json::from_value(init_value)?;

    Ok(PluginHost {
      manifest,
      initialize_result,
      conn,
    })
  }

  pub fn manifest(&self) -> &PluginManifest {
    &self.manifest
  }

  pub fn initialize_result(&self) -> &InitializeResult {
    &self.initialize_result
  }

  /// Convenience: list every node type the plugin declared at handshake.
  pub fn declared_node_types(&self) -> Vec<String> {
    self
      .initialize_result
      .nodes
      .iter()
      .map(|n| n.node_type.clone())
      .collect()
  }

  /// Invoke a plugin-declared node type with `inputs` and return its outputs.
  pub async fn execute_node(
    &self,
    node_type: &str,
    inputs: HashMap<String, FlowValue>,
  ) -> Result<ExecuteResult, PluginError> {
    let params = ExecuteParams {
      node_type: node_type.to_string(),
      inputs,
      run_id: None,
    };
    let value = self
      .conn
      .call(
        methods::NODE_EXECUTE,
        Some(serde_json::to_value(params)?),
        DEFAULT_CALL_TIMEOUT,
      )
      .await?;
    let result: ExecuteResult = serde_json::from_value(value)?;
    Ok(result)
  }

  /// Gracefully terminate the plugin process. Idempotent.
  pub async fn shutdown(&self) -> Result<(), PluginError> {
    self.conn.shutdown().await
  }
}

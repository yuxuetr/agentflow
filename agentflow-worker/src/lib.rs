//! Worker runtime for distributed AgentFlow execution.
//!
//! The runtime is transport-agnostic: it drives any
//! [`WorkerProtocol`](agentflow_server::WorkerProtocol) implementation through
//! heartbeat, claim, execute, and report-result steps. The first binary uses
//! the in-memory protocol for local smoke tests; the gRPC adapter can plug in
//! behind the same API.
//!
//! ## Supported `NodeExecutionPayload` types (P2.8)
//!
//! The worker dispatches on `payload.node_type`:
//!
//! - `template` → [`agentflow_nodes::nodes::template::TemplateNode`]
//! - `file` → [`agentflow_nodes::nodes::file::FileNode`]
//! - `mock` → in-crate stub used by the scheduler smoke tests
//! - `llm` → [`agentflow_nodes::nodes::llm::LlmNode`]
//! - `http` → [`agentflow_nodes::nodes::http::HttpNode`]
//! - `mcp` → [`agentflow_nodes::nodes::mcp::MCPNode`]
//! - `agent` → minimal [`agentflow_agents::react::ReActAgent`] loop with an
//!   empty [`agentflow_tools::ToolRegistry`]
//!
//! Unknown node types produce a non-retryable
//! [`AgentFlowError::FlowDefinitionError`], so a typo in YAML cannot
//! hot-loop the pool. See `docs/DISTRIBUTED.md` for the canonical
//! contract and test references.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use agentflow_agents::react::{ReActAgent, ReActConfig};
use agentflow_core::{
  AgentFlowError, FlowValue,
  async_node::{AsyncNode, AsyncNodeResult},
};
use agentflow_memory::SessionMemory;
use agentflow_nodes::nodes::{
  file::FileNode, http::HttpNode, llm::LlmNode, mcp::MCPNode, template::TemplateNode,
};
use agentflow_server::{
  ClaimHints, NodeExecutionPayload, SchedulerError, WorkerCapabilities, WorkerHeartbeat, WorkerId,
  WorkerProtocol, WorkerTask, WorkerTaskResult, WorkerTraceEvent,
};
use agentflow_tools::ToolRegistry;
use thiserror::Error;
use tokio::time::sleep;

/// Per-worker resource limits applied to every dispatched node.
///
/// **Stability:** experimental — see `docs/STABILITY.md` for the
/// distributed worker control plane row. The knobs below cover what
/// the worker can enforce in-process today; cgroup-level memory caps
/// are a documented gap on macOS (see `docs/DISTRIBUTED.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerResourceLimits {
  /// Hard wall-clock cap on a single dispatch invocation. `None` means
  /// the worker waits forever for the node to finish — only safe in
  /// tests with built-in timeouts.
  pub default_timeout: Option<Duration>,
  /// Cap on the serialized size of the success output map. When
  /// exceeded, the worker replaces the output with a small JSON marker
  /// (`{"truncated": true, "limit": N, "size": M}`) and adds a
  /// `worker.task.output_truncated` trace event.
  pub max_output_bytes: Option<usize>,
}

impl Default for WorkerResourceLimits {
  fn default() -> Self {
    Self {
      // Conservative production-leaning default. Specific deployments
      // override via `WorkerConfig::with_resource_limits`.
      default_timeout: Some(Duration::from_secs(300)),
      // 1 MiB matches the default `MAX_OUTPUT_BYTES` used by the
      // harness background-task runtime, so the two surfaces feel
      // consistent.
      max_output_bytes: Some(1024 * 1024),
    }
  }
}

impl WorkerResourceLimits {
  /// "No limits" preset — useful for the existing scheduler smokes
  /// that need to keep running 100-node fan-outs without per-task
  /// envelopes.
  pub fn unlimited() -> Self {
    Self {
      default_timeout: None,
      max_output_bytes: None,
    }
  }
}

/// Worker process configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerConfig {
  pub worker_id: WorkerId,
  pub control_plane: String,
  pub free_slots: u32,
  pub poll_interval: Duration,
  pub heartbeat_interval: Duration,
  pub resource_limits: WorkerResourceLimits,
  /// Capabilities advertised in every claim + heartbeat
  /// (P10.16.2-FU1). Empty = "any task" (pre-FU1 default).
  /// A worker that knows it only handles `template` / `file`
  /// nodes can set this to skip the server-side filter cost on
  /// unmatched tasks.
  pub capabilities: WorkerCapabilities,
}

impl WorkerConfig {
  pub fn new(worker_id: WorkerId, control_plane: impl Into<String>) -> Self {
    Self {
      worker_id,
      control_plane: control_plane.into(),
      free_slots: 1,
      poll_interval: Duration::from_millis(250),
      heartbeat_interval: Duration::from_secs(5),
      // Existing scheduler smokes pre-date P5.6 and don't expect
      // timeouts — keep the default unlimited so they continue to
      // pass unchanged. Production callers should override via
      // `with_resource_limits` with the prod-leaning preset.
      resource_limits: WorkerResourceLimits::unlimited(),
      capabilities: WorkerCapabilities::default(),
    }
  }

  pub fn with_resource_limits(mut self, limits: WorkerResourceLimits) -> Self {
    self.resource_limits = limits;
    self
  }

  /// Advertise a fixed capability set on every heartbeat + claim
  /// (P10.16.2-FU1). Equivalent to setting `self.capabilities`
  /// directly.
  pub fn with_capabilities(mut self, capabilities: WorkerCapabilities) -> Self {
    self.capabilities = capabilities;
    self
  }
}

/// Errors emitted by the worker runtime.
#[derive(Debug, Error)]
pub enum WorkerError {
  #[error("scheduler error: {0}")]
  Scheduler(#[from] SchedulerError),

  #[error("invalid configuration: {0}")]
  InvalidConfig(String),
}

/// Cooperative cancellation token shared between the supervising
/// runtime and the worker. The runtime checks the flag before
/// dispatching the next task and before the inner await; cancellation
/// arriving mid-dispatch lets the current task finish and is reported
/// as a non-retryable cancellation failure.
///
/// **Stability:** experimental, tracked under P5.6 (see
/// `docs/STABILITY.md`). The shape may grow to thread per-task
/// cancellation in later milestones.
#[derive(Debug, Clone, Default)]
pub struct WorkerCancellationToken {
  flag: Arc<AtomicBool>,
}

impl WorkerCancellationToken {
  pub fn new() -> Self {
    Self::default()
  }

  /// Trip the cancellation flag. Subsequent dispatches return
  /// immediately with a cancellation failure; an already-running
  /// dispatch finishes naturally (no abort).
  pub fn cancel(&self) {
    self.flag.store(true, Ordering::SeqCst);
  }

  pub fn is_cancelled(&self) -> bool {
    self.flag.load(Ordering::SeqCst)
  }
}

/// Transport-independent worker loop.
#[derive(Debug, Clone)]
pub struct WorkerRuntime<P> {
  protocol: P,
  config: WorkerConfig,
  cancellation: WorkerCancellationToken,
}

impl<P> WorkerRuntime<P>
where
  P: WorkerProtocol,
{
  pub fn new(protocol: P, config: WorkerConfig) -> Self {
    Self {
      protocol,
      config,
      cancellation: WorkerCancellationToken::new(),
    }
  }

  /// Replace the runtime's cancellation token. Tests and supervisors
  /// keep a clone to signal a graceful shutdown.
  pub fn with_cancellation(mut self, token: WorkerCancellationToken) -> Self {
    self.cancellation = token;
    self
  }

  pub fn cancellation_token(&self) -> WorkerCancellationToken {
    self.cancellation.clone()
  }

  /// Run one heartbeat/claim/execute/report cycle.
  pub async fn run_once(&self) -> Result<Option<WorkerTask>, WorkerError> {
    self
      .protocol
      .heartbeat(
        WorkerHeartbeat::now(self.config.worker_id.clone(), None, self.config.free_slots)
          .with_capabilities(self.config.capabilities.clone()),
      )
      .await?;

    // P10.16.2-FU1: send the worker's capabilities (and an empty
    // locality hint — the server defaults to "most-recently-
    // claimed run" when this is absent) so the queue scan can
    // skip work the worker can't run.
    let hints = ClaimHints::default().with_capabilities(self.config.capabilities.clone());
    let Some(task) = self
      .protocol
      .claim_task_with_hints(self.config.worker_id.clone(), &hints)
      .await?
    else {
      return Ok(None);
    };
    let result = execute_stub(
      &self.config.worker_id,
      &task,
      &self.config.resource_limits,
      &self.cancellation,
    )
    .await;
    self
      .protocol
      .report_result(self.config.worker_id.clone(), task.task_id, result)
      .await?;
    Ok(Some(task))
  }

  /// Run until the process is interrupted.
  pub async fn run_forever(&self) -> Result<(), WorkerError> {
    loop {
      if self.cancellation.is_cancelled() {
        return Ok(());
      }
      let _ = self.run_once().await?;
      sleep(self.config.poll_interval).await;
    }
  }
}

async fn execute_stub(
  worker_id: &WorkerId,
  task: &WorkerTask,
  limits: &WorkerResourceLimits,
  cancellation: &WorkerCancellationToken,
) -> WorkerTaskResult {
  // Pre-cancel check: tasks that were claimed before the runtime was
  // asked to shut down are still rejected so we don't run extra work
  // post-cancellation. The claim itself is allowed because that path
  // is owned by the supervising runtime.
  if cancellation.is_cancelled() {
    return cancelled_result(worker_id, task);
  }

  if let Ok(payload) = serde_json::from_value::<NodeExecutionPayload>(task.payload.clone()) {
    return execute_node_payload(worker_id, task, payload, limits, cancellation).await;
  }

  WorkerTaskResult::Succeeded {
    output: serde_json::json!({
      "worker_id": worker_id.0,
      "task_id": task.task_id,
      "node_id": task.node_id,
      "attempt": task.attempt,
      "payload": task.payload,
    }),
    events: vec![
      WorkerTraceEvent {
        seq: 0,
        kind: "worker.task.started".into(),
        payload: serde_json::json!({
          "worker_id": worker_id.0,
          "task_id": task.task_id,
          "node_id": task.node_id,
        }),
      },
      WorkerTraceEvent {
        seq: 1,
        kind: "worker.task.completed".into(),
        payload: serde_json::json!({
          "worker_id": worker_id.0,
          "task_id": task.task_id,
          "node_id": task.node_id,
        }),
      },
    ],
  }
}

/// Future that resolves once the cancellation flag flips. Polled
/// alongside the dispatcher so the worker reacts to cancel within one
/// poll interval.
async fn wait_for_cancel(token: &WorkerCancellationToken) {
  loop {
    if token.is_cancelled() {
      return;
    }
    tokio::time::sleep(Duration::from_millis(25)).await;
  }
}

fn cancelled_during_dispatch(
  worker_id: &WorkerId,
  task: &WorkerTask,
  node_type: &str,
  started: WorkerTraceEvent,
) -> WorkerTaskResult {
  WorkerTaskResult::Failed {
    error: format!("distributed worker cancelled mid-dispatch of node '{node_type}'"),
    retryable: false,
    events: vec![
      started,
      WorkerTraceEvent {
        seq: 1,
        kind: "worker.task.cancelled".into(),
        payload: serde_json::json!({
          "worker_id": worker_id.0,
          "task_id": task.task_id,
          "node_id": task.node_id,
          "node_type": node_type,
          "attempt": task.attempt,
        }),
      },
    ],
  }
}

/// Cap the serialized success output. When `max_output_bytes` is set
/// and the output exceeds the cap, replace it with a small marker
/// envelope and emit a `worker.task.output_truncated` trace event so
/// operators can audit where the cut happened.
fn cap_success_output(
  worker_id: &WorkerId,
  task: &WorkerTask,
  outputs: std::collections::HashMap<String, FlowValue>,
  max_output_bytes: Option<usize>,
) -> (serde_json::Value, Vec<WorkerTraceEvent>) {
  let value = serde_json::to_value(&outputs).unwrap_or_else(|_| serde_json::json!({}));
  let Some(max) = max_output_bytes else {
    return (value, Vec::new());
  };
  let serialized = match serde_json::to_vec(&value) {
    Ok(bytes) => bytes,
    Err(_) => return (value, Vec::new()),
  };
  if serialized.len() <= max {
    return (value, Vec::new());
  }
  let truncated = serde_json::json!({
    "truncated": true,
    "limit_bytes": max,
    "size_bytes": serialized.len(),
  });
  let event = WorkerTraceEvent {
    // `seq` here is 1; `execute_node_payload` re-indexes the event
    // stream so this stays consistent with the `started` event.
    seq: 1,
    kind: "worker.task.output_truncated".into(),
    payload: serde_json::json!({
      "worker_id": worker_id.0,
      "task_id": task.task_id,
      "node_id": task.node_id,
      "attempt": task.attempt,
      "limit_bytes": max,
      "size_bytes": serialized.len(),
    }),
  };
  (truncated, vec![event])
}

fn cancelled_result(worker_id: &WorkerId, task: &WorkerTask) -> WorkerTaskResult {
  WorkerTaskResult::Failed {
    error: "worker cancelled before dispatching task".to_string(),
    // Cancellation is operator-initiated, never a transport hiccup.
    // The scheduler treats this as terminal so retries don't loop
    // when the worker is draining.
    retryable: false,
    events: vec![WorkerTraceEvent {
      seq: 0,
      kind: "worker.task.cancelled".into(),
      payload: serde_json::json!({
        "worker_id": worker_id.0,
        "task_id": task.task_id,
        "node_id": task.node_id,
        "attempt": task.attempt,
      }),
    }],
  }
}

async fn execute_node_payload(
  worker_id: &WorkerId,
  task: &WorkerTask,
  payload: NodeExecutionPayload,
  limits: &WorkerResourceLimits,
  cancellation: &WorkerCancellationToken,
) -> WorkerTaskResult {
  let started = WorkerTraceEvent {
    seq: 0,
    kind: "worker.task.started".into(),
    payload: serde_json::json!({
      "worker_id": worker_id.0,
      "task_id": task.task_id,
      "node_id": task.node_id,
      "node_type": payload.node_type,
      "attempt": task.attempt,
    }),
  };

  let node_type = payload.node_type.clone();
  let inner = execute_supported_node_payload(payload, task.attempt);
  let timeout = limits.default_timeout;

  let dispatch = async {
    if let Some(deadline) = timeout {
      match tokio::time::timeout(deadline, inner).await {
        Ok(result) => result,
        Err(_) => Err(AgentFlowError::AsyncExecutionError {
          message: format!("distributed worker timeout: node '{node_type}' exceeded {deadline:?}"),
        }),
      }
    } else {
      inner.await
    }
  };

  // Cancellation cuts the dispatch off as soon as it can yield. The
  // inner await is the only suspension point we control, so we race
  // it against a cancellation poll.
  let result = tokio::select! {
    biased;
    () = wait_for_cancel(cancellation) => {
      return cancelled_during_dispatch(worker_id, task, &node_type, started);
    }
    res = dispatch => res,
  };

  match result {
    Ok(outputs) => {
      let (output_value, mut extra_events) =
        cap_success_output(worker_id, task, outputs, limits.max_output_bytes);
      let mut events = vec![started];
      events.append(&mut extra_events);
      events.push(WorkerTraceEvent {
        seq: events.len() as i64,
        kind: "worker.task.completed".into(),
        payload: serde_json::json!({
          "worker_id": worker_id.0,
          "task_id": task.task_id,
          "node_id": task.node_id,
          "attempt": task.attempt,
        }),
      });
      WorkerTaskResult::Succeeded {
        output: output_value,
        events,
      }
    }
    Err(error) => WorkerTaskResult::Failed {
      error: error.to_string(),
      retryable: matches!(error, AgentFlowError::AsyncExecutionError { .. }),
      events: vec![
        started,
        WorkerTraceEvent {
          seq: 1,
          kind: "worker.task.failed".into(),
          payload: serde_json::json!({
            "worker_id": worker_id.0,
            "task_id": task.task_id,
            "node_id": task.node_id,
            "attempt": task.attempt,
            "error": error.to_string(),
          }),
        },
      ],
    },
  }
}

async fn execute_supported_node_payload(
  payload: NodeExecutionPayload,
  attempt: u32,
) -> Result<std::collections::HashMap<String, FlowValue>, AgentFlowError> {
  match payload.node_type.as_str() {
    "template" => execute_template_payload(&payload).await,
    "file" => execute_file_payload(&payload).await,
    "mock" => execute_mock_payload(&payload, attempt).await,
    // P2.8: distributed support for LLM / HTTP / MCP / agent payloads.
    // The local scheduler already inlines `parameters` into `inputs` (see
    // `gather_inputs` in `agentflow-server::scheduler::distributed`), so
    // each node's `execute` receives the same input map it would in-process.
    "llm" => execute_llm_payload(&payload).await,
    "http" => execute_http_payload(&payload).await,
    "mcp" => execute_mcp_payload(&payload).await,
    "agent" => execute_agent_payload(&payload).await,
    other => Err(AgentFlowError::FlowDefinitionError {
      message: format!("distributed worker does not support node type '{other}'"),
    }),
  }
}

async fn execute_template_payload(payload: &NodeExecutionPayload) -> AsyncNodeResult {
  let template = string_parameter(payload, "template")?;
  let mut node = TemplateNode::new(&payload.node_id, &template);
  if let Some(output_key) = optional_string_parameter(payload, "output_key") {
    node = node.with_output_key(&output_key);
  }
  if let Some(output_format) = optional_string_parameter(payload, "output_format") {
    node = node.with_format(&output_format);
  }
  node.execute(&payload.inputs).await
}

async fn execute_file_payload(payload: &NodeExecutionPayload) -> AsyncNodeResult {
  FileNode.execute(&payload.inputs).await
}

async fn execute_mock_payload(payload: &NodeExecutionPayload, attempt: u32) -> AsyncNodeResult {
  if let Some(fail_until_attempt) = payload
    .parameters
    .get("fail_until_attempt")
    .and_then(|value| value.as_u64())
    && u64::from(attempt) < fail_until_attempt
  {
    return Err(AgentFlowError::AsyncExecutionError {
      message: format!("mock node requested failure until attempt {fail_until_attempt}"),
    });
  }
  if matches!(
    payload
      .parameters
      .get("fail")
      .and_then(|value| value.as_bool()),
    Some(true)
  ) {
    return Err(AgentFlowError::AsyncExecutionError {
      message: "mock node requested failure".to_string(),
    });
  }
  // P5.6 — synthetic runaway hook: a non-zero `sleep_ms` parameter
  // makes the mock node yield for the given wall-clock duration so
  // the timeout / cancellation paths can be exercised deterministically
  // without spinning up a real long-running node.
  if let Some(sleep_ms) = payload
    .parameters
    .get("sleep_ms")
    .and_then(|value| value.as_u64())
    && sleep_ms > 0
  {
    tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
  }
  let mut outputs = std::collections::HashMap::new();
  let value = payload
    .parameters
    .get("value")
    .cloned()
    .unwrap_or_else(|| serde_json::json!(payload.node_id));
  outputs.insert("output".to_string(), FlowValue::Json(value));
  // P5.6 — synthetic large-output hook: emit an extra `payload` key
  // with `output_size_bytes` worth of 'x' characters so the
  // truncation path is testable hermetically.
  if let Some(size) = payload
    .parameters
    .get("output_size_bytes")
    .and_then(|value| value.as_u64())
    && size > 0
  {
    let big = "x".repeat(size.min(64 * 1024 * 1024) as usize);
    outputs.insert(
      "payload".to_string(),
      FlowValue::Json(serde_json::json!(big)),
    );
  }
  Ok(outputs)
}

async fn execute_llm_payload(payload: &NodeExecutionPayload) -> AsyncNodeResult {
  LlmNode.execute(&payload.inputs).await
}

async fn execute_http_payload(payload: &NodeExecutionPayload) -> AsyncNodeResult {
  HttpNode.execute(&payload.inputs).await
}

async fn execute_mcp_payload(payload: &NodeExecutionPayload) -> AsyncNodeResult {
  MCPNode::default().execute(&payload.inputs).await
}

/// Minimal ReAct loop dispatcher for distributed `agent` nodes.
///
/// The worker reads the canonical agent inputs (`message`, `model`, optional
/// `persona` / `max_iterations`) from the gathered input map. The agent runs
/// against a fresh `SessionMemory` and an empty `ToolRegistry`; richer tool
/// wiring rides on the same `parameters` plumbing once the tool-distribution
/// contract is decided (tracked under P5.5 worker admission).
async fn execute_agent_payload(payload: &NodeExecutionPayload) -> AsyncNodeResult {
  let message = required_string_input(payload, "message")?;
  let model = required_string_input(payload, "model")?;
  let persona =
    optional_string_input(payload, "persona").or_else(|| optional_string_input(payload, "system"));
  let max_iterations = optional_u64_input(payload, "max_iterations");

  let mut config = ReActConfig::new(model);
  if let Some(persona) = persona {
    config = config.with_persona(persona);
  }
  if let Some(max_iterations) = max_iterations {
    config = config.with_max_iterations(max_iterations.min(usize::MAX as u64) as usize);
  }

  let mut agent = ReActAgent::new(
    config,
    Box::new(SessionMemory::default_window()),
    Arc::new(ToolRegistry::new()),
  );

  let result =
    agent
      .run_with_trace(&message)
      .await
      .map_err(|e| AgentFlowError::AsyncExecutionError {
        message: format!("distributed agent run failed: {e}"),
      })?;

  let stop_reason =
    serde_json::to_value(&result.stop_reason).map_err(|e| AgentFlowError::AsyncExecutionError {
      message: format!("failed to serialize agent stop reason: {e}"),
    })?;

  let mut outputs = std::collections::HashMap::new();
  outputs.insert(
    "answer".to_string(),
    FlowValue::Json(serde_json::Value::String(
      result.answer.clone().unwrap_or_default(),
    )),
  );
  outputs.insert("stop_reason".to_string(), FlowValue::Json(stop_reason));
  outputs.insert(
    "session_id".to_string(),
    FlowValue::Json(serde_json::Value::String(result.session_id.clone())),
  );
  outputs.insert(
    "step_count".to_string(),
    FlowValue::Json(serde_json::json!(result.steps.len())),
  );
  Ok(outputs)
}

fn required_string_input(
  payload: &NodeExecutionPayload,
  key: &str,
) -> Result<String, AgentFlowError> {
  optional_string_input(payload, key).ok_or_else(|| AgentFlowError::NodeInputError {
    message: format!(
      "distributed node '{}' requires string input '{}'",
      payload.node_id, key
    ),
  })
}

fn optional_string_input(payload: &NodeExecutionPayload, key: &str) -> Option<String> {
  payload.inputs.get(key).and_then(|value| match value {
    FlowValue::Json(serde_json::Value::String(s)) => Some(s.clone()),
    _ => None,
  })
}

fn optional_u64_input(payload: &NodeExecutionPayload, key: &str) -> Option<u64> {
  payload.inputs.get(key).and_then(|value| match value {
    FlowValue::Json(serde_json::Value::Number(n)) => n.as_u64(),
    _ => None,
  })
}

fn string_parameter(payload: &NodeExecutionPayload, key: &str) -> Result<String, AgentFlowError> {
  optional_string_parameter(payload, key).ok_or_else(|| AgentFlowError::NodeInputError {
    message: format!(
      "distributed node '{}' requires string parameter '{}'",
      payload.node_id, key
    ),
  })
}

fn optional_string_parameter(payload: &NodeExecutionPayload, key: &str) -> Option<String> {
  payload
    .parameters
    .get(key)
    .and_then(|value| value.as_str())
    .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
  use super::*;
  use agentflow_server::{
    GrpcWorkerProtocol, InMemoryWorkerProtocol, RunControlStatus, WorkerControlPlane,
    WorkerControlServer,
    scheduler::distributed::{mock_flow, mock_node},
  };
  use chrono::Duration as ChronoDuration;
  use std::net::SocketAddr;
  use tokio::sync::oneshot;
  use tonic::transport::Server;
  use uuid::Uuid;

  #[tokio::test]
  async fn run_once_heartbeats_claims_and_reports_success() {
    let protocol = InMemoryWorkerProtocol::new();
    let run_id = Uuid::new_v4();
    let task = WorkerTask::new(run_id, "node-a", serde_json::json!({"input": 1}));
    let task_id = task.task_id;
    protocol.submit_task(task).await.unwrap();

    let worker_id = WorkerId::new("worker-a").unwrap();
    let runtime = WorkerRuntime::new(
      protocol.clone(),
      WorkerConfig::new(worker_id.clone(), "memory://local"),
    );
    let claimed = runtime.run_once().await.unwrap();

    assert_eq!(claimed.map(|task| task.task_id), Some(task_id));
    assert!(protocol.last_heartbeat(&worker_id).await.is_some());
    let result = protocol.completed_result(task_id).await.unwrap();
    let WorkerTaskResult::Succeeded { output, events } = result else {
      panic!("expected success");
    };
    assert_eq!(output["node_id"], "node-a");
    assert_eq!(events.len(), 2);
  }

  #[tokio::test]
  async fn run_once_returns_none_when_queue_is_empty() {
    let protocol = InMemoryWorkerProtocol::new();
    let worker_id = WorkerId::new("worker-a").unwrap();
    let runtime = WorkerRuntime::new(
      protocol,
      WorkerConfig::new(worker_id.clone(), "memory://local"),
    );

    assert!(runtime.run_once().await.unwrap().is_none());
  }

  #[tokio::test]
  async fn two_workers_claim_and_report_over_grpc() {
    let protocol = InMemoryWorkerProtocol::new();
    let control = WorkerControlPlane::new(protocol);
    let run_id = Uuid::new_v4();
    control
      .schedule_task(WorkerTask::new(
        run_id,
        "node-a",
        serde_json::json!({"input": "a"}),
      ))
      .await
      .unwrap();
    control
      .schedule_task(WorkerTask::new(
        run_id,
        "node-b",
        serde_json::json!({"input": "b"}),
      ))
      .await
      .unwrap();

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let addr = unused_local_addr();
    let server_control = control.clone();
    let server = tokio::spawn(async move {
      Server::builder()
        .add_service(WorkerControlServer::new(server_control))
        .serve_with_shutdown(addr, async {
          let _ = shutdown_rx.await;
        })
        .await
    });

    let endpoint = format!("http://{addr}");
    let worker_a = WorkerId::new("worker-a").unwrap();
    let worker_b = WorkerId::new("worker-b").unwrap();
    let runtime_a = WorkerRuntime::new(
      connect_with_retry(&endpoint).await,
      WorkerConfig::new(worker_a.clone(), endpoint.clone()),
    );
    let runtime_b = WorkerRuntime::new(
      connect_with_retry(&endpoint).await,
      WorkerConfig::new(worker_b.clone(), endpoint),
    );

    let task_a = runtime_a.run_once().await.unwrap().unwrap();
    let task_b = runtime_b.run_once().await.unwrap().unwrap();
    assert_ne!(task_a.task_id, task_b.task_id);

    let snapshot = control.run_snapshot(run_id).await.unwrap();
    assert_eq!(snapshot.status, RunControlStatus::Succeeded);
    assert_eq!(snapshot.succeeded_tasks, 2);
    assert_eq!(snapshot.outputs.len(), 2);
    assert_eq!(snapshot.stitched_trace_events.len(), 4);
    assert!(control.worker_heartbeat(&worker_a).await.is_some());
    assert!(control.worker_heartbeat(&worker_b).await.is_some());

    let _ = shutdown_tx.send(());
    server.await.unwrap().unwrap();
  }

  #[tokio::test]
  async fn run_once_executes_distributed_template_payload() {
    let protocol = InMemoryWorkerProtocol::new();
    let worker_id = WorkerId::new("worker-template").unwrap();
    let run_id = Uuid::new_v4();
    let payload = NodeExecutionPayload::new(
      "render",
      "template",
      std::collections::HashMap::from([(
        "template".to_string(),
        serde_json::json!("Hello {{ name }}"),
      )]),
      std::collections::HashMap::from([(
        "name".to_string(),
        FlowValue::Json(serde_json::json!("Ada")),
      )]),
    );
    let task = WorkerTask::new(run_id, "render", serde_json::to_value(payload).unwrap());
    let task_id = task.task_id;
    protocol.submit_task(task).await.unwrap();

    let runtime = WorkerRuntime::new(
      protocol.clone(),
      WorkerConfig::new(worker_id, "memory://local"),
    );
    runtime.run_once().await.unwrap();

    let WorkerTaskResult::Succeeded { output, .. } =
      protocol.completed_result(task_id).await.unwrap()
    else {
      panic!("expected template success");
    };
    assert_eq!(output["output"]["value"], "Hello Ada");
  }

  #[tokio::test]
  async fn run_once_executes_distributed_file_payload() {
    let protocol = InMemoryWorkerProtocol::new();
    let worker_id = WorkerId::new("worker-file").unwrap();
    let run_id = Uuid::new_v4();
    let path = std::env::temp_dir().join(format!("agentflow-worker-{}.txt", Uuid::new_v4()));
    let payload = NodeExecutionPayload::new(
      "write_file",
      "file",
      std::collections::HashMap::new(),
      std::collections::HashMap::from([
        (
          "operation".to_string(),
          FlowValue::Json(serde_json::json!("write")),
        ),
        (
          "path".to_string(),
          FlowValue::Json(serde_json::json!(path.to_string_lossy())),
        ),
        (
          "content".to_string(),
          FlowValue::Json(serde_json::json!("distributed file write")),
        ),
      ]),
    );
    protocol
      .submit_task(WorkerTask::new(
        run_id,
        "write_file",
        serde_json::to_value(payload).unwrap(),
      ))
      .await
      .unwrap();

    let runtime = WorkerRuntime::new(protocol, WorkerConfig::new(worker_id, "memory://local"));
    runtime.run_once().await.unwrap();

    let content = tokio::fs::read_to_string(&path).await.unwrap();
    assert_eq!(content, "distributed file write");
    let _ = tokio::fs::remove_file(path).await;
  }

  #[tokio::test]
  async fn distributed_scheduler_runs_100_mock_nodes_with_two_workers() {
    let protocol = InMemoryWorkerProtocol::new();
    let control = WorkerControlPlane::new(protocol);
    let run_id = Uuid::new_v4();
    let nodes = (0..100)
      .map(|idx| mock_node(format!("node-{idx}"), Vec::new(), serde_json::json!(idx)))
      .collect::<Vec<_>>();
    let flow = mock_flow("large mock", nodes);
    let mut scheduler =
      agentflow_server::DistributedDagScheduler::new(run_id, flow, control.clone()).unwrap();
    let worker_a = WorkerRuntime::new(
      control.clone(),
      WorkerConfig::new(WorkerId::new("worker-a").unwrap(), "memory://local"),
    );
    let worker_b = WorkerRuntime::new(
      control.clone(),
      WorkerConfig::new(WorkerId::new("worker-b").unwrap(), "memory://local"),
    );

    while !scheduler.is_terminal() {
      scheduler.dispatch_ready().await.unwrap();
      let claimed_a = worker_a.run_once().await.unwrap();
      let claimed_b = worker_b.run_once().await.unwrap();
      scheduler.reconcile_results().await.unwrap();
      if claimed_a.is_none() && claimed_b.is_none() && scheduler.running_count() == 0 {
        break;
      }
    }

    let result = scheduler.run_result();
    assert!(result.succeeded);
    assert_eq!(result.state_pool.len(), 100);
    let snapshot = control.run_snapshot(run_id).await.unwrap();
    assert_eq!(snapshot.succeeded_tasks, 100);
    assert_eq!(snapshot.stitched_trace_events.len(), 200);
  }

  #[tokio::test]
  async fn distributed_scheduler_retries_retryable_failure() {
    let protocol = InMemoryWorkerProtocol::new();
    let control = WorkerControlPlane::new(protocol);
    let run_id = Uuid::new_v4();
    let mut node = mock_node("retry-once", Vec::new(), serde_json::json!("ok"));
    node.parameters.insert(
      "fail_until_attempt".to_string(),
      serde_yaml::to_value(1).unwrap(),
    );
    let flow = mock_flow("retry mock", vec![node]);
    let mut scheduler =
      agentflow_server::DistributedDagScheduler::new(run_id, flow, control.clone())
        .unwrap()
        .with_max_attempts(2);
    let worker = WorkerRuntime::new(
      control.clone(),
      WorkerConfig::new(WorkerId::new("worker-retry").unwrap(), "memory://local"),
    );

    while !scheduler.is_terminal() {
      scheduler.dispatch_ready().await.unwrap();
      let _ = worker.run_once().await.unwrap();
      scheduler.reconcile_results().await.unwrap();
    }

    let result = scheduler.run_result();
    assert!(result.succeeded);
    let snapshot = control.run_snapshot(run_id).await.unwrap();
    assert_eq!(snapshot.failed_tasks, 1);
    assert_eq!(snapshot.succeeded_tasks, 1);
  }

  #[tokio::test]
  async fn distributed_scheduler_requeues_stale_heartbeat_task() {
    let protocol = InMemoryWorkerProtocol::new();
    let control = WorkerControlPlane::new(protocol);
    let run_id = Uuid::new_v4();
    let flow = mock_flow(
      "stale mock",
      vec![mock_node("stale-node", Vec::new(), serde_json::json!("ok"))],
    );
    let mut scheduler =
      agentflow_server::DistributedDagScheduler::new(run_id, flow, control.clone())
        .unwrap()
        .with_max_attempts(2)
        .with_heartbeat_timeout(Duration::from_millis(1));
    scheduler.dispatch_ready().await.unwrap();

    let worker_id = WorkerId::new("stale-worker").unwrap();
    let claimed = control
      .claim_task(worker_id.clone())
      .await
      .unwrap()
      .unwrap();
    control
      .heartbeat(WorkerHeartbeat {
        worker_id,
        active_task: Some(claimed.task_id),
        free_slots: 0,
        ts: chrono::Utc::now() - ChronoDuration::seconds(5),
        capabilities: Default::default(),
      })
      .await
      .unwrap();

    let requeued = scheduler.requeue_stale_tasks().await.unwrap();
    assert_eq!(requeued, 1);
    assert_eq!(
      scheduler.node_status("stale-node"),
      Some(agentflow_server::DistributedNodeStatus::Pending)
    );
    scheduler.dispatch_ready().await.unwrap();
    assert_eq!(scheduler.running_count(), 1);
  }

  async fn connect_with_retry(endpoint: &str) -> GrpcWorkerProtocol {
    let mut last_error = None;
    for _ in 0..20 {
      match GrpcWorkerProtocol::connect(endpoint).await {
        Ok(protocol) => return protocol,
        Err(err) => {
          last_error = Some(err);
          sleep(Duration::from_millis(25)).await;
        }
      }
    }
    panic!("failed to connect to gRPC worker control: {last_error:?}");
  }

  fn unused_local_addr() -> SocketAddr {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
  }
}

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
  NodeExecutionPayload, SchedulerError, WorkerHeartbeat, WorkerId, WorkerProtocol, WorkerTask,
  WorkerTaskResult, WorkerTraceEvent,
};
use agentflow_tools::ToolRegistry;
use thiserror::Error;
use tokio::time::sleep;

/// Worker process configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerConfig {
  pub worker_id: WorkerId,
  pub control_plane: String,
  pub free_slots: u32,
  pub poll_interval: Duration,
  pub heartbeat_interval: Duration,
}

impl WorkerConfig {
  pub fn new(worker_id: WorkerId, control_plane: impl Into<String>) -> Self {
    Self {
      worker_id,
      control_plane: control_plane.into(),
      free_slots: 1,
      poll_interval: Duration::from_millis(250),
      heartbeat_interval: Duration::from_secs(5),
    }
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

/// Transport-independent worker loop.
#[derive(Debug, Clone)]
pub struct WorkerRuntime<P> {
  protocol: P,
  config: WorkerConfig,
}

impl<P> WorkerRuntime<P>
where
  P: WorkerProtocol,
{
  pub fn new(protocol: P, config: WorkerConfig) -> Self {
    Self { protocol, config }
  }

  /// Run one heartbeat/claim/execute/report cycle.
  pub async fn run_once(&self) -> Result<Option<WorkerTask>, WorkerError> {
    self
      .protocol
      .heartbeat(WorkerHeartbeat::now(
        self.config.worker_id.clone(),
        None,
        self.config.free_slots,
      ))
      .await?;

    let Some(task) = self
      .protocol
      .claim_task(self.config.worker_id.clone())
      .await?
    else {
      return Ok(None);
    };
    let result = execute_stub(&self.config.worker_id, &task).await;
    self
      .protocol
      .report_result(self.config.worker_id.clone(), task.task_id, result)
      .await?;
    Ok(Some(task))
  }

  /// Run until the process is interrupted.
  pub async fn run_forever(&self) -> Result<(), WorkerError> {
    loop {
      let _ = self.run_once().await?;
      sleep(self.config.poll_interval).await;
    }
  }
}

async fn execute_stub(worker_id: &WorkerId, task: &WorkerTask) -> WorkerTaskResult {
  if let Ok(payload) = serde_json::from_value::<NodeExecutionPayload>(task.payload.clone()) {
    return execute_node_payload(worker_id, task, payload).await;
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

async fn execute_node_payload(
  worker_id: &WorkerId,
  task: &WorkerTask,
  payload: NodeExecutionPayload,
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

  let result = execute_supported_node_payload(payload, task.attempt).await;

  match result {
    Ok(outputs) => WorkerTaskResult::Succeeded {
      output: serde_json::to_value(outputs).unwrap_or_else(|_| serde_json::json!({})),
      events: vec![
        started,
        WorkerTraceEvent {
          seq: 1,
          kind: "worker.task.completed".into(),
          payload: serde_json::json!({
            "worker_id": worker_id.0,
            "task_id": task.task_id,
            "node_id": task.node_id,
            "attempt": task.attempt,
          }),
        },
      ],
    },
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
  let mut outputs = std::collections::HashMap::new();
  let value = payload
    .parameters
    .get("value")
    .cloned()
    .unwrap_or_else(|| serde_json::json!(payload.node_id));
  outputs.insert("output".to_string(), FlowValue::Json(value));
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

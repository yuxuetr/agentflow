//! P2.8 — distributed dispatcher routing for `http`, `mcp`, and unknown
//! node types.
//!
//! The mocks here are intentionally thin: we exercise *which* dispatcher
//! the worker selects, not the full transport happy path. `HttpNode` and
//! `MCPNode` already have their own integration tests in their owning
//! crates; the worker's contribution is the type tag → executor mapping.
//!
//! Live HTTP / MCP smokes are covered by the wider distributed
//! end-to-end suite (P5.5 onward) where the proxy / process-spawn
//! requirements are handled by the harness fixtures rather than
//! ad-hoc test setup.

use std::collections::HashMap;

use agentflow_core::FlowValue;
use agentflow_server::{
  InMemoryWorkerProtocol, NodeExecutionPayload, WorkerId, WorkerProtocol, WorkerTask,
  WorkerTaskResult,
};
use agentflow_worker::{WorkerConfig, WorkerRuntime};
use serde_json::json;
use uuid::Uuid;

fn worker_id(label: &str) -> WorkerId {
  WorkerId::new(label).expect("worker label is valid")
}

async fn run_payload(worker: &str, payload: NodeExecutionPayload) -> WorkerTaskResult {
  let protocol = InMemoryWorkerProtocol::new();
  let run_id = Uuid::new_v4();
  let node_id = payload.node_id.clone();
  let task = WorkerTask::new(
    run_id,
    node_id,
    serde_json::to_value(payload).expect("payload serializes"),
  );
  let task_id = task.task_id;
  protocol.submit_task(task).await.expect("submit task");

  let runtime = WorkerRuntime::new(
    protocol.clone(),
    WorkerConfig::new(worker_id(worker), "memory://local"),
  );
  runtime.run_once().await.expect("run once");
  protocol
    .completed_result(task_id)
    .await
    .expect("result recorded")
}

#[tokio::test]
async fn unsupported_node_type_returns_structured_failure() {
  let payload = NodeExecutionPayload::new(
    "unknown-node",
    "definitely-not-a-real-type",
    HashMap::new(),
    HashMap::new(),
  );
  let WorkerTaskResult::Failed {
    error, retryable, ..
  } = run_payload("worker-unsupported", payload).await
  else {
    panic!("expected unsupported node payload to fail");
  };
  assert!(
    error.contains("distributed worker does not support node type"),
    "error was: {error}"
  );
  // Flow-definition errors are not transport-retryable; the scheduler
  // would otherwise loop forever on a typo in the YAML.
  assert!(!retryable, "definition errors must not be retryable");
}

#[tokio::test]
async fn http_payload_routes_to_http_node_dispatcher() {
  // A bogus URL is enough to prove dispatch. `HttpNode` returns
  // `AsyncExecutionError` for transport failures, which the scheduler
  // marks retryable. The key assertion is that we don't get
  // "does not support node type 'http'".
  let mut inputs = HashMap::new();
  inputs.insert(
    "url".to_string(),
    FlowValue::Json(json!("http://127.0.0.1:1/this-port-is-unbound")),
  );
  inputs.insert("method".to_string(), FlowValue::Json(json!("GET")));
  let payload = NodeExecutionPayload::new("fetch", "http", HashMap::new(), inputs);

  let WorkerTaskResult::Failed {
    error, retryable, ..
  } = run_payload("worker-http", payload).await
  else {
    panic!("expected http dispatch failure (port is unbound)");
  };
  assert!(
    !error.contains("does not support node type"),
    "http payload should have routed to HttpNode; error was: {error}"
  );
  // Transport errors are retryable — distinguishes them from the
  // unsupported-type case above.
  assert!(retryable, "http transport failures should be retryable");
}

#[tokio::test]
async fn mcp_payload_routes_to_mcp_node_dispatcher() {
  // Point at a binary that will never exist so we exercise the
  // dispatcher without spawning a real MCP server. Connection
  // failure is the expected outcome; "unsupported node type" is not.
  let mut inputs = HashMap::new();
  inputs.insert(
    "server_command".to_string(),
    FlowValue::Json(json!(["/does/not/exist/agentflow-mcp-stub", "--unused"])),
  );
  inputs.insert("tool_name".to_string(), FlowValue::Json(json!("noop")));
  inputs.insert("tool_params".to_string(), FlowValue::Json(json!({})));
  inputs.insert("timeout_ms".to_string(), FlowValue::Json(json!(500u64)));
  inputs.insert("max_retries".to_string(), FlowValue::Json(json!(0u64)));
  let payload = NodeExecutionPayload::new("invoke", "mcp", HashMap::new(), inputs);

  let WorkerTaskResult::Failed { error, .. } = run_payload("worker-mcp", payload).await else {
    panic!("expected mcp dispatch failure (no server binary)");
  };
  assert!(
    !error.contains("does not support node type"),
    "mcp payload should have routed to MCPNode; error was: {error}"
  );
}

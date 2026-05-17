//! P5.6 — distributed worker resource limits + cancellation.
//!
//! These tests exercise the worker-level guarantees:
//!
//! - Per-node timeout cuts off a runaway dispatch.
//! - Cancellation propagates through the dispatcher and surfaces as
//!   a non-retryable cancellation failure.
//! - Output size limit truncates the result envelope and emits the
//!   `worker.task.output_truncated` trace event.
//! - Retry semantics are honored (a retryable failure followed by a
//!   timeout still terminates after the attempt budget).
//!
//! The hermetic runaway fixture rides on the `mock` payload's
//! `sleep_ms` / `output_size_bytes` knobs — see
//! `execute_mock_payload` in `agentflow-worker::lib`.

use std::collections::HashMap;
use std::time::Duration;

use agentflow_server::{
  InMemoryWorkerProtocol, NodeExecutionPayload, WorkerId, WorkerProtocol, WorkerTask,
  WorkerTaskResult,
};
use agentflow_worker::{
  WorkerCancellationToken, WorkerConfig, WorkerResourceLimits, WorkerRuntime,
};
use serde_json::json;
use uuid::Uuid;

fn worker_id(label: &str) -> WorkerId {
  WorkerId::new(label).expect("valid worker label")
}

fn runaway_payload(node_id: &str, sleep_ms: u64) -> NodeExecutionPayload {
  NodeExecutionPayload::new(
    node_id,
    "mock",
    HashMap::from([("sleep_ms".to_string(), json!(sleep_ms))]),
    HashMap::new(),
  )
}

fn big_output_payload(node_id: &str, output_size_bytes: u64) -> NodeExecutionPayload {
  NodeExecutionPayload::new(
    node_id,
    "mock",
    HashMap::from([("output_size_bytes".to_string(), json!(output_size_bytes))]),
    HashMap::new(),
  )
}

async fn submit_and_run(
  protocol: &InMemoryWorkerProtocol,
  runtime: &WorkerRuntime<InMemoryWorkerProtocol>,
  payload: NodeExecutionPayload,
) -> WorkerTaskResult {
  let node_id = payload.node_id.clone();
  let task = WorkerTask::new(
    Uuid::new_v4(),
    node_id,
    serde_json::to_value(payload).expect("payload serializes"),
  );
  let task_id = task.task_id;
  protocol.submit_task(task).await.expect("submit");
  runtime.run_once().await.expect("run_once");
  protocol
    .completed_result(task_id)
    .await
    .expect("result recorded")
}

#[tokio::test(flavor = "current_thread")]
async fn runaway_node_is_cut_off_by_default_timeout() {
  let protocol = InMemoryWorkerProtocol::new();
  let limits = WorkerResourceLimits {
    default_timeout: Some(Duration::from_millis(150)),
    max_output_bytes: None,
  };
  let config =
    WorkerConfig::new(worker_id("worker-timeout"), "memory://local").with_resource_limits(limits);
  let runtime = WorkerRuntime::new(protocol.clone(), config);

  let result = submit_and_run(&protocol, &runtime, runaway_payload("runaway", 10_000)).await;
  let WorkerTaskResult::Failed {
    error, retryable, ..
  } = result
  else {
    panic!("expected runaway dispatch to fail, got {result:?}");
  };
  assert!(
    error.contains("distributed worker timeout"),
    "error should mention worker timeout, got: {error}"
  );
  // Timeouts are AsyncExecutionError → retryable; that gives the
  // scheduler the option to reattempt elsewhere or with a longer
  // budget.
  assert!(retryable, "timeout should be retryable");
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_short_circuits_in_flight_dispatch() {
  let protocol = InMemoryWorkerProtocol::new();
  let limits = WorkerResourceLimits {
    // Generous timeout so the cancellation path wins, not the timeout.
    default_timeout: Some(Duration::from_secs(60)),
    max_output_bytes: None,
  };
  let config =
    WorkerConfig::new(worker_id("worker-cancel"), "memory://local").with_resource_limits(limits);
  let cancel = WorkerCancellationToken::new();
  let runtime = WorkerRuntime::new(protocol.clone(), config).with_cancellation(cancel.clone());

  // Queue a slow task.
  let payload = runaway_payload("cancel-me", 5_000);
  let task = WorkerTask::new(
    Uuid::new_v4(),
    payload.node_id.clone(),
    serde_json::to_value(payload).expect("payload serializes"),
  );
  let task_id = task.task_id;
  protocol.submit_task(task).await.expect("submit");

  // Fire cancellation just after the worker starts. We can use a
  // separate task because the cancellation flag is `Arc<AtomicBool>`.
  let cancel_handle = cancel.clone();
  tokio::spawn(async move {
    tokio::time::sleep(Duration::from_millis(60)).await;
    cancel_handle.cancel();
  });

  runtime.run_once().await.expect("run_once");
  let result = protocol
    .completed_result(task_id)
    .await
    .expect("result recorded");

  let WorkerTaskResult::Failed {
    error,
    retryable,
    events,
  } = result
  else {
    panic!("expected cancelled dispatch to fail, got {result:?}");
  };
  assert!(
    error.contains("cancelled mid-dispatch") || error.contains("cancelled before dispatching"),
    "error should mention cancellation, got: {error}"
  );
  assert!(!retryable, "cancellation must be non-retryable");
  assert!(
    events
      .iter()
      .any(|event| event.kind == "worker.task.cancelled"),
    "events should include worker.task.cancelled, got: {events:?}"
  );
}

#[tokio::test(flavor = "current_thread")]
async fn oversized_output_is_truncated_and_traced() {
  let protocol = InMemoryWorkerProtocol::new();
  let limits = WorkerResourceLimits {
    default_timeout: Some(Duration::from_secs(30)),
    max_output_bytes: Some(2_048),
  };
  let config =
    WorkerConfig::new(worker_id("worker-truncate"), "memory://local").with_resource_limits(limits);
  let runtime = WorkerRuntime::new(protocol.clone(), config);

  let result = submit_and_run(&protocol, &runtime, big_output_payload("big", 8_192)).await;
  let WorkerTaskResult::Succeeded { output, events } = result else {
    panic!("oversized output should still succeed, got {result:?}");
  };
  assert_eq!(
    output["truncated"],
    json!(true),
    "output should be replaced with the truncation envelope: {output}"
  );
  assert_eq!(output["limit_bytes"], json!(2_048));
  assert!(
    output["size_bytes"].as_u64().unwrap_or(0) >= 8_192,
    "size_bytes should reflect the pre-truncation size: {output}"
  );
  assert!(
    events
      .iter()
      .any(|event| event.kind == "worker.task.output_truncated"),
    "events should include worker.task.output_truncated: {events:?}"
  );
}

#[tokio::test(flavor = "current_thread")]
async fn retry_semantics_honor_attempt_budget_under_timeout() {
  // A runaway mock node fails with a retryable timeout. The
  // distributed scheduler should request another attempt up to its
  // configured budget, then mark the run failed.
  use agentflow_server::{
    DistributedDagScheduler, DistributedNodeStatus, WorkerControlPlane,
    scheduler::distributed::{mock_flow, mock_node},
  };

  let protocol = InMemoryWorkerProtocol::new();
  let control = WorkerControlPlane::new(protocol);
  let limits = WorkerResourceLimits {
    default_timeout: Some(Duration::from_millis(100)),
    max_output_bytes: None,
  };
  let config =
    WorkerConfig::new(worker_id("worker-retry"), "memory://local").with_resource_limits(limits);
  let runtime = WorkerRuntime::new(control.clone(), config);

  let mut node = mock_node("timeout-loop", Vec::new(), json!("never"));
  node.parameters.insert(
    "sleep_ms".to_string(),
    serde_yaml::to_value(5_000u64).unwrap(),
  );

  let flow = mock_flow("retry timeout mock", vec![node]);
  let run_id = Uuid::new_v4();
  let mut scheduler = DistributedDagScheduler::new(run_id, flow, control.clone())
    .expect("scheduler builds")
    .with_max_attempts(2);

  while !scheduler.is_terminal() {
    scheduler.dispatch_ready().await.expect("dispatch");
    let _ = runtime.run_once().await.expect("run_once");
    scheduler.reconcile_results().await.expect("reconcile");
  }

  let result = scheduler.run_result();
  assert!(!result.succeeded, "run should fail after attempt budget");
  assert_eq!(
    scheduler.node_status("timeout-loop"),
    Some(DistributedNodeStatus::Failed)
  );
}

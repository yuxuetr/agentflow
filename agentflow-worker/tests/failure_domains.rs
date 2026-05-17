//! P5.7 — distributed failure-domain matrix.
//!
//! These tests pin down the six failure paths spelled out in
//! `TODOs.md` P5.7. They exercise the `DistributedDagScheduler`
//! orchestration loop end-to-end against the in-memory transport so
//! the protocol surface and the scheduler's reattempt logic are
//! covered as a single unit.
//!
//! Scenarios:
//!
//! 1. Stale heartbeat → server marks worker dead, redistributes task.
//! 2. Worker crash mid-task → task reattempted on another worker
//!    (modeled as stale heartbeat from a worker that disappears).
//! 3. Retryable failure → retry on the same or a different worker.
//! 4. Non-retryable failure → terminal state, no replay.
//! 5. Duplicate completion → idempotent on `report_result`.
//! 6. Trace stitching across reattempts → single ordered stitched
//!    trace.
//!
//! The `mock` node is the synthetic stress harness:
//! `fail_until_attempt` triggers retryable failures, the
//! `definitely-not-a-real-type` payload triggers a non-retryable
//! definition error, and the existing `sleep_ms` + heartbeat-timeout
//! plumbing covers stale workers.

use std::time::Duration;

use agentflow_server::{
  DistributedDagScheduler, DistributedNodeStatus, InMemoryWorkerProtocol, WorkerControlPlane,
  WorkerHeartbeat, WorkerId, WorkerProtocol, WorkerTask, WorkerTaskResult, WorkerTraceEvent,
  scheduler::distributed::{mock_flow, mock_node},
};
use agentflow_worker::{WorkerConfig, WorkerRuntime};
use chrono::Duration as ChronoDuration;
use serde_json::json;
use uuid::Uuid;

fn worker_id(label: &str) -> WorkerId {
  WorkerId::new(label).expect("valid worker label")
}

fn runtime(
  control: WorkerControlPlane<InMemoryWorkerProtocol>,
  label: &str,
) -> WorkerRuntime<WorkerControlPlane<InMemoryWorkerProtocol>> {
  WorkerRuntime::new(
    control,
    WorkerConfig::new(worker_id(label), "memory://local"),
  )
}

#[tokio::test(flavor = "current_thread")]
async fn stale_heartbeat_redistributes_to_another_worker() {
  let protocol = InMemoryWorkerProtocol::new();
  let control = WorkerControlPlane::new(protocol);
  let run_id = Uuid::new_v4();
  let flow = mock_flow(
    "stale heartbeat",
    vec![mock_node("flaky", Vec::new(), json!("done"))],
  );
  let mut scheduler = DistributedDagScheduler::new(run_id, flow, control.clone())
    .expect("scheduler builds")
    .with_max_attempts(2)
    .with_heartbeat_timeout(Duration::from_millis(1));

  scheduler.dispatch_ready().await.expect("dispatch");

  // Worker A claims, then "disappears" — heartbeat stuck in the past.
  let worker_a = worker_id("worker-a");
  let claimed = control
    .claim_task(worker_a.clone())
    .await
    .expect("claim")
    .expect("task available");
  control
    .heartbeat(WorkerHeartbeat {
      worker_id: worker_a.clone(),
      active_task: Some(claimed.task_id),
      free_slots: 0,
      ts: chrono::Utc::now() - ChronoDuration::seconds(10),
    })
    .await
    .expect("heartbeat");

  let requeued = scheduler
    .requeue_stale_tasks()
    .await
    .expect("requeue stale");
  assert_eq!(requeued, 1, "scheduler must requeue exactly one stale task");
  assert_eq!(
    scheduler.node_status("flaky"),
    Some(DistributedNodeStatus::Pending)
  );

  // Worker B picks it up and completes it.
  let worker_b_runtime = runtime(control.clone(), "worker-b");
  scheduler.dispatch_ready().await.expect("dispatch");
  worker_b_runtime.run_once().await.expect("run_once");
  scheduler.reconcile_results().await.expect("reconcile");

  let result = scheduler.run_result();
  assert!(result.succeeded, "task must complete on the second worker");
}

#[tokio::test(flavor = "current_thread")]
async fn worker_crash_midtask_is_reattempted_elsewhere() {
  // We can't actually crash a worker process, but the externally
  // observable signal is identical to a stale heartbeat: the
  // assignment lingers and the heartbeat ages past the timeout. This
  // test asserts the scheduler completes the run on a *different*
  // worker after the first one disappears mid-task.
  let protocol = InMemoryWorkerProtocol::new();
  let control = WorkerControlPlane::new(protocol);
  let run_id = Uuid::new_v4();
  let flow = mock_flow(
    "crash midtask",
    vec![mock_node("survivor", Vec::new(), json!("done"))],
  );
  let mut scheduler = DistributedDagScheduler::new(run_id, flow, control.clone())
    .expect("scheduler builds")
    .with_max_attempts(2)
    .with_heartbeat_timeout(Duration::from_millis(1));

  scheduler.dispatch_ready().await.expect("dispatch");

  let dead = worker_id("worker-dead");
  let alive = worker_id("worker-alive");
  let claimed = control
    .claim_task(dead.clone())
    .await
    .expect("dead claim")
    .expect("task available");
  // Single stale heartbeat then silence — the worker "crashed".
  control
    .heartbeat(WorkerHeartbeat {
      worker_id: dead.clone(),
      active_task: Some(claimed.task_id),
      free_slots: 0,
      ts: chrono::Utc::now() - ChronoDuration::seconds(10),
    })
    .await
    .expect("heartbeat");

  scheduler
    .requeue_stale_tasks()
    .await
    .expect("requeue stale");
  scheduler.dispatch_ready().await.expect("redispatch");

  // The reaped task is now waiting on the queue. The surviving
  // worker should claim and complete it.
  let alive_runtime = WorkerRuntime::new(
    control.clone(),
    WorkerConfig::new(alive.clone(), "memory://local"),
  );
  alive_runtime.run_once().await.expect("run_once");
  scheduler.reconcile_results().await.expect("reconcile");

  let result = scheduler.run_result();
  assert!(result.succeeded);
  let snapshot = control.run_snapshot(run_id).await.expect("snapshot");
  // The successful attempt's stitched trace records the surviving
  // worker's ownership.
  assert!(
    snapshot
      .stitched_trace_events
      .iter()
      .any(|e| e.worker_id == alive),
    "stitched trace must include surviving worker events"
  );
}

#[tokio::test(flavor = "current_thread")]
async fn retryable_failure_retries_on_another_worker() {
  let protocol = InMemoryWorkerProtocol::new();
  let control = WorkerControlPlane::new(protocol);
  let run_id = Uuid::new_v4();

  let mut node = mock_node("retry-me", Vec::new(), json!("eventually"));
  node.parameters.insert(
    "fail_until_attempt".to_string(),
    serde_yaml::to_value(1u64).unwrap(),
  );
  let flow = mock_flow("retry across workers", vec![node]);

  let mut scheduler = DistributedDagScheduler::new(run_id, flow, control.clone())
    .expect("scheduler builds")
    .with_max_attempts(2);

  let worker_a = runtime(control.clone(), "worker-a");
  let worker_b = runtime(control.clone(), "worker-b");

  while !scheduler.is_terminal() {
    scheduler.dispatch_ready().await.expect("dispatch");
    let _ = worker_a.run_once().await.expect("worker_a run");
    let _ = worker_b.run_once().await.expect("worker_b run");
    scheduler.reconcile_results().await.expect("reconcile");
  }

  let result = scheduler.run_result();
  assert!(result.succeeded);
  let snapshot = control.run_snapshot(run_id).await.expect("snapshot");
  assert_eq!(snapshot.failed_tasks, 1, "first attempt must fail");
  assert_eq!(snapshot.succeeded_tasks, 1, "second attempt must succeed");
}

#[tokio::test(flavor = "current_thread")]
async fn non_retryable_failure_is_terminal() {
  let protocol = InMemoryWorkerProtocol::new();
  let control = WorkerControlPlane::new(protocol);
  let run_id = Uuid::new_v4();

  // Unknown node type → FlowDefinitionError → retryable: false.
  let mut bad_node = mock_node("bad", Vec::new(), json!("never"));
  bad_node.node_type = "definitely-not-a-real-type".to_string();
  let flow = mock_flow("non-retryable", vec![bad_node]);

  let mut scheduler = DistributedDagScheduler::new(run_id, flow, control.clone())
    .expect("scheduler builds")
    // High budget on purpose: terminal must NOT consume retries.
    .with_max_attempts(5);
  let worker = runtime(control.clone(), "worker-only");

  while !scheduler.is_terminal() {
    scheduler.dispatch_ready().await.expect("dispatch");
    let _ = worker.run_once().await.expect("run_once");
    scheduler.reconcile_results().await.expect("reconcile");
  }

  let result = scheduler.run_result();
  assert!(!result.succeeded);
  assert_eq!(
    scheduler.node_status("bad"),
    Some(DistributedNodeStatus::Failed)
  );
  let snapshot = control.run_snapshot(run_id).await.expect("snapshot");
  assert_eq!(
    snapshot.failed_tasks, 1,
    "non-retryable failure must record exactly one attempt"
  );
}

#[tokio::test(flavor = "current_thread")]
async fn duplicate_completion_is_idempotent() {
  // `report_result` is invoked via the transport once per worker
  // outcome. If the wire layer (e.g. gRPC retry) delivers the same
  // result twice, the inner protocol should reject the second call
  // rather than double-count outputs. The in-memory protocol's
  // assignment table is the source of truth.
  let protocol = InMemoryWorkerProtocol::new();
  let run_id = Uuid::new_v4();
  let task = WorkerTask::new(run_id, "once-only", json!({"input": 1}));
  let task_id = task.task_id;
  protocol.submit_task(task).await.expect("submit");

  let worker = worker_id("worker-once");
  let _claimed = protocol
    .claim_task(worker.clone())
    .await
    .expect("claim")
    .expect("task available");

  let result = WorkerTaskResult::Succeeded {
    output: json!({"answer": "ok"}),
    events: vec![WorkerTraceEvent {
      seq: 0,
      kind: "worker.task.completed".into(),
      payload: json!({}),
    }],
  };
  protocol
    .report_result(worker.clone(), task_id, result.clone())
    .await
    .expect("first report ok");

  // Second report for the same task → the protocol must reject it
  // so the run accounting stays consistent.
  let duplicate = protocol.report_result(worker, task_id, result).await;
  assert!(
    duplicate.is_err(),
    "duplicate completion must be rejected by the protocol layer"
  );
}

#[tokio::test(flavor = "current_thread")]
async fn trace_stitching_preserves_both_attempts() {
  let protocol = InMemoryWorkerProtocol::new();
  let control = WorkerControlPlane::new(protocol);
  let run_id = Uuid::new_v4();

  let mut node = mock_node("retry-trace", Vec::new(), json!("ok"));
  node.parameters.insert(
    "fail_until_attempt".to_string(),
    serde_yaml::to_value(1u64).unwrap(),
  );
  let flow = mock_flow("retry trace", vec![node]);

  let mut scheduler = DistributedDagScheduler::new(run_id, flow, control.clone())
    .expect("scheduler builds")
    .with_max_attempts(2);
  let worker = runtime(control.clone(), "worker-trace");

  while !scheduler.is_terminal() {
    scheduler.dispatch_ready().await.expect("dispatch");
    let _ = worker.run_once().await.expect("run_once");
    scheduler.reconcile_results().await.expect("reconcile");
  }

  let snapshot = control.run_snapshot(run_id).await.expect("snapshot");
  // Two attempts → at least one started + one failed/completed
  // pair from each. The stitched stream is monotonically ordered by
  // `global_seq`.
  let started_events = snapshot
    .stitched_trace_events
    .iter()
    .filter(|e| e.kind == "worker.task.started")
    .count();
  let terminal_events = snapshot
    .stitched_trace_events
    .iter()
    .filter(|e| e.kind == "worker.task.completed" || e.kind == "worker.task.failed")
    .count();
  assert!(
    started_events >= 2,
    "stitched trace should record both attempt starts; got {started_events}"
  );
  assert!(
    terminal_events >= 2,
    "stitched trace should record both attempt terminals; got {terminal_events}"
  );

  let mut seqs: Vec<i64> = snapshot
    .stitched_trace_events
    .iter()
    .map(|e| e.global_seq)
    .collect();
  let original = seqs.clone();
  seqs.sort_unstable();
  assert_eq!(
    seqs, original,
    "stitched events must already be sorted by global_seq"
  );
}

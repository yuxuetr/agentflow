//! Distributed scheduler protocol boundaries.
//!
//! The first distributed milestone keeps transport concerns out of the
//! control plane. [`WorkerProtocol`] defines the semantics the server needs:
//! enqueue work, let workers claim work, accept results, and track heartbeats.
//! A later adapter can expose the same contract over gRPC, NATS, or Redis
//! Streams without changing run routes.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Transport selected for the v1.0-rc distributed control plane.
pub const SELECTED_TRANSPORT: WorkerTransport = WorkerTransport::Grpc;

/// Distributed worker transport options considered by the design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerTransport {
  /// Primary v1.0-rc path: HTTP/2 streaming RPC via tonic.
  Grpc,
  /// Reserved for high-throughput event-bus deployments.
  Nats,
  /// Reserved for teams that already run Redis as infrastructure.
  RedisStreams,
}

impl WorkerTransport {
  pub fn as_str(self) -> &'static str {
    match self {
      Self::Grpc => "grpc",
      Self::Nats => "nats",
      Self::RedisStreams => "redis_streams",
    }
  }
}

/// Stable worker identity supplied by the worker process at startup.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkerId(pub String);

impl WorkerId {
  pub fn new(value: impl Into<String>) -> Result<Self, SchedulerError> {
    let value = value.into();
    if value.trim().is_empty() {
      return Err(SchedulerError::InvalidWorkerId);
    }
    Ok(Self(value))
  }
}

/// One schedulable unit of distributed work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerTask {
  pub task_id: Uuid,
  pub run_id: Uuid,
  pub node_id: String,
  pub attempt: u32,
  pub payload: serde_json::Value,
}

impl WorkerTask {
  pub fn new(run_id: Uuid, node_id: impl Into<String>, payload: serde_json::Value) -> Self {
    Self {
      task_id: Uuid::new_v4(),
      run_id,
      node_id: node_id.into(),
      attempt: 0,
      payload,
    }
  }
}

/// Task execution result sent from a worker back to the control plane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WorkerTaskResult {
  Succeeded {
    output: serde_json::Value,
    events: Vec<WorkerTraceEvent>,
  },
  Failed {
    error: String,
    retryable: bool,
    events: Vec<WorkerTraceEvent>,
  },
}

impl WorkerTaskResult {
  pub fn events(&self) -> &[WorkerTraceEvent] {
    match self {
      Self::Succeeded { events, .. } | Self::Failed { events, .. } => events,
    }
  }
}

/// Trace fragment emitted by a worker. The control plane persists these with
/// the run and later maps them into OTel spans.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerTraceEvent {
  pub seq: i64,
  pub kind: String,
  pub payload: serde_json::Value,
}

/// Worker heartbeat payload. `active_task` is `None` when the worker is idle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerHeartbeat {
  pub worker_id: WorkerId,
  pub active_task: Option<Uuid>,
  pub free_slots: u32,
  pub ts: DateTime<Utc>,
}

impl WorkerHeartbeat {
  pub fn now(worker_id: WorkerId, active_task: Option<Uuid>, free_slots: u32) -> Self {
    Self {
      worker_id,
      active_task,
      free_slots,
      ts: Utc::now(),
    }
  }
}

/// Protocol boundary between the server control plane and worker processes.
#[async_trait]
pub trait WorkerProtocol: Send + Sync {
  /// Submit one task to the distributed queue.
  async fn submit_task(&self, task: WorkerTask) -> Result<(), SchedulerError>;

  /// Claim the next available task for `worker_id`.
  async fn claim_task(&self, worker_id: WorkerId) -> Result<Option<WorkerTask>, SchedulerError>;

  /// Report a terminal result for a task previously claimed by a worker.
  async fn report_result(
    &self,
    worker_id: WorkerId,
    task_id: Uuid,
    result: WorkerTaskResult,
  ) -> Result<(), SchedulerError>;

  /// Record liveness and current capacity for a worker.
  async fn heartbeat(&self, heartbeat: WorkerHeartbeat) -> Result<(), SchedulerError>;
}

/// In-memory implementation used for unit tests and local control-plane
/// prototyping. It is intentionally single-process and not durable.
#[derive(Debug, Clone, Default)]
pub struct InMemoryWorkerProtocol {
  state: Arc<Mutex<InMemoryState>>,
}

#[derive(Debug, Default)]
struct InMemoryState {
  queued: VecDeque<WorkerTask>,
  claimed: HashMap<Uuid, ClaimedTask>,
  completed: HashMap<Uuid, CompletedTask>,
  heartbeats: HashMap<WorkerId, WorkerHeartbeat>,
}

#[derive(Debug)]
struct ClaimedTask {
  worker_id: WorkerId,
}

#[derive(Debug)]
struct CompletedTask {
  worker_id: WorkerId,
  result: WorkerTaskResult,
}

impl InMemoryWorkerProtocol {
  pub fn new() -> Self {
    Self::default()
  }

  /// Snapshot the last heartbeat seen for a worker.
  pub async fn last_heartbeat(&self, worker_id: &WorkerId) -> Option<WorkerHeartbeat> {
    self.state.lock().await.heartbeats.get(worker_id).cloned()
  }

  /// Snapshot a completed task result. This is test/debug support, not part
  /// of the distributed protocol contract.
  pub async fn completed_result(&self, task_id: Uuid) -> Option<WorkerTaskResult> {
    self
      .state
      .lock()
      .await
      .completed
      .get(&task_id)
      .map(|completed| completed.result.clone())
  }

  /// Snapshot the worker that completed a task.
  pub async fn completed_by(&self, task_id: Uuid) -> Option<WorkerId> {
    self
      .state
      .lock()
      .await
      .completed
      .get(&task_id)
      .map(|completed| completed.worker_id.clone())
  }
}

#[async_trait]
impl WorkerProtocol for InMemoryWorkerProtocol {
  async fn submit_task(&self, task: WorkerTask) -> Result<(), SchedulerError> {
    self.state.lock().await.queued.push_back(task);
    Ok(())
  }

  async fn claim_task(&self, worker_id: WorkerId) -> Result<Option<WorkerTask>, SchedulerError> {
    let mut state = self.state.lock().await;
    let Some(task) = state.queued.pop_front() else {
      return Ok(None);
    };
    state
      .claimed
      .insert(task.task_id, ClaimedTask { worker_id });
    Ok(Some(task))
  }

  async fn report_result(
    &self,
    worker_id: WorkerId,
    task_id: Uuid,
    result: WorkerTaskResult,
  ) -> Result<(), SchedulerError> {
    let mut state = self.state.lock().await;
    let Some(claimed) = state.claimed.remove(&task_id) else {
      return Err(SchedulerError::TaskNotClaimed { task_id });
    };
    if claimed.worker_id != worker_id {
      state.claimed.insert(task_id, claimed);
      return Err(SchedulerError::WorkerMismatch { task_id });
    }
    state
      .completed
      .insert(task_id, CompletedTask { worker_id, result });
    Ok(())
  }

  async fn heartbeat(&self, heartbeat: WorkerHeartbeat) -> Result<(), SchedulerError> {
    self
      .state
      .lock()
      .await
      .heartbeats
      .insert(heartbeat.worker_id.clone(), heartbeat);
    Ok(())
  }
}

/// Scheduler/protocol errors. These are intentionally transport-neutral.
#[derive(Debug, Error)]
pub enum SchedulerError {
  #[error("worker id must not be empty")]
  InvalidWorkerId,
  #[error("task {task_id} has not been claimed")]
  TaskNotClaimed { task_id: Uuid },
  #[error("task {task_id} was claimed by a different worker")]
  WorkerMismatch { task_id: Uuid },
  #[error("transport error: {message}")]
  Transport { message: String },
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn selected_transport_is_grpc() {
    assert_eq!(SELECTED_TRANSPORT.as_str(), "grpc");
  }

  #[test]
  fn worker_id_rejects_blank_values() {
    assert!(matches!(
      WorkerId::new("  "),
      Err(SchedulerError::InvalidWorkerId)
    ));
  }

  #[tokio::test]
  async fn in_memory_protocol_claims_tasks_fifo() {
    let protocol = InMemoryWorkerProtocol::new();
    let worker = WorkerId::new("worker-a").expect("valid worker");
    let run_id = Uuid::new_v4();
    let first = WorkerTask::new(run_id, "node_a", serde_json::json!({"n": 1}));
    let second = WorkerTask::new(run_id, "node_b", serde_json::json!({"n": 2}));

    protocol
      .submit_task(first.clone())
      .await
      .expect("submit first");
    protocol
      .submit_task(second.clone())
      .await
      .expect("submit second");

    assert_eq!(
      protocol
        .claim_task(worker.clone())
        .await
        .expect("claim first"),
      Some(first)
    );
    assert_eq!(
      protocol.claim_task(worker).await.expect("claim second"),
      Some(second)
    );
  }

  #[tokio::test]
  async fn in_memory_protocol_records_results_for_claiming_worker() {
    let protocol = InMemoryWorkerProtocol::new();
    let worker = WorkerId::new("worker-a").expect("valid worker");
    let task = WorkerTask::new(
      Uuid::new_v4(),
      "node_a",
      serde_json::json!({"input": "hello"}),
    );
    let task_id = task.task_id;
    protocol.submit_task(task).await.expect("submit");
    protocol
      .claim_task(worker.clone())
      .await
      .expect("claim")
      .expect("task");

    let result = WorkerTaskResult::Succeeded {
      output: serde_json::json!({"ok": true}),
      events: vec![WorkerTraceEvent {
        seq: 0,
        kind: "node_completed".into(),
        payload: serde_json::json!({"worker": "worker-a"}),
      }],
    };
    protocol
      .report_result(worker.clone(), task_id, result.clone())
      .await
      .expect("report");

    assert_eq!(protocol.completed_result(task_id).await, Some(result));
    assert_eq!(protocol.completed_by(task_id).await, Some(worker));
  }

  #[tokio::test]
  async fn in_memory_protocol_rejects_result_from_wrong_worker() {
    let protocol = InMemoryWorkerProtocol::new();
    let worker_a = WorkerId::new("worker-a").expect("valid worker");
    let worker_b = WorkerId::new("worker-b").expect("valid worker");
    let task = WorkerTask::new(Uuid::new_v4(), "node_a", serde_json::json!({}));
    let task_id = task.task_id;
    protocol.submit_task(task).await.expect("submit");
    protocol
      .claim_task(worker_a)
      .await
      .expect("claim")
      .expect("task");

    let err = protocol
      .report_result(
        worker_b,
        task_id,
        WorkerTaskResult::Failed {
          error: "boom".into(),
          retryable: true,
          events: Vec::new(),
        },
      )
      .await
      .expect_err("wrong worker must fail");

    assert!(matches!(err, SchedulerError::WorkerMismatch { .. }));
  }

  #[tokio::test]
  async fn in_memory_protocol_records_heartbeats() {
    let protocol = InMemoryWorkerProtocol::new();
    let worker = WorkerId::new("worker-a").expect("valid worker");
    let heartbeat = WorkerHeartbeat::now(worker.clone(), None, 4);

    protocol
      .heartbeat(heartbeat.clone())
      .await
      .expect("heartbeat");

    assert_eq!(protocol.last_heartbeat(&worker).await, Some(heartbeat));
  }
}

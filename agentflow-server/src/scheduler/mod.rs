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

use agentflow_tracing::{OtelAttribute, OtelSpan, OtelSpanEvent, OtelSpanKind, OtelStatus};

pub mod admission;
pub mod distributed;
pub mod grpc;

pub use admission::{
  AdmissionError, AuthenticatedControlPlane, ControlError, WorkerAdmissionPolicy, WorkerCredential,
};
pub use distributed::{
  DistributedDagRunResult, DistributedDagScheduler, DistributedNodeStatus, NodeExecutionPayload,
};
pub use grpc::{GrpcWorkerProtocol, GrpcWorkerService, WorkerControlServer};

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

  pub fn with_attempt(
    run_id: Uuid,
    node_id: impl Into<String>,
    attempt: u32,
    payload: serde_json::Value,
  ) -> Self {
    Self {
      task_id: Uuid::new_v4(),
      run_id,
      node_id: node_id.into(),
      attempt,
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

/// Worker trace event after the control plane assigns global ordering and
/// execution ownership metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StitchedWorkerTraceEvent {
  pub global_seq: i64,
  pub task_id: Uuid,
  pub worker_id: WorkerId,
  pub run_id: Uuid,
  pub node_id: String,
  pub attempt: u32,
  pub local_seq: i64,
  pub kind: String,
  pub payload: serde_json::Value,
  pub ts: DateTime<Utc>,
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

/// Lightweight control-plane façade over a [`WorkerProtocol`].
///
/// This is the first server-side scheduling layer: it dispatches tasks into
/// the protocol, records which worker claimed each task, aggregates terminal
/// results per run, and keeps worker trace fragments available for later DB /
/// OTel persistence wiring.
#[derive(Debug, Clone)]
pub struct WorkerControlPlane<P> {
  protocol: P,
  state: Arc<Mutex<ControlPlaneState>>,
}

#[derive(Debug, Default)]
struct ControlPlaneState {
  runs: HashMap<Uuid, RunControlSnapshot>,
  assignments: HashMap<Uuid, WorkerAssignment>,
  heartbeats: HashMap<WorkerId, WorkerHeartbeat>,
}

impl<P> WorkerControlPlane<P>
where
  P: WorkerProtocol,
{
  pub fn new(protocol: P) -> Self {
    Self {
      protocol,
      state: Arc::new(Mutex::new(ControlPlaneState::default())),
    }
  }

  /// Queue a task and update the run-level control-plane counters.
  pub async fn schedule_task(&self, task: WorkerTask) -> Result<(), SchedulerError> {
    self.protocol.submit_task(task.clone()).await?;
    let mut state = self.state.lock().await;
    let run = state
      .runs
      .entry(task.run_id)
      .or_insert_with(|| RunControlSnapshot::new(task.run_id));
    run.queued_tasks += 1;
    run.status = RunControlStatus::Queued;
    Ok(())
  }

  /// Claim a task for a worker and mark it running in the control plane.
  pub async fn claim_task(
    &self,
    worker_id: WorkerId,
  ) -> Result<Option<WorkerTask>, SchedulerError> {
    let Some(task) = self.protocol.claim_task(worker_id.clone()).await? else {
      return Ok(None);
    };
    let mut state = self.state.lock().await;
    state.assignments.insert(
      task.task_id,
      WorkerAssignment {
        task_id: task.task_id,
        worker_id,
        run_id: task.run_id,
        node_id: task.node_id.clone(),
        attempt: task.attempt,
      },
    );
    let run = state
      .runs
      .entry(task.run_id)
      .or_insert_with(|| RunControlSnapshot::new(task.run_id));
    run.queued_tasks = run.queued_tasks.saturating_sub(1);
    run.running_tasks += 1;
    run.status = RunControlStatus::Running;
    Ok(Some(task))
  }

  /// Submit a worker result, aggregate run counters, and append worker trace
  /// fragments to the run snapshot.
  pub async fn report_result(
    &self,
    worker_id: WorkerId,
    task_id: Uuid,
    result: WorkerTaskResult,
  ) -> Result<(), SchedulerError> {
    let assignment = {
      let state = self.state.lock().await;
      state
        .assignments
        .get(&task_id)
        .cloned()
        .ok_or(SchedulerError::TaskNotClaimed { task_id })?
    };

    self
      .protocol
      .report_result(worker_id, task_id, result.clone())
      .await?;

    let mut state = self.state.lock().await;
    state.assignments.remove(&task_id);
    let run = state
      .runs
      .entry(assignment.run_id)
      .or_insert_with(|| RunControlSnapshot::new(assignment.run_id));
    run.running_tasks = run.running_tasks.saturating_sub(1);
    let next_global_seq = run.stitched_trace_events.len() as i64;
    let stitched_at = Utc::now();
    run
      .stitched_trace_events
      .extend(
        result
          .events()
          .iter()
          .enumerate()
          .map(|(idx, event)| StitchedWorkerTraceEvent {
            global_seq: next_global_seq + idx as i64,
            task_id,
            worker_id: assignment.worker_id.clone(),
            run_id: assignment.run_id,
            node_id: assignment.node_id.clone(),
            attempt: assignment.attempt,
            local_seq: event.seq,
            kind: event.kind.clone(),
            payload: event.payload.clone(),
            ts: stitched_at,
          }),
      );
    run.trace_events.extend(result.events().iter().cloned());
    match result {
      WorkerTaskResult::Succeeded { output, .. } => {
        run.succeeded_tasks += 1;
        run.outputs.insert(assignment.node_id, output);
      }
      WorkerTaskResult::Failed {
        error, retryable, ..
      } => {
        let node_id = assignment.node_id;
        run.failed_tasks += 1;
        run.last_error = Some(error.clone());
        run.retryable_failures += usize::from(retryable);
        run.failures.insert(
          node_id.clone(),
          WorkerTaskFailure {
            task_id,
            worker_id: assignment.worker_id,
            run_id: assignment.run_id,
            node_id,
            attempt: assignment.attempt,
            error,
            retryable,
          },
        );
      }
    }
    run.status = run.derive_status();
    Ok(())
  }

  /// Record a worker heartbeat in both the protocol and control-plane state.
  pub async fn heartbeat(&self, heartbeat: WorkerHeartbeat) -> Result<(), SchedulerError> {
    self.protocol.heartbeat(heartbeat.clone()).await?;
    self
      .state
      .lock()
      .await
      .heartbeats
      .insert(heartbeat.worker_id.clone(), heartbeat);
    Ok(())
  }

  pub async fn run_snapshot(&self, run_id: Uuid) -> Option<RunControlSnapshot> {
    self.state.lock().await.runs.get(&run_id).cloned()
  }

  /// Return the stitched cross-worker trace for one run.
  pub async fn stitched_trace(&self, run_id: Uuid) -> Vec<StitchedWorkerTraceEvent> {
    self
      .state
      .lock()
      .await
      .runs
      .get(&run_id)
      .map(|run| run.stitched_trace_events.clone())
      .unwrap_or_default()
  }

  /// Return OpenTelemetry boundary spans derived from the stitched worker
  /// trace for one run.
  pub async fn stitched_otel_spans(&self, run_id: Uuid) -> Vec<OtelSpan> {
    stitched_trace_to_otel_spans(run_id, &self.stitched_trace(run_id).await)
  }

  pub async fn worker_heartbeat(&self, worker_id: &WorkerId) -> Option<WorkerHeartbeat> {
    self.state.lock().await.heartbeats.get(worker_id).cloned()
  }

  pub async fn assignments_for_run(&self, run_id: Uuid) -> Vec<WorkerAssignment> {
    self
      .state
      .lock()
      .await
      .assignments
      .values()
      .filter(|assignment| assignment.run_id == run_id)
      .cloned()
      .collect()
  }

  pub async fn forget_assignment(&self, task_id: Uuid) -> Option<WorkerAssignment> {
    let mut state = self.state.lock().await;
    let assignment = state.assignments.remove(&task_id)?;
    if let Some(run) = state.runs.get_mut(&assignment.run_id) {
      run.running_tasks = run.running_tasks.saturating_sub(1);
    }
    Some(assignment)
  }
}

#[async_trait]
impl<P> WorkerProtocol for WorkerControlPlane<P>
where
  P: WorkerProtocol + Clone,
{
  async fn submit_task(&self, task: WorkerTask) -> Result<(), SchedulerError> {
    WorkerControlPlane::schedule_task(self, task).await
  }

  async fn claim_task(&self, worker_id: WorkerId) -> Result<Option<WorkerTask>, SchedulerError> {
    WorkerControlPlane::claim_task(self, worker_id).await
  }

  async fn report_result(
    &self,
    worker_id: WorkerId,
    task_id: Uuid,
    result: WorkerTaskResult,
  ) -> Result<(), SchedulerError> {
    WorkerControlPlane::report_result(self, worker_id, task_id, result).await
  }

  async fn heartbeat(&self, heartbeat: WorkerHeartbeat) -> Result<(), SchedulerError> {
    WorkerControlPlane::heartbeat(self, heartbeat).await
  }
}

/// Worker assignment tracked by the control plane after a claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerAssignment {
  pub task_id: Uuid,
  pub worker_id: WorkerId,
  pub run_id: Uuid,
  pub node_id: String,
  pub attempt: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerTaskFailure {
  pub task_id: Uuid,
  pub worker_id: WorkerId,
  pub run_id: Uuid,
  pub node_id: String,
  pub attempt: u32,
  pub error: String,
  pub retryable: bool,
}

/// Run status derived from distributed task counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunControlStatus {
  Queued,
  Running,
  Succeeded,
  Failed,
}

/// In-memory snapshot of one distributed run from the control-plane view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunControlSnapshot {
  pub run_id: Uuid,
  pub status: RunControlStatus,
  pub queued_tasks: usize,
  pub running_tasks: usize,
  pub succeeded_tasks: usize,
  pub failed_tasks: usize,
  pub retryable_failures: usize,
  pub last_error: Option<String>,
  pub outputs: HashMap<String, serde_json::Value>,
  pub failures: HashMap<String, WorkerTaskFailure>,
  pub trace_events: Vec<WorkerTraceEvent>,
  pub stitched_trace_events: Vec<StitchedWorkerTraceEvent>,
}

impl RunControlSnapshot {
  fn new(run_id: Uuid) -> Self {
    Self {
      run_id,
      status: RunControlStatus::Queued,
      queued_tasks: 0,
      running_tasks: 0,
      succeeded_tasks: 0,
      failed_tasks: 0,
      retryable_failures: 0,
      last_error: None,
      outputs: HashMap::new(),
      failures: HashMap::new(),
      trace_events: Vec::new(),
      stitched_trace_events: Vec::new(),
    }
  }

  fn derive_status(&self) -> RunControlStatus {
    if self.failed_tasks > 0 {
      RunControlStatus::Failed
    } else if self.running_tasks > 0 {
      RunControlStatus::Running
    } else if self.queued_tasks > 0 {
      RunControlStatus::Queued
    } else {
      RunControlStatus::Succeeded
    }
  }
}

/// Convert stitched worker trace events into OpenTelemetry boundary spans.
///
/// The output contains one distributed-run root span plus one child span per
/// task attempt. Worker-local trace fragments are preserved as span events in
/// global order.
pub fn stitched_trace_to_otel_spans(
  run_id: Uuid,
  events: &[StitchedWorkerTraceEvent],
) -> Vec<OtelSpan> {
  let trace_id = hex_hash(&format!("distributed-run:{run_id}"), 16);
  let root_span_id = hex_hash(&format!("distributed-run:{run_id}:root"), 8);
  let start = events
    .first()
    .map(|event| unix_nanos(event.ts))
    .unwrap_or_default();
  let end = events
    .last()
    .map(|event| unix_nanos(event.ts))
    .unwrap_or(start);
  let mut spans = vec![OtelSpan {
    trace_id: trace_id.clone(),
    span_id: root_span_id.clone(),
    parent_span_id: None,
    name: format!("agentflow.distributed_run {run_id}"),
    kind: OtelSpanKind::Internal,
    start_time_unix_nano: start,
    end_time_unix_nano: end,
    attributes: vec![
      OtelAttribute::string("agentflow.run.id", run_id.to_string()),
      OtelAttribute::i64("agentflow.worker.event_count", events.len() as i64),
    ],
    events: Vec::new(),
    status: otel_status_for_events(events),
  }];

  let mut groups: Vec<TaskTraceGroup> = Vec::new();
  for event in events {
    if let Some(group) = groups
      .iter_mut()
      .find(|group| group.task_id == event.task_id && group.attempt == event.attempt)
    {
      group.events.push(event.clone());
    } else {
      groups.push(TaskTraceGroup {
        task_id: event.task_id,
        worker_id: event.worker_id.clone(),
        run_id: event.run_id,
        node_id: event.node_id.clone(),
        attempt: event.attempt,
        events: vec![event.clone()],
      });
    }
  }

  for group in groups {
    spans.push(group.into_otel_span(&trace_id, &root_span_id));
  }

  spans
}

#[derive(Debug)]
struct TaskTraceGroup {
  task_id: Uuid,
  worker_id: WorkerId,
  run_id: Uuid,
  node_id: String,
  attempt: u32,
  events: Vec<StitchedWorkerTraceEvent>,
}

impl TaskTraceGroup {
  fn into_otel_span(self, trace_id: &str, parent_span_id: &str) -> OtelSpan {
    let start = self
      .events
      .first()
      .map(|event| unix_nanos(event.ts))
      .unwrap_or_default();
    let end = self
      .events
      .last()
      .map(|event| unix_nanos(event.ts))
      .unwrap_or(start);
    let span_events = self
      .events
      .iter()
      .map(|event| OtelSpanEvent {
        name: event.kind.clone(),
        time_unix_nano: unix_nanos(event.ts),
        attributes: vec![
          OtelAttribute::i64("agentflow.worker.global_seq", event.global_seq),
          OtelAttribute::i64("agentflow.worker.local_seq", event.local_seq),
          OtelAttribute::string("agentflow.worker.payload", event.payload.to_string()),
        ],
      })
      .collect();

    OtelSpan {
      trace_id: trace_id.to_string(),
      span_id: hex_hash(
        &format!("{}:{}:{}", self.task_id, self.worker_id.0, self.attempt),
        8,
      ),
      parent_span_id: Some(parent_span_id.to_string()),
      name: format!("agentflow.worker_task {}", self.node_id),
      kind: OtelSpanKind::Internal,
      start_time_unix_nano: start,
      end_time_unix_nano: end,
      attributes: vec![
        OtelAttribute::string("agentflow.run.id", self.run_id.to_string()),
        OtelAttribute::string("agentflow.worker.id", self.worker_id.0),
        OtelAttribute::string("agentflow.task.id", self.task_id.to_string()),
        OtelAttribute::string("agentflow.node.id", self.node_id),
        OtelAttribute::i64("agentflow.task.attempt", i64::from(self.attempt)),
      ],
      events: span_events,
      status: otel_status_for_events(&self.events),
    }
  }
}

fn otel_status_for_events(events: &[StitchedWorkerTraceEvent]) -> OtelStatus {
  if let Some(event) = events.iter().find(|event| {
    let kind = event.kind.to_ascii_lowercase();
    kind.contains("failed") || kind.contains("error")
  }) {
    OtelStatus::error(event.kind.clone())
  } else {
    OtelStatus::ok()
  }
}

fn unix_nanos(time: DateTime<Utc>) -> u64 {
  time.timestamp_nanos_opt().unwrap_or_default() as u64
}

fn hex_hash(input: &str, bytes: usize) -> String {
  let mut hash = 0xcbf29ce484222325u64;
  for byte in input.as_bytes() {
    hash ^= u64::from(*byte);
    hash = hash.wrapping_mul(0x100000001b3);
  }

  let mut out = format!("{hash:016x}");
  let required_len = bytes * 2;
  while out.len() < required_len {
    hash ^= hash.rotate_left(13);
    hash = hash.wrapping_mul(0x100000001b3);
    out.push_str(&format!("{hash:016x}"));
  }
  out.truncate(required_len);
  out
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

  #[tokio::test]
  async fn control_plane_schedules_claims_and_tracks_running_state() {
    let protocol = InMemoryWorkerProtocol::new();
    let control = WorkerControlPlane::new(protocol);
    let run_id = Uuid::new_v4();
    let worker = WorkerId::new("worker-a").expect("valid worker");
    let task = WorkerTask::new(run_id, "node_a", serde_json::json!({"input": 1}));

    control.schedule_task(task.clone()).await.expect("schedule");
    let queued = control.run_snapshot(run_id).await.expect("run snapshot");
    assert_eq!(queued.status, RunControlStatus::Queued);
    assert_eq!(queued.queued_tasks, 1);

    let claimed = control
      .claim_task(worker)
      .await
      .expect("claim")
      .expect("task");
    assert_eq!(claimed.task_id, task.task_id);

    let running = control.run_snapshot(run_id).await.expect("run snapshot");
    assert_eq!(running.status, RunControlStatus::Running);
    assert_eq!(running.queued_tasks, 0);
    assert_eq!(running.running_tasks, 1);
  }

  #[tokio::test]
  async fn control_plane_aggregates_success_outputs_and_trace() {
    let protocol = InMemoryWorkerProtocol::new();
    let control = WorkerControlPlane::new(protocol);
    let run_id = Uuid::new_v4();
    let worker = WorkerId::new("worker-a").expect("valid worker");
    let task = WorkerTask::new(run_id, "node_a", serde_json::json!({"input": 1}));
    let task_id = task.task_id;

    control.schedule_task(task).await.expect("schedule");
    control
      .claim_task(worker.clone())
      .await
      .expect("claim")
      .expect("task");
    control
      .report_result(
        worker,
        task_id,
        WorkerTaskResult::Succeeded {
          output: serde_json::json!({"answer": 42}),
          events: vec![WorkerTraceEvent {
            seq: 7,
            kind: "node_completed".into(),
            payload: serde_json::json!({"worker": "worker-a"}),
          }],
        },
      )
      .await
      .expect("report");

    let snapshot = control.run_snapshot(run_id).await.expect("run snapshot");
    assert_eq!(snapshot.status, RunControlStatus::Succeeded);
    assert_eq!(snapshot.running_tasks, 0);
    assert_eq!(snapshot.succeeded_tasks, 1);
    assert_eq!(
      snapshot.outputs.get("node_a"),
      Some(&serde_json::json!({"answer": 42}))
    );
    assert_eq!(snapshot.trace_events.len(), 1);
    assert_eq!(snapshot.trace_events[0].kind, "node_completed");
    assert_eq!(snapshot.stitched_trace_events.len(), 1);
    assert_eq!(snapshot.stitched_trace_events[0].global_seq, 0);
    assert_eq!(snapshot.stitched_trace_events[0].local_seq, 7);
    assert_eq!(snapshot.stitched_trace_events[0].task_id, task_id);
    assert_eq!(snapshot.stitched_trace_events[0].worker_id.0, "worker-a");
    assert_eq!(snapshot.stitched_trace_events[0].node_id, "node_a");

    let stitched = control.stitched_trace(run_id).await;
    assert_eq!(stitched, snapshot.stitched_trace_events);
    let spans = control.stitched_otel_spans(run_id).await;
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].parent_span_id, None);
    assert_eq!(
      spans[0].attributes[1],
      OtelAttribute::i64("agentflow.worker.event_count", 1)
    );
    assert_eq!(spans[1].parent_span_id, Some(spans[0].span_id.clone()));
    assert_eq!(spans[1].name, "agentflow.worker_task node_a");
    assert_eq!(spans[1].events.len(), 1);
    assert_eq!(spans[1].events[0].name, "node_completed");
  }

  #[tokio::test]
  async fn control_plane_aggregates_failure_state() {
    let protocol = InMemoryWorkerProtocol::new();
    let control = WorkerControlPlane::new(protocol);
    let run_id = Uuid::new_v4();
    let worker = WorkerId::new("worker-a").expect("valid worker");
    let task = WorkerTask::new(run_id, "node_a", serde_json::json!({}));
    let task_id = task.task_id;

    control.schedule_task(task).await.expect("schedule");
    control
      .claim_task(worker.clone())
      .await
      .expect("claim")
      .expect("task");
    control
      .report_result(
        worker,
        task_id,
        WorkerTaskResult::Failed {
          error: "node failed".into(),
          retryable: true,
          events: vec![WorkerTraceEvent {
            seq: 2,
            kind: "node_failed".into(),
            payload: serde_json::json!({"retryable": true}),
          }],
        },
      )
      .await
      .expect("report");

    let snapshot = control.run_snapshot(run_id).await.expect("run snapshot");
    assert_eq!(snapshot.status, RunControlStatus::Failed);
    assert_eq!(snapshot.failed_tasks, 1);
    assert_eq!(snapshot.retryable_failures, 1);
    assert_eq!(snapshot.last_error.as_deref(), Some("node failed"));
    assert_eq!(snapshot.trace_events[0].kind, "node_failed");
  }

  #[tokio::test]
  async fn control_plane_rejects_wrong_worker_without_mutating_state() {
    let protocol = InMemoryWorkerProtocol::new();
    let control = WorkerControlPlane::new(protocol);
    let run_id = Uuid::new_v4();
    let worker_a = WorkerId::new("worker-a").expect("valid worker");
    let worker_b = WorkerId::new("worker-b").expect("valid worker");
    let task = WorkerTask::new(run_id, "node_a", serde_json::json!({}));
    let task_id = task.task_id;

    control.schedule_task(task).await.expect("schedule");
    control
      .claim_task(worker_a)
      .await
      .expect("claim")
      .expect("task");

    let err = control
      .report_result(
        worker_b,
        task_id,
        WorkerTaskResult::Succeeded {
          output: serde_json::json!({}),
          events: Vec::new(),
        },
      )
      .await
      .expect_err("wrong worker must fail");

    assert!(matches!(err, SchedulerError::WorkerMismatch { .. }));
    let snapshot = control.run_snapshot(run_id).await.expect("run snapshot");
    assert_eq!(snapshot.status, RunControlStatus::Running);
    assert_eq!(snapshot.running_tasks, 1);
  }

  #[tokio::test]
  async fn control_plane_records_heartbeats() {
    let protocol = InMemoryWorkerProtocol::new();
    let control = WorkerControlPlane::new(protocol);
    let worker = WorkerId::new("worker-a").expect("valid worker");
    let heartbeat = WorkerHeartbeat::now(worker.clone(), None, 3);

    control
      .heartbeat(heartbeat.clone())
      .await
      .expect("heartbeat");

    assert_eq!(control.worker_heartbeat(&worker).await, Some(heartbeat));
  }
}

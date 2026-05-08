//! Worker runtime for distributed AgentFlow execution.
//!
//! The runtime is transport-agnostic: it drives any
//! [`WorkerProtocol`](agentflow_server::WorkerProtocol) implementation through
//! heartbeat, claim, execute, and report-result steps. The first binary uses
//! the in-memory protocol for local smoke tests; the gRPC adapter can plug in
//! behind the same API.

use std::time::Duration;

use agentflow_server::{
  SchedulerError, WorkerHeartbeat, WorkerId, WorkerProtocol, WorkerTask, WorkerTaskResult,
  WorkerTraceEvent,
};
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
    let result = execute_stub(&self.config.worker_id, &task);
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

fn execute_stub(worker_id: &WorkerId, task: &WorkerTask) -> WorkerTaskResult {
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

#[cfg(test)]
mod tests {
  use super::*;
  use agentflow_server::InMemoryWorkerProtocol;
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
}

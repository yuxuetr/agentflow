//! P5.5 — distributed worker admission.
//!
//! These tests pin down the three guarantees the TODO calls out:
//!
//! 1. Rejected worker (unknown ID) cannot poll tasks.
//! 2. Admitted worker can poll, heartbeat, and report.
//! 3. Token rotation works without dropping in-flight tasks.

use std::collections::{HashMap, HashSet};

use agentflow_server::{
  AdmissionError, AuthenticatedControlPlane, ControlError, InMemoryWorkerProtocol,
  WorkerAdmissionPolicy, WorkerControlPlane, WorkerCredential, WorkerHeartbeat, WorkerId,
  WorkerTask, WorkerTaskResult, WorkerTraceEvent,
};
use uuid::Uuid;

fn worker(label: &str) -> WorkerId {
  WorkerId::new(label).expect("valid worker label")
}

fn psk_policy(workers: &[(WorkerId, &[&str])]) -> WorkerAdmissionPolicy {
  let allowed: HashSet<WorkerId> = workers.iter().map(|(w, _)| w.clone()).collect();
  let mut keys = HashMap::new();
  for (worker_id, tokens) in workers {
    keys.insert(
      worker_id.clone(),
      tokens.iter().map(|t| t.to_string()).collect(),
    );
  }
  WorkerAdmissionPolicy {
    allowed_workers: Some(allowed),
    pre_shared_keys: keys,
    ..Default::default()
  }
}

#[tokio::test]
async fn unknown_worker_cannot_claim_or_heartbeat() {
  let protocol = InMemoryWorkerProtocol::new();
  let inner = WorkerControlPlane::new(protocol);
  let admitted = worker("trusted");
  let policy = psk_policy(&[(admitted.clone(), &["sekret"])]);
  let plane = AuthenticatedControlPlane::new(inner, policy);

  // Queue a task; it should still be there after the intruder bounces.
  let task = WorkerTask::new(Uuid::new_v4(), "node-a", serde_json::json!({"n": 1}));
  plane
    .inner()
    .schedule_task(task.clone())
    .await
    .expect("schedule task");

  let intruder = WorkerCredential::new(worker("intruder"), Some("sekret".to_string()));
  let claim_err = plane
    .claim_task(intruder.clone())
    .await
    .expect_err("intruder must be rejected");
  match claim_err {
    ControlError::Admission(AdmissionError::UnknownWorker { worker_id }) => {
      assert_eq!(worker_id, "intruder");
    }
    other => panic!("expected UnknownWorker, got {other:?}"),
  }

  let heartbeat_err = plane
    .heartbeat(intruder, WorkerHeartbeat::now(worker("intruder"), None, 1))
    .await
    .expect_err("intruder heartbeat must be rejected");
  assert!(matches!(
    heartbeat_err,
    ControlError::Admission(AdmissionError::UnknownWorker { .. })
  ));

  // The trusted worker still sees the task waiting for it.
  let trusted = WorkerCredential::new(admitted.clone(), Some("sekret".to_string()));
  let claimed = plane
    .claim_task(trusted)
    .await
    .expect("trusted claim should succeed");
  assert!(claimed.is_some(), "trusted worker should claim queued task");
}

#[tokio::test]
async fn admitted_worker_can_poll_heartbeat_and_report() {
  let protocol = InMemoryWorkerProtocol::new();
  let inner = WorkerControlPlane::new(protocol);
  let id = worker("worker-a");
  let policy = psk_policy(&[(id.clone(), &["good"])]);
  let plane = AuthenticatedControlPlane::new(inner, policy);

  let task = WorkerTask::new(Uuid::new_v4(), "node-a", serde_json::json!({"input": 1}));
  let task_id = task.task_id;
  plane
    .inner()
    .schedule_task(task)
    .await
    .expect("schedule task");

  let cred = WorkerCredential::new(id.clone(), Some("good".to_string()));

  plane
    .heartbeat(cred.clone(), WorkerHeartbeat::now(id.clone(), None, 1))
    .await
    .expect("heartbeat ok");
  let claimed = plane
    .claim_task(cred.clone())
    .await
    .expect("claim ok")
    .expect("task available");
  assert_eq!(claimed.task_id, task_id);

  plane
    .report_result(
      cred,
      task_id,
      WorkerTaskResult::Succeeded {
        output: serde_json::json!({"ok": true}),
        events: vec![WorkerTraceEvent {
          seq: 0,
          kind: "worker.task.completed".into(),
          payload: serde_json::json!({}),
        }],
      },
    )
    .await
    .expect("report ok");

  assert_eq!(plane.admitted_worker_count().await, 1);
}

#[tokio::test]
async fn token_rotation_does_not_drop_in_flight_tasks() {
  let protocol = InMemoryWorkerProtocol::new();
  let inner = WorkerControlPlane::new(protocol);
  let id = worker("rolling");
  // Operator stages a rotation: the new key "v2" is added alongside
  // the existing "v1" so both are accepted during the rollover.
  let policy = psk_policy(&[(id.clone(), &["v1", "v2"])]);
  let plane = AuthenticatedControlPlane::new(inner, policy);

  let task = WorkerTask::new(
    Uuid::new_v4(),
    "node-rotating",
    serde_json::json!({"phase": "claim"}),
  );
  let task_id = task.task_id;
  plane.inner().schedule_task(task).await.expect("schedule");

  // Claim under the old token. The control plane records the
  // in-flight task against the worker.
  let old_cred = WorkerCredential::new(id.clone(), Some("v1".to_string()));
  let claimed = plane
    .claim_task(old_cred)
    .await
    .expect("claim under v1")
    .expect("task available");
  assert_eq!(claimed.task_id, task_id);

  // The worker rolls over to the new token and reports the result.
  // The in-flight task survives because admission is per-call, not
  // per-task.
  let new_cred = WorkerCredential::new(id.clone(), Some("v2".to_string()));
  plane
    .report_result(
      new_cred,
      task_id,
      WorkerTaskResult::Succeeded {
        output: serde_json::json!({"phase": "report"}),
        events: vec![WorkerTraceEvent {
          seq: 0,
          kind: "worker.task.completed".into(),
          payload: serde_json::json!({}),
        }],
      },
    )
    .await
    .expect("report under v2");
}

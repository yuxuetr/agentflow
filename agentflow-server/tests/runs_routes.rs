//! End-to-end tests for `POST /v1/runs` and `GET /v1/runs/{id}`.
//!
//! Requires a live Postgres pointed to by `AGENTFLOW_DATABASE_TEST_URL`.
//! Without it the tests exit early so workspace `cargo test` stays
//! hermetic. The default executor runs fixed config-first DAGs through
//! `agentflow-core::Flow`.

use agentflow_core::FlowCancellationToken;
use agentflow_db::{Database, EventRepo, RunRepo, RunStatus};
use agentflow_server::{AppState, create_router};
use axum::{
  body::Body,
  http::{Request, StatusCode, header::CONTENT_TYPE},
};
use serde_json::json;
use tokio::time::{Duration, sleep};
use tower::ServiceExt;
use uuid::Uuid;

const FIXED_DAG_WORKFLOW: &str = r#"
name: Server Fixed DAG
nodes:
  - id: render
    type: template
    parameters:
      template: "hello server"
"#;

fn live_url() -> Option<String> {
  std::env::var("AGENTFLOW_DATABASE_TEST_URL").ok()
}

async fn fresh_state() -> Option<AppState> {
  let url = live_url()?;
  let db = Database::connect_and_migrate(&url, 4).await.ok()?;
  sqlx::query("TRUNCATE runs RESTART IDENTITY CASCADE")
    .execute(&db.pool)
    .await
    .ok()?;
  Some(AppState::new(db))
}

#[tokio::test]
async fn submit_run_returns_run_id_and_persists_row() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping submit_run_returns_run_id_and_persists_row");
    return;
  };
  let app = create_router(state.clone());

  let body = json!({"workflow": FIXED_DAG_WORKFLOW}).to_string();
  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/v1/runs")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let bytes = axum::body::to_bytes(response.into_body(), 4096)
    .await
    .unwrap();
  let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  let run_id: Uuid = body["run_id"].as_str().unwrap().parse().unwrap();
  assert_eq!(body["status"], "queued");
  let row = state.repos.runs.get(run_id).await.unwrap().unwrap();
  assert!(
    row
      .run_dir
      .as_deref()
      .unwrap_or_default()
      .contains(&run_id.to_string())
  );

  // Flow executor flips the run to `succeeded` after executing the fixed DAG.
  for _ in 0..40 {
    sleep(Duration::from_millis(25)).await;
    let row = state.repos.runs.get(run_id).await.unwrap();
    if matches!(row.as_ref().map(|r| r.status.as_str()), Some("succeeded")) {
      return;
    }
  }
  panic!("run never reached succeeded status within 1s");
}

#[tokio::test]
async fn submit_run_executes_fixed_dag_and_persists_workflow_events() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping submit_run_executes_fixed_dag_and_persists_workflow_events");
    return;
  };
  let app = create_router(state.clone());

  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/v1/runs")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
          json!({"workflow": FIXED_DAG_WORKFLOW}).to_string(),
        ))
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let bytes = axum::body::to_bytes(response.into_body(), 4096)
    .await
    .unwrap();
  let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  let run_id: Uuid = body["run_id"].as_str().unwrap().parse().unwrap();

  for _ in 0..40 {
    sleep(Duration::from_millis(25)).await;
    let row = state.repos.runs.get(run_id).await.unwrap().unwrap();
    if row.status == "succeeded" {
      let events = state
        .repos
        .events
        .list_after(run_id, -1, 100)
        .await
        .unwrap();
      assert!(events.iter().any(|event| event.kind == "workflow.started"));
      assert!(events.iter().any(|event| event.kind == "node.started"));
      assert!(events.iter().any(|event| event.kind == "node.completed"));
      assert!(
        events
          .iter()
          .any(|event| event.kind == "workflow.completed")
      );
      return;
    }
  }
  panic!("fixed DAG run never reached succeeded status within 1s");
}

#[tokio::test]
async fn submit_run_marks_invalid_workflow_failed() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping submit_run_marks_invalid_workflow_failed");
    return;
  };
  let app = create_router(state.clone());

  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/v1/runs")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
          json!({"workflow": "name: broken\nnodes: []\n"}).to_string(),
        ))
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let bytes = axum::body::to_bytes(response.into_body(), 4096)
    .await
    .unwrap();
  let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  let run_id: Uuid = body["run_id"].as_str().unwrap().parse().unwrap();

  for _ in 0..40 {
    sleep(Duration::from_millis(25)).await;
    let row = state.repos.runs.get(run_id).await.unwrap().unwrap();
    if row.status == "failed" {
      assert!(
        row
          .error
          .as_deref()
          .unwrap_or_default()
          .contains("failed schema validation")
      );
      return;
    }
  }
  panic!("invalid workflow run never reached failed status within 1s");
}

#[tokio::test]
async fn submit_run_without_workflow_returns_bad_request() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping submit_run_without_workflow_returns_bad_request");
    return;
  };
  let app = create_router(state);

  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/v1/runs")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from("{}"))
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  let bytes = axum::body::to_bytes(response.into_body(), 4096)
    .await
    .unwrap();
  let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  assert_eq!(body["error"]["code"], "bad_request");
}

/// P10.14.1: `retention_overrides` on the create body persists to
/// the runs row so the cleanup sweep can honor it later.
#[tokio::test]
async fn submit_run_persists_retention_overrides() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping submit_run_persists_retention_overrides");
    return;
  };
  let app = create_router(state.clone());

  let body = json!({
    "workflow": FIXED_DAG_WORKFLOW,
    "retention_overrides": {"events_days": 90, "artifacts_days": 180}
  })
  .to_string();
  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/v1/runs")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let bytes = axum::body::to_bytes(response.into_body(), 4096)
    .await
    .unwrap();
  let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  let run_id: Uuid = parsed["run_id"].as_str().unwrap().parse().unwrap();

  let row = state.repos.runs.get(run_id).await.unwrap().unwrap();
  assert_eq!(row.events_retention_days, Some(90));
  assert_eq!(row.artifacts_retention_days, Some(180));
}

/// P10.14.1: negative overrides are rejected at the API layer
/// instead of silently degrading to the global default.
#[tokio::test]
async fn submit_run_rejects_negative_retention_override() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping submit_run_rejects_negative_retention_override");
    return;
  };
  let app = create_router(state);

  let body = json!({
    "workflow": FIXED_DAG_WORKFLOW,
    "retention_overrides": {"events_days": -1}
  })
  .to_string();
  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/v1/runs")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  let bytes = axum::body::to_bytes(response.into_body(), 4096)
    .await
    .unwrap();
  let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  assert_eq!(parsed["error"]["code"], "bad_request");
}

/// P10.14.1: `Some(0)` is accepted (caller convenience) and
/// normalized to NULL in the DB so the audit story is honest.
#[tokio::test]
async fn submit_run_normalizes_zero_override_to_null() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping submit_run_normalizes_zero_override_to_null");
    return;
  };
  let app = create_router(state.clone());

  let body = json!({
    "workflow": FIXED_DAG_WORKFLOW,
    "retention_overrides": {"events_days": 0, "artifacts_days": 30}
  })
  .to_string();
  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/v1/runs")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let bytes = axum::body::to_bytes(response.into_body(), 4096)
    .await
    .unwrap();
  let parsed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  let run_id: Uuid = parsed["run_id"].as_str().unwrap().parse().unwrap();

  let row = state.repos.runs.get(run_id).await.unwrap().unwrap();
  assert!(row.events_retention_days.is_none());
  assert_eq!(row.artifacts_retention_days, Some(30));
}

#[tokio::test]
async fn get_run_returns_404_when_missing() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping get_run_returns_404_when_missing");
    return;
  };
  let app = create_router(state);

  let unknown = Uuid::new_v4();
  let response = app
    .oneshot(
      Request::builder()
        .uri(format!("/v1/runs/{}", unknown))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);
  let bytes = axum::body::to_bytes(response.into_body(), 4096)
    .await
    .unwrap();
  let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn cancel_unknown_run_returns_404() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping cancel_unknown_run_returns_404");
    return;
  };
  let app = create_router(state);
  let unknown = Uuid::new_v4();

  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri(format!("/v1/runs/{}:cancel", unknown))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn cancel_running_run_marks_cancelled_and_emits_event() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping cancel_running_run_marks_cancelled_and_emits_event");
    return;
  };
  let id = Uuid::new_v4();
  state
    .repos
    .runs
    .create(agentflow_db::NewRun {
      id,
      workflow: FIXED_DAG_WORKFLOW.into(),
      status: RunStatus::Running,
      run_dir: None,
      tenant_id: "default".into(),
      events_retention_days: None,
      artifacts_retention_days: None,
    })
    .await
    .unwrap();
  let token = FlowCancellationToken::new();
  let handle = tokio::spawn(async {
    tokio::time::sleep(Duration::from_secs(30)).await;
  });
  state
    .cancellation_registry
    .register(id, token.clone(), handle.abort_handle());

  let app = create_router(state.clone());
  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri(format!("/v1/runs/{}:cancel", id))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
  assert!(token.is_cancelled());
  assert!(
    tokio::time::timeout(Duration::from_secs(1), handle)
      .await
      .is_ok(),
    "registered run task was not aborted"
  );
  let bytes = axum::body::to_bytes(response.into_body(), 8192)
    .await
    .unwrap();
  let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  assert_eq!(body["status"], "cancelled");
  assert_eq!(body["cancelled"], true);

  let events = state.repos.events.list_after(id, -1, 100).await.unwrap();
  assert!(events.iter().any(|event| event.kind == "run.cancelled"));
}

#[tokio::test]
async fn cancel_completed_run_returns_current_terminal_state() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping cancel_completed_run_returns_current_terminal_state");
    return;
  };
  let id = Uuid::new_v4();
  state
    .repos
    .runs
    .create(agentflow_db::NewRun {
      id,
      workflow: FIXED_DAG_WORKFLOW.into(),
      status: RunStatus::Succeeded,
      run_dir: None,
      tenant_id: "default".into(),
      events_retention_days: None,
      artifacts_retention_days: None,
    })
    .await
    .unwrap();

  let app = create_router(state);
  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri(format!("/v1/runs/{}:cancel", id))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
  let bytes = axum::body::to_bytes(response.into_body(), 8192)
    .await
    .unwrap();
  let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  assert_eq!(body["status"], "succeeded");
  assert_eq!(body["cancelled"], false);
}

#[tokio::test]
async fn repeated_cancel_is_idempotent() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping repeated_cancel_is_idempotent");
    return;
  };
  let id = Uuid::new_v4();
  state
    .repos
    .runs
    .create(agentflow_db::NewRun {
      id,
      workflow: FIXED_DAG_WORKFLOW.into(),
      status: RunStatus::Running,
      run_dir: None,
      tenant_id: "default".into(),
      events_retention_days: None,
      artifacts_retention_days: None,
    })
    .await
    .unwrap();
  let token = FlowCancellationToken::new();
  let handle = tokio::spawn(async {
    tokio::time::sleep(Duration::from_secs(30)).await;
  });
  state
    .cancellation_registry
    .register(id, token, handle.abort_handle());

  let app = create_router(state);
  for expected_cancelled in [true, false] {
    let response = app
      .clone()
      .oneshot(
        Request::builder()
          .method("POST")
          .uri(format!("/v1/runs/{}:cancel", id))
          .body(Body::empty())
          .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 8192)
      .await
      .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["status"], "cancelled");
    assert_eq!(body["cancelled"], expected_cancelled);
  }
}

#[tokio::test]
async fn get_run_returns_persisted_row() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping get_run_returns_persisted_row");
    return;
  };
  let id = Uuid::new_v4();
  state
    .repos
    .runs
    .create(agentflow_db::NewRun {
      id,
      workflow: "x".into(),
      status: RunStatus::Queued,
      run_dir: None,
      tenant_id: "default".into(),
      events_retention_days: None,
      artifacts_retention_days: None,
    })
    .await
    .unwrap();

  let app = create_router(state);
  let response = app
    .oneshot(
      Request::builder()
        .uri(format!("/v1/runs/{}", id))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let bytes = axum::body::to_bytes(response.into_body(), 4096)
    .await
    .unwrap();
  let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  assert_eq!(body["id"], id.to_string());
  assert_eq!(body["workflow"], "x");
  assert_eq!(body["status"], "queued");
}

#[tokio::test]
async fn list_runs_returns_recent_rows_for_tenant() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping list_runs_returns_recent_rows_for_tenant");
    return;
  };
  let first_id = Uuid::new_v4();
  let second_id = Uuid::new_v4();
  state
    .repos
    .runs
    .create(agentflow_db::NewRun {
      id: first_id,
      workflow: "first".into(),
      status: RunStatus::Queued,
      run_dir: None,
      tenant_id: "tenant-a".into(),
      events_retention_days: None,
      artifacts_retention_days: None,
    })
    .await
    .unwrap();
  state
    .repos
    .runs
    .create(agentflow_db::NewRun {
      id: second_id,
      workflow: "second".into(),
      status: RunStatus::Running,
      run_dir: None,
      tenant_id: "tenant-a".into(),
      events_retention_days: None,
      artifacts_retention_days: None,
    })
    .await
    .unwrap();
  state
    .repos
    .runs
    .create(agentflow_db::NewRun {
      id: Uuid::new_v4(),
      workflow: "other".into(),
      status: RunStatus::Queued,
      run_dir: None,
      tenant_id: "tenant-b".into(),
      events_retention_days: None,
      artifacts_retention_days: None,
    })
    .await
    .unwrap();

  let app = create_router(state);
  let response = app
    .oneshot(
      Request::builder()
        .uri("/v1/runs?tenant_id=tenant-a&limit=10")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let bytes = axum::body::to_bytes(response.into_body(), 8192)
    .await
    .unwrap();
  let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  let runs = body["runs"].as_array().unwrap();
  assert_eq!(runs.len(), 2);
  assert!(runs.iter().any(|run| run["id"] == first_id.to_string()));
  assert!(runs.iter().any(|run| run["id"] == second_id.to_string()));
  assert!(runs.iter().all(|run| run["tenant_id"] == "tenant-a"));
}

// (P10.13.1: get_run_graph_returns_visualized_workflow_with_status
// was removed alongside the `/v1/runs/{id}/graph` endpoint when
// the `agentflow-viz` crate was deleted. See `docs/ROADMAP_v2.md`
// Theme D for the decision rationale.)

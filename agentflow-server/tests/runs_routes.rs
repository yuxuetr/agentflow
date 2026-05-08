//! End-to-end tests for `POST /v1/runs` and `GET /v1/runs/{id}`.
//!
//! Requires a live Postgres pointed to by `AGENTFLOW_DATABASE_TEST_URL`.
//! Without it the tests exit early so workspace `cargo test` stays
//! hermetic. Wires the [`StubExecutor`] so submission completes without
//! depending on `agentflow-core`.

use agentflow_db::{Database, RunRepo, RunStatus};
use agentflow_server::{AppState, create_router};
use axum::{
  body::Body,
  http::{Request, StatusCode, header::CONTENT_TYPE},
};
use serde_json::json;
use tokio::time::{Duration, sleep};
use tower::ServiceExt;
use uuid::Uuid;

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

  let body = json!({"workflow": "name: demo\nnodes: []\n"}).to_string();
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

  // Stub executor flips the run to `succeeded` after a short delay.
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

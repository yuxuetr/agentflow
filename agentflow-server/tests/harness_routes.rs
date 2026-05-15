//! P-H.5 Harness Mode HTTP integration tests.
//!
//! Exercises the gateway's `/v1/harness/sessions*` surface end-to-end:
//! submit a session, read it back, list sessions for a tenant, cancel
//! one, and replay events via the JSON-history + SSE endpoints. The
//! stub executor finishes synchronously (~50 ms) so SSE subscribers
//! always receive the full lifecycle from DB backfill — no live broker
//! attach needed.
//!
//! Tests require Postgres pointed to by `AGENTFLOW_DATABASE_TEST_URL`.
//! Without it they self-skip so workspace `cargo test` stays hermetic.

use agentflow_db::{Database, HarnessSessionStatus};
use agentflow_server::{AppState, create_router};
use axum::{
  body::{Body, to_bytes},
  http::{Request, StatusCode},
};
use futures::StreamExt;
use serde_json::{Value, json};
use tokio::time::{Duration, timeout};
use tower::ServiceExt;
use uuid::Uuid;

fn live_url() -> Option<String> {
  std::env::var("AGENTFLOW_DATABASE_TEST_URL").ok()
}

async fn fresh_state() -> Option<AppState> {
  let url = live_url()?;
  let db = Database::connect_and_migrate(&url, 4).await.ok()?;
  // Two TRUNCATEs because `harness_session_events` references
  // `harness_sessions` and we want a clean slate for every test run.
  sqlx::query("TRUNCATE harness_sessions RESTART IDENTITY CASCADE")
    .execute(&db.pool)
    .await
    .ok()?;
  Some(AppState::new(db))
}

async fn body_json(response: axum::response::Response) -> Value {
  let bytes = to_bytes(response.into_body(), 1024 * 1024)
    .await
    .expect("body collected");
  serde_json::from_slice(&bytes).expect("body is JSON")
}

async fn submit_basic_session(app: axum::Router, prompt: &str) -> Uuid {
  submit_for_tenant(app, prompt, "default").await
}

/// Variant that pins a specific tenant so list-ordering tests can isolate
/// themselves from parallel test inserts on the shared `default` tenant.
async fn submit_for_tenant(app: axum::Router, prompt: &str, tenant: &str) -> Uuid {
  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/v1/harness/sessions")
        .header("content-type", "application/json")
        .body(Body::from(
          serde_json::to_vec(&json!({
            "user_input": prompt,
            "workspace_root": "/tmp",
            "tenant_id": tenant,
          }))
          .unwrap(),
        ))
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = body_json(response).await;
  assert_eq!(body["status"], "running");
  let id_raw = body["session_id"].as_str().expect("session_id is string");
  Uuid::parse_str(id_raw).expect("session_id is a uuid")
}

#[tokio::test]
async fn submit_then_get_returns_stub_terminal_state() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping submit_then_get_returns_stub_terminal_state");
    return;
  };

  let app = create_router(state.clone());
  let id = submit_basic_session(app.clone(), "hello harness").await;

  // Give the stub executor a beat to finish (~50 ms inside the stub).
  tokio::time::sleep(Duration::from_millis(250)).await;

  let response = app
    .oneshot(
      Request::builder()
        .method("GET")
        .uri(format!("/v1/harness/sessions/{id}"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = body_json(response).await;
  // Stub marks the session as `failed` with a known reason. The schema
  // is intentionally explicit so the future LLM-backed executor can be
  // dropped in without changing the route shape.
  assert_eq!(body["status"], HarnessSessionStatus::Failed.as_str());
  assert_eq!(body["workspace_root"], "/tmp");
  assert_eq!(body["profile"], "local");
  assert_eq!(body["runtime_kind"], "react");
  assert_eq!(body["error"], "executor_not_yet_wired");
}

#[tokio::test]
async fn submit_rejects_empty_user_input() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping submit_rejects_empty_user_input");
    return;
  };

  let response = create_router(state)
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/v1/harness/sessions")
        .header("content-type", "application/json")
        .body(Body::from(
          serde_json::to_vec(&json!({
            "user_input": "  ",
            "workspace_root": "/tmp",
          }))
          .unwrap(),
        ))
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn list_sessions_returns_newest_first() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping list_sessions_returns_newest_first");
    return;
  };

  // Pin both submissions to a unique tenant so parallel tests can't
  // race their own `default`-tenant inserts into our listing.
  let tenant = format!("list-ordering-{}", Uuid::new_v4());
  let app = create_router(state);
  let first = submit_for_tenant(app.clone(), "first", &tenant).await;
  // Small delay so started_at ordering is stable across the two rows.
  tokio::time::sleep(Duration::from_millis(10)).await;
  let second = submit_for_tenant(app.clone(), "second", &tenant).await;

  let response = app
    .oneshot(
      Request::builder()
        .method("GET")
        .uri(format!("/v1/harness/sessions?tenant_id={}", tenant))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = body_json(response).await;
  let sessions = body["sessions"].as_array().expect("sessions is array");
  assert_eq!(sessions.len(), 2, "tenant-scoped listing returns both rows");
  // Newest first — submitted-second should land at index 0.
  assert_eq!(sessions[0]["id"], second.to_string());
  assert_eq!(sessions[1]["id"], first.to_string());
}

#[tokio::test]
async fn cancel_running_session_transitions_to_cancelled() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping cancel_running_session_transitions_to_cancelled");
    return;
  };

  // Use a state with a no-op executor so the session stays "running"
  // long enough for the cancel request to observe the non-terminal
  // status. The default stub executor finishes in ~50 ms.
  use agentflow_server::HarnessSessionExecutor;
  use async_trait::async_trait;
  struct SleepyExecutor;
  #[async_trait]
  impl HarnessSessionExecutor for SleepyExecutor {
    async fn execute(&self, _ctx: agentflow_server::HarnessSessionContext) {
      // Hold the row in `running` indefinitely; the cancel route owns
      // the terminal status transition.
      tokio::time::sleep(Duration::from_secs(60)).await;
    }
  }

  let state = state.with_harness_executor(std::sync::Arc::new(SleepyExecutor));
  let app = create_router(state.clone());
  let id = submit_basic_session(app.clone(), "long-running").await;

  // Submit the cancel.
  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .method("POST")
        .uri(format!("/v1/harness/sessions/{id}:cancel"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = body_json(response).await;
  assert_eq!(body["cancelled"], true);
  assert_eq!(body["status"], HarnessSessionStatus::Cancelled.as_str());

  // Second cancel returns `cancelled: false` since the row is terminal.
  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri(format!("/v1/harness/sessions/{id}:cancel"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = body_json(response).await;
  assert_eq!(body["cancelled"], false);
}

#[tokio::test]
async fn events_history_returns_full_session_log_from_db() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping events_history_returns_full_session_log_from_db");
    return;
  };

  let app = create_router(state);
  let id = submit_basic_session(app.clone(), "events history").await;
  tokio::time::sleep(Duration::from_millis(250)).await;

  let response = app
    .oneshot(
      Request::builder()
        .method("GET")
        .uri(format!("/v1/harness/sessions/{id}/events/history"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = body_json(response).await;
  let events = body.as_array().expect("events history is array");
  assert_eq!(events.len(), 2, "stub executor emits exactly two events");
  assert_eq!(events[0]["kind"], "session_started");
  assert_eq!(events[1]["kind"], "stopped");
  assert_eq!(events[1]["payload"]["reason"], "executor_not_yet_wired");
}

#[tokio::test]
async fn events_sse_replays_persisted_lifecycle() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping events_sse_replays_persisted_lifecycle");
    return;
  };

  let app = create_router(state);
  let id = submit_basic_session(app.clone(), "sse replay").await;
  // Let the stub finish so SSE backfill has the full lifecycle ready.
  tokio::time::sleep(Duration::from_millis(250)).await;

  let response = app
    .oneshot(
      Request::builder()
        .method("GET")
        .uri(format!("/v1/harness/sessions/{id}/events"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);

  let mut body = response.into_body().into_data_stream();
  let mut buffer = String::new();
  let _ = timeout(Duration::from_millis(500), async {
    while let Some(chunk) = body.next().await {
      if let Ok(bytes) = chunk {
        buffer.push_str(&String::from_utf8_lossy(&bytes));
      }
    }
  })
  .await;

  let ids: Vec<&str> = buffer
    .lines()
    .filter_map(|line| line.strip_prefix("id: "))
    .collect();
  assert!(ids.contains(&"0"), "missing seq 0 in {ids:?}");
  assert!(ids.contains(&"1"), "missing seq 1 in {ids:?}");
}

#[tokio::test]
async fn sse_against_unknown_session_returns_404() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping sse_against_unknown_session_returns_404");
    return;
  };
  let response = create_router(state)
    .oneshot(
      Request::builder()
        .method("GET")
        .uri(format!("/v1/harness/sessions/{}/events", Uuid::new_v4()))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

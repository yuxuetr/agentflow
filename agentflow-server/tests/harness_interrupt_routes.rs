//! V2.3 — interrupt/resume route integration tests.
//!
//! Exercises `GET /v1/harness/sessions/{id}/interrupt` and
//! `POST /v1/harness/sessions/{id}/interrupt/answer` end-to-end against a
//! real Postgres instance. The tests seed a session row directly and
//! drive it into `awaiting_input` via `set_pending_question` rather than
//! running a real LLM-backed loop — keeping the cases hermetic from any
//! LLM provider, same posture as `harness_approval_routes.rs`. The
//! default `StubHarnessExecutor` (no `resume_interrupt` override) fails
//! the session with a clear error on resume, which is itself a
//! deterministic, testable outcome — see `handle_live_failure`'s
//! `LiveHarnessExecutor` counterpart for the real-agent path.
//!
//! Requires Postgres pointed to by `AGENTFLOW_DATABASE_TEST_URL`.
//! Without it the tests self-skip.

use agentflow_db::{HarnessSessionRepo, NewHarnessSession};
use agentflow_server::{AppState, create_router};
use axum::{
  body::{Body, to_bytes},
  http::{Request, StatusCode},
};
use serde_json::{Value, json};
use tokio::time::{Duration, timeout};
use tower::ServiceExt;
use uuid::Uuid;

const TENANT_HEADER: &str = "x-agentflow-tenant";

fn live_url() -> Option<String> {
  std::env::var("AGENTFLOW_DATABASE_TEST_URL").ok()
}

async fn fresh_state() -> Option<AppState> {
  let url = live_url()?;
  let db = agentflow_db::Database::connect_and_migrate(&url, 4)
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

async fn insert_running_session(state: &AppState, tenant: &str) -> Uuid {
  let id = Uuid::new_v4();
  state
    .repos
    .harness_sessions
    .create(NewHarnessSession {
      id,
      tenant_id: tenant.to_string(),
      user_input: "interrupt test".into(),
      workspace_root: "/tmp".into(),
      profile: "local".into(),
      runtime_kind: "react".into(),
      model: "stub".into(),
      skill_name: None,
    })
    .await
    .expect("session inserted");
  id
}

async fn get_interrupt(
  app: axum::Router,
  session_id: Uuid,
  tenant: &str,
) -> axum::response::Response {
  app
    .oneshot(
      Request::builder()
        .uri(format!("/v1/harness/sessions/{session_id}/interrupt"))
        .header(TENANT_HEADER, tenant)
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap()
}

async fn answer_interrupt(
  app: axum::Router,
  session_id: Uuid,
  tenant: &str,
  answer: &str,
) -> axum::response::Response {
  app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri(format!(
          "/v1/harness/sessions/{session_id}/interrupt/answer"
        ))
        .header("content-type", "application/json")
        .header(TENANT_HEADER, tenant)
        .body(Body::from(
          serde_json::to_vec(&json!({ "answer": answer })).unwrap(),
        ))
        .unwrap(),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn interrupt_get_returns_null_pending_for_a_plain_running_session() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping interrupt_get_returns_null_pending_for_a_plain_running_session");
    return;
  };
  let tenant = format!("tenant-interrupt-null-{}", Uuid::new_v4());
  let session_id = insert_running_session(&state, &tenant).await;
  let app = create_router(state);

  let response = get_interrupt(app, session_id, &tenant).await;
  assert_eq!(response.status(), StatusCode::OK);
  let body = body_json(response).await;
  assert_eq!(body["pending"], Value::Null);
}

#[tokio::test]
async fn interrupt_get_surfaces_the_pending_question_once_set() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping interrupt_get_surfaces_the_pending_question_once_set");
    return;
  };
  let tenant = format!("tenant-interrupt-pending-{}", Uuid::new_v4());
  let session_id = insert_running_session(&state, &tenant).await;
  state
    .repos
    .harness_sessions
    .set_pending_question(session_id, "which file should I edit?", 3)
    .await
    .expect("set pending question");
  let app = create_router(state);

  let response = get_interrupt(app, session_id, &tenant).await;
  assert_eq!(response.status(), StatusCode::OK);
  let body = body_json(response).await;
  assert_eq!(body["pending"]["question"], "which file should I edit?");
  assert_eq!(body["pending"]["step_index"], 3);
}

#[tokio::test]
async fn interrupt_answer_rejects_a_session_that_is_not_awaiting_input() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping interrupt_answer_rejects_a_session_that_is_not_awaiting_input");
    return;
  };
  let tenant = format!("tenant-interrupt-not-awaiting-{}", Uuid::new_v4());
  let session_id = insert_running_session(&state, &tenant).await;
  let app = create_router(state);

  let response = answer_interrupt(app, session_id, &tenant, "the answer").await;
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn interrupt_answer_rejects_an_empty_answer() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping interrupt_answer_rejects_an_empty_answer");
    return;
  };
  let tenant = format!("tenant-interrupt-empty-{}", Uuid::new_v4());
  let session_id = insert_running_session(&state, &tenant).await;
  state
    .repos
    .harness_sessions
    .set_pending_question(session_id, "which file?", 1)
    .await
    .expect("set pending question");
  let app = create_router(state);

  let response = answer_interrupt(app, session_id, &tenant, "   ").await;
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn interrupt_answer_clears_pending_state_and_dispatches_resume() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping interrupt_answer_clears_pending_state_and_dispatches_resume");
    return;
  };
  let tenant = format!("tenant-interrupt-happy-{}", Uuid::new_v4());
  let session_id = insert_running_session(&state, &tenant).await;
  state
    .repos
    .harness_sessions
    .set_pending_question(session_id, "which file?", 2)
    .await
    .expect("set pending question");
  let app = create_router(state);

  let response = answer_interrupt(app.clone(), session_id, &tenant, "src/main.rs").await;
  assert_eq!(response.status(), StatusCode::OK);
  let body = body_json(response).await;
  // Fire-and-forget: the row already reflects `running` with pending
  // columns cleared, even though the resume itself dispatches in the
  // background.
  assert_eq!(body["resumed"], true);
  assert_eq!(body["status"], "running");
  assert_eq!(body["pending_question"], Value::Null);

  // `StubHarnessExecutor` has no `resume_interrupt` override, so the
  // trait default deterministically fails the session — proving the
  // route actually dispatched the executor call rather than being a
  // no-op.
  let terminal = timeout(Duration::from_secs(5), async {
    loop {
      let row = body_json(
        axum::Router::clone(&app)
          .oneshot(
            Request::builder()
              .uri(format!("/v1/harness/sessions/{session_id}"))
              .header(TENANT_HEADER, tenant.as_str())
              .body(Body::empty())
              .unwrap(),
          )
          .await
          .unwrap(),
      )
      .await;
      if row["status"] != "running" {
        return row;
      }
      tokio::time::sleep(Duration::from_millis(20)).await;
    }
  })
  .await
  .expect("session reached a terminal state");

  assert_eq!(terminal["status"], "failed");
  assert!(
    terminal["error"]
      .as_str()
      .unwrap_or_default()
      .contains("does not support interrupt resume")
  );
}

#[tokio::test]
async fn cross_tenant_interrupt_get_returns_404() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping cross_tenant_interrupt_get_returns_404");
    return;
  };
  let owner = format!("tenant-interrupt-owner-{}", Uuid::new_v4());
  let session_id = insert_running_session(&state, &owner).await;
  let app = create_router(state);

  let response = get_interrupt(
    app,
    session_id,
    &format!("tenant-intruder-{}", Uuid::new_v4()),
  )
  .await;
  assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn cross_tenant_interrupt_answer_returns_404() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping cross_tenant_interrupt_answer_returns_404");
    return;
  };
  let owner = format!("tenant-interrupt-answer-owner-{}", Uuid::new_v4());
  let session_id = insert_running_session(&state, &owner).await;
  state
    .repos
    .harness_sessions
    .set_pending_question(session_id, "which file?", 1)
    .await
    .expect("set pending question");
  let app = create_router(state);

  let response = answer_interrupt(
    app,
    session_id,
    &format!("tenant-intruder-{}", Uuid::new_v4()),
    "the answer",
  )
  .await;
  assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

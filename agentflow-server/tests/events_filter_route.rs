//! P10.17.3 route-level coverage: `GET /v1/runs/{id}/events/history?filter=…`.
//!
//! Self-skips when `AGENTFLOW_DATABASE_TEST_URL` isn't set, matching
//! every other agentflow-server e2e file (sse_events, harness_routes).
//! The bulk of the filter behaviour is pinned by the unit tests in
//! `agentflow-server/src/events_filter.rs::tests`; this file only
//! verifies the HTTP-layer wiring (handler invokes the parser,
//! 400-on-bad-filter, filter applied before JSON serialisation).

use agentflow_db::{Database, EventRepo, NewEvent, NewRun, RunRepo, RunStatus};
use agentflow_server::{AppState, create_router};
use axum::{
  body::Body,
  http::{Request, StatusCode},
};
use serde_json::{Value, json};
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

async fn seed_run_with_events(state: &AppState, events: &[(&str, serde_json::Value)]) -> Uuid {
  let run_id = Uuid::new_v4();
  state
    .repos
    .runs
    .create(NewRun {
      id: run_id,
      workflow: "fixture".into(),
      status: RunStatus::Succeeded,
      run_dir: None,
      tenant_id: "default".into(),
    })
    .await
    .expect("create run");
  for (idx, (kind, payload)) in events.iter().enumerate() {
    state
      .repos
      .events
      .append(NewEvent {
        run_id,
        seq: idx as i64,
        kind: (*kind).into(),
        payload: payload.clone(),
        tenant_id: Some("default".into()),
      })
      .await
      .expect("append event");
  }
  run_id
}

#[tokio::test]
async fn history_filter_returns_only_matching_kind() {
  let Some(state) = fresh_state().await else {
    eprintln!(
      "skipping history_filter_returns_only_matching_kind — set AGENTFLOW_DATABASE_TEST_URL"
    );
    return;
  };
  let app = create_router(state.clone());
  let run_id = seed_run_with_events(
    &state,
    &[
      ("run_started", json!({})),
      ("step_started", json!({ "step_index": 0 })),
      (
        "tool_call_started",
        json!({ "step_index": 1, "tool": "shell" }),
      ),
      ("tool_call_completed", json!({ "step_index": 1 })),
      ("run_completed", json!({})),
    ],
  )
  .await;

  let response = app
    .oneshot(
      Request::builder()
        .uri(format!(
          "/v1/runs/{run_id}/events/history?filter={}",
          urlencoding::encode("kind~tool")
        ))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
    .await
    .unwrap();
  let events: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
  // The `kind~tool` clause matches both tool_call_started +
  // tool_call_completed. Two events; in seq order.
  assert_eq!(
    events.len(),
    2,
    "kind~tool must match tool_call_started + tool_call_completed: {events:?}"
  );
  for event in &events {
    let kind = event["kind"].as_str().unwrap();
    assert!(
      kind.contains("tool"),
      "every returned event must satisfy kind~tool: got {kind}",
    );
  }
}

#[tokio::test]
async fn history_filter_combined_with_after_seq_applies_both_constraints() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping history_filter_combined_with_after_seq_applies_both_constraints");
    return;
  };
  let app = create_router(state.clone());
  let run_id = seed_run_with_events(
    &state,
    &[
      ("run_started", json!({})),
      ("step_started", json!({ "step_index": 0 })),
      ("step_started", json!({ "step_index": 1 })),
      ("step_started", json!({ "step_index": 2 })),
    ],
  )
  .await;

  // after_seq=0 → drop event 0 (run_started); filter=kind=step_started
  // → drop run_started anyway, so visible set is the three step_started
  // rows minus the one with seq=0 if it had matched.
  let response = app
    .oneshot(
      Request::builder()
        .uri(format!(
          "/v1/runs/{run_id}/events/history?after_seq=1&filter={}",
          urlencoding::encode("kind=step_started"),
        ))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
    .await
    .unwrap();
  let events: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
  // after_seq=1 returns rows with seq > 1 → seqs 2 and 3. Both are
  // step_started so the filter doesn't drop any further.
  assert_eq!(
    events.len(),
    2,
    "after_seq + kind filter compose: {events:?}"
  );
  for event in &events {
    assert_eq!(event["kind"], "step_started");
    assert!(event["seq"].as_i64().unwrap() > 1);
  }
}

#[tokio::test]
async fn history_filter_parse_error_returns_400() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping history_filter_parse_error_returns_400");
    return;
  };
  let app = create_router(state.clone());
  let run_id = seed_run_with_events(&state, &[("run_started", json!({}))]).await;

  // `step>oops` fails the parser at threshold parsing.
  let response = app
    .oneshot(
      Request::builder()
        .uri(format!(
          "/v1/runs/{run_id}/events/history?filter={}",
          urlencoding::encode("step>oops"),
        ))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    StatusCode::BAD_REQUEST,
    "malformed filter must surface as 400, not silently degrade",
  );
  let bytes = axum::body::to_bytes(response.into_body(), 4096)
    .await
    .unwrap();
  let body: Value = serde_json::from_slice(&bytes).unwrap();
  // The error envelope wraps the parser message; lock the
  // user-actionable substring so the UI's 400-fallback path can
  // pattern-match without breaking on future wording drift.
  let message = body
    .get("error")
    .and_then(|e| e.get("message"))
    .and_then(|m| m.as_str())
    .unwrap_or("");
  assert!(
    message.contains("invalid filter") || message.contains("not a valid i64"),
    "error must explain the parse failure: {body}",
  );
}

#[tokio::test]
async fn history_filter_empty_param_is_no_op() {
  // `?filter=` with an empty value must NOT 400; it should
  // behave identically to no `filter` param at all. Pin so a
  // future strict-input change doesn't break callers that
  // unconditionally append the param.
  let Some(state) = fresh_state().await else {
    eprintln!("skipping history_filter_empty_param_is_no_op");
    return;
  };
  let app = create_router(state.clone());
  let run_id = seed_run_with_events(
    &state,
    &[("run_started", json!({})), ("run_completed", json!({}))],
  )
  .await;

  let response = app
    .oneshot(
      Request::builder()
        .uri(format!("/v1/runs/{run_id}/events/history?filter="))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
    .await
    .unwrap();
  let events: Vec<Value> = serde_json::from_slice(&bytes).unwrap();
  assert_eq!(events.len(), 2, "empty filter must be a no-op: {events:?}");
}

//! Route-level integration test for `GET /v1/diagnostics` (P6.2).
//!
//! No live database required — the diagnostics handler does not
//! touch `AppState.db`, so a lazy-connected pool is enough to mount
//! the router and oneshot a request.

use agentflow_db::Database;
use agentflow_server::{AppState, create_router};
use axum::{
  body::Body,
  http::{Request, StatusCode},
};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

fn lazy_state() -> AppState {
  let pool = PgPoolOptions::new()
    .connect_lazy("postgres://postgres:postgres@localhost:5432/agentflow_test")
    .expect("lazy pg");
  AppState::new(Database { pool })
}

#[tokio::test]
async fn diagnostics_returns_a_json_envelope() {
  let app = create_router(lazy_state());
  let response = app
    .oneshot(
      Request::builder()
        .uri("/v1/diagnostics")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
  let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&bytes).expect("valid JSON body");

  assert!(json.get("status").is_some(), "missing status field");
  let status = json["status"].as_str().expect("status is a string");
  assert!(
    matches!(status, "ok" | "warning" | "fail"),
    "unexpected status: {status}"
  );

  // Confirm the schema the UI relies on is present.
  for path in ["/config", "/security", "/sandbox", "/disk", "/environment"] {
    assert!(
      json.pointer(path).is_some(),
      "diagnostics report missing {path}"
    );
  }
}

#[tokio::test]
async fn diagnostics_does_not_leak_token_value_through_route() {
  // Mirror the unit test's defense-in-depth check at the route layer.
  let key = "AGENTFLOW_API_TOKEN";
  let secret = "sk-route-leak-test-must-not-appear-in-body";
  let previous = std::env::var(key).ok();
  // SAFETY: tests touch shared process env. We restore before
  // assertions; this is the established pattern in this crate's
  // diagnostics unit test.
  unsafe {
    std::env::set_var(key, secret);
  }

  let app = create_router(lazy_state());
  let response = app
    .oneshot(
      Request::builder()
        .uri("/v1/diagnostics")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  // SAFETY: see comment above.
  unsafe {
    match previous {
      Some(value) => std::env::set_var(key, value),
      None => std::env::remove_var(key),
    }
  }

  assert_eq!(response.status(), StatusCode::OK);
  let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
    .await
    .unwrap();
  let body = String::from_utf8_lossy(&bytes);
  assert!(
    !body.contains(secret),
    "diagnostics body leaked the AGENTFLOW_API_TOKEN value"
  );
}

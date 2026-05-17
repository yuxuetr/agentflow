//! End-to-end tests for the P6.4 `/v1/preferences` routes.
//!
//! Self-skip without `AGENTFLOW_DATABASE_TEST_URL` so workspace
//! `cargo test` stays hermetic. Each test uses a per-invocation
//! UUID-suffixed tenant so they're isolated from anything else
//! running against the same Postgres.

use agentflow_db::Database;
use agentflow_server::{AppState, create_router};
use axum::{
  body::Body,
  http::{Request, StatusCode, header::CONTENT_TYPE},
};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

const TENANT_HEADER: &str = "x-agentflow-tenant";

fn live_url() -> Option<String> {
  std::env::var("AGENTFLOW_DATABASE_TEST_URL").ok()
}

async fn fresh_state() -> Option<AppState> {
  let url = live_url()?;
  let db = Database::connect_and_migrate(&url, 4).await.ok()?;
  Some(AppState::new(db))
}

fn fresh_tenant(label: &str) -> String {
  format!("{label}-{}", Uuid::new_v4())
}

#[tokio::test]
async fn get_preferences_returns_empty_for_new_tenant() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping get_preferences_returns_empty_for_new_tenant");
    return;
  };
  let tenant = fresh_tenant("pref-empty");

  let app = create_router(state);
  let response = app
    .oneshot(
      Request::builder()
        .uri("/v1/preferences")
        .header(TENANT_HEADER, &tenant)
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: Value = serde_json::from_slice(
    &axum::body::to_bytes(response.into_body(), 4096)
      .await
      .unwrap(),
  )
  .unwrap();
  assert!(
    body["preferences"].as_object().unwrap().is_empty(),
    "expected empty preferences for fresh tenant; got {body}"
  );
}

#[tokio::test]
async fn put_preferences_round_trips_via_get() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping put_preferences_round_trips_via_get");
    return;
  };
  let tenant = fresh_tenant("pref-roundtrip");

  let payload = json!({
    "preferences": {
      "theme": "dark",
      "ui.run-list.page-size": 25,
      "filter:default": "status=running"
    }
  });

  let app = create_router(state.clone());
  let put_response = app
    .oneshot(
      Request::builder()
        .method("PUT")
        .uri("/v1/preferences")
        .header(TENANT_HEADER, &tenant)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(put_response.status(), StatusCode::OK);
  let put_body: Value = serde_json::from_slice(
    &axum::body::to_bytes(put_response.into_body(), 4096)
      .await
      .unwrap(),
  )
  .unwrap();
  // PUT echoes the persisted set so the caller's local cache stays in sync.
  assert_eq!(put_body["preferences"]["theme"], "dark");
  assert_eq!(put_body["preferences"]["ui.run-list.page-size"], 25);

  // Independent GET against the same tenant must see all 3 keys.
  let app = create_router(state);
  let get_response = app
    .oneshot(
      Request::builder()
        .uri("/v1/preferences")
        .header(TENANT_HEADER, &tenant)
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(get_response.status(), StatusCode::OK);
  let get_body: Value = serde_json::from_slice(
    &axum::body::to_bytes(get_response.into_body(), 4096)
      .await
      .unwrap(),
  )
  .unwrap();
  let prefs = get_body["preferences"].as_object().unwrap();
  assert_eq!(prefs.len(), 3);
  assert_eq!(prefs["theme"], "dark");
  assert_eq!(prefs["ui.run-list.page-size"], 25);
  assert_eq!(prefs["filter:default"], "status=running");
}

#[tokio::test]
async fn put_preferences_rejects_token_shaped_values() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping put_preferences_rejects_token_shaped_values");
    return;
  };
  let tenant = fresh_tenant("pref-token");

  // sk- prefix + 36 chars trips the API-key heuristic.
  let payload = json!({
    "preferences": {
      "theme": "dark",
      "secret": "sk-abcdefghijklmnopqrstuvwxyz0123456789"
    }
  });

  let app = create_router(state.clone());
  let response = app
    .oneshot(
      Request::builder()
        .method("PUT")
        .uri("/v1/preferences")
        .header(TENANT_HEADER, &tenant)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  let body: Value = serde_json::from_slice(
    &axum::body::to_bytes(response.into_body(), 4096)
      .await
      .unwrap(),
  )
  .unwrap();
  let message = body["error"]["message"].as_str().unwrap_or("");
  assert!(
    message.contains("secret"),
    "error must name the rejected key, got: {body}"
  );

  // Atomicity: no rows must have been persisted for this tenant.
  let app = create_router(state);
  let get_response = app
    .oneshot(
      Request::builder()
        .uri("/v1/preferences")
        .header(TENANT_HEADER, &tenant)
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  let body: Value = serde_json::from_slice(
    &axum::body::to_bytes(get_response.into_body(), 4096)
      .await
      .unwrap(),
  )
  .unwrap();
  assert!(
    body["preferences"].as_object().unwrap().is_empty(),
    "rejected PUT must not persist any of the keys; got {body}"
  );
}

#[tokio::test]
async fn put_preferences_rejects_invalid_key() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping put_preferences_rejects_invalid_key");
    return;
  };
  let tenant = fresh_tenant("pref-badkey");

  // Key with a space — outside the allowed character set.
  let payload = json!({
    "preferences": {
      "has spaces": "ok"
    }
  });

  let app = create_router(state);
  let response = app
    .oneshot(
      Request::builder()
        .method("PUT")
        .uri("/v1/preferences")
        .header(TENANT_HEADER, &tenant)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn put_preferences_isolates_tenants() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping put_preferences_isolates_tenants");
    return;
  };
  let tenant_a = fresh_tenant("pref-iso-a");
  let tenant_b = fresh_tenant("pref-iso-b");

  // Tenant A writes a theme.
  let app = create_router(state.clone());
  let _ = app
    .oneshot(
      Request::builder()
        .method("PUT")
        .uri("/v1/preferences")
        .header(TENANT_HEADER, &tenant_a)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(
          json!({"preferences": {"theme": "dark"}}).to_string(),
        ))
        .unwrap(),
    )
    .await
    .unwrap();

  // Tenant B reads — must be empty.
  let app = create_router(state);
  let response = app
    .oneshot(
      Request::builder()
        .uri("/v1/preferences")
        .header(TENANT_HEADER, &tenant_b)
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  let body: Value = serde_json::from_slice(
    &axum::body::to_bytes(response.into_body(), 4096)
      .await
      .unwrap(),
  )
  .unwrap();
  assert!(body["preferences"].as_object().unwrap().is_empty());
}

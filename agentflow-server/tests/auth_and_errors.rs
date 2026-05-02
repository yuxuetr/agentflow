//! End-to-end checks for auth wiring and the unified error envelope.
//!
//! These tests don't require a database — they hit the auth-protected
//! `/v1/whoami` smoke route plus the unauthenticated `/health` route, and
//! exercise the bearer-token middleware via Tower's `oneshot` helper. DB-
//! backed route tests live elsewhere and are gated by
//! `AGENTFLOW_DATABASE_TEST_URL`.

use agentflow_server::{ApiError, AuthConfig};
use axum::{
  Router,
  body::Body,
  http::{Request, StatusCode, header::AUTHORIZATION},
  response::IntoResponse,
};
use tower::ServiceExt;

/// Build a router with a single authenticated `/v1/ping` route and the
/// supplied auth config attached. We don't use the full `create_router`
/// here because that requires a live `Database`; this gives us a direct
/// path to assert the middleware behaviour.
fn auth_only_router(auth: AuthConfig) -> Router {
  use axum::{middleware, routing::get};
  Router::new()
    .route("/v1/ping", get(|| async { "pong" }))
    .layer(middleware::from_fn_with_state(
      auth,
      agentflow_server::require_bearer_token,
    ))
}

#[tokio::test]
async fn missing_authorization_header_returns_401_with_unified_envelope() {
  let router = auth_only_router(AuthConfig {
    expected_token: "secret".into(),
  });

  let response = router
    .oneshot(
      Request::builder()
        .uri("/v1/ping")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
  let bytes = axum::body::to_bytes(response.into_body(), 4096)
    .await
    .unwrap();
  let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  assert_eq!(body["error"]["code"], "unauthorized");
  assert!(body["error"]["message"].is_string());
}

#[tokio::test]
async fn wrong_token_returns_403() {
  let router = auth_only_router(AuthConfig {
    expected_token: "secret".into(),
  });

  let response = router
    .oneshot(
      Request::builder()
        .uri("/v1/ping")
        .header(AUTHORIZATION, "Bearer not-the-right-one")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::FORBIDDEN);
  let bytes = axum::body::to_bytes(response.into_body(), 4096)
    .await
    .unwrap();
  let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  assert_eq!(body["error"]["code"], "forbidden");
}

#[tokio::test]
async fn correct_token_passes_through() {
  let router = auth_only_router(AuthConfig {
    expected_token: "secret".into(),
  });

  let response = router
    .oneshot(
      Request::builder()
        .uri("/v1/ping")
        .header(AUTHORIZATION, "Bearer secret")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn malformed_authorization_header_returns_401() {
  let router = auth_only_router(AuthConfig {
    expected_token: "secret".into(),
  });

  for header_value in ["secret", "Token secret", "Bearer ", "Bearer  "] {
    let response = router
      .clone()
      .oneshot(
        Request::builder()
          .uri("/v1/ping")
          .header(AUTHORIZATION, header_value)
          .body(Body::empty())
          .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(
      response.status(),
      StatusCode::UNAUTHORIZED,
      "header '{header_value}' should be rejected as malformed"
    );
  }
}

#[tokio::test]
async fn api_error_database_maps_to_500_with_database_code() {
  let err = ApiError::Database(agentflow_db::error::DbError::ConfigError {
    message: "boom".into(),
  });
  let response = err.into_response();
  assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
  let bytes = axum::body::to_bytes(response.into_body(), 4096)
    .await
    .unwrap();
  let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
  assert_eq!(body["error"]["code"], "database_error");
}

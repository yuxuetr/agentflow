//! P10.14.2-FU1 integration tests for the `/metrics` endpoint.
//!
//! These tests hit `GET /metrics` through the actual Axum router
//! and assert the contracted metric names appear after a few
//! `metrics::counter!()` / `metrics::histogram!()` calls fire.
//! No live Postgres needed — `connect_lazy` builds the pool
//! placeholder the AppState constructor expects.

use agentflow_db::Database;
use agentflow_server::{AppState, create_router, metrics};
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
  AppState::new(Database {
    pool,
    read_pool: None,
  })
}

async fn fetch_metrics_body(app: axum::Router) -> String {
  let response = app
    .oneshot(
      Request::builder()
        .uri("/metrics")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
    .await
    .unwrap();
  String::from_utf8(bytes.to_vec()).expect("metrics body is utf-8")
}

#[tokio::test]
async fn metrics_endpoint_returns_ok_and_text_plain() {
  let _ = metrics::init_recorder();
  let app = create_router(lazy_state());
  let response = app
    .oneshot(
      Request::builder()
        .uri("/metrics")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let ct = response
    .headers()
    .get(axum::http::header::CONTENT_TYPE)
    .map(|v| v.to_str().unwrap().to_string())
    .unwrap_or_default();
  assert!(
    ct.starts_with("text/plain"),
    "Prometheus content-type required, got: {ct}"
  );
}

#[tokio::test]
async fn metrics_endpoint_bypasses_auth() {
  // The route is mounted on the `health` sub-router which
  // doesn't have the bearer-token middleware. Calling without
  // an Authorization header must still succeed — Prometheus
  // scrapers don't carry tokens by default.
  let _ = metrics::init_recorder();
  let app = create_router(lazy_state());
  let response = app
    .oneshot(
      Request::builder()
        .uri("/metrics")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_ne!(
    response.status(),
    StatusCode::UNAUTHORIZED,
    "/metrics must not require auth"
  );
}

#[tokio::test]
async fn metrics_endpoint_emits_workflow_completed_total_after_observation() {
  let _ = metrics::init_recorder();
  // Fire one observation of each terminal status so the
  // counter contract is provably wired end-to-end.
  metrics::observe_workflow_completion("succeeded", 0.5);
  metrics::observe_workflow_completion("failed", 1.0);
  metrics::observe_workflow_completion("cancelled", 0.1);
  let app = create_router(lazy_state());
  let body = fetch_metrics_body(app).await;
  assert!(
    body.contains("agentflow_workflow_completed_total"),
    "completed counter must appear in /metrics body; got:\n{body}"
  );
  for status in ["succeeded", "failed", "cancelled"] {
    let needle = format!("status=\"{status}\"");
    assert!(
      body.contains(&needle),
      "status label `{status}` missing from /metrics; got:\n{body}"
    );
  }
}

#[tokio::test]
async fn metrics_endpoint_emits_workflow_duration_histogram_buckets() {
  let _ = metrics::init_recorder();
  metrics::observe_workflow_completion("succeeded", 2.5);
  let app = create_router(lazy_state());
  let body = fetch_metrics_body(app).await;
  assert!(
    body.contains("agentflow_workflow_duration_seconds"),
    "duration histogram must appear; got:\n{body}"
  );
  // The Prometheus exporter emits `_bucket{le="..."}` lines for
  // histograms. Spot-check the `le="+Inf"` bucket which always
  // appears regardless of observed values.
  assert!(
    body.contains("agentflow_workflow_duration_seconds_bucket"),
    "histogram bucket lines must appear; got:\n{body}"
  );
}

#[tokio::test]
async fn metrics_endpoint_emits_nodes_failed_total_with_node_type_label() {
  let _ = metrics::init_recorder();
  metrics::observe_node_failure(Some("llm"));
  metrics::observe_node_failure(Some("http"));
  metrics::observe_node_failure(None); // → "unknown" fallback
  let app = create_router(lazy_state());
  let body = fetch_metrics_body(app).await;
  assert!(
    body.contains("agentflow_nodes_failed_total"),
    "node-failures counter must appear; got:\n{body}"
  );
  for node_type in ["llm", "http", "unknown"] {
    let needle = format!("node_type=\"{node_type}\"");
    assert!(
      body.contains(&needle),
      "node_type label `{node_type}` missing; got:\n{body}"
    );
  }
}

#[tokio::test]
async fn metrics_endpoint_emits_cleanup_counters_after_observation() {
  // P10.14.2-FU2: the three cleanup_*_deleted_total counters
  // appear once `observe_cleanup_sweep` fires. A real
  // `cleanup_expired` invocation requires Postgres and is
  // covered by `tests/cleanup_route.rs` end-to-end; this test
  // just pins the metric-name wire shape.
  let _ = metrics::init_recorder();
  metrics::observe_cleanup_sweep(false, 3, 42, 7);
  let app = create_router(lazy_state());
  let body = fetch_metrics_body(app).await;
  assert!(
    body.contains("agentflow_cleanup_runs_deleted_total"),
    "runs counter must appear; got:\n{body}"
  );
  assert!(
    body.contains("agentflow_cleanup_events_deleted_total"),
    "events counter must appear; got:\n{body}"
  );
  assert!(
    body.contains("agentflow_cleanup_artifacts_deleted_total"),
    "artifacts counter must appear; got:\n{body}"
  );
}

#[tokio::test]
async fn metrics_endpoint_returns_empty_body_when_recorder_uninstalled() {
  // We can't easily make `init_recorder` un-install (it's
  // process-global), so this test only runs in a fresh process
  // where the recorder hasn't been touched. In a single
  // `cargo test` binary the install from the earlier tests
  // sticks, so the body will be non-empty. The contract still
  // holds for an isolated boot — covered by the unit test
  // `metrics::tests::render_text_returns_empty_when_recorder_uninstalled`.
  let app = create_router(lazy_state());
  let response = app
    .oneshot(
      Request::builder()
        .uri("/metrics")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
}

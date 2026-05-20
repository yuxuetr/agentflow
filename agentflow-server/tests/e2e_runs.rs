//! End-to-end run-lifecycle tests for `agentflow-server` (P2.3).
//!
//! `runs_routes.rs` already covers the core happy / cancel / 4xx paths and
//! the mid-run graph snapshot. This file closes the remaining P2.3 spec
//! cells:
//!
//! - Pagination via `?limit=N&offset=N` returns disjoint slices.
//! - Status filter via `?status=running` rejects unknown values and
//!   isolates running rows from terminal rows.
//! - Graph snapshots taken before any events and after a run terminates
//!   both reflect the persisted run state without crashing the route.
//! - Authenticated paths under a production-style profile gate every
//!   submitted run behind the bearer-token middleware while keeping
//!   `/health` open.
//!
//! Skipped without `AGENTFLOW_DATABASE_TEST_URL`; the workspace `cargo
//! test` stays hermetic.

use agentflow_db::{Database, NewRun, RunRepo, RunStatus};
use agentflow_server::{AppState, AuthConfig, create_router};
use axum::{
  body::Body,
  http::{Request, StatusCode, header::CONTENT_TYPE},
};
use serde_json::{Value, json};
use tower::ServiceExt;
use uuid::Uuid;

const FIXED_DAG: &str = r#"
name: P2.3 End-to-End
nodes:
  - id: render
    type: template
    parameters:
      template: "hello"
"#;

fn live_url() -> Option<String> {
  std::env::var("AGENTFLOW_DATABASE_TEST_URL").ok()
}

async fn fresh_state() -> Option<AppState> {
  let url = live_url()?;
  let db = Database::connect_and_migrate(&url, 4).await.ok()?;
  // Intentionally no TRUNCATE: integration tests run in parallel and a
  // global wipe races other tests mid-flight. Every test uses unique
  // (tenant, run_id) scope keys instead — see the per-test seed_runs
  // helper and the seeded UUIDs.
  Some(AppState::new(db))
}

/// Seed the runs table with `count` rows under `tenant` and the given
/// status. Returns the inserted ids in insertion order. Each row uses a
/// distinct workflow string so callers can pluck out which row is which.
async fn seed_runs(state: &AppState, tenant: &str, status: RunStatus, count: usize) -> Vec<Uuid> {
  let mut ids = Vec::with_capacity(count);
  for i in 0..count {
    let id = Uuid::new_v4();
    state
      .repos
      .runs
      .create(NewRun {
        id,
        workflow: format!("seed-{i}"),
        status,
        run_dir: None,
        tenant_id: tenant.into(),
        events_retention_days: None,
        artifacts_retention_days: None,
      })
      .await
      .unwrap();
    ids.push(id);
    // Stagger started_at so the ORDER BY started_at DESC produces a
    // deterministic order. NOW() resolution can collide otherwise.
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
  }
  ids
}

// ── Pagination + status filter ─────────────────────────────────────────────

#[tokio::test]
async fn list_runs_offset_pagination_returns_disjoint_pages() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping list_runs_offset_pagination — set AGENTFLOW_DATABASE_TEST_URL");
    return;
  };
  let tenant = format!("tenant-page-{}", Uuid::new_v4());
  let _ids = seed_runs(&state, &tenant, RunStatus::Queued, 5).await;

  // Page 1 — first 2 rows newest-first.
  let app = create_router(state.clone());
  let response = app
    .oneshot(
      Request::builder()
        .uri(format!("/v1/runs?tenant_id={tenant}&limit=2&offset=0"))
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
  let page1 = body["runs"].as_array().unwrap().clone();
  assert_eq!(page1.len(), 2);

  // Page 2 — next 2 rows. Must not overlap page 1.
  let app = create_router(state);
  let response = app
    .oneshot(
      Request::builder()
        .uri(format!("/v1/runs?tenant_id={tenant}&limit=2&offset=2"))
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
  let page2 = body["runs"].as_array().unwrap().clone();
  assert_eq!(page2.len(), 2);

  let page1_ids: Vec<&str> = page1.iter().map(|r| r["id"].as_str().unwrap()).collect();
  let page2_ids: Vec<&str> = page2.iter().map(|r| r["id"].as_str().unwrap()).collect();
  for id in &page2_ids {
    assert!(
      !page1_ids.contains(id),
      "pages must be disjoint; id {id} appeared in both"
    );
  }
}

#[tokio::test]
async fn list_runs_status_filter_isolates_running_rows() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping list_runs_status_filter — set AGENTFLOW_DATABASE_TEST_URL");
    return;
  };
  let tenant = format!("tenant-status-{}", Uuid::new_v4());
  let _running = seed_runs(&state, &tenant, RunStatus::Running, 3).await;
  let _queued = seed_runs(&state, &tenant, RunStatus::Queued, 2).await;
  let _failed = seed_runs(&state, &tenant, RunStatus::Failed, 1).await;

  let app = create_router(state);
  let response = app
    .oneshot(
      Request::builder()
        .uri(format!(
          "/v1/runs?tenant_id={tenant}&status=running&limit=10"
        ))
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
  let runs = body["runs"].as_array().unwrap();
  assert_eq!(runs.len(), 3, "only the 3 running rows must match");
  for run in runs {
    assert_eq!(run["status"], "running");
  }
}

#[tokio::test]
async fn list_runs_rejects_unknown_status_value() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping list_runs_rejects_unknown_status — set AGENTFLOW_DATABASE_TEST_URL");
    return;
  };
  let app = create_router(state);
  let response = app
    .oneshot(
      Request::builder()
        .uri("/v1/runs?tenant_id=default&status=invented_state")
        .body(Body::empty())
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
  assert_eq!(body["error"]["code"], "bad_request");
  let message = body["error"]["message"].as_str().unwrap();
  assert!(
    message.contains("invented_state"),
    "error must echo the bad value, got: {message}"
  );
}

#[tokio::test]
async fn list_runs_offset_beyond_total_returns_empty_page() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping list_runs_offset_beyond — set AGENTFLOW_DATABASE_TEST_URL");
    return;
  };
  let tenant = format!("tenant-empty-{}", Uuid::new_v4());
  let _ids = seed_runs(&state, &tenant, RunStatus::Queued, 2).await;
  let app = create_router(state);
  let response = app
    .oneshot(
      Request::builder()
        .uri(format!("/v1/runs?tenant_id={tenant}&limit=10&offset=500"))
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
    body["runs"].as_array().unwrap().is_empty(),
    "offset past total must produce an empty page, not an error"
  );
}

// (P10.13.1: the `/v1/runs/{id}/graph` endpoint + its two e2e tests
// were removed when `agentflow-viz` was deleted. Workflow DAG
// visualisation is intentionally out of scope; see
// `docs/ROADMAP_v2.md` Theme D for the decision rationale.)

// ── Authenticated path under production-style profile ─────────────────────

fn auth_token() -> String {
  "p23-test-token".to_string()
}

#[tokio::test]
async fn submit_run_without_token_is_rejected_under_auth() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping submit_run_without_token — set AGENTFLOW_DATABASE_TEST_URL");
    return;
  };
  let state = state.with_auth(Some(AuthConfig {
    expected_token: auth_token(),
  }));
  let app = create_router(state);

  let body = json!({"workflow": FIXED_DAG}).to_string();
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
  assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
  let body: Value = serde_json::from_slice(
    &axum::body::to_bytes(response.into_body(), 4096)
      .await
      .unwrap(),
  )
  .unwrap();
  assert_eq!(body["error"]["code"], "unauthorized");
}

#[tokio::test]
async fn submit_run_with_token_succeeds_under_auth() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping submit_run_with_token — set AGENTFLOW_DATABASE_TEST_URL");
    return;
  };
  let state = state.with_auth(Some(AuthConfig {
    expected_token: auth_token(),
  }));
  let app = create_router(state);

  let body = json!({"workflow": FIXED_DAG}).to_string();
  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/v1/runs")
        .header(CONTENT_TYPE, "application/json")
        .header("Authorization", format!("Bearer {}", auth_token()))
        .body(Body::from(body))
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
  assert!(body["run_id"].is_string());
  assert_eq!(body["status"], "queued");
}

// ── P2.6: tenant/session boundary ─────────────────────────────────────────

const TENANT_HEADER: &str = "x-agentflow-tenant";

#[tokio::test]
async fn cross_tenant_get_run_returns_404() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping cross_tenant_get_run_returns_404");
    return;
  };
  let id = Uuid::new_v4();
  state
    .repos
    .runs
    .create(NewRun {
      id,
      workflow: "x-tenant-isolation".into(),
      status: RunStatus::Queued,
      run_dir: None,
      tenant_id: format!("tenant-alpha-{}", Uuid::new_v4()),
      events_retention_days: None,
      artifacts_retention_days: None,
    })
    .await
    .unwrap();

  let app = create_router(state);
  // Tenant beta tries to read tenant-alpha's run — must 404 (hide
  // existence; don't leak with 403).
  let response = app
    .oneshot(
      Request::builder()
        .uri(format!("/v1/runs/{id}"))
        .header(
          TENANT_HEADER,
          format!("tenant-beta-{}", Uuid::new_v4()).as_str(),
        )
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);
  let body: Value = serde_json::from_slice(
    &axum::body::to_bytes(response.into_body(), 4096)
      .await
      .unwrap(),
  )
  .unwrap();
  assert_eq!(body["error"]["code"], "not_found");
}

#[tokio::test]
async fn cross_tenant_cancel_run_returns_404_and_leaves_row_intact() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping cross_tenant_cancel_run_returns_404_and_leaves_row_intact");
    return;
  };
  let id = Uuid::new_v4();
  state
    .repos
    .runs
    .create(NewRun {
      id,
      workflow: "x-tenant-cancel".into(),
      status: RunStatus::Running,
      run_dir: None,
      tenant_id: format!("tenant-owner-{}", Uuid::new_v4()),
      events_retention_days: None,
      artifacts_retention_days: None,
    })
    .await
    .unwrap();

  let app = create_router(state.clone());
  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri(format!("/v1/runs/{id}:cancel"))
        .header(
          TENANT_HEADER,
          format!("tenant-intruder-{}", Uuid::new_v4()).as_str(),
        )
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);

  // The row stays in `running` — the cancel attempt didn't transition it.
  let row = state.repos.runs.get(id).await.unwrap().unwrap();
  assert_eq!(row.status, "running");
}

#[tokio::test]
async fn same_tenant_get_run_succeeds_via_header_binding() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping same_tenant_get_run_succeeds_via_header_binding");
    return;
  };
  let id = Uuid::new_v4();
  state
    .repos
    .runs
    .create(NewRun {
      id,
      workflow: "header-bound-tenant".into(),
      status: RunStatus::Queued,
      run_dir: None,
      tenant_id: format!("tenant-correct-{}", Uuid::new_v4()),
      events_retention_days: None,
      artifacts_retention_days: None,
    })
    .await
    .unwrap();
  let owner = state.repos.runs.get(id).await.unwrap().unwrap().tenant_id;

  let app = create_router(state);
  let response = app
    .oneshot(
      Request::builder()
        .uri(format!("/v1/runs/{id}"))
        .header(TENANT_HEADER, owner.as_str())
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
  // RunResponse uses #[serde(flatten)] so Run fields land at the
  // top level of the body.
  assert_eq!(body["id"], id.to_string());
  assert_eq!(body["tenant_id"], owner);
}

#[tokio::test]
async fn list_runs_uses_header_tenant_when_query_param_absent() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping list_runs_uses_header_tenant_when_query_param_absent");
    return;
  };
  let alpha = format!("tenant-list-alpha-{}", Uuid::new_v4());
  let beta = format!("tenant-list-beta-{}", Uuid::new_v4());
  let _alpha = seed_runs(&state, &alpha, RunStatus::Queued, 2).await;
  let _beta = seed_runs(&state, &beta, RunStatus::Queued, 3).await;

  let app = create_router(state);
  let response = app
    .oneshot(
      Request::builder()
        .uri("/v1/runs")
        .header(TENANT_HEADER, alpha.as_str())
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: Value = serde_json::from_slice(
    &axum::body::to_bytes(response.into_body(), 8192)
      .await
      .unwrap(),
  )
  .unwrap();
  let runs = body["runs"].as_array().unwrap();
  assert_eq!(runs.len(), 2);
  for run in runs {
    assert_eq!(run["tenant_id"], alpha);
  }
}

#[tokio::test]
async fn list_runs_query_param_overrides_header() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping list_runs_query_param_overrides_header");
    return;
  };
  let alpha = format!("tenant-override-alpha-{}", Uuid::new_v4());
  let beta = format!("tenant-override-beta-{}", Uuid::new_v4());
  let _alpha = seed_runs(&state, &alpha, RunStatus::Queued, 2).await;
  let _beta = seed_runs(&state, &beta, RunStatus::Queued, 1).await;

  let app = create_router(state);
  // Header says alpha, query says beta — query wins for backward compat
  // with existing dashboards. Documented explicitly in the handler doc.
  let response = app
    .oneshot(
      Request::builder()
        .uri(format!("/v1/runs?tenant_id={beta}"))
        .header(TENANT_HEADER, alpha.as_str())
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body: Value = serde_json::from_slice(
    &axum::body::to_bytes(response.into_body(), 8192)
      .await
      .unwrap(),
  )
  .unwrap();
  let runs = body["runs"].as_array().unwrap();
  assert_eq!(runs.len(), 1);
  assert_eq!(runs[0]["tenant_id"], beta);
}

#[tokio::test]
async fn missing_tenant_header_defaults_to_default_tenant() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping missing_tenant_header_defaults_to_default_tenant");
    return;
  };
  let id = Uuid::new_v4();
  state
    .repos
    .runs
    .create(NewRun {
      id,
      workflow: "zero-config".into(),
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
        .uri(format!("/v1/runs/{id}"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn health_route_stays_open_under_auth() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping health_route_open_under_auth — set AGENTFLOW_DATABASE_TEST_URL");
    return;
  };
  let state = state.with_auth(Some(AuthConfig {
    expected_token: auth_token(),
  }));
  let app = create_router(state);

  // No token; /health must still succeed because the route is
  // unauthenticated by contract (used by orchestrators).
  let response = app
    .oneshot(
      Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(
    response.status(),
    StatusCode::OK,
    "health route must always be reachable without auth"
  );
}

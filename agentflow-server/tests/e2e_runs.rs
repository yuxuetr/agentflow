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

use agentflow_db::{Database, EventRepo, NewEvent, NewRun, RunRepo, RunStatus};
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
  sqlx::query("TRUNCATE runs RESTART IDENTITY CASCADE")
    .execute(&db.pool)
    .await
    .ok()?;
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
  let _ids = seed_runs(&state, "tenant-page", RunStatus::Queued, 5).await;

  // Page 1 — first 2 rows newest-first.
  let app = create_router(state.clone());
  let response = app
    .oneshot(
      Request::builder()
        .uri("/v1/runs?tenant_id=tenant-page&limit=2&offset=0")
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
        .uri("/v1/runs?tenant_id=tenant-page&limit=2&offset=2")
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
  let _running = seed_runs(&state, "tenant-status", RunStatus::Running, 3).await;
  let _queued = seed_runs(&state, "tenant-status", RunStatus::Queued, 2).await;
  let _failed = seed_runs(&state, "tenant-status", RunStatus::Failed, 1).await;

  let app = create_router(state);
  let response = app
    .oneshot(
      Request::builder()
        .uri("/v1/runs?tenant_id=tenant-status&status=running&limit=10")
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
  let _ids = seed_runs(&state, "tenant-empty", RunStatus::Queued, 2).await;
  let app = create_router(state);
  let response = app
    .oneshot(
      Request::builder()
        .uri("/v1/runs?tenant_id=tenant-empty&limit=10&offset=500")
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

// ── Graph snapshot before / during (covered upstream) / after ──────────────

#[tokio::test]
async fn get_run_graph_returns_snapshot_before_any_events() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping get_run_graph_before_events — set AGENTFLOW_DATABASE_TEST_URL");
    return;
  };
  let id = Uuid::new_v4();
  state
    .repos
    .runs
    .create(NewRun {
      id,
      workflow: r#"
name: Pre-Event Graph
nodes:
  - id: alpha
    type: template
  - id: beta
    type: template
    dependencies: [alpha]
"#
      .into(),
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
        .uri(format!("/v1/runs/{id}/graph"))
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
  assert!(body["graph"].is_object(), "graph snapshot must be present");
  // No events ⇒ no active node.
  assert!(
    body["active_node"].is_null(),
    "before any events the active_node must be null"
  );
  // The mermaid rendering still contains both node ids so a UI can
  // render the workflow shape even pre-run.
  let mermaid = body["mermaid"].as_str().unwrap();
  assert!(mermaid.contains("alpha"), "mermaid must list 'alpha' node");
  assert!(mermaid.contains("beta"), "mermaid must list 'beta' node");
}

#[tokio::test]
async fn get_run_graph_returns_snapshot_after_run_completes() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping get_run_graph_after_run_completes — set AGENTFLOW_DATABASE_TEST_URL");
    return;
  };
  let id = Uuid::new_v4();
  state
    .repos
    .runs
    .create(NewRun {
      id,
      workflow: r#"
name: Post-Run Graph
nodes:
  - id: alpha
    type: template
  - id: beta
    type: template
    dependencies: [alpha]
"#
      .into(),
      status: RunStatus::Running,
      run_dir: None,
      tenant_id: "default".into(),
    })
    .await
    .unwrap();

  // Replay node.started → node.completed for both nodes, then mark
  // the run succeeded.
  let events = [
    ("node.started", json!({"node_id": "alpha"})),
    ("node.completed", json!({"node_id": "alpha"})),
    ("node.started", json!({"node_id": "beta"})),
    ("node.completed", json!({"node_id": "beta"})),
  ];
  for (seq, (kind, payload)) in events.iter().enumerate() {
    state
      .repos
      .events
      .append(NewEvent {
        run_id: id,
        seq: seq as i64,
        kind: kind.to_string(),
        payload: payload.clone(),
      })
      .await
      .unwrap();
  }
  state
    .repos
    .runs
    .update_status(id, RunStatus::Succeeded, None)
    .await
    .unwrap();

  let app = create_router(state);
  let response = app
    .oneshot(
      Request::builder()
        .uri(format!("/v1/runs/{id}/graph"))
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
  assert!(body["graph"].is_object());
  // The route's `active_node` is "last-touched" not "currently-running",
  // so after the run finishes the last node that had a node.* event is
  // still surfaced. This is the contract downstream consumers depend on
  // for highlighting the last-known cursor in the UI.
  assert_eq!(
    body["active_node"], "beta",
    "the last touched node id must be surfaced as active_node"
  );
  let mermaid = body["mermaid"].as_str().unwrap();
  assert!(mermaid.contains("alpha"));
  assert!(mermaid.contains("beta"));
}

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

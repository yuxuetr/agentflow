//! End-to-end checks for `GET /v1/skills` + `POST /v1/skills/{name}:run`.
//!
//! Skill resolution requires only the in-memory catalog, so these tests
//! work without a live DB *for the listing path*. The submit-run path
//! reuses the runs table and is gated by `AGENTFLOW_DATABASE_TEST_URL`
//! the same way as the other server e2e tests.

use agentflow_db::Database;
use agentflow_server::{AppState, SkillCatalog, create_router};
use axum::{
  body::Body,
  http::{Request, StatusCode, header::CONTENT_TYPE},
};
use serde_json::json;
use tower::ServiceExt;

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
async fn list_skills_returns_empty_when_catalog_unset() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping list_skills_returns_empty_when_catalog_unset");
    return;
  };
  let app = create_router(state);

  let response = app
    .oneshot(
      Request::builder()
        .uri("/v1/skills")
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
  assert_eq!(body["skills"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn run_skill_returns_404_when_skill_unknown() {
  let Some(state) = fresh_state().await else {
    eprintln!("skipping run_skill_returns_404_when_skill_unknown");
    return;
  };
  let app = create_router(state.with_skills(SkillCatalog::empty()));

  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/v1/skills/missing:run")
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(json!({"input": "hi"}).to_string()))
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

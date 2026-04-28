use agentflow_db::Database;
use axum::{Json, Router, routing::get};
use serde::Serialize;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

pub mod error;

#[derive(Clone)]
pub struct AppState {
  pub db: Database,
}

pub fn create_router(state: AppState) -> Router {
  Router::new()
    .route("/health", get(health_check))
    .route("/health/live", get(liveness_check))
    .route("/health/ready", get(readiness_check))
    .layer(CorsLayer::permissive())
    .layer(TraceLayer::new_for_http())
    .with_state(state)
}

#[derive(Debug, Serialize)]
struct HealthResponse {
  status: &'static str,
  service: &'static str,
}

async fn health_check() -> Json<HealthResponse> {
  Json(healthy())
}

async fn liveness_check() -> Json<HealthResponse> {
  Json(healthy())
}

async fn readiness_check() -> Json<HealthResponse> {
  Json(healthy())
}

fn healthy() -> HealthResponse {
  HealthResponse {
    status: "ok",
    service: "agentflow-server",
  }
}

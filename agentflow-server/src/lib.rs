use agentflow_db::Database;
use axum::{Router, routing::get};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

pub mod error;

#[derive(Clone)]
pub struct AppState {
  pub db: Database,
}

pub fn create_router(state: AppState) -> Router {
  Router::new()
    .route("/health", get(health_check))
    .layer(CorsLayer::permissive())
    .layer(TraceLayer::new_for_http())
    .with_state(state)
}

async fn health_check() -> &'static str {
  "OK"
}

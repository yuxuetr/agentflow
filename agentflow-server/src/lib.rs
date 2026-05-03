//! AgentFlow Gateway server crate.
//!
//! v0.3.0 N8 milestone: a minimum viable control plane built on Axum that
//! lets clients submit, query, and stream workflow runs over HTTP. This
//! crate stays narrow on purpose — it owns routing, AuthN, error envelope,
//! and SSE plumbing; persistence lives in `agentflow-db` and execution in
//! `agentflow-core` / `agentflow-agents`.

use axum::{
  Json, Router, middleware,
  routing::{get, post},
};
use serde::Serialize;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use agentflow_db::{Database, Repositories};

pub mod auth;
pub mod error;
pub mod events_stream;
pub mod runs;

pub use auth::{AuthConfig, require_bearer_token};
pub use error::ApiError;
pub use events_stream::{
  EventBroker, EventSink, PersistingEventSink, StreamedEvent, list_events, publish_through,
  stream_events,
};
pub use runs::{
  CreateRunRequest, CreateRunResponse, RunContext, RunExecutor, RunResponse, StubExecutor,
  default_executor, get_run, submit_run,
};

/// Server-wide state injected into every handler.
#[derive(Clone)]
pub struct AppState {
  pub db: Database,
  pub repos: Repositories,
  /// Auth configuration. `None` means auth is disabled — used in tests and
  /// local dev. Production deployments should always populate this.
  pub auth: Option<AuthConfig>,
  /// Optional list of installed skills exposed via `/v1/skills`. Wrapped in
  /// `Arc` so handlers can share it without cloning the inner vec on every
  /// request.
  pub skills: Arc<Vec<String>>,
  /// Background executor for submitted runs. `Arc<dyn _>` so production
  /// deployments can swap in a real Flow runner while tests use
  /// [`StubExecutor`].
  pub executor: Arc<dyn RunExecutor>,
  /// Process-local broker that fans persisted run events out to SSE
  /// subscribers. Cloning is cheap (Arc-backed).
  pub event_broker: EventBroker,
}

impl std::fmt::Debug for AppState {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("AppState")
      .field("db", &self.db)
      .field("repos", &self.repos)
      .field("auth", &self.auth)
      .field("skills", &self.skills)
      .field("executor", &"<dyn RunExecutor>")
      .finish()
  }
}

impl AppState {
  pub fn new(db: Database) -> Self {
    let repos = Repositories::from_pool(db.pool.clone());
    Self {
      db,
      repos,
      auth: None,
      skills: Arc::new(Vec::new()),
      executor: default_executor(),
      event_broker: EventBroker::new(),
    }
  }

  /// Attach an auth configuration. `None` keeps auth disabled.
  pub fn with_auth(mut self, auth: Option<AuthConfig>) -> Self {
    self.auth = auth;
    self
  }

  /// Attach a custom run executor (e.g. wired to `agentflow-core::Flow`).
  pub fn with_executor(mut self, executor: Arc<dyn RunExecutor>) -> Self {
    self.executor = executor;
    self
  }
}

/// Build the gateway router with the standard health / authenticated split.
///
/// Health probes (`/health`, `/health/live`, `/health/ready`) are
/// intentionally unauthenticated so kubelet / load-balancer probes don't
/// need to know the API token. `/v1/*` routes inherit the bearer-token
/// middleware when [`AppState::auth`] is `Some`; otherwise they pass
/// through (only safe in tests / local dev).
pub fn create_router(state: AppState) -> Router {
  let health = Router::new()
    .route("/health", get(health_check))
    .route("/health/live", get(liveness_check))
    .route("/health/ready", get(readiness_check));

  let v1 = Router::new()
    .route("/v1/whoami", get(whoami))
    .route("/v1/runs", post(submit_run))
    .route("/v1/runs/:id", get(get_run))
    .route("/v1/runs/:id/events", get(stream_events));

  let v1 = match state.auth.clone() {
    Some(auth) => v1.layer(middleware::from_fn_with_state(auth, require_bearer_token)),
    None => v1,
  };

  Router::new()
    .merge(health)
    .merge(v1)
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

#[derive(Debug, Serialize)]
struct WhoamiResponse {
  authenticated: bool,
}

/// Smoke endpoint that requires bearer auth when configured. Subsequent
/// commits replace this with real `/v1/runs` etc., but it gives the auth
/// middleware something concrete to gate during initial rollout.
async fn whoami() -> Json<WhoamiResponse> {
  Json(WhoamiResponse {
    authenticated: true,
  })
}

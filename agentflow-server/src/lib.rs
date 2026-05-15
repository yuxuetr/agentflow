//! AgentFlow Gateway server crate.
//!
//! v0.3.0 N8 milestone: a minimum viable control plane built on Axum that
//! lets clients submit, query, and stream workflow runs over HTTP. This
//! crate stays narrow on purpose — it owns routing, AuthN, error envelope,
//! and SSE plumbing; persistence lives in `agentflow-db` and execution in
//! `agentflow-core` / `agentflow-agents`.

use axum::{
  Json, Router,
  extract::DefaultBodyLimit,
  http::{HeaderValue, Method, header},
  middleware,
  routing::{get, post},
};
use serde::Serialize;
use std::sync::Arc;
use tower_http::{
  cors::{AllowOrigin, Any, CorsLayer},
  trace::TraceLayer,
};

use agentflow_db::{Database, Repositories};
use agentflow_tools::{CorsMode, SecurityProfile, SecurityProfileDefaults};

pub mod auth;
pub mod cleanup;
pub mod error;
pub mod events_stream;
pub mod runs;
pub mod scheduler;
pub mod serve;
pub mod skills;
pub mod ui;

pub use auth::{
  AuthConfig, AuthConfigError, require_bearer_token, resolve_auth_config,
  resolve_auth_config_from_env,
};
pub use cleanup::{
  CleanupConfig, CleanupError, CleanupReport, DEFAULT_CLEANUP_INTERVAL, cleanup_expired,
};
pub use error::ApiError;
pub use events_stream::{
  EventBroker, EventSink, PersistingEventSink, StreamedEvent, WorkflowEventListener, list_events,
  publish_through, stream_events,
};
pub use runs::{
  CancelRunResponse, CreateRunRequest, CreateRunResponse, FlowRunExecutor, ListRunsQuery,
  ListRunsResponse, ResumePlanQuery, RunCancellationRegistry, RunContext, RunExecutor,
  RunGraphResponse, RunResponse, StubExecutor, cancel_run, default_executor, get_run,
  get_run_graph, get_run_resume_plan, list_runs, submit_run,
};
pub use scheduler::{
  DistributedDagRunResult, DistributedDagScheduler, DistributedNodeStatus, GrpcWorkerProtocol,
  GrpcWorkerService, InMemoryWorkerProtocol, NodeExecutionPayload, RunControlSnapshot,
  RunControlStatus, SELECTED_TRANSPORT, SchedulerError, StitchedWorkerTraceEvent, WorkerAssignment,
  WorkerControlPlane, WorkerControlServer, WorkerHeartbeat, WorkerId, WorkerProtocol, WorkerTask,
  WorkerTaskResult, WorkerTraceEvent, WorkerTransport, stitched_trace_to_otel_spans,
};
pub use serve::{
  AGENTFLOW_SERVE_BIND_ENV, DEFAULT_SERVE_BIND, ServeConfig, ServeError, ServeReadiness,
  StartupReport, build_startup_report, run, run_check,
};
pub use skills::{
  ListSkillsResponse, RunSkillRequest, SkillCatalog, SkillEntry, list_skills, run_skill,
};
pub use ui::{asset_response, index_html, ui_router};

pub const CORS_ALLOWED_ORIGINS_ENV: &str = "AGENTFLOW_CORS_ALLOWED_ORIGINS";
pub const MAX_REQUEST_BODY_BYTES_ENV: &str = "AGENTFLOW_MAX_REQUEST_BODY_BYTES";
pub const MAX_WORKFLOW_SUBMIT_BYTES_ENV: &str = "AGENTFLOW_MAX_WORKFLOW_SUBMIT_BYTES";
pub const MAX_SKILL_RUN_BYTES_ENV: &str = "AGENTFLOW_MAX_SKILL_RUN_BYTES";

/// Server-wide state injected into every handler.
#[derive(Clone)]
pub struct AppState {
  pub db: Database,
  pub repos: Repositories,
  /// Auth configuration. `None` means auth is disabled — used in tests and
  /// local dev. Production deployments should always populate this.
  pub auth: Option<AuthConfig>,
  /// Catalog of installed skills exposed via `/v1/skills` and resolved by
  /// `/v1/skills/{name}:run`. Empty when no `AGENTFLOW_SKILLS_INDEX` is
  /// configured — the routes still work, they just return 404 on resolve.
  pub skills: SkillCatalog,
  /// Background executor for submitted runs. Defaults to [`FlowRunExecutor`];
  /// tests can swap in [`StubExecutor`] when they only need route plumbing.
  pub executor: Arc<dyn RunExecutor>,
  /// Process-local broker that fans persisted run events out to SSE
  /// subscribers. Cloning is cheap (Arc-backed).
  pub event_broker: EventBroker,
  /// Process-local cancellation registry for queued/running background runs.
  pub cancellation_registry: RunCancellationRegistry,
  /// Active security profile and documented defaults. Enforcement is rolled
  /// out by the follow-up P1 tasks without changing local behavior here.
  pub security: SecurityProfileDefaults,
}

impl std::fmt::Debug for AppState {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("AppState")
      .field("db", &self.db)
      .field("repos", &self.repos)
      .field("auth", &self.auth)
      .field("skills", &self.skills)
      .field("executor", &"<dyn RunExecutor>")
      .field("cancellation_registry", &self.cancellation_registry)
      .field("security", &self.security)
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
      skills: SkillCatalog::empty(),
      executor: default_executor(),
      event_broker: EventBroker::new(),
      cancellation_registry: RunCancellationRegistry::new(),
      security: SecurityProfile::default().defaults(),
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

  /// Attach a populated skill catalog. Defaults to empty.
  pub fn with_skills(mut self, skills: SkillCatalog) -> Self {
    self.skills = skills;
    self
  }

  /// Attach the active security profile defaults.
  pub fn with_security_profile(mut self, profile: SecurityProfile) -> Self {
    self.security = profile.defaults();
    self
  }

  /// Attach fully-resolved security defaults, including server-side env
  /// overrides for CORS origins and request body limits.
  pub fn with_security_defaults(mut self, defaults: SecurityProfileDefaults) -> Self {
    self.security = defaults;
    self
  }
}

pub fn server_security_defaults_from_env(
  profile: SecurityProfile,
) -> Result<SecurityProfileDefaults, ServerHttpConfigError> {
  let mut defaults = profile.defaults();

  if let Some(origins) = comma_separated_env(CORS_ALLOWED_ORIGINS_ENV) {
    validate_origins(&origins)?;
    defaults.cors.allowed_origins = origins;
  }
  if let Some(limit) = u64_env(MAX_REQUEST_BODY_BYTES_ENV)? {
    defaults.request_limits.max_request_body_bytes = limit;
  }
  if let Some(limit) = u64_env(MAX_WORKFLOW_SUBMIT_BYTES_ENV)? {
    defaults.request_limits.max_workflow_submit_bytes = limit;
  }
  if let Some(limit) = u64_env(MAX_SKILL_RUN_BYTES_ENV)? {
    defaults.request_limits.max_skill_run_bytes = limit;
  }

  Ok(defaults)
}

#[derive(Debug, thiserror::Error)]
pub enum ServerHttpConfigError {
  #[error("{name} contains invalid HTTP origin '{value}': {source}")]
  InvalidOrigin {
    name: &'static str,
    value: String,
    source: axum::http::header::InvalidHeaderValue,
  },
  #[error("{name} must be a positive integer byte count, got '{value}'")]
  InvalidByteLimit { name: &'static str, value: String },
}

fn comma_separated_env(name: &'static str) -> Option<Vec<String>> {
  let value = std::env::var(name).ok()?;
  let values = value
    .split(',')
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .map(ToOwned::to_owned)
    .collect::<Vec<_>>();
  Some(values)
}

fn u64_env(name: &'static str) -> Result<Option<u64>, ServerHttpConfigError> {
  let Some(value) = std::env::var(name).ok() else {
    return Ok(None);
  };
  let limit = value
    .trim()
    .parse()
    .map_err(|_| ServerHttpConfigError::InvalidByteLimit {
      name,
      value: value.clone(),
    })?;
  if limit == 0 {
    return Err(ServerHttpConfigError::InvalidByteLimit { name, value });
  }
  Ok(Some(limit))
}

fn validate_origins(origins: &[String]) -> Result<(), ServerHttpConfigError> {
  for origin in origins {
    HeaderValue::from_str(origin).map_err(|source| ServerHttpConfigError::InvalidOrigin {
      name: CORS_ALLOWED_ORIGINS_ENV,
      value: origin.clone(),
      source,
    })?;
  }
  Ok(())
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

  let workflow_limit = state.security.request_limits.max_workflow_submit_bytes as usize;
  let skill_limit = state.security.request_limits.max_skill_run_bytes as usize;

  let v1 = Router::new()
    .route("/v1/whoami", get(whoami))
    .route(
      "/v1/runs",
      get(list_runs)
        .post(submit_run)
        .layer(DefaultBodyLimit::max(workflow_limit)),
    )
    .route("/v1/runs/:id", get(get_run).post(cancel_run))
    .route("/v1/runs/:id/graph", get(get_run_graph))
    .route("/v1/runs/:id/resume-plan", get(get_run_resume_plan))
    .route("/v1/runs/:id/events/history", get(list_events))
    .route("/v1/runs/:id/events", get(stream_events))
    .route("/v1/skills", get(list_skills))
    // The `:run` suffix is part of the path. Axum's pattern can't match a
    // literal segment containing `:`, so we capture the whole tail and
    // strip the suffix in the handler.
    .route(
      "/v1/skills/:name_run",
      post(run_skill).layer(DefaultBodyLimit::max(skill_limit)),
    );

  let v1 = match state.auth.clone() {
    Some(auth) => v1.layer(middleware::from_fn_with_state(auth, require_bearer_token)),
    None => v1,
  };

  Router::new()
    .merge(health)
    .merge(v1)
    .merge(ui_router())
    .layer(cors_layer(&state.security))
    .layer(TraceLayer::new_for_http())
    .with_state(state)
}

fn cors_layer(defaults: &SecurityProfileDefaults) -> CorsLayer {
  match defaults.cors.mode {
    CorsMode::Permissive => CorsLayer::permissive(),
    CorsMode::ExplicitOrigins => {
      let origins = defaults
        .cors
        .allowed_origins
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect::<Vec<_>>();
      let allow_origin = if origins.is_empty() {
        AllowOrigin::list(Vec::<HeaderValue>::new())
      } else {
        AllowOrigin::list(origins)
      };
      CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        .expose_headers(Any)
    }
  }
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

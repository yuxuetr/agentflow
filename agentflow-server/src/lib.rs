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
pub mod diagnostics;
pub mod error;
pub mod events_filter;
pub mod events_stream;
pub mod harness;
pub mod harness_approval;
pub mod harness_live;
pub mod live_state_registry;
pub mod metrics;
pub mod preferences;
pub mod runs;
pub mod scheduler;
pub mod serve;
pub mod skills;
pub mod tenant;
pub mod ui;

pub use auth::{
  AuthConfig, AuthConfigError, require_bearer_token, resolve_auth_config,
  resolve_auth_config_from_env,
};
pub use cleanup::{
  CleanupConfig, CleanupError, CleanupReport, DEFAULT_CLEANUP_INTERVAL, cleanup_expired,
};
pub use error::{ApiError, JsonReq};
pub use events_stream::{
  EventBroker, EventSink, PersistingEventSink, StreamedEvent, WorkflowEventListener, list_events,
  publish_through, stream_events,
};
pub use harness::{
  CancelHarnessSessionResponse, CreateHarnessSessionRequest, CreateHarnessSessionResponse,
  HarnessEventBroker, HarnessEventsQuery, HarnessSessionContext, HarnessSessionExecutor,
  HarnessSessionResponse, ListHarnessSessionsQuery, ListHarnessSessionsResponse,
  ResumeHarnessSessionRequest, ResumeHarnessSessionResponse, StreamedHarnessEvent,
  StubHarnessExecutor, cancel_harness_session, default_harness_executor, get_harness_session,
  list_harness_events, list_harness_sessions, post_harness_session_action, resume_harness_session,
  stream_harness_events, submit_harness_session,
};
pub use harness_approval::{
  ApprovalDecisionRequest, ApprovalDecisionResponse, ApprovalResolveError, PendingApprovalRegistry,
  PendingApprovalsResponse, ServerApprovalProvider, decide_approval, list_pending_approvals,
};
pub use harness_live::{LiveHarnessExecutor, ServerHarnessEventSink};
pub use live_state_registry::LiveStateRegistry;
pub use runs::{
  CancelRunResponse, CreateRunRequest, CreateRunResponse, FlowRunExecutor, ListRunsQuery,
  ListRunsResponse, ResumePlanQuery, RetentionOverrides, RunCancellationRegistry, RunContext,
  RunExecutor, RunResponse, StubExecutor, cancel_run, default_executor, get_run,
  get_run_resume_plan, list_runs, submit_run,
};
pub use scheduler::{
  AdmissionError, AuthenticatedControlPlane, ClaimHints, ControlError, DistributedDagRunResult,
  DistributedDagScheduler, DistributedNodeStatus, GrpcWorkerProtocol, GrpcWorkerService,
  InMemoryWorkerProtocol, NodeExecutionPayload, RunControlSnapshot, RunControlStatus,
  SELECTED_TRANSPORT, SchedulerError, StitchedWorkerTraceEvent, WorkerAdmissionPolicy,
  WorkerAssignment, WorkerCapabilities, WorkerControlPlane, WorkerControlServer, WorkerCredential,
  WorkerHeartbeat, WorkerId, WorkerProtocol, WorkerTask, WorkerTaskResult, WorkerTraceEvent,
  WorkerTransport, stitched_trace_to_otel_spans,
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
  /// Background executor for submitted harness sessions (P-H.5). Defaults
  /// to [`StubHarnessExecutor`] until the LLM-backed runtime lands.
  pub harness_executor: Arc<dyn harness::HarnessSessionExecutor>,
  /// Process-local broker that fans persisted harness session events out
  /// to SSE subscribers. Parallel to [`AppState::event_broker`] so a slow
  /// workflow subscriber can't lag a harness session and vice versa.
  pub harness_broker: HarnessEventBroker,
  /// Process-local pending-approval registry (P-H.5 slice 2). The
  /// `ServerApprovalProvider` parks each pending request here; the
  /// `POST /v1/harness/sessions/{id}/approvals/{request_id}` route
  /// resolves the oneshot from the HTTP side.
  pub approval_registry: PendingApprovalRegistry,
  /// Process-local snapshot of live `Flow::state_pool` sizes per active
  /// run (P10.14.2-FU6). Written by the DAG executor through the
  /// `StateSizeObserver` interface; read at scrape time by the
  /// `/metrics` handler to emit `agentflow_state_size_bytes{run_id}`.
  pub live_state_registry: live_state_registry::LiveStateRegistry,
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
      .field("harness_executor", &"<dyn HarnessSessionExecutor>")
      .field("harness_broker", &self.harness_broker)
      .field("security", &self.security)
      .finish()
  }
}

impl AppState {
  pub fn new(db: Database) -> Self {
    // P10.15.2: prefer the replica-aware constructor so reads
    // route to `db.read_pool()` when configured. Falls through
    // to the primary when no replica is set.
    let repos = Repositories::from_database(&db);
    Self {
      db,
      repos,
      auth: None,
      skills: SkillCatalog::empty(),
      executor: default_executor(),
      event_broker: EventBroker::new(),
      cancellation_registry: RunCancellationRegistry::new(),
      harness_executor: default_harness_executor(),
      harness_broker: HarnessEventBroker::new(),
      approval_registry: PendingApprovalRegistry::new(),
      live_state_registry: live_state_registry::LiveStateRegistry::new(),
      security: SecurityProfile::default().defaults(),
    }
  }

  /// Attach a custom harness session executor (e.g. wired to a real
  /// `agentflow-harness::HarnessRuntime`). Tests use this to keep the
  /// route layer + DB plumbing decoupled from the agent stack.
  pub fn with_harness_executor(
    mut self,
    executor: Arc<dyn harness::HarnessSessionExecutor>,
  ) -> Self {
    self.harness_executor = executor;
    self
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
/// Axum handler for the `GET /metrics` endpoint (P10.14.2-FU1).
/// Returns the current Prometheus snapshot as text. Bypasses auth
/// so Prometheus scrapers can poll without a bearer token — the
/// same convention as `/health`.
///
/// P10.14.2-FU4: gauges that aren't kept current by per-event
/// observers are refreshed at scrape time. The Harness session
/// status buckets and the pending-approval count fall into this
/// bucket — they're cheap to compute (one indexed `SELECT` +
/// one mutex read) and there's no benefit to maintaining them
/// out-of-band.
async fn prometheus_metrics(
  axum::extract::State(state): axum::extract::State<AppState>,
) -> impl axum::response::IntoResponse {
  // Refresh the scrape-time gauges before rendering. Failures
  // here are logged + swallowed — `/metrics` must never 500
  // because a single panel can't compute, otherwise the whole
  // scrape stops.
  refresh_scrape_time_gauges(&state).await;

  let body = metrics::render_text();
  (
    [(
      axum::http::header::CONTENT_TYPE,
      "text/plain; version=0.0.4; charset=utf-8",
    )],
    body,
  )
}

/// Run the scrape-time inspectors that compute gauges from
/// "live" sources (DB row counts, in-memory registries) rather
/// than per-event observation.
///
/// Adding a new scrape-time gauge: emit it from a helper in
/// `crate::metrics::observe_*`, then call the helper here. The
/// per-gauge cost is one query / mutex read; the total stays
/// well under typical scrape budgets (15-30s).
async fn refresh_scrape_time_gauges(state: &AppState) {
  // P10.14.2-FU4 — Harness session status buckets.
  match sqlx::query_as::<_, (String, i64)>(
    "SELECT status, COUNT(*)::BIGINT FROM harness_sessions GROUP BY status",
  )
  .fetch_all(state.db.read_pool())
  .await
  {
    Ok(rows) => {
      // Always emit all four known statuses so a status that
      // drops to zero renders as zero, not as a stale value.
      // Unknown statuses (DB / app drift) are ignored — we
      // don't invent a label the dashboard doesn't expect.
      let mut totals = std::collections::HashMap::<&'static str, u64>::from([
        ("running", 0),
        ("completed", 0),
        ("failed", 0),
        ("cancelled", 0),
      ]);
      for (status, count) in rows {
        if let Some(slot) = totals.get_mut(status.as_str()) {
          *slot = count.max(0) as u64;
        }
      }
      for (status, count) in totals {
        metrics::observe_harness_sessions_active(status, count);
      }
    }
    Err(err) => {
      // Common case in tests where the DB pool is lazy and the
      // server isn't actually connected to Postgres. Log at
      // debug so production scrape errors still surface but
      // tests stay quiet.
      tracing::debug!(
        error = %err,
        "refresh_scrape_time_gauges: harness session count query failed"
      );
    }
  }

  // P10.14.2-FU4 — pending approval count from the in-process
  // registry. Always succeeds.
  metrics::observe_harness_approvals_pending(state.approval_registry.pending_count());

  // P10.14.2-FU5 — health status per component.
  //
  // `component="system"` is always 1 if we're computing it,
  // because the only way to reach this code is via a successful
  // `/metrics` poll — the rest of the stat panel is hyperbole.
  // `component="database"` is a `SELECT 1` probe against the
  // read pool so it picks up replica unavailability too. Both
  // failures map to gauge 0 (Stat panel red); never block the
  // scrape.
  metrics::observe_health_status("system", true);
  let db_up = sqlx::query("SELECT 1")
    .execute(state.db.read_pool())
    .await
    .is_ok();
  metrics::observe_health_status("database", db_up);

  // P10.14.2-FU5 — process resident memory. Linux reads from
  // `/proc/self/statm`; non-Linux falls back to 0 with a debug
  // log. Production deployments are Linux 99% of the time.
  if let Some(bytes) = process_memory_bytes() {
    metrics::observe_memory_usage_bytes(bytes);
  } else {
    // Emit 0 so the panel renders cleanly on dev macOS / Windows
    // hosts and operators can still see the scrape worked.
    metrics::observe_memory_usage_bytes(0);
  }

  // P10.14.2-FU5 — active runs per tenant. `active` = queued
  // + running, never terminal. One indexed query against the
  // read pool. Same fail-soft pattern as the harness session
  // refresh — a query error skips this gauge but doesn't block
  // the rest of the scrape.
  match sqlx::query_as::<_, (String, i64)>(
    "SELECT tenant_id, COUNT(*)::BIGINT FROM runs \
       WHERE status IN ('queued', 'running') GROUP BY tenant_id",
  )
  .fetch_all(state.db.read_pool())
  .await
  {
    Ok(rows) => {
      for (tenant, count) in rows {
        metrics::observe_workflow_runs_active(&tenant, count.max(0) as u64);
      }
    }
    Err(err) => {
      tracing::debug!(
        error = %err,
        "refresh_scrape_time_gauges: workflow_runs_active query failed"
      );
    }
  }

  // P10.14.2-FU6 — live state-pool size per active run. Pure
  // in-process registry read: no DB, no syscall. The DAG executor
  // is responsible for deregistering on terminal transitions, so
  // the snapshot here only contains currently-running runs and
  // gauge cardinality stays bounded.
  for (run_id, bytes) in state.live_state_registry.snapshot() {
    metrics::observe_state_size_bytes(&run_id.to_string(), bytes);
  }
}

/// Best-effort process resident-memory probe (P10.14.2-FU5).
/// Reads `/proc/self/statm` on Linux (the second whitespace-
/// separated field is resident pages; multiplying by 4096
/// gives bytes — Linux page size is 4096 on every architecture
/// the gateway targets). Returns `None` on non-Linux + on any
/// I/O / parse failure so the caller can degrade to a `0`
/// gauge rather than fail the scrape.
fn process_memory_bytes() -> Option<u64> {
  #[cfg(target_os = "linux")]
  {
    let s = std::fs::read_to_string("/proc/self/statm").ok()?;
    let field = s.split_whitespace().nth(1)?;
    let pages: u64 = field.parse().ok()?;
    Some(pages.saturating_mul(4096))
  }
  #[cfg(not(target_os = "linux"))]
  {
    None
  }
}

pub fn create_router(state: AppState) -> Router {
  let health = Router::new()
    .route("/health", get(health_check))
    .route("/health/live", get(liveness_check))
    .route("/health/ready", get(readiness_check))
    // P10.14.2-FU1: Prometheus metrics endpoint. No auth — same
    // convention as `/health` so scrapers don't need a bearer
    // token. The Grafana dashboard
    // (`dashboards/grafana/agentflow-overview.json`) consumes
    // this surface.
    .route("/metrics", get(prometheus_metrics));

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
    )
    .route(
      "/v1/harness/sessions",
      get(list_harness_sessions)
        .post(submit_harness_session)
        .layer(DefaultBodyLimit::max(workflow_limit)),
    )
    // GET captures `:id` as Uuid; POST captures the raw path (including the
    // literal `:cancel` / `:resume` suffix) as String and dispatches inside
    // `post_harness_session_action`. Same single-route + dual-method trick
    // as `/v1/runs/:id`.
    .route(
      "/v1/harness/sessions/:id",
      get(get_harness_session).post(post_harness_session_action),
    )
    .route(
      "/v1/harness/sessions/:id/events/history",
      get(list_harness_events),
    )
    .route(
      "/v1/harness/sessions/:id/events",
      get(stream_harness_events),
    )
    .route(
      "/v1/harness/sessions/:id/approvals",
      get(list_pending_approvals),
    )
    .route(
      "/v1/harness/sessions/:id/approvals/:request_id",
      post(decide_approval),
    )
    .route("/v1/diagnostics", get(diagnostics::get_diagnostics))
    .route(
      "/v1/preferences",
      get(preferences::list_preferences).put(preferences::put_preferences),
    );

  let v1 = match state.auth.clone() {
    Some(auth) => v1.layer(middleware::from_fn_with_state(auth, require_bearer_token)),
    None => v1,
  };
  // P2.6: bind X-Agentflow-Tenant header into a TenantId extension on
  // every /v1/* request. Falls back to TenantId("default") when absent
  // so single-tenant local-dev stays zero-config.
  let v1 = v1.layer(middleware::from_fn(tenant::extract_tenant_id));

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

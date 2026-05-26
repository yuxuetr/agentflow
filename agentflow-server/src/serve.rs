//! Programmatic entry points for booting the AgentFlow Gateway.
//!
//! `agentflow_server::run(config)` and `agentflow_server::run_check(config)`
//! are the two public bootstrap functions used by:
//!
//! - `agentflow-server`'s own `main.rs` (binary entry point), and
//! - `agentflow-cli`'s `agentflow serve` command (P2.1).
//!
//! The CLI wraps `run` for the binding path and `run_check` for the
//! non-binding readiness diagnostics. Both share [`ServeConfig`] so
//! the two surfaces never drift.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Default deadline a `ServerApprovalProvider` waits for an operator
/// decision before timing out (P-H.5 slice 2). Five minutes balances UI
/// reaction time against not stalling a multi-tenant deployment
/// indefinitely.
const HARNESS_APPROVAL_TIMEOUT: Duration = Duration::from_secs(5 * 60);

use serde::Serialize;
use tracing::{error, info, warn};

use agentflow_db::Database;
use agentflow_tools::sandbox::{SandboxEnforcement, SandboxStatus, default_backend};
use agentflow_tools::{SECURITY_PROFILE_ENV, SecurityProfile};

use crate::auth::{AuthConfigError, resolve_auth_config_from_env};
use crate::skills::SkillCatalog;
use crate::{
  AppState, LiveHarnessExecutor, ServerHttpConfigError, create_router,
  server_security_defaults_from_env,
};

/// Default bind address when neither the CLI flag nor the env var is set.
pub const DEFAULT_SERVE_BIND: &str = "127.0.0.1:8080";

/// Env var consulted as the second-priority source for the bind address.
pub const AGENTFLOW_SERVE_BIND_ENV: &str = "AGENTFLOW_SERVE_BIND";

/// Programmatic config for booting the gateway. Both `run` and
/// `run_check` accept this struct so the CLI and the binary entry
/// point go through one validation funnel.
#[derive(Debug, Clone)]
pub struct ServeConfig {
  /// `host:port` to bind to.
  pub bind: SocketAddr,
  /// Postgres connection URL. The `--check` path treats absence as a
  /// warning; the binding path treats absence as a hard error.
  pub database_url: Option<String>,
  /// Optional read-replica connection URL (P10.15.2). When set,
  /// every `get_*` / `list_*` repo method routes to this pool;
  /// `INSERT` / `UPDATE` / `DELETE` always hit `database_url`.
  /// When absent (the default), reads fall back to the primary.
  pub read_database_url: Option<String>,
  /// Optional override for `AGENTFLOW_RUN_DIR`.
  pub run_dir: Option<PathBuf>,
  /// Optional override for `AGENTFLOW_TRACE_DIR`.
  pub trace_dir: Option<PathBuf>,
  /// Active security profile.
  pub security_profile: SecurityProfile,
  /// Env var name that carries the bearer token. Defaults to
  /// `AGENTFLOW_API_TOKEN`.
  pub auth_token_env: String,
  /// Explicit list of CORS-allowed origins. Empty list defers to the
  /// security profile defaults.
  pub cors_origins: Vec<String>,
  /// Request body cap in megabytes. `None` defers to the security
  /// profile default.
  pub max_body_mb: Option<u64>,
}

impl ServeConfig {
  pub fn defaults() -> Self {
    Self {
      bind: DEFAULT_SERVE_BIND
        .parse()
        .expect("DEFAULT_SERVE_BIND is a valid SocketAddr"),
      database_url: None,
      read_database_url: None,
      run_dir: None,
      trace_dir: None,
      security_profile: SecurityProfile::default(),
      auth_token_env: "AGENTFLOW_API_TOKEN".to_string(),
      cors_origins: Vec::new(),
      max_body_mb: None,
    }
  }
}

/// Errors surfaced by the serve entry points.
#[derive(Debug, thiserror::Error)]
pub enum ServeError {
  #[error("invalid bind address '{value}': {source}")]
  InvalidBind {
    value: String,
    #[source]
    source: std::net::AddrParseError,
  },
  #[error("DATABASE_URL is required to start the gateway")]
  MissingDatabaseUrl,
  #[error("database connection failed: {0}")]
  Database(String),
  #[error("auth configuration error: {0}")]
  Auth(#[from] AuthConfigError),
  #[error(transparent)]
  HttpConfig(#[from] ServerHttpConfigError),
  #[error("failed to bind {bind}: {source}")]
  Bind {
    bind: SocketAddr,
    #[source]
    source: std::io::Error,
  },
  #[error("server runtime error: {0}")]
  Runtime(String),
  #[error("readiness check failed: {0}")]
  ReadinessFailed(String),
}

/// Tri-state readiness verdict used by `agentflow serve --check`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ServeReadiness {
  Ok,
  Warn,
  Fail,
}

impl ServeReadiness {
  pub fn exit_code(&self) -> i32 {
    match self {
      Self::Ok => 0,
      Self::Warn => 1,
      Self::Fail => 2,
    }
  }

  fn promote(&mut self, other: Self) {
    if other > *self {
      *self = other;
    }
  }
}

impl PartialOrd for ServeReadiness {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for ServeReadiness {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    self.rank().cmp(&other.rank())
  }
}

impl ServeReadiness {
  fn rank(&self) -> u8 {
    match self {
      Self::Ok => 0,
      Self::Warn => 1,
      Self::Fail => 2,
    }
  }
}

/// Structured startup diagnostics emitted by `run_check` and (in
/// human form) `run`. Secrets are never embedded; the auth section
/// only records the env var name + a bool flag.
#[derive(Debug, Clone, Serialize)]
pub struct StartupReport {
  pub version: &'static str,
  pub bind: String,
  pub security_profile: SecurityProfile,
  pub auth: AuthReport,
  pub database: DatabaseReport,
  pub paths: PathsReport,
  pub sandbox: SandboxReport,
  pub plugin_runtime: PluginRuntimeReport,
  pub readiness: ServeReadiness,
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub warnings: Vec<String>,
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthReport {
  pub token_env: String,
  pub token_present: bool,
  pub require_token: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DatabaseReport {
  pub url_present: bool,
  pub host: Option<String>,
  pub reachable: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PathsReport {
  pub run_dir: Option<String>,
  pub trace_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SandboxReport {
  pub backend: String,
  pub enforcement: SandboxEnforcement,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginRuntimeReport {
  /// `true` if `agentflow-cli` was compiled with the `plugin` feature.
  /// We can only observe the compile-time flag at the binary boundary,
  /// not here; the check command supplies the value through the
  /// config's `cors_origins` runtime hook is therefore not appropriate.
  /// Instead we record the env-var snapshot (`AGENTFLOW_PLUGINS_DIR`).
  pub plugins_dir: Option<String>,
}

/// Build a [`StartupReport`] without binding any sockets. Reaches out
/// to Postgres (when configured) to confirm reachability; otherwise
/// emits a `Warn` readiness.
pub async fn build_startup_report(config: &ServeConfig) -> StartupReport {
  let mut warnings = Vec::new();
  let mut errors = Vec::new();
  let mut readiness = ServeReadiness::Ok;

  // Auth.
  let token_present = std::env::var(&config.auth_token_env)
    .map(|value| !value.trim().is_empty())
    .unwrap_or(false);
  let defaults = config.security_profile.defaults();
  let require_token = defaults.auth.require_api_token;
  if require_token && !token_present {
    errors.push(format!(
      "{} profile requires bearer auth but ${} is not set",
      config.security_profile, config.auth_token_env
    ));
    readiness.promote(ServeReadiness::Fail);
  } else if !require_token && !token_present {
    warnings.push(format!(
      "${} is not set; the gateway will run without bearer auth (acceptable for `{}`)",
      config.auth_token_env, config.security_profile
    ));
    readiness.promote(ServeReadiness::Warn);
  }

  // Database reachability.
  let mut db_report = DatabaseReport {
    url_present: config.database_url.is_some(),
    host: config.database_url.as_deref().and_then(database_host),
    reachable: None,
    error: None,
  };
  match &config.database_url {
    None => {
      warnings.push(
        "DATABASE_URL is not set; the gateway cannot accept run submissions until it is configured"
          .into(),
      );
      readiness.promote(ServeReadiness::Warn);
    }
    Some(url) => match Database::connect_and_migrate(url, 4).await {
      Ok(_) => {
        db_report.reachable = Some(true);
      }
      Err(err) => {
        db_report.reachable = Some(false);
        db_report.error = Some(err.to_string());
        errors.push(format!("database unreachable: {err}"));
        readiness.promote(ServeReadiness::Fail);
      }
    },
  }

  // Sandbox.
  let backend = default_backend();
  let sandbox_status = SandboxStatus::from_backend(backend.as_ref());
  let sandbox_report = SandboxReport {
    backend: sandbox_status.backend,
    enforcement: sandbox_status.enforcement,
  };

  // Plugin runtime hint (we only observe env state from here).
  let plugin_report = PluginRuntimeReport {
    plugins_dir: std::env::var("AGENTFLOW_PLUGINS_DIR").ok(),
  };

  let paths_report = PathsReport {
    run_dir: config
      .run_dir
      .as_ref()
      .map(|p| p.display().to_string())
      .or_else(|| std::env::var("AGENTFLOW_RUN_DIR").ok()),
    trace_dir: config
      .trace_dir
      .as_ref()
      .map(|p| p.display().to_string())
      .or_else(|| std::env::var("AGENTFLOW_TRACE_DIR").ok()),
  };

  StartupReport {
    version: env!("CARGO_PKG_VERSION"),
    bind: config.bind.to_string(),
    security_profile: config.security_profile,
    auth: AuthReport {
      token_env: config.auth_token_env.clone(),
      token_present,
      require_token,
    },
    database: db_report,
    paths: paths_report,
    sandbox: sandbox_report,
    plugin_runtime: plugin_report,
    readiness,
    warnings,
    errors,
  }
}

/// Run the gateway diagnostics without binding any sockets. Used by
/// `agentflow serve --check` and CI smoke tests.
pub async fn run_check(config: ServeConfig) -> Result<StartupReport, ServeError> {
  Ok(build_startup_report(&config).await)
}

/// Boot the gateway and serve forever. The function returns only when
/// the underlying axum server exits (e.g. on SIGTERM / drop of the
/// listener).
pub async fn run(config: ServeConfig) -> Result<(), ServeError> {
  // P10.14.2-FU1: install the Prometheus recorder once at boot so
  // any `metrics::counter!()` / `metrics::histogram!()` call
  // throughout the workspace contributes to the `/metrics`
  // snapshot. Failure is logged but not fatal — the rest of the
  // gateway boots and `/metrics` returns an empty body, which is
  // the documented behaviour when no recorder is installed (see
  // `dashboards/README.md` "Current emission status").
  if let Err(err) = crate::metrics::init_recorder() {
    warn!("Failed to install Prometheus metrics recorder: {err}");
  } else {
    info!("Prometheus metrics recorder installed; /metrics endpoint is live.");
  }

  // Apply env-shaped overrides for downstream tooling that still reads
  // from env (legacy paths). Set vars only when explicit overrides are
  // supplied so existing process env is preserved.
  // SAFETY: env mutation occurs once at startup before any reads.
  unsafe {
    if let Some(dir) = config.run_dir.as_ref() {
      std::env::set_var("AGENTFLOW_RUN_DIR", dir);
    }
    if let Some(dir) = config.trace_dir.as_ref() {
      std::env::set_var("AGENTFLOW_TRACE_DIR", dir);
    }
    std::env::set_var(SECURITY_PROFILE_ENV, format!("{}", config.security_profile));
    if !config.cors_origins.is_empty() {
      std::env::set_var(
        crate::CORS_ALLOWED_ORIGINS_ENV,
        config.cors_origins.join(","),
      );
    }
    if let Some(mb) = config.max_body_mb {
      std::env::set_var(
        crate::MAX_REQUEST_BODY_BYTES_ENV,
        (mb * 1024 * 1024).to_string(),
      );
    }
  }

  let db_url = config
    .database_url
    .clone()
    .ok_or(ServeError::MissingDatabaseUrl)?;

  info!("Initializing database connection and applying migrations…");
  let db = if let Some(read_url) = config.read_database_url.clone() {
    info!("Read-replica URL configured; reads will route to the replica.");
    // The replica pool gets 2× the primary's connection budget on
    // the assumption that the gateway is read-heavy. Operators with
    // unusual ratios can rebuild from the same primitives via
    // `Database::connect_with_replica` directly.
    Database::connect_and_migrate_with_replica(&db_url, &read_url, 8, 16)
      .await
      .map_err(|e| {
        error!("Failed to connect to database (primary or replica): {e}");
        ServeError::Database(e.to_string())
      })?
  } else {
    Database::connect_and_migrate(&db_url, 8)
      .await
      .map_err(|e| {
        error!("Failed to connect to database: {e}");
        ServeError::Database(e.to_string())
      })?
  };

  let security_defaults =
    server_security_defaults_from_env(config.security_profile).map_err(ServeError::HttpConfig)?;
  info!("Using '{}' security profile", config.security_profile);

  let auth = resolve_auth_config_from_env(config.security_profile).map_err(ServeError::Auth)?;
  if auth.is_none() && !security_defaults.auth.require_api_token {
    warn!(
      "{} is not set; the gateway is running without bearer auth.",
      config.auth_token_env
    );
  }

  let state = AppState::new(db.clone())
    .with_security_defaults(security_defaults)
    .with_auth(auth)
    .with_skills(SkillCatalog::from_env());
  // Swap the default `StubHarnessExecutor` for the LLM-backed
  // `LiveHarnessExecutor` (P-H.5 slice 2). Tests keep the stub by
  // building `AppState::new(db)` without this hop, so unit tests don't
  // pay for `AgentFlow::init` or contact an LLM provider.
  let live_harness =
    LiveHarnessExecutor::new(state.approval_registry.clone(), HARNESS_APPROVAL_TIMEOUT);
  let state: Arc<AppState> = Arc::new(state.with_harness_executor(Arc::new(live_harness)));
  let app = create_router((*state).clone());

  // Spawn the background cleanup loop (`P2.2`). Uses the active
  // security profile's retention defaults; the interval defaults to
  // 1 h. Failures are logged but never crash the gateway.
  spawn_cleanup_loop(
    db.clone(),
    config.security_profile,
    config.run_dir.clone(),
    config.trace_dir.clone(),
  );

  info!("Starting AgentFlow Gateway on {}", config.bind);
  let listener = tokio::net::TcpListener::bind(config.bind)
    .await
    .map_err(|err| ServeError::Bind {
      bind: config.bind,
      source: err,
    })?;

  // Q3.1.1: graceful shutdown on SIGTERM / Ctrl-C. Without this, k8s
  // rolling deploys (which send SIGTERM and expect the pod to drain in
  // its terminationGracePeriodSeconds window) instead killed every
  // in-flight HTTP request mid-flight, leaving harness sessions /
  // run submissions half-applied. The shutdown signal also fires on
  // local Ctrl-C so `cargo run -p agentflow-server` exits cleanly.
  axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal())
    .await
    .map_err(|err| ServeError::Runtime(err.to_string()))
}

/// Q3.1.1 + Q5.3: shutdown trigger — fires when the runtime receives
/// either `SIGTERM` (k8s, systemd, supervisord) or `SIGINT` / Ctrl-C.
/// On non-unix targets only Ctrl-C is honored. The future resolves
/// once any signal arrives, which
/// `axum::serve(...).with_graceful_shutdown` uses as the cue to stop
/// accepting new connections and drain in-flight requests.
///
/// Q5.3: delegates to the shared `agentflow_core::shutdown` helper
/// so the server, CLI, and worker share one implementation. Replaces
/// the pre-Q5.3 inlined `.expect("install … signal handler")` calls
/// — the helper now logs + falls through on install failure instead
/// of panicking the gateway process at startup.
async fn shutdown_signal() {
  let reason = agentflow_core::shutdown::shutdown_signal_with_reason().await;
  match reason {
    agentflow_core::shutdown::ShutdownReason::Interrupt => {
      info!("received SIGINT / Ctrl-C; beginning graceful shutdown");
    }
    agentflow_core::shutdown::ShutdownReason::Terminate => {
      info!("received SIGTERM; beginning graceful shutdown");
    }
  }
}

fn spawn_cleanup_loop(
  db: Database,
  profile: SecurityProfile,
  run_dir: Option<std::path::PathBuf>,
  trace_dir: Option<std::path::PathBuf>,
) {
  let cfg = crate::cleanup::CleanupConfig::for_profile(profile);
  tokio::spawn(async move {
    // Initial delay so the gateway is fully serving before the
    // first sweep kicks in.
    tokio::time::sleep(cfg.interval).await;
    loop {
      match crate::cleanup::cleanup_expired(
        &db,
        run_dir.as_deref(),
        trace_dir.as_deref(),
        &cfg,
      )
      .await
      {
        Ok(report) => {
          info!(
            runs_deleted = report.runs_deleted,
            events_deleted = report.events_deleted,
            artifacts_deleted = report.artifacts_deleted,
            run_dirs_deleted = report.run_dirs_deleted,
            trace_files_deleted = report.trace_files_deleted,
            "background cleanup completed"
          );
        }
        Err(err) => {
          warn!(error = %err, "background cleanup failed (will retry next interval)");
        }
      }
      tokio::time::sleep(cfg.interval).await;
    }
  });
}

fn database_host(url: &str) -> Option<String> {
  // Strip scheme.
  let after_scheme = url.split("://").nth(1)?;
  // Strip optional `user[:pass]@` prefix.
  let host_and_more = after_scheme.split('@').next_back()?;
  // Strip path / query.
  let host_with_port = host_and_more.split('/').next()?;
  Some(host_with_port.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn defaults_parse_default_bind_address() {
    let cfg = ServeConfig::defaults();
    assert_eq!(cfg.bind.to_string(), "127.0.0.1:8080");
    assert_eq!(cfg.auth_token_env, "AGENTFLOW_API_TOKEN");
    assert!(cfg.database_url.is_none());
  }

  #[test]
  fn readiness_promotes_to_higher_severity() {
    let mut readiness = ServeReadiness::Ok;
    readiness.promote(ServeReadiness::Warn);
    assert_eq!(readiness, ServeReadiness::Warn);
    readiness.promote(ServeReadiness::Ok);
    assert_eq!(readiness, ServeReadiness::Warn);
    readiness.promote(ServeReadiness::Fail);
    assert_eq!(readiness, ServeReadiness::Fail);
  }

  #[test]
  fn database_host_handles_full_postgres_url() {
    let host = database_host("postgres://user:pass@db.example:5432/agentflow").unwrap();
    assert_eq!(host, "db.example:5432");
  }

  #[test]
  fn database_host_handles_bare_url() {
    let host = database_host("postgres://localhost:5432/agentflow").unwrap();
    assert_eq!(host, "localhost:5432");
  }

  #[tokio::test]
  async fn run_check_without_database_emits_warning() {
    let mut cfg = ServeConfig::defaults();
    cfg.security_profile = SecurityProfile::Local;
    let report = run_check(cfg).await.unwrap();
    assert!(!report.database.url_present);
    assert!(report.readiness >= ServeReadiness::Warn);
    assert!(
      report
        .warnings
        .iter()
        .any(|w| w.contains("DATABASE_URL is not set"))
    );
  }

  #[tokio::test]
  async fn run_check_production_without_auth_token_fails() {
    let mut cfg = ServeConfig::defaults();
    cfg.security_profile = SecurityProfile::Production;
    cfg.auth_token_env = "AGENTFLOW_API_TOKEN_TEST_MISSING".into();
    // SAFETY: we use a dedicated env var name so concurrent tests cannot collide.
    unsafe {
      std::env::remove_var("AGENTFLOW_API_TOKEN_TEST_MISSING");
    }
    let report = run_check(cfg).await.unwrap();
    assert_eq!(report.readiness, ServeReadiness::Fail);
    assert!(
      report
        .errors
        .iter()
        .any(|e| e.contains("requires bearer auth"))
    );
  }

  #[tokio::test]
  async fn run_check_local_profile_with_token_is_ok() {
    let mut cfg = ServeConfig::defaults();
    cfg.security_profile = SecurityProfile::Local;
    cfg.auth_token_env = "AGENTFLOW_API_TOKEN_TEST_PRESENT".into();
    // SAFETY: dedicated env var name.
    unsafe {
      std::env::set_var("AGENTFLOW_API_TOKEN_TEST_PRESENT", "topsecret");
    }
    // Suppress DB warning by leaving database_url None and accepting Warn.
    let report = run_check(cfg).await.unwrap();
    assert!(report.auth.token_present);
    // SAFETY: cleanup the dedicated env var.
    unsafe {
      std::env::remove_var("AGENTFLOW_API_TOKEN_TEST_PRESENT");
    }
    // Without a database URL we still expect Warn; promote check.
    assert!(report.readiness >= ServeReadiness::Warn);
  }
}

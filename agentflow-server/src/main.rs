//! Binary entry for the AgentFlow Gateway. All real boot logic lives
//! in [`agentflow_server::serve`] so the same code path is shared with
//! the `agentflow serve` CLI subcommand (P2.1).
//!
//! Flag surface:
//! - `--check` — non-binding readiness diagnostics; prints a JSON
//!   report and exits with `0` (Ok), `1` (Warn), or `2` (Fail).
//! - any other args are passed through but ignored for now.
//!
//! All other configuration is taken from environment variables so the
//! `agentflow serve` CLI subcommand can drive this binary by setting
//! env vars + arguments without linking the server crate (which would
//! introduce a cycle with `agentflow-cli`).

use std::net::SocketAddr;

use agentflow_db::Database;
use agentflow_server::{
  AGENTFLOW_SERVE_BIND_ENV, CleanupConfig, DEFAULT_SERVE_BIND, ServeConfig, ServeError,
  cleanup_expired, run, run_check,
};
use agentflow_tools::{SECURITY_PROFILE_ENV, SecurityProfile};
use std::path::PathBuf;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  tracing_subscriber::registry()
    .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
    .with(tracing_subscriber::fmt::layer())
    .init();

  let _ = dotenvy::dotenv();

  let args: Vec<String> = std::env::args().collect();
  let check_mode = args.iter().any(|arg| arg == "--check");
  let cleanup_mode = args.iter().any(|arg| arg == "--cleanup");
  let dry_run = args.iter().any(|arg| arg == "--dry-run");

  let config = build_config_from_env()?;

  if check_mode {
    let report = run_check(config).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    std::process::exit(report.readiness.exit_code());
  }

  if cleanup_mode {
    let report = run_cleanup_once(&config, dry_run).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    return Ok(());
  }

  match run(config).await {
    Ok(()) => Ok(()),
    Err(err) => {
      eprintln!("{err}");
      match &err {
        ServeError::Database(_) | ServeError::MissingDatabaseUrl => Err(err.into()),
        _ => Err(err.into()),
      }
    }
  }
}

fn build_config_from_env() -> Result<ServeConfig, Box<dyn std::error::Error>> {
  let bind = resolve_bind()?;
  let security_profile = SecurityProfile::from_env().map_err(|err| {
    eprintln!("Invalid {SECURITY_PROFILE_ENV}: {err}");
    err
  })?;
  let database_url = std::env::var("DATABASE_URL")
    .ok()
    .filter(|value| !value.trim().is_empty());
  let auth_token_env = std::env::var("AGENTFLOW_SERVE_AUTH_TOKEN_ENV")
    .unwrap_or_else(|_| "AGENTFLOW_API_TOKEN".to_string());

  Ok(ServeConfig {
    bind,
    database_url,
    run_dir: std::env::var("AGENTFLOW_RUN_DIR").ok().map(Into::into),
    trace_dir: std::env::var("AGENTFLOW_TRACE_DIR").ok().map(Into::into),
    security_profile,
    auth_token_env,
    cors_origins: Vec::new(),
    max_body_mb: None,
  })
}

async fn run_cleanup_once(
  config: &ServeConfig,
  dry_run: bool,
) -> Result<agentflow_server::CleanupReport, Box<dyn std::error::Error>> {
  let db_url = config
    .database_url
    .as_ref()
    .ok_or("DATABASE_URL is required for cleanup")?;
  let db = Database::connect_and_migrate(db_url, 4).await?;
  let cleanup_cfg = CleanupConfig::for_profile(config.security_profile).with_dry_run(dry_run);
  let run_root: Option<PathBuf> = config.run_dir.clone();
  let report = cleanup_expired(&db, run_root.as_deref(), &cleanup_cfg).await?;
  Ok(report)
}

fn resolve_bind() -> Result<SocketAddr, std::net::AddrParseError> {
  // Backwards compatibility: prefer the historical `PORT` env, then the
  // new `AGENTFLOW_SERVE_BIND`, then the documented default.
  if let Ok(port) = std::env::var("PORT") {
    return format!("0.0.0.0:{port}").parse();
  }
  if let Ok(addr) = std::env::var(AGENTFLOW_SERVE_BIND_ENV) {
    return addr.parse();
  }
  DEFAULT_SERVE_BIND.parse()
}

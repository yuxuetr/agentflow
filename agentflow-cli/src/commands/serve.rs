//! `agentflow serve` — spawn the `agentflow-server` binary with the
//! configured bind address, security profile, and operational paths.
//!
//! The CLI deliberately does not link against `agentflow-server` (that
//! would cycle through `agentflow-cli`), so this command is a thin
//! wrapper that:
//!
//! 1. Locates the `agentflow-server` binary (sibling of the current
//!    executable, then PATH lookup).
//! 2. Translates CLI flags into the env vars / arguments the server
//!    binary already honours.
//! 3. Spawns the binary, forwards its stdout/stderr, and propagates
//!    its exit code.
//!
//! `--check` runs the server's non-binding readiness diagnostic which
//! does not require Postgres.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};

pub const DEFAULT_BIND: &str = "127.0.0.1:8080";
pub const AGENTFLOW_SERVE_BIND_ENV: &str = "AGENTFLOW_SERVE_BIND";

#[allow(clippy::too_many_arguments)]
pub async fn execute(
  bind: Option<String>,
  database_url: Option<String>,
  read_database_url: Option<String>,
  run_dir: Option<String>,
  trace_dir: Option<String>,
  security_profile: String,
  auth_token_env: String,
  cors_origins: Vec<String>,
  max_body_mb: Option<u64>,
  check: bool,
) -> Result<()> {
  let server_bin = locate_server_binary().context(
    "could not find the `agentflow-server` executable — is it installed alongside `agentflow`?",
  )?;

  let bind_value = bind
    .or_else(|| std::env::var(AGENTFLOW_SERVE_BIND_ENV).ok())
    .unwrap_or_else(|| DEFAULT_BIND.to_string());

  let mut cmd = Command::new(&server_bin);
  cmd.env(AGENTFLOW_SERVE_BIND_ENV, &bind_value);
  cmd.env("AGENTFLOW_SECURITY_PROFILE", &security_profile);
  cmd.env("AGENTFLOW_SERVE_AUTH_TOKEN_ENV", &auth_token_env);
  if let Some(url) = database_url.or_else(|| std::env::var("DATABASE_URL").ok()) {
    cmd.env("DATABASE_URL", url);
  }
  // P10.15.2 read-replica forwarding. The server binary reads
  // `AGENTFLOW_DATABASE_READ_URL` directly; the CLI just
  // forwards the value it got (flag → env passthrough fallback).
  if let Some(url) = read_database_url.or_else(|| std::env::var("AGENTFLOW_DATABASE_READ_URL").ok())
  {
    cmd.env("AGENTFLOW_DATABASE_READ_URL", url);
  }
  if let Some(dir) = run_dir.or_else(|| std::env::var("AGENTFLOW_RUN_DIR").ok()) {
    cmd.env("AGENTFLOW_RUN_DIR", dir);
  }
  if let Some(dir) = trace_dir.or_else(|| std::env::var("AGENTFLOW_TRACE_DIR").ok()) {
    cmd.env("AGENTFLOW_TRACE_DIR", dir);
  }
  if !cors_origins.is_empty() {
    cmd.env("AGENTFLOW_CORS_ALLOWED_ORIGINS", cors_origins.join(","));
  }
  if let Some(mb) = max_body_mb {
    let bytes = mb.saturating_mul(1024 * 1024);
    cmd.env("AGENTFLOW_MAX_REQUEST_BODY_BYTES", bytes.to_string());
  }

  if check {
    cmd.arg("--check");
  }

  // Print a short startup banner (text mode only) before handing the
  // process off so operators see what was about to run.
  if !check {
    eprintln!("🚀 agentflow serve");
    eprintln!("   bind: {bind_value}");
    eprintln!("   security-profile: {security_profile}");
    eprintln!("   auth-token-env: {auth_token_env}");
    eprintln!("   binary: {}", server_bin.display());
  }

  let status = cmd.status().with_context(|| {
    format!(
      "failed to spawn agentflow-server at {}",
      server_bin.display()
    )
  })?;
  if status.success() {
    Ok(())
  } else if let Some(code) = status.code() {
    std::process::exit(code);
  } else {
    Err(anyhow::anyhow!(
      "agentflow-server terminated by signal: {status}"
    ))
  }
}

/// Locate the `agentflow-server` binary. Prefers a sibling of the
/// current executable (works for `cargo build` workspaces and `cargo
/// install` deployments) and falls back to PATH lookup.
fn locate_server_binary() -> Result<PathBuf> {
  let exe_name = if cfg!(windows) {
    "agentflow-server.exe"
  } else {
    "agentflow-server"
  };

  if let Ok(current) = std::env::current_exe()
    && let Some(dir) = current.parent()
  {
    let candidate = dir.join(exe_name);
    if candidate.is_file() {
      return Ok(candidate);
    }
  }

  if let Some(path) = which_in_path(exe_name) {
    return Ok(path);
  }

  Err(anyhow::anyhow!(
    "{exe_name} not found next to {} or on PATH",
    std::env::current_exe()
      .map(|p| p.display().to_string())
      .unwrap_or_else(|_| "<current_exe unavailable>".into())
  ))
}

fn which_in_path(exe: &str) -> Option<PathBuf> {
  let path = std::env::var_os("PATH")?;
  for entry in std::env::split_paths(&path) {
    let candidate = entry.join(exe);
    if candidate.is_file() {
      return Some(candidate);
    }
  }
  None
}

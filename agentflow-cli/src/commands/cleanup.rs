//! `agentflow cleanup` — run the server's retention sweep once and
//! exit. Mirrors `agentflow serve` (P2.1) by spawning the
//! `agentflow-server` binary in `--cleanup` mode so the CLI does not
//! need to link the server crate (cyclic dep with `agentflow-cli`).

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};

#[allow(clippy::too_many_arguments)]
pub async fn execute(
  database_url: Option<String>,
  run_dir: Option<String>,
  trace_dir: Option<String>,
  security_profile: String,
  dry_run: bool,
) -> Result<()> {
  let server_bin = locate_server_binary().context(
    "could not find the `agentflow-server` executable — is it installed alongside `agentflow`?",
  )?;

  let mut cmd = Command::new(&server_bin);
  cmd.arg("--cleanup");
  if dry_run {
    cmd.arg("--dry-run");
  }
  cmd.env("AGENTFLOW_SECURITY_PROFILE", &security_profile);
  if let Some(url) = database_url.or_else(|| std::env::var("DATABASE_URL").ok()) {
    cmd.env("DATABASE_URL", url);
  }
  if let Some(dir) = run_dir.or_else(|| std::env::var("AGENTFLOW_RUN_DIR").ok()) {
    cmd.env("AGENTFLOW_RUN_DIR", dir);
  }
  if let Some(dir) = trace_dir.or_else(|| std::env::var("AGENTFLOW_TRACE_DIR").ok()) {
    cmd.env("AGENTFLOW_TRACE_DIR", dir);
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
      "agentflow-server cleanup terminated by signal: {status}"
    ))
  }
}

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

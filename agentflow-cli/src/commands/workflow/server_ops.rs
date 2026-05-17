//! Server-backed `workflow` subcommands (P2.5).
//!
//! These delegate to the [`ServerClient`] when the caller passes
//! `--server <url>` (or sets `AGENTFLOW_SERVER_URL`). They never reach
//! into the in-process executor — that path is in `run.rs` and stays
//! the default when no server URL is configured.

use anyhow::Result;

use crate::server_client::{ServerClient, resolve_auth_token, resolve_tenant_id};

/// Build a [`ServerClient`] from `(server_url, auth_token, tenant)` flags
/// after applying the env fallbacks. The caller has already decided to
/// enter server mode (i.e. `resolve_server_url` returned `Some`).
pub fn build_client(
  server_url: &str,
  auth_token: Option<&str>,
  tenant: Option<&str>,
) -> Result<ServerClient> {
  let token = resolve_auth_token(auth_token);
  let tenant_id = resolve_tenant_id(tenant);
  ServerClient::new(server_url.to_string(), token, tenant_id)
}

pub async fn list(
  server_url: &str,
  auth_token: Option<&str>,
  tenant: Option<&str>,
  limit: Option<i64>,
  offset: Option<i64>,
  status: Option<&str>,
) -> Result<()> {
  let client = build_client(server_url, auth_token, tenant)?;
  let body = client.list_runs(limit, offset, status).await?;
  println!("{}", serde_json::to_string_pretty(&body)?);
  Ok(())
}

pub async fn cancel(
  server_url: &str,
  auth_token: Option<&str>,
  tenant: Option<&str>,
  run_id: &str,
) -> Result<()> {
  let client = build_client(server_url, auth_token, tenant)?;
  let body = client.cancel_run(run_id).await?;
  println!("{}", serde_json::to_string_pretty(&body)?);
  Ok(())
}

pub async fn graph(
  server_url: &str,
  auth_token: Option<&str>,
  tenant: Option<&str>,
  run_id: &str,
) -> Result<()> {
  let client = build_client(server_url, auth_token, tenant)?;
  let body = client.get_run_graph(run_id).await?;
  println!("{}", serde_json::to_string_pretty(&body)?);
  Ok(())
}

/// Submit a workflow body via the server and poll until terminal.
/// Returns the final run JSON from `GET /v1/runs/{id}`.
pub async fn run_via_server(
  server_url: &str,
  auth_token: Option<&str>,
  tenant: Option<&str>,
  workflow_text: &str,
) -> Result<()> {
  let client = build_client(server_url, auth_token, tenant)?;
  let submission = client.submit_run(workflow_text).await?;
  let run_id = submission["run_id"]
    .as_str()
    .ok_or_else(|| anyhow::anyhow!("server response missing run_id: {submission}"))?
    .to_string();
  println!(
    "📋 Submitted run {run_id}; status: {}",
    submission["status"]
  );

  // Poll the run until it reaches a terminal status. The server flips
  // the row to `succeeded` / `failed` / `cancelled` once the inline
  // executor finishes. Bounded so a stuck server doesn't hang the CLI.
  const POLL_TIMEOUT_MS: u64 = 60_000;
  const POLL_INTERVAL_MS: u64 = 250;
  let mut waited = 0u64;
  loop {
    let row = client.get_run(&run_id).await?;
    let status = row["status"].as_str().unwrap_or("unknown");
    if matches!(status, "succeeded" | "failed" | "cancelled") {
      println!("{}", serde_json::to_string_pretty(&row)?);
      return Ok(());
    }
    if waited >= POLL_TIMEOUT_MS {
      anyhow::bail!(
        "run {run_id} did not reach a terminal status within {POLL_TIMEOUT_MS} ms; last status: {status}"
      );
    }
    tokio::time::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS)).await;
    waited += POLL_INTERVAL_MS;
  }
}

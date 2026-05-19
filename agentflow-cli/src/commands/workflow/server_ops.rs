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

/// Render the server response — either as the legacy bare JSON
/// body (default) or wrapped in the canonical `CliJsonEnvelope`.
fn print_server_response(command: &str, format: &str, body: &serde_json::Value) -> Result<()> {
  if format == "json-envelope" {
    let envelope = crate::json_envelope::CliJsonEnvelope::ok(command, body);
    println!("{}", serde_json::to_string_pretty(&envelope)?);
  } else {
    println!("{}", serde_json::to_string_pretty(body)?);
  }
  Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn list(
  server_url: &str,
  auth_token: Option<&str>,
  tenant: Option<&str>,
  limit: Option<i64>,
  offset: Option<i64>,
  status: Option<&str>,
  format: &str,
) -> Result<()> {
  let client = build_client(server_url, auth_token, tenant)?;
  let body = client.list_runs(limit, offset, status).await?;
  print_server_response("workflow list", format, &body)
}

pub async fn cancel(
  server_url: &str,
  auth_token: Option<&str>,
  tenant: Option<&str>,
  run_id: &str,
  format: &str,
) -> Result<()> {
  let client = build_client(server_url, auth_token, tenant)?;
  let body = client.cancel_run(run_id).await?;
  print_server_response("workflow cancel", format, &body)
}

pub async fn graph(
  server_url: &str,
  auth_token: Option<&str>,
  tenant: Option<&str>,
  run_id: &str,
  format: &str,
) -> Result<()> {
  let client = build_client(server_url, auth_token, tenant)?;
  let body = client.get_run_graph(run_id).await?;
  print_server_response("workflow graph", format, &body)
}

/// Submit a workflow body via the server and poll until terminal.
/// Returns the final run JSON from `GET /v1/runs/{id}`.
///
/// `format` chooses the stdout shape:
/// - `"text"` (default): emoji submit line + final pretty JSON on stdout.
/// - `"json-envelope"`: submit / poll progress lines go to **stderr** so
///   stdout stays a single parseable `CliJsonEnvelope`. Non-success
///   terminal status surfaces in `envelope.errors[]`.
pub async fn run_via_server(
  server_url: &str,
  auth_token: Option<&str>,
  tenant: Option<&str>,
  workflow_text: &str,
  format: &str,
) -> Result<()> {
  let is_envelope = format == "json-envelope";
  let client = build_client(server_url, auth_token, tenant)?;
  let submission = client.submit_run(workflow_text).await?;
  let run_id = submission["run_id"]
    .as_str()
    .ok_or_else(|| anyhow::anyhow!("server response missing run_id: {submission}"))?
    .to_string();

  // Progress goes to stderr in envelope mode so stdout stays clean
  // for `jq` consumers. Text mode keeps the original stdout
  // behaviour for back-compat with shell logs that captured it.
  let progress_line = format!(
    "📋 Submitted run {run_id}; status: {}",
    submission["status"]
  );
  if is_envelope {
    eprintln!("{progress_line}");
  } else {
    println!("{progress_line}");
  }

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
      if is_envelope {
        // P3.3 migration: wrap the terminal run row in the canonical
        // envelope. Non-success statuses populate `errors[]` so
        // shell tooling can branch on `errors.length > 0` without
        // walking `result.status`. The bare run row stays available
        // as `envelope.result` so consumers who pinned to the legacy
        // shape migrate by reading `envelope.result.<field>`.
        let errors: Vec<String> = if status == "succeeded" {
          Vec::new()
        } else {
          vec![format!("run {run_id} ended with status '{status}'")]
        };
        let envelope =
          crate::json_envelope::CliJsonEnvelope::with_errors("workflow run", &row, errors);
        println!("{}", serde_json::to_string_pretty(&envelope)?);
      } else {
        println!("{}", serde_json::to_string_pretty(&row)?);
      }
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

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

/// `agentflow workflow logs <run_id>` — stream the server's
/// persisted event log for a run. Without `--follow`, fetches the
/// historical events as a single JSON array via
/// `GET /v1/runs/{id}/events/history`. With `--follow`, opens an
/// SSE connection at `GET /v1/runs/{id}/events` and keeps printing
/// events until the server closes (run reaches a terminal state)
/// or the user cancels.
///
/// `format` chooses the stdout shape:
/// - `"text"` (default): one line per event:
///   `[seq] kind ts payload_summary`.
/// - `"json"`: one [`serde_json::Value`] per line (JSONL).
/// - `"json-envelope"`: a single canonical `CliJsonEnvelope`
///   wrapping the events array. Only valid for the non-follow
///   path — envelope mode requires a single bounded output, so
///   combining it with `--follow` is rejected with a clear error.
///
/// `after_seq` lets a reconnecting consumer resume past a known
/// last-seen `seq`, avoiding duplicate prints.
#[allow(clippy::too_many_arguments)]
pub async fn logs(
  server_url: &str,
  auth_token: Option<&str>,
  tenant: Option<&str>,
  run_id: &str,
  follow: bool,
  after_seq: Option<i64>,
  format: &str,
) -> Result<()> {
  let client = build_client(server_url, auth_token, tenant)?;

  if follow && format == "json-envelope" {
    anyhow::bail!(
      "--format json-envelope is incompatible with --follow: an envelope wraps a bounded result, \
       but --follow produces an open-ended stream. Use --format json (JSONL) or --format text."
    );
  }

  if !follow {
    let body = client.list_events_history(run_id, after_seq).await?;
    return print_events_history(format, run_id, &body);
  }

  // Follow mode: stream events and print each as it arrives.
  client
    .stream_events_sse(run_id, after_seq, |event| {
      // Errors writing to stdout/stderr surface as broken pipes
      // only when the consumer (e.g., `| head -10`) closed early;
      // honour that by terminating the process via `Result` only
      // after the stream loop ends. Print errors go through
      // `eprintln!` here because emitting a JSON error in the
      // middle of a JSONL stream would corrupt downstream parsers.
      if let Err(err) = print_single_event(format, &event) {
        eprintln!("warning: failed to render event: {err}");
      }
    })
    .await?;
  Ok(())
}

fn print_events_history(format: &str, run_id: &str, body: &serde_json::Value) -> Result<()> {
  match format {
    "json-envelope" => {
      let envelope = crate::json_envelope::CliJsonEnvelope::ok("workflow logs", body);
      println!("{}", serde_json::to_string_pretty(&envelope)?);
    }
    "json" => {
      // JSONL: one event per line. Bare `Value` rendering keeps
      // downstream `jq -c` filters happy.
      let events = body.as_array().ok_or_else(|| {
        anyhow::anyhow!("server response for {run_id} events was not a JSON array (got: {body:?})")
      })?;
      for event in events {
        println!("{}", serde_json::to_string(event)?);
      }
    }
    _ => {
      // text: human-readable summary, one line per event.
      let events = body.as_array().ok_or_else(|| {
        anyhow::anyhow!("server response for {run_id} events was not a JSON array (got: {body:?})")
      })?;
      if events.is_empty() {
        eprintln!("(no events for run {run_id})");
      }
      for event in events {
        println!("{}", format_event_text(event));
      }
    }
  }
  Ok(())
}

fn print_single_event(format: &str, event: &serde_json::Value) -> Result<()> {
  match format {
    "json" => println!("{}", serde_json::to_string(event)?),
    _ => println!("{}", format_event_text(event)),
  }
  Ok(())
}

/// Format a single `StreamedEvent` JSON value as a one-line human
/// summary. Shape matches `agentflow_server::events_stream::
/// StreamedEvent`: `{ run_id, seq, kind, payload, ts }`.
/// Missing fields render as `?` rather than panicking — the
/// command's job is to surface what the server sent, not validate
/// it.
fn format_event_text(event: &serde_json::Value) -> String {
  let seq = event
    .get("seq")
    .and_then(|s| s.as_i64())
    .map(|s| s.to_string())
    .unwrap_or_else(|| "?".into());
  let kind = event.get("kind").and_then(|k| k.as_str()).unwrap_or("?");
  let ts = event.get("ts").and_then(|t| t.as_str()).unwrap_or("?");
  // Render payload as compact JSON so the line stays scannable.
  // Truncate to 240 chars to keep `tail -f` legible — full payload
  // is available via `--format json`.
  let payload = event
    .get("payload")
    .map(|p| serde_json::to_string(p).unwrap_or_else(|_| "{}".into()))
    .unwrap_or_else(|| "{}".into());
  const MAX_PAYLOAD_CHARS: usize = 240;
  let payload_display = if payload.chars().count() > MAX_PAYLOAD_CHARS {
    let mut truncated: String = payload.chars().take(MAX_PAYLOAD_CHARS).collect();
    truncated.push('…');
    truncated
  } else {
    payload
  };
  format!("[{seq}] {kind:<32} {ts} {payload_display}")
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

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn format_event_text_renders_all_fields() {
    let event = json!({
      "run_id": "00000000-0000-0000-0000-000000000001",
      "seq": 12,
      "kind": "step_started",
      "payload": { "node_id": "render" },
      "ts": "2026-05-20T10:00:00Z",
    });
    let line = format_event_text(&event);
    assert!(line.contains("[12]"));
    assert!(line.contains("step_started"));
    assert!(line.contains("2026-05-20T10:00:00Z"));
    assert!(line.contains(r#"{"node_id":"render"}"#));
  }

  #[test]
  fn format_event_text_handles_missing_fields_gracefully() {
    let event = json!({});
    let line = format_event_text(&event);
    // Missing seq → "?"; missing kind → "?"; missing ts → "?";
    // missing payload → "{}". Must never panic on partial input.
    assert!(line.contains("[?]"));
    assert!(line.contains('?'));
  }

  #[test]
  fn format_event_text_truncates_long_payloads() {
    let big_string: String = std::iter::repeat_n('x', 1000).collect();
    let event = json!({
      "seq": 1,
      "kind": "noisy",
      "ts": "now",
      "payload": { "blob": big_string },
    });
    let line = format_event_text(&event);
    // Truncation appends a `…` glyph; cap is 240 chars of payload.
    assert!(line.contains('…'), "long payloads must be truncated");
    // The wrapping format `[seq] kind ts payload` makes the line
    // somewhat longer than 240; ensure we did NOT include the full
    // 1000-char blob.
    assert!(
      !line.contains(&"x".repeat(1000)),
      "full blob must not appear in the truncated line"
    );
  }

  /// P10.11.1: --follow + --format json-envelope is rejected with
  /// a clear error. The contract: an envelope is bounded; a follow
  /// stream is unbounded — they're mutually exclusive.
  #[tokio::test]
  async fn logs_rejects_follow_with_json_envelope_format() {
    // No need for a real server — the validation runs before any
    // HTTP call. Using a junk URL exercises the early-bail path.
    let err = logs(
      "http://127.0.0.1:1",
      None,
      None,
      "any-run-id",
      true,            // follow
      None,            // after_seq
      "json-envelope", // incompatible
    )
    .await
    .expect_err("must reject the combination");
    let msg = err.to_string();
    assert!(
      msg.contains("incompatible with --follow"),
      "error must explain the incompatibility, got: {msg}"
    );
  }
}

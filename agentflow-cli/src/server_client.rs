//! Minimal HTTP client for `agentflow-server` (P2.5).
//!
//! When the user passes `--server <url>` (or sets
//! `AGENTFLOW_SERVER_URL`) the CLI dispatches selected commands to the
//! remote gateway instead of executing them in-process. This module is
//! the single layer that knows how to talk to the server — every
//! command that needs server-mode goes through [`ServerClient`].
//!
//! Auth: bearer token comes from `--auth-token` (planned per-command) or
//! `AGENTFLOW_API_TOKEN` env. Tenant: defaults to `"default"` but can
//! be overridden via `--tenant` or `AGENTFLOW_TENANT`.
//!
//! Wire shape: the client targets the v1 routes documented in
//! `docs/STABILITY.md` (`/v1/runs`, `/v1/runs/{id}`, `/v1/runs/{id}/graph`,
//! `/v1/runs/{id}:cancel`). All responses come back as `serde_json::Value`
//! so the CLI can pass them through the P3.3 envelope without coupling
//! to per-route response structs.

use anyhow::{Context, Result, anyhow, bail};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;

/// Environment variable that points the CLI at a remote `agentflow-server`.
pub const SERVER_URL_ENV: &str = "AGENTFLOW_SERVER_URL";
/// Environment variable carrying the bearer auth token for server-mode
/// requests. Mirrors `AGENTFLOW_API_TOKEN` used by the server side.
pub const SERVER_TOKEN_ENV: &str = "AGENTFLOW_API_TOKEN";
/// Environment variable overriding the active tenant for server-mode
/// requests (default `"default"`). Header is `X-Agentflow-Tenant` per P2.6.
pub const TENANT_ENV: &str = "AGENTFLOW_TENANT";

/// Tenant scope header recognized by the server (P2.6).
const TENANT_HEADER: HeaderName = HeaderName::from_static("x-agentflow-tenant");

/// Resolve the server base URL from the explicit flag or env var.
/// Returns `Ok(None)` when neither is set — callers fall back to
/// in-process execution.
pub fn resolve_server_url(flag: Option<&str>) -> Option<String> {
  resolve_server_url_from(flag, std::env::var(SERVER_URL_ENV).ok().as_deref())
}

/// Pure variant for unit tests — the public wrapper reads the env var.
fn resolve_server_url_from(flag: Option<&str>, env_value: Option<&str>) -> Option<String> {
  if let Some(url) = flag
    && !url.trim().is_empty()
  {
    return Some(trim_trailing_slash(url.trim()));
  }
  env_value
    .map(|s| trim_trailing_slash(s.trim()))
    .filter(|s| !s.is_empty())
}

fn trim_trailing_slash(url: impl Into<String>) -> String {
  let mut s = url.into();
  while s.ends_with('/') {
    s.pop();
  }
  s
}

pub fn resolve_auth_token(flag: Option<&str>) -> Option<String> {
  resolve_auth_token_from(flag, std::env::var(SERVER_TOKEN_ENV).ok().as_deref())
}

fn resolve_auth_token_from(flag: Option<&str>, env_value: Option<&str>) -> Option<String> {
  if let Some(token) = flag
    && !token.trim().is_empty()
  {
    return Some(token.trim().to_string());
  }
  env_value
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
}

pub fn resolve_tenant_id(flag: Option<&str>) -> String {
  resolve_tenant_id_from(flag, std::env::var(TENANT_ENV).ok().as_deref())
}

fn resolve_tenant_id_from(flag: Option<&str>, env_value: Option<&str>) -> String {
  if let Some(tenant) = flag
    && !tenant.trim().is_empty()
  {
    return tenant.trim().to_string();
  }
  env_value
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "default".to_string())
}

/// Thin HTTP wrapper for the v1 control-plane API.
pub struct ServerClient {
  base_url: String,
  http: reqwest::Client,
  auth_token: Option<String>,
  tenant_id: String,
}

impl ServerClient {
  /// Build a client targeting `base_url`. `auth_token` is added as
  /// `Authorization: Bearer <token>` on every request when present.
  ///
  /// The underlying reqwest client disables system proxies — `localhost`
  /// roundtrips through a Clash/V2Ray-style HTTP proxy on macOS
  /// otherwise (see CLAUDE.md "Rust HTTP Testing Guidelines"). The
  /// reasoning applies to CLI use against a local `agentflow serve` too.
  pub fn new(base_url: String, auth_token: Option<String>, tenant_id: String) -> Result<Self> {
    let http = reqwest::Client::builder()
      .no_proxy()
      .timeout(std::time::Duration::from_secs(30))
      .build()
      .context("failed to build server HTTP client")?;
    Ok(Self {
      base_url: trim_trailing_slash(base_url),
      http,
      auth_token,
      tenant_id,
    })
  }

  fn url(&self, path: &str) -> String {
    if path.starts_with('/') {
      format!("{}{}", self.base_url, path)
    } else {
      format!("{}/{}", self.base_url, path)
    }
  }

  fn auth_headers(&self) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Some(token) = &self.auth_token
      && let Ok(value) = HeaderValue::from_str(&format!("Bearer {token}"))
    {
      headers.insert(AUTHORIZATION, value);
    }
    if let Ok(value) = HeaderValue::from_str(&self.tenant_id) {
      headers.insert(TENANT_HEADER, value);
    }
    headers
  }

  /// `POST /v1/runs` — submit a workflow body. Returns the parsed JSON
  /// `{ run_id, status }` envelope.
  pub async fn submit_run(&self, workflow: &str) -> Result<Value> {
    let body = serde_json::json!({
      "workflow": workflow,
      "tenant_id": self.tenant_id,
    });
    let response = self
      .http
      .post(self.url("/v1/runs"))
      .headers(self.auth_headers())
      .header(CONTENT_TYPE, "application/json")
      .body(body.to_string())
      .send()
      .await
      .context("failed to POST /v1/runs")?;
    expect_success(response).await
  }

  /// `GET /v1/runs/{id}` — fetch the current run state.
  pub async fn get_run(&self, run_id: &str) -> Result<Value> {
    let response = self
      .http
      .get(self.url(&format!("/v1/runs/{run_id}")))
      .headers(self.auth_headers())
      .send()
      .await
      .context("failed to GET /v1/runs/{id}")?;
    expect_success(response).await
  }

  /// `GET /v1/runs` — list runs, optionally filtered.
  pub async fn list_runs(
    &self,
    limit: Option<i64>,
    offset: Option<i64>,
    status: Option<&str>,
  ) -> Result<Value> {
    let mut url = format!("{}/v1/runs?tenant_id={}", self.base_url, self.tenant_id);
    if let Some(limit) = limit {
      url.push_str(&format!("&limit={limit}"));
    }
    if let Some(offset) = offset {
      url.push_str(&format!("&offset={offset}"));
    }
    if let Some(status) = status {
      url.push_str(&format!("&status={status}"));
    }
    let response = self
      .http
      .get(url)
      .headers(self.auth_headers())
      .send()
      .await
      .context("failed to GET /v1/runs")?;
    expect_success(response).await
  }

  /// `POST /v1/runs/{id}:cancel`.
  pub async fn cancel_run(&self, run_id: &str) -> Result<Value> {
    let response = self
      .http
      .post(self.url(&format!("/v1/runs/{run_id}:cancel")))
      .headers(self.auth_headers())
      .send()
      .await
      .context("failed to cancel run")?;
    expect_success(response).await
  }

  /// `GET /v1/runs/{id}/graph`.
  pub async fn get_run_graph(&self, run_id: &str) -> Result<Value> {
    let response = self
      .http
      .get(self.url(&format!("/v1/runs/{run_id}/graph")))
      .headers(self.auth_headers())
      .send()
      .await
      .context("failed to fetch run graph")?;
    expect_success(response).await
  }

  /// `GET /v1/runs/{id}/events/history?after_seq=<n>` — fetches the
  /// already-persisted event log as a bare JSON array. Used by the
  /// `workflow logs <run_id>` command without `--follow`. The server
  /// caps the page at 1000 events so callers needing more should
  /// pass an increased `after_seq` and re-call.
  pub async fn list_events_history(&self, run_id: &str, after_seq: Option<i64>) -> Result<Value> {
    let mut url = self.url(&format!("/v1/runs/{run_id}/events/history"));
    if let Some(seq) = after_seq {
      url.push_str(&format!("?after_seq={seq}"));
    }
    let response = self
      .http
      .get(url)
      .headers(self.auth_headers())
      .send()
      .await
      .context("failed to GET /v1/runs/{id}/events/history")?;
    expect_success(response).await
  }

  /// `GET /v1/runs/{id}/events?after_seq=<n>` — opens an SSE stream.
  /// Each parsed event payload is delivered to `on_event` as a
  /// [`serde_json::Value`] (the wire shape matches
  /// `agentflow_server::events_stream::StreamedEvent`). When the
  /// server closes the connection, the call returns `Ok(())`. When
  /// the stream reports a transport error, it returns `Err`.
  ///
  /// The follow path uses a much longer reqwest timeout than the
  /// short-poll routes — SSE connections are expected to stay open
  /// for the lifetime of the run.
  ///
  /// `on_event` is `FnMut` so callers can keep counters / I/O
  /// handles without wrapping in a `Mutex`.
  pub async fn stream_events_sse<F>(
    &self,
    run_id: &str,
    after_seq: Option<i64>,
    mut on_event: F,
  ) -> Result<()>
  where
    F: FnMut(Value),
  {
    let mut url = self.url(&format!("/v1/runs/{run_id}/events"));
    if let Some(seq) = after_seq {
      url.push_str(&format!("?after_seq={seq}"));
    }
    // The default 30s `ServerClient` timeout would tear down a live
    // SSE follow inside a minute; build a dedicated long-lived
    // client for this single request.
    let follow_client = reqwest::Client::builder()
      .no_proxy()
      // No request-level timeout: SSE streams stay open indefinitely
      // (the server uses a 15s keep-alive). A read-timeout could
      // belong here in the future but isn't required for v1.
      .build()
      .context("failed to build SSE follow HTTP client")?;
    let response = follow_client
      .get(url)
      .headers(self.auth_headers())
      .send()
      .await
      .context("failed to open SSE stream")?;
    if !response.status().is_success() {
      let status = response.status();
      let body = response.text().await.unwrap_or_default();
      bail!(
        "server returned {} {} on SSE open: {body}",
        status.as_u16(),
        status.canonical_reason().unwrap_or("")
      );
    }
    let mut byte_stream = response.bytes_stream();
    let mut buffer = String::new();
    use futures::stream::StreamExt;
    while let Some(chunk_result) = byte_stream.next().await {
      let chunk = chunk_result.context("SSE chunk read failed")?;
      buffer.push_str(&String::from_utf8_lossy(&chunk));
      // SSE events are separated by "\n\n"; within an event, fields
      // ("data:", "event:", "id:", ":") are separated by single
      // newlines. We only care about the `data:` field — that's
      // where the JSON-serialised `StreamedEvent` lives.
      while let Some(boundary) = buffer.find("\n\n") {
        let raw_event: String = buffer.drain(..boundary + 2).collect();
        if let Some(payload) = parse_sse_event_payload(&raw_event)
          && let Ok(value) = serde_json::from_str::<Value>(&payload)
        {
          on_event(value);
        }
      }
    }
    Ok(())
  }
}

/// Extract the `data:` payload from a single raw SSE event block.
/// Returns `None` for keep-alives (comment lines starting with `:`)
/// or events with no `data:` field. Multi-line `data:` payloads are
/// re-joined with newlines per the SSE spec.
pub(crate) fn parse_sse_event_payload(raw_event: &str) -> Option<String> {
  let mut data_lines: Vec<&str> = Vec::new();
  for line in raw_event.lines() {
    if let Some(rest) = line.strip_prefix("data:") {
      // SSE spec: a single leading space after the colon is
      // stripped; everything else is preserved verbatim.
      let trimmed = rest.strip_prefix(' ').unwrap_or(rest);
      data_lines.push(trimmed);
    }
    // Lines starting with `:` are keep-alive comments — ignore.
    // Lines starting with `id:` / `event:` / `retry:` are
    // structural — also ignored (we identify events by their JSON
    // body, not the `event:` kind, because the kind is already
    // inside the payload).
  }
  if data_lines.is_empty() {
    return None;
  }
  Some(data_lines.join("\n"))
}

async fn expect_success(response: reqwest::Response) -> Result<Value> {
  let status = response.status();
  let text = response
    .text()
    .await
    .context("failed to read response body")?;
  if !status.is_success() {
    let parsed: Result<Value, _> = serde_json::from_str(&text);
    if let Ok(value) = parsed
      && let Some(message) = value
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
    {
      bail!(
        "server returned {} {}: {message}",
        status.as_u16(),
        status.canonical_reason().unwrap_or("")
      );
    }
    bail!(
      "server returned {} {}: {text}",
      status.as_u16(),
      status.canonical_reason().unwrap_or("")
    );
  }
  serde_json::from_str(&text)
    .map_err(|e| anyhow!("server response was not valid JSON: {e} (body: {text})"))
}

#[cfg(test)]
mod tests {
  use super::*;

  // Pure-function unit tests — avoid env mutation so cargo's parallel
  // test runner doesn't race other tests in this crate.

  #[test]
  fn resolve_server_url_prefers_flag_over_env() {
    assert_eq!(
      resolve_server_url_from(Some("http://from-flag"), Some("http://from-env")),
      Some("http://from-flag".to_string())
    );
  }

  #[test]
  fn resolve_server_url_falls_back_to_env_when_flag_absent() {
    assert_eq!(
      resolve_server_url_from(None, Some("http://from-env/")),
      Some("http://from-env".to_string()),
      "trailing slash must be trimmed"
    );
  }

  #[test]
  fn resolve_server_url_returns_none_when_unset() {
    assert!(resolve_server_url_from(None, None).is_none());
  }

  #[test]
  fn resolve_server_url_treats_empty_flag_as_unset() {
    assert!(resolve_server_url_from(Some("   "), None).is_none());
  }

  #[test]
  fn resolve_server_url_treats_empty_env_as_unset() {
    assert!(resolve_server_url_from(None, Some("")).is_none());
  }

  #[test]
  fn resolve_tenant_id_defaults_to_default() {
    assert_eq!(resolve_tenant_id_from(None, None), "default");
  }

  #[test]
  fn resolve_tenant_id_respects_flag() {
    assert_eq!(
      resolve_tenant_id_from(Some("from-flag"), Some("from-env")),
      "from-flag"
    );
  }

  #[test]
  fn resolve_tenant_id_falls_back_to_env() {
    assert_eq!(resolve_tenant_id_from(None, Some("from-env")), "from-env");
  }

  #[test]
  fn resolve_auth_token_returns_none_when_blank() {
    assert!(resolve_auth_token_from(None, Some("")).is_none());
    assert!(resolve_auth_token_from(Some("  "), None).is_none());
  }

  #[test]
  fn resolve_auth_token_uses_flag_first() {
    assert_eq!(
      resolve_auth_token_from(Some("flag-token"), Some("env-token")),
      Some("flag-token".to_string())
    );
  }

  /// P10.11.1: the SSE `data:` extraction must produce the exact
  /// JSON the server sent, including stripping the single leading
  /// space per the SSE spec (`Sse::data(json_text)` in axum prefixes
  /// payloads with `data: `, not `data:`).
  #[test]
  fn parse_sse_event_payload_strips_single_leading_space() {
    let raw = "id: 7\nevent: step_started\ndata: {\"seq\":7}\n\n";
    let payload = parse_sse_event_payload(raw).expect("data line present");
    assert_eq!(payload, r#"{"seq":7}"#);
  }

  #[test]
  fn parse_sse_event_payload_joins_multi_line_data() {
    // SSE spec: multi-line `data:` fields are re-joined with `\n`.
    // axum's SSE serialiser emits single-line JSON so this is
    // defensive, but the spec compliance matters if the server
    // ever splits a large payload.
    let raw = "data: line one\ndata: line two\n\n";
    let payload = parse_sse_event_payload(raw).expect("data lines present");
    assert_eq!(payload, "line one\nline two");
  }

  #[test]
  fn parse_sse_event_payload_returns_none_for_keepalive_only() {
    // Keep-alive: `:keep-alive\n\n`. Comment-only event with no
    // `data:` line — must surface as None so the consumer doesn't
    // try to parse it as JSON and log a spurious error.
    let raw = ":keep-alive\n\n";
    assert!(parse_sse_event_payload(raw).is_none());
  }

  #[test]
  fn parse_sse_event_payload_ignores_structural_fields_without_data() {
    let raw = "id: 42\nevent: heartbeat\n\n";
    assert!(parse_sse_event_payload(raw).is_none());
  }
}

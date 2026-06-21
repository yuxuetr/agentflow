//! `GET /v1/diagnostics` — surfaces the same data as `agentflow doctor
//! --output json`, but in-process so the Web UI can render a
//! per-component pass/warn/fail table without shelling out.
//!
//! The handler delegates to `agentflow_config::diagnostics::build_report`,
//! the shared source of truth for the doctor schema (used by both the CLI
//! `doctor` command and this route), so the CLI and the UI never drift; the
//! server is a thin pass-through.
//!
//! Security note: the doctor report never includes API key values
//! (only env-var *names* in `config.missing_env_vars` and a boolean
//! `environment.agentflow_api_token_set`). The route still inherits
//! the same bearer-token gate as every other `/v1/*` endpoint.

use axum::Json;

use agentflow_config::diagnostics::{DoctorProfile, build_report};

use crate::error::ApiError;

/// Handler for `GET /v1/diagnostics`.
///
/// Always invokes the doctor with `DoctorProfile::Local` and no
/// `--server` probe / no `--backup-check`. The UI doesn't need to
/// drive those toggles today; if it ever does, this signature can
/// grow query parameters without breaking existing clients.
pub async fn get_diagnostics() -> Result<Json<serde_json::Value>, ApiError> {
  // Server-side diagnostics intentionally skip the heavier opt-in
  // probes (backup_check, check_installations) — they're filesystem-
  // bound and only useful to a human invoking the CLI.
  // The server skips installation probes, so the top-level MCP registry
  // (a CLI-side `mcp.toml` concern) is empty here.
  let report = build_report(DoctorProfile::Local, None, false, false, (None, Vec::new())).await;
  let value = serde_json::to_value(&report).map_err(|err| ApiError::Internal(err.to_string()))?;
  Ok(Json(value))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn handler_returns_a_status_field() {
    let result = get_diagnostics().await.expect("handler ok");
    let json = result.0;
    let status = json.get("status").expect("status field");
    let label = status.as_str().expect("status is a string");
    assert!(
      matches!(label, "ok" | "warning" | "fail"),
      "unexpected status label: {label}"
    );
  }

  #[tokio::test]
  async fn handler_never_leaks_api_token_value() {
    // Defense-in-depth: even if an upstream change to `build_report`
    // ever started including the actual token, the diagnostics
    // handler would surface it here. Set a known token; assert the
    // JSON contains the boolean flag, not the value.
    // SAFETY: tests run on a single Tokio thread per test; setting a
    // process-wide env var is the only way to exercise the
    // `AGENTFLOW_API_TOKEN` branch in build_report. We restore the
    // previous value on the happy path so concurrent tests sharing
    // the same env don't bleed.
    let key = "AGENTFLOW_API_TOKEN";
    let previous = std::env::var(key).ok();
    let secret = "sk-test-diagnostics-secret-do-not-leak";
    // SAFETY: see comment above.
    unsafe {
      std::env::set_var(key, secret);
    }

    let result = get_diagnostics().await;

    // Restore env before any assertion can fail.
    // SAFETY: see comment above.
    unsafe {
      match previous {
        Some(value) => std::env::set_var(key, value),
        None => std::env::remove_var(key),
      }
    }

    let json = result.expect("handler ok").0;
    let serialized = serde_json::to_string(&json).expect("serialize");
    assert!(
      !serialized.contains(secret),
      "diagnostics JSON unexpectedly contains the API token value"
    );
    assert_eq!(
      json
        .pointer("/environment/agentflow_api_token_set")
        .and_then(|v| v.as_bool()),
      Some(true),
      "diagnostics must report the api_token_set flag"
    );
  }
}

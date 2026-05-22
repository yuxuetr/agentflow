//! Server-backed `skill run` (P10.11.2).
//!
//! Mirrors [`crate::commands::workflow::server_ops`] for skills:
//! when the operator passes `--server <url>` (or sets
//! `AGENTFLOW_SERVER_URL`), `agentflow skill run` dispatches to the
//! remote gateway's `POST /v1/skills/{name}:run` endpoint instead of
//! loading the skill manifest locally.
//!
//! ## Semantic shift on the positional argument
//!
//! The local form takes a `skill_dir` (filesystem path). The server
//! form takes a `skill_name` (resolved via the server's
//! `AGENTFLOW_SKILLS_INDEX` catalog). The CLI reuses the same
//! positional argument and forwards it verbatim — server resolution
//! errors surface as a clear 404 when the name isn't installed.
//!
//! ## Local-only flags
//!
//! `--memory`, `--model`, `--session`, and `--trace` are local-only
//! because today's `POST /v1/skills/{name}:run` body
//! (`agentflow_server::skills::RunSkillRequest`) only accepts
//! `input` + `tenant_id`. Combining them with `--server` is
//! rejected with a clear error rather than silently dropping the
//! flag — silent drops are the kind of bug operators only catch in
//! prod. The contract can widen later if the server gains
//! per-request overrides; today the manifest declares the model + memory.

use anyhow::Result;

use crate::server_client::{ServerClient, resolve_auth_token, resolve_tenant_id};

/// Build a [`ServerClient`] from `(server_url, auth_token, tenant)` flags.
/// Mirrors the workflow-side helper so future refactors can collapse the
/// two if the wire-shape divergence stays small.
fn build_client(
  server_url: &str,
  auth_token: Option<&str>,
  tenant: Option<&str>,
) -> Result<ServerClient> {
  let token = resolve_auth_token(auth_token);
  let tenant_id = resolve_tenant_id(tenant);
  ServerClient::new(server_url.to_string(), token, tenant_id)
}

/// Local-only flags the server doesn't accept. Centralising the
/// rejection list here keeps the dispatch arm in `main.rs` thin and
/// the error messages consistent across operations that need the
/// same guard.
pub fn reject_local_only_flags(
  model_override: Option<&str>,
  memory_override: Option<&str>,
  session_id: Option<&str>,
  trace: bool,
) -> Result<()> {
  if model_override.is_some() {
    anyhow::bail!(
      "--model is local-only (the server uses the model declared in the skill manifest \
       loaded by the catalog at AGENTFLOW_SKILLS_INDEX). Drop --model when using --server, \
       or run the skill locally to override the model per-invocation."
    );
  }
  if memory_override.is_some() {
    anyhow::bail!(
      "--memory is local-only (the server uses the memory backend declared in the skill \
       manifest). Drop --memory when using --server."
    );
  }
  if session_id.is_some() {
    anyhow::bail!(
      "--session is local-only (the server creates a fresh run per POST /v1/skills/{{name}}:run). \
       Drop --session when using --server; multi-turn server-side sessions are a future API addition."
    );
  }
  if trace {
    anyhow::bail!(
      "--trace is local-only (server runs persist their trace through the event log; consume it \
       via `agentflow workflow logs <run_id>` after the run completes). Drop --trace when using \
       --server."
    );
  }
  Ok(())
}

/// Submit a skill run to the server and poll until terminal.
///
/// `format` controls stdout shape:
/// - `"text"` (default): emoji-prefixed submission line + final
///   pretty JSON of the run row. Mirrors `workflow run --server`.
/// - `"json-envelope"`: a single canonical `CliJsonEnvelope`
///   wrapping the terminal run row. Progress lines go to stderr so
///   stdout stays a single parseable JSON object. Non-success
///   terminal status populates `envelope.errors[]`.
///
/// Server mode does not pre-render the skill banner (skill name,
/// model, memory) that the local mode prints, because the server's
/// catalog owns those details — the CLI doesn't load the manifest.
/// Progress and final row identify the skill by name + run id.
#[allow(clippy::too_many_arguments)]
pub async fn run_via_server(
  server_url: &str,
  auth_token: Option<&str>,
  tenant: Option<&str>,
  skill_name: &str,
  message: &str,
  format: &str,
) -> Result<()> {
  let is_envelope = format == "json-envelope";
  let client = build_client(server_url, auth_token, tenant)?;
  let submission = client.submit_skill_run(skill_name, message).await?;
  let run_id = submission["run_id"]
    .as_str()
    .ok_or_else(|| anyhow::anyhow!("server response missing run_id: {submission}"))?
    .to_string();

  let progress_line = format!(
    "📋 Submitted skill '{skill_name}' as run {run_id}; status: {}",
    submission["status"]
  );
  if is_envelope {
    eprintln!("{progress_line}");
  } else {
    println!("{progress_line}");
  }

  // Polling loop mirrors `workflow::server_ops::run_via_server`.
  // 60s is the same conservative cap; skill runs are expected to
  // complete in seconds, so blowing past 60s indicates either a
  // stuck executor or a long-running ReAct loop that the operator
  // should investigate via `workflow logs`. The interval is short
  // (250ms) because there's no SSE for the run status itself —
  // only for its events.
  const POLL_TIMEOUT_MS: u64 = 60_000;
  const POLL_INTERVAL_MS: u64 = 250;
  let mut waited = 0u64;
  loop {
    let row = client.get_run(&run_id).await?;
    let status = row["status"].as_str().unwrap_or("unknown");
    if matches!(status, "succeeded" | "failed" | "cancelled") {
      if is_envelope {
        let errors: Vec<String> = if status == "succeeded" {
          Vec::new()
        } else {
          vec![format!(
            "skill run {run_id} ended with status '{status}' — \
             consult `agentflow workflow logs {run_id}` for the event log"
          )]
        };
        let envelope =
          crate::json_envelope::CliJsonEnvelope::with_errors("skill run", &row, errors);
        println!("{}", serde_json::to_string_pretty(&envelope)?);
      } else {
        println!("{}", serde_json::to_string_pretty(&row)?);
      }
      return Ok(());
    }
    if waited >= POLL_TIMEOUT_MS {
      anyhow::bail!(
        "skill run {run_id} did not reach a terminal status within {POLL_TIMEOUT_MS} ms; \
         last status: {status}. Inspect with `agentflow workflow logs {run_id}`."
      );
    }
    tokio::time::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS)).await;
    waited += POLL_INTERVAL_MS;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reject_local_only_flags_accepts_no_flags() {
    // The all-None / false path is the only one that's allowed in
    // server mode; pin it so a regression to "always bail" surfaces.
    reject_local_only_flags(None, None, None, false).expect("baseline must pass");
  }

  #[test]
  fn reject_local_only_flags_rejects_model_override() {
    let err = reject_local_only_flags(Some("gpt-4o"), None, None, false)
      .expect_err("--model must be rejected in server mode");
    let msg = err.to_string();
    assert!(msg.contains("--model is local-only"), "{msg}");
    assert!(
      msg.contains("AGENTFLOW_SKILLS_INDEX") || msg.contains("manifest"),
      "error should point at the server-side source of truth: {msg}"
    );
  }

  #[test]
  fn reject_local_only_flags_rejects_memory_override() {
    let err = reject_local_only_flags(None, Some("sqlite"), None, false)
      .expect_err("--memory must be rejected in server mode");
    assert!(err.to_string().contains("--memory is local-only"));
  }

  #[test]
  fn reject_local_only_flags_rejects_session_id() {
    let err = reject_local_only_flags(None, None, Some("sid-xyz"), false)
      .expect_err("--session must be rejected in server mode");
    let msg = err.to_string();
    assert!(msg.contains("--session is local-only"), "{msg}");
    // The hint about consulting logs after the run completes is the
    // operator-facing remediation; pin it so it doesn't regress.
    assert!(msg.contains("future API addition"), "{msg}");
  }

  #[test]
  fn reject_local_only_flags_rejects_trace() {
    let err = reject_local_only_flags(None, None, None, true)
      .expect_err("--trace must be rejected in server mode");
    let msg = err.to_string();
    assert!(msg.contains("--trace is local-only"), "{msg}");
    // Operators get pointed at `workflow logs` as the consumption
    // surface for server-side traces.
    assert!(msg.contains("workflow logs"), "{msg}");
  }
}

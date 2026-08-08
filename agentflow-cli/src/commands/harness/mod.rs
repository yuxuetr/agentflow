//! `agentflow harness …` CLI surface.
//!
//! Phase H1 ships four subcommands wired to `agentflow_harness`:
//! - `run` — bootstrap and execute a single Harness session.
//! - `resume` — re-stream a persisted session log.
//! - `list` — enumerate session logs on disk.
//! - `inspect` — summarise a single session log.

pub mod chat;
pub mod cli;
pub mod inspect;
pub mod list;
pub mod replay;
pub mod resume;
pub mod resume_loop;
pub mod run;
pub mod run_flow;

use std::path::PathBuf;

use anyhow::{Context, Result};

use agentflow_harness::{AGENTFLOW_TRACE_DIR_ENV, HarnessProfile};

/// Resolve the directory used to store Harness session JSONL files.
///
/// Precedence:
/// 1. explicit `--run-dir` flag.
/// 2. `AGENTFLOW_RUN_DIR` env var (workflow-style run artifact root).
/// 3. `AGENTFLOW_TRACE_DIR` env var — the
///    [`agentflow_harness::tracing_bridge`] convention that lets trace
///    replay / TUI tooling pick up Harness session logs automatically.
/// 4. `~/.agentflow/runs`.
///
/// The actual session files live one level deeper at
/// `<root>/harness/sessions/<session_id>.jsonl` so they do not collide
/// with workflow run artifacts (see
/// [`agentflow_harness::default_session_dir`]).
pub(crate) fn resolve_run_dir(run_dir: Option<String>) -> Result<PathBuf> {
  if let Some(dir) = run_dir {
    return Ok(PathBuf::from(dir));
  }
  if let Ok(dir) = std::env::var("AGENTFLOW_RUN_DIR")
    && !dir.trim().is_empty()
  {
    return Ok(PathBuf::from(dir));
  }
  if let Ok(dir) = std::env::var(AGENTFLOW_TRACE_DIR_ENV)
    && !dir.trim().is_empty()
  {
    return Ok(PathBuf::from(dir));
  }
  Ok(
    dirs::home_dir()
      .context("Could not determine home directory for default run directory")?
      .join(".agentflow")
      .join("runs"),
  )
}

/// Parse `--profile` flag.
pub(crate) fn parse_profile(value: &str) -> Result<HarnessProfile> {
  match value {
    "dev" => Ok(HarnessProfile::Dev),
    "local" => Ok(HarnessProfile::Local),
    "production" => Ok(HarnessProfile::Production),
    other => anyhow::bail!("unsupported --profile '{other}', expected dev | local | production"),
  }
}

/// Resolve the effective `--approve` mode. Originally T1.3 (`workflow
/// dynamic`, where an LLM-authored plan is adversarial by construction);
/// U2.3 extended it to `harness run`/`chat`, whose `--approve` previously
/// hardcoded `"none"` regardless of `--profile` — the same "unsupervised
/// by default" gap T1.3 had already closed for `workflow dynamic`. An
/// *unset* `--approve` defaults to requiring approval (`"cli"`) under
/// `local`/`production`; `dev` keeps the historical unsupervised default
/// (`"none"`) so local iteration stays uninterrupted. An explicitly
/// passed `--approve` (including `--approve none`) always wins, on any
/// profile — this only changes what happens when the flag is omitted.
pub(crate) fn resolve_approve_default(approve: Option<String>, profile: HarnessProfile) -> String {
  approve.unwrap_or_else(|| match profile {
    HarnessProfile::Dev => "none".to_string(),
    HarnessProfile::Local | HarnessProfile::Production => "cli".to_string(),
  })
}

/// Parse `--output` flag.
///
/// - `text`: colored human-readable output (default).
/// - `json`: bare JSON summary (legacy; preserved for back-compat).
/// - `stream-json`: one JSON event per line (event stream — `run`
///   emits live, `list` / `inspect` / `resume` stream from disk).
/// - `json-envelope`: canonical `CliJsonEnvelope` wrapping the same
///   summary `json` emits. `stream-json` events stay raw because
///   wrapping each line in an envelope would defeat the purpose of
///   stream-friendly framing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputFormat {
  Text,
  Json,
  StreamJson,
  JsonEnvelope,
}

impl OutputFormat {
  pub fn parse(value: &str) -> Result<Self> {
    match value {
      "text" => Ok(Self::Text),
      "json" => Ok(Self::Json),
      "stream-json" => Ok(Self::StreamJson),
      "json-envelope" => Ok(Self::JsonEnvelope),
      other => {
        anyhow::bail!(
          "unsupported --output '{other}', expected text | json | stream-json | json-envelope"
        )
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  // ── U2.3 (originally T1.3): --approve profile-aware default ────────────

  #[test]
  fn unset_approve_requires_cli_approval_under_local_and_production() {
    assert_eq!(
      resolve_approve_default(None, HarnessProfile::Local),
      "cli",
      "must not run unsupervised under local by default"
    );
    assert_eq!(
      resolve_approve_default(None, HarnessProfile::Production),
      "cli",
      "must not run unsupervised under production by default"
    );
  }

  #[test]
  fn unset_approve_stays_unsupervised_under_dev() {
    assert_eq!(
      resolve_approve_default(None, HarnessProfile::Dev),
      "none",
      "dev profile keeps the historical unsupervised default for fast local iteration"
    );
  }

  #[test]
  fn explicit_approve_none_overrides_the_profile_default_on_every_profile() {
    for profile in [
      HarnessProfile::Dev,
      HarnessProfile::Local,
      HarnessProfile::Production,
    ] {
      assert_eq!(
        resolve_approve_default(Some("none".to_string()), profile),
        "none",
        "an explicit --approve none must win over the profile-aware default on {profile:?}"
      );
    }
  }

  #[test]
  fn explicit_approve_mode_always_wins_regardless_of_profile() {
    for profile in [
      HarnessProfile::Dev,
      HarnessProfile::Local,
      HarnessProfile::Production,
    ] {
      assert_eq!(
        resolve_approve_default(Some("auto-deny".to_string()), profile),
        "auto-deny"
      );
    }
  }
}

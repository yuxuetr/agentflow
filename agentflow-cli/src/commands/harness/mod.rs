//! `agentflow harness …` CLI surface.
//!
//! Phase H1 ships four subcommands wired to `agentflow_harness`:
//! - `run` — bootstrap and execute a single Harness session.
//! - `resume` — re-stream a persisted session log.
//! - `list` — enumerate session logs on disk.
//! - `inspect` — summarise a single session log.

pub mod inspect;
pub mod list;
pub mod resume;
pub mod run;

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

//! Filesystem-level bridge from Harness sessions into the
//! `agentflow-tracing` directory convention.
//!
//! The deeper integration (storing Harness envelopes inside
//! `agentflow-tracing::TraceStorage` alongside agent traces) lands with
//! the server work in Phase H5. Phase H1 ships the contract that
//! makes that future bridge non-disruptive:
//!
//! - Harness session logs are JSONL files under a fixed
//!   `harness/sessions/` subdirectory.
//! - The base directory follows the same precedence as the rest of
//!   AgentFlow: explicit override → `AGENTFLOW_TRACE_DIR` env var →
//!   `~/.agentflow/traces`.
//!
//! Any trace replay / TUI tool can crawl that location with no special
//! knowledge of Harness internals: each `<session_id>.jsonl` file
//! decodes through [`crate::HarnessEvent`] and shares its monotonic
//! `seq` semantics with the SSE / replay surfaces.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::HarnessError;
use crate::persistence::{HarnessEventSink, JsonlEventSink, default_session_dir};

/// Env var honored by every AgentFlow trace surface, including this
/// bridge.
pub const AGENTFLOW_TRACE_DIR_ENV: &str = "AGENTFLOW_TRACE_DIR";

/// Resolve the directory where Harness session logs should live when
/// integrated with the rest of the trace tooling.
///
/// Precedence:
/// 1. `override_path` (when `Some`, used verbatim).
/// 2. `$AGENTFLOW_TRACE_DIR`.
/// 3. `$HOME/.agentflow/traces`.
///
/// The resolved path is `<base>/harness/sessions/` — i.e. callers can
/// hand the returned [`PathBuf`] to [`JsonlEventSink::new`] without
/// further plumbing.
pub fn resolve_trace_session_dir(override_path: Option<&Path>) -> Result<PathBuf, HarnessError> {
  if let Some(path) = override_path {
    return Ok(default_session_dir(path));
  }
  if let Ok(env_dir) = std::env::var(AGENTFLOW_TRACE_DIR_ENV)
    && !env_dir.trim().is_empty()
  {
    return Ok(default_session_dir(Path::new(&env_dir)));
  }
  let home = dirs_home_dir().ok_or_else(|| {
    HarnessError::Other("cannot determine $HOME for default trace directory".into())
  })?;
  Ok(default_session_dir(&home.join(".agentflow").join("traces")))
}

/// Build a [`JsonlEventSink`] anchored at the bridge directory. Equivalent
/// to `JsonlEventSink::new(resolve_trace_session_dir(override_path)?)`.
pub fn open_tracing_sink(
  override_path: Option<&Path>,
) -> Result<Arc<dyn HarnessEventSink>, HarnessError> {
  let dir = resolve_trace_session_dir(override_path)?;
  Ok(Arc::new(JsonlEventSink::new(dir)))
}

fn dirs_home_dir() -> Option<PathBuf> {
  // `home_dir` is not part of std; replicate the cross-platform check
  // used by `dirs` without taking the dep here so the harness crate
  // stays lean.
  if let Some(value) = std::env::var_os("HOME")
    && !value.is_empty()
  {
    return Some(PathBuf::from(value));
  }
  #[cfg(windows)]
  {
    if let Some(value) = std::env::var_os("USERPROFILE")
      && !value.is_empty()
    {
      return Some(PathBuf::from(value));
    }
  }
  None
}

#[cfg(test)]
mod tests {
  use super::*;
  use tempfile::TempDir;

  #[test]
  fn explicit_override_wins_over_env() {
    // SAFETY: serializing env mutation by running this test alone in
    // the module; tests inside the same binary do not race because
    // cargo serializes tests in the same `#[cfg(test)]` block by
    // default unless `#[parallel]` is used. Here the test sets the
    // var and immediately reads it.
    unsafe {
      std::env::set_var(AGENTFLOW_TRACE_DIR_ENV, "/tmp/should-be-ignored");
    }
    let override_dir = TempDir::new().unwrap();
    let resolved = resolve_trace_session_dir(Some(override_dir.path())).unwrap();
    assert!(resolved.starts_with(override_dir.path()));
    assert!(resolved.ends_with("harness/sessions"));
    unsafe {
      std::env::remove_var(AGENTFLOW_TRACE_DIR_ENV);
    }
  }

  #[test]
  fn env_var_wins_over_default() {
    let env_dir = TempDir::new().unwrap();
    unsafe {
      std::env::set_var(AGENTFLOW_TRACE_DIR_ENV, env_dir.path());
    }
    let resolved = resolve_trace_session_dir(None).unwrap();
    assert!(resolved.starts_with(env_dir.path()));
    assert!(resolved.ends_with("harness/sessions"));
    unsafe {
      std::env::remove_var(AGENTFLOW_TRACE_DIR_ENV);
    }
  }

  #[test]
  fn open_tracing_sink_returns_jsonl_sink_at_resolved_path() {
    let dir = TempDir::new().unwrap();
    let sink = open_tracing_sink(Some(dir.path())).unwrap();
    assert_eq!(sink.name(), "jsonl");
  }
}

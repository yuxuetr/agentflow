use anyhow::{Context, Result};
use std::path::PathBuf;

pub mod replay;
pub mod tui;

pub(crate) fn resolve_trace_dir(trace_dir: Option<String>) -> Result<PathBuf> {
  if let Some(dir) = trace_dir.or_else(|| std::env::var("AGENTFLOW_TRACE_DIR").ok()) {
    return Ok(PathBuf::from(dir));
  }

  Ok(
    dirs::home_dir()
      .context("Could not determine home directory for default trace path")?
      .join(".agentflow")
      .join("traces"),
  )
}

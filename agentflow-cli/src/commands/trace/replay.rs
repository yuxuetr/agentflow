use anyhow::{Context, Result};
use std::path::PathBuf;

use agentflow_tracing::{
  format_trace_replay, storage::file::FileTraceStorage, ReplayOptions, TraceStorage,
};

pub async fn execute(
  run_id: String,
  trace_dir: Option<String>,
  include_json: bool,
  max_field_chars: usize,
) -> Result<()> {
  let trace_dir = match trace_dir {
    Some(dir) => PathBuf::from(dir),
    None => dirs::home_dir()
      .context("Could not determine home directory for default trace path")?
      .join(".agentflow")
      .join("traces"),
  };

  let storage = FileTraceStorage::new(trace_dir.clone())
    .with_context(|| format!("Failed to open trace directory '{}'", trace_dir.display()))?;
  let trace = storage
    .get_trace(&run_id)
    .await
    .with_context(|| format!("Failed to load trace '{}'", run_id))?
    .with_context(|| format!("Trace '{}' not found in '{}'", run_id, trace_dir.display()))?;

  let replay = format_trace_replay(
    &trace,
    ReplayOptions {
      include_json,
      max_field_chars,
    },
  );
  println!("{replay}");

  Ok(())
}

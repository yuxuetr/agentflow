use anyhow::{Context, Result};

use super::resolve_trace_dir;
use agentflow_tracing::{
  RedactionConfig, ReplayOptions, TraceStorage, format_trace_replay, redact_trace,
  storage::file::FileTraceStorage,
};

pub async fn execute(
  run_id: String,
  trace_dir: Option<String>,
  include_json: bool,
  max_field_chars: usize,
) -> Result<()> {
  let trace_dir = resolve_trace_dir(trace_dir)?;

  let storage = FileTraceStorage::new(trace_dir.clone())
    .with_context(|| format!("Failed to open trace directory '{}'", trace_dir.display()))?;
  let mut trace = storage
    .get_trace(&run_id)
    .await
    .with_context(|| format!("Failed to load trace '{}'", run_id))?
    .with_context(|| format!("Trace '{}' not found in '{}'", run_id, trace_dir.display()))?;
  redact_trace(&mut trace, &RedactionConfig::default());

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

use anyhow::{bail, Context, Result};
use std::{path::PathBuf, str::FromStr};

use agentflow_tracing::{
  format_trace_tui, storage::file::FileTraceStorage, TraceStorage, TraceTuiFilter, TraceTuiOptions,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliTraceTuiFilter {
  All,
  Workflow,
  Agent,
  Tool,
  Mcp,
}

impl FromStr for CliTraceTuiFilter {
  type Err = anyhow::Error;

  fn from_str(value: &str) -> Result<Self> {
    match value {
      "all" => Ok(Self::All),
      "workflow" => Ok(Self::Workflow),
      "agent" => Ok(Self::Agent),
      "tool" => Ok(Self::Tool),
      "mcp" => Ok(Self::Mcp),
      other => bail!(
        "unsupported trace TUI filter '{other}' (expected all, workflow, agent, tool, or mcp)"
      ),
    }
  }
}

impl From<CliTraceTuiFilter> for TraceTuiFilter {
  fn from(value: CliTraceTuiFilter) -> Self {
    match value {
      CliTraceTuiFilter::All => Self::All,
      CliTraceTuiFilter::Workflow => Self::Workflow,
      CliTraceTuiFilter::Agent => Self::Agent,
      CliTraceTuiFilter::Tool => Self::Tool,
      CliTraceTuiFilter::Mcp => Self::Mcp,
    }
  }
}

pub async fn execute(
  run_id: String,
  trace_dir: Option<String>,
  filter: CliTraceTuiFilter,
  details: bool,
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

  let output = format_trace_tui(
    &trace,
    TraceTuiOptions {
      filter: filter.into(),
      details,
      max_field_chars,
    },
  );
  println!("{output}");

  Ok(())
}

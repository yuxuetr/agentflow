use clap::{Args, Subcommand};

use super::{replay, tui};

#[derive(Args)]
pub struct TraceArgs {
  #[command(subcommand)]
  command: TraceCommands,
}

#[derive(Subcommand)]
enum TraceCommands {
  /// Replay a persisted workflow/agent trace without re-executing tools or LLMs
  Replay {
    /// Workflow run ID / trace ID to replay
    run_id: String,
    /// Directory containing file-backed traces (default: AGENTFLOW_TRACE_DIR or ~/.agentflow/traces)
    #[arg(long)]
    dir: Option<String>,
    /// Include raw trace JSON after the replay timeline (text format only)
    #[arg(long)]
    json: bool,
    /// Maximum characters printed for prompt, response, params, and output fields
    #[arg(long, default_value_t = 160)]
    max_field_chars: usize,
    /// Output format: text (default; pairs with `--json` for trailing
    /// JSON) or json-envelope (canonical `CliJsonEnvelope` —
    /// `agentflow.cli/1` wire schema; skips text replay, `--json` is
    /// ignored since the envelope already carries the full trace).
    #[arg(long, default_value = "text", value_parser = ["text", "json-envelope"])]
    format: String,
  },
  /// Inspect a persisted trace as a focused terminal timeline
  Tui {
    /// Workflow run ID / trace ID to inspect
    run_id: String,
    /// Directory containing file-backed traces (default: AGENTFLOW_TRACE_DIR or ~/.agentflow/traces)
    #[arg(long)]
    dir: Option<String>,
    /// Timeline focus: all, workflow, agent, tool, or mcp
    #[arg(long, default_value = "all")]
    filter: tui::CliTraceTuiFilter,
    /// Expand matching timeline rows with captured fields
    #[arg(long)]
    details: bool,
    /// Maximum characters printed for params, steps, input, and output fields
    #[arg(long, default_value_t = 120)]
    max_field_chars: usize,
  },
}

pub async fn dispatch(args: TraceArgs) -> anyhow::Result<()> {
  match args.command {
    TraceCommands::Replay {
      run_id,
      dir,
      json,
      max_field_chars,
      format,
    } => replay::execute(run_id, dir, json, max_field_chars, format).await,
    TraceCommands::Tui {
      run_id,
      dir,
      filter,
      details,
      max_field_chars,
    } => tui::execute(run_id, dir, filter, details, max_field_chars).await,
  }
}

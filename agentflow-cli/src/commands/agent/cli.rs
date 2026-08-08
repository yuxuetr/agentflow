use clap::{Args, Subcommand};

use super::replay;

#[derive(Args)]
pub struct AgentArgs {
  #[command(subcommand)]
  command: AgentCommands,
}

#[derive(Subcommand)]
enum AgentCommands {
  /// Compare a fresh ReAct agent trace against a golden baseline.
  ///
  /// Both arguments are JSONL files containing one `AgentEvent` per
  /// line. The diff reduces them to step-order + stop-reason +
  /// per-step token usage; exits non-zero on tool-call or stop-reason
  /// divergence. Token deltas are reported but don't fail the gate
  /// unless `--strict-tokens` is set (LLM token counts jitter
  /// between identical requests).
  Replay {
    /// The fresh trace to compare against the baseline.
    current: std::path::PathBuf,
    /// Path to the golden baseline trace.
    #[arg(long)]
    diff: std::path::PathBuf,
    /// Treat any non-zero token-count delta as a divergence rather
    /// than a soft variance. Off by default because LLM token
    /// accounting varies a handful of tokens between identical runs.
    #[arg(long)]
    strict_tokens: bool,
    /// Output format.
    #[arg(long, default_value = "text", value_parser = ["text", "stream-json", "json-envelope"])]
    format: String,
  },
}

pub async fn dispatch(args: AgentArgs) -> anyhow::Result<()> {
  match args.command {
    AgentCommands::Replay {
      current,
      diff,
      strict_tokens,
      format,
    } => replay::execute(current, diff, format, strict_tokens).await,
  }
}

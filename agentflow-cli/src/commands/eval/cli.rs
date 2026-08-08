use clap::{Args, Subcommand};

use super::execute;

#[derive(Args)]
pub struct EvalArgs {
  #[command(subcommand)]
  command: EvalCommands,
}

#[derive(Subcommand)]
enum EvalCommands {
  /// Execute an eval dataset and emit the structured report
  Run {
    /// Path to the eval dataset directory (`dataset.toml` + `cases.jsonl`)
    dataset_dir: String,
    /// Output format: text, json (legacy bare body), or json-envelope
    /// (canonical `CliJsonEnvelope` — `agentflow.cli/1` wire schema)
    #[arg(long, default_value = "text", value_parser = ["text", "json", "json-envelope"])]
    format: String,
    /// Glob filter applied to case ids (supports `*` and `?`); non-
    /// matching cases are reported as skipped.
    #[arg(long)]
    filter: Option<String>,
    /// Exit-status policy: `failed` (any failed case → exit 1) or
    /// `never` (always exit 0 unless dataset itself is malformed).
    #[arg(long = "fail-on-status", default_value = "failed", value_parser = ["failed", "never"])]
    fail_on_status: String,
    /// T2.1: compare this run's summary metrics (success rate, average
    /// step/tool-call counts) against a checked-in baseline JSON file
    /// (see `EvalBaseline`); exits nonzero on regression beyond
    /// tolerance regardless of `--fail-on-status`. Mutually exclusive
    /// with `--dump-baseline`.
    #[arg(long)]
    compare_baseline: Option<String>,
    /// T2.1: write this run's summary metrics as a new baseline JSON
    /// file at the given path, for later use with `--compare-baseline`.
    /// Mutually exclusive with `--compare-baseline`.
    #[arg(long)]
    dump_baseline: Option<String>,
  },
}

pub async fn dispatch(args: EvalArgs) -> anyhow::Result<()> {
  match args.command {
    EvalCommands::Run {
      dataset_dir,
      format,
      filter,
      fail_on_status,
      compare_baseline,
      dump_baseline,
    } => {
      execute(
        dataset_dir,
        format,
        filter,
        fail_on_status,
        compare_baseline,
        dump_baseline,
      )
      .await
    }
  }
}

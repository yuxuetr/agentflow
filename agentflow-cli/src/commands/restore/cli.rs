use clap::Args;

use crate::commands::backup::parse_includes;

use super::execute;

#[derive(Args)]
pub struct RestoreArgs {
  /// Bundle directory produced by a prior `agentflow backup --output <path>`.
  input: std::path::PathBuf,
  /// Postgres URL to restore into (default env: DATABASE_URL). Only
  /// consulted when the `db` include is requested and present in the
  /// bundle's manifest.
  #[arg(long)]
  database_url: Option<String>,
  /// Print the plan (parsed manifest + which steps would run), mutate nothing.
  #[arg(long)]
  dry_run: bool,
  /// Overwrite an existing target directory for a filesystem include
  /// by removing it first, instead of failing that step.
  #[arg(long)]
  force: bool,
  /// Restrict to one or more includes (repeat the flag). Empty = every
  /// include present in the bundle's manifest. Same aliases as
  /// `agentflow backup --include`.
  #[arg(long = "include", short = 'i', value_name = "INCLUDE", num_args = 0..)]
  includes: Vec<String>,
  /// Output format (canonical `CliJsonEnvelope` — `agentflow.cli/1`
  /// wire schema for `json-envelope`).
  #[arg(long, default_value = "text", value_parser = ["text", "json", "json-envelope"])]
  format: String,
  /// Restore an artifact even when its recomputed SHA-256 does not
  /// match the manifest's recorded hash (U0.2). Not recommended — a
  /// mismatch means the artifact was modified since `agentflow
  /// backup` wrote it (corruption or tampering).
  #[arg(long)]
  skip_integrity_check: bool,
}

pub async fn dispatch(args: RestoreArgs) -> anyhow::Result<()> {
  match parse_includes(&args.includes) {
    Ok(includes) => {
      execute(super::RestoreArgs {
        input: args.input,
        database_url: args.database_url,
        dry_run: args.dry_run,
        force: args.force,
        includes,
        format: args.format,
        skip_integrity_check: args.skip_integrity_check,
      })
      .await
    }
    Err(err) => Err(err),
  }
}

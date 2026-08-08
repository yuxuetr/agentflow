use clap::Args;

use super::{execute, parse_includes};

#[derive(Args)]
pub struct BackupArgs {
  /// Destination directory for the bundle. Created if missing.
  /// Refuses to overwrite a non-empty directory without --force.
  #[arg(long, short = 'o')]
  output: std::path::PathBuf,
  /// Postgres URL (default env: DATABASE_URL). Only consulted
  /// when the `db` include is in the set.
  #[arg(long)]
  database_url: Option<String>,
  /// Print the plan + which steps would run, mutate nothing.
  #[arg(long)]
  dry_run: bool,
  /// Overwrite a non-empty `--output` directory.
  #[arg(long)]
  force: bool,
  /// Restrict to one or more includes (repeat the flag). Empty =
  /// all 6 (db, run_dir, trace_dir, marketplace_cache,
  /// skills_dir, plugins_dir). Aliases accepted: `runs` →
  /// `run_dir`, `traces` → `trace_dir`, `database` → `db`, etc.
  #[arg(long = "include", short = 'i', value_name = "INCLUDE", num_args = 0..)]
  includes: Vec<String>,
  /// Output format (canonical `CliJsonEnvelope` — `agentflow.cli/1`
  /// wire schema for `json-envelope`).
  #[arg(long, default_value = "text", value_parser = ["text", "json", "json-envelope"])]
  format: String,
}

pub async fn dispatch(args: BackupArgs) -> anyhow::Result<()> {
  match parse_includes(&args.includes) {
    Ok(includes) => {
      execute(super::BackupArgs {
        output: args.output,
        database_url: args.database_url,
        dry_run: args.dry_run,
        force: args.force,
        includes,
        format: args.format,
      })
      .await
    }
    Err(err) => Err(err),
  }
}

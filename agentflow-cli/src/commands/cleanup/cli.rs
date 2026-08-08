use clap::Args;

use super::execute;

#[derive(Args)]
pub struct CleanupArgs {
  /// Postgres URL (default env: DATABASE_URL)
  #[arg(long)]
  database_url: Option<String>,
  /// Workflow run-artifact root (env: AGENTFLOW_RUN_DIR)
  #[arg(long)]
  run_dir: Option<String>,
  /// Trace directory (env: AGENTFLOW_TRACE_DIR)
  #[arg(long)]
  trace_dir: Option<String>,
  /// Active security profile (drives retention defaults)
  #[arg(long, default_value = "local", value_parser = ["dev", "local", "production"])]
  security_profile: String,
  /// Preview targets without deleting anything
  #[arg(long)]
  dry_run: bool,
}

pub async fn dispatch(args: CleanupArgs) -> anyhow::Result<()> {
  execute(
    args.database_url,
    args.run_dir,
    args.trace_dir,
    args.security_profile,
    args.dry_run,
  )
  .await
}

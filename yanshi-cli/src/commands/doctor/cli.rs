use clap::Args;

use super::{DoctorProfile, OutputFormat, execute};

#[derive(Args)]
pub struct DoctorArgs {
  /// Output format. `text` (default) prints a human-readable report.
  /// `json` emits the legacy raw `DoctorReport`. `json-envelope` wraps
  /// the report in the canonical CLI JSON envelope from P3.3 (see
  /// `docs/CLI_JSON_OUTPUT.md`).
  #[arg(long, default_value = "text", value_parser = ["text", "json", "json-envelope"])]
  format: String,
  /// Pass/fail threshold profile
  #[arg(long, default_value = "local", value_parser = ["dev", "local", "production"])]
  profile: String,
  /// When supplied, also probe `<url>/health` for server reachability
  #[arg(long)]
  server: Option<String>,
  /// Add a backup-readiness section: explicit writability probe for
  /// run_dir / trace_dir / marketplace_cache / skills_dir / plugins_dir
  #[arg(long = "backup-check")]
  backup_check: bool,
  /// Add an `installations` section: walks the local skills + plugins
  /// dirs, surfaces every declared MCP server command + plugin
  /// entrypoint, and flags unreachable binaries. Lite alternative to
  /// the deferred transport-level MCP / plugin probes (P3.4).
  #[arg(long = "check-installations")]
  check_installations: bool,
}

pub async fn dispatch(args: DoctorArgs) -> anyhow::Result<()> {
  match (
    OutputFormat::parse(&args.format),
    DoctorProfile::parse(&args.profile),
  ) {
    (Ok(format), Ok(profile)) => {
      execute(
        format,
        profile,
        args.server,
        args.backup_check,
        args.check_installations,
      )
      .await
    }
    (Err(err), _) | (_, Err(err)) => Err(err),
  }
}

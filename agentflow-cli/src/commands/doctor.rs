//! `agentflow doctor` — environment / installation diagnostics.
//!
//! The report model + builder (`DoctorReport`, `DoctorProfile`, `build_report`,
//! `print_text_report`, …) live in `agentflow_config::diagnostics` so the server
//! gateway can reuse them without depending on the CLI (P-A2.4). This module
//! keeps the CLI command handler — which owns the JSON-envelope output and the
//! top-level MCP probe that reads the CLI's `mcp.toml` config format — and
//! re-exports the diagnostics surface under the original `doctor::*` paths.

use anyhow::Result;

pub use agentflow_config::diagnostics::*;

/// Run the `doctor` command: build the report (injecting the CLI's top-level
/// MCP probe) and render it in the requested format.
pub async fn execute(
  format: OutputFormat,
  profile: DoctorProfile,
  server: Option<String>,
  backup_check: bool,
  check_installations: bool,
) -> Result<()> {
  // Only probe the top-level `mcp.toml` registry when installation checks are
  // requested — the report builder ignores it otherwise.
  let top_level_mcp = if check_installations {
    probe_top_level_mcp_config()
  } else {
    (None, Vec::new())
  };
  let report = build_report(
    profile,
    server.as_deref(),
    backup_check,
    check_installations,
    top_level_mcp,
  )
  .await;
  match format {
    OutputFormat::Json => {
      println!("{}", serde_json::to_string_pretty(&report)?);
    }
    OutputFormat::JsonEnvelope => {
      let envelope = crate::json_envelope::CliJsonEnvelope::ok("doctor", &report);
      println!("{}", serde_json::to_string_pretty(&envelope)?);
    }
    OutputFormat::Text => print_text_report(&report),
  }
  std::process::exit(report.exit_code());
}

/// Walk the top-level MCP registry (`~/.agentflow/mcp.toml` or its
/// `AGENTFLOW_MCP_CONFIG` env override). Errors loading the file
/// (parse / validation) are swallowed and reported as `source =
/// Some("<err message>")` so doctor still completes; the caller
/// surfaces this via status promotion.
///
/// Kept in the CLI (rather than the shared `diagnostics` builder) because it
/// reads `McpConfigFile`, the CLI's `mcp.toml` config format.
fn probe_top_level_mcp_config() -> (Option<String>, Vec<McpServerProbe>) {
  use crate::commands::mcp::config::McpConfigFile;
  match McpConfigFile::load_default() {
    Ok((config, source)) => {
      let source_str = source.path().map(|_| source.display_path());
      let probes = config
        .mcp_servers
        .iter()
        .map(|server| {
          let cmd = server.command.trim();
          let reachable = if cmd.is_empty() {
            false
          } else {
            which::which(cmd).is_ok() || std::path::Path::new(cmd).is_file()
          };
          McpServerProbe {
            skill: None,
            server: server.name.clone(),
            command: cmd.to_string(),
            reachable,
          }
        })
        .collect();
      (source_str, probes)
    }
    Err(e) => (
      // Surface the load failure so operators see it in the report
      // rather than getting a silently-empty list.
      Some(format!("error loading mcp.toml: {e}")),
      Vec::new(),
    ),
  }
}

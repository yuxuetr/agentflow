//! Top-level MCP server configuration (P3.4-PR.2).
//!
//! Today MCP servers are only declarable inside skills (`skill.toml`
//! `[[mcp_servers]]`). The doctor's lite probe walks installed skills
//! and reports whether each declared server's binary exists. That
//! misses the case where an operator wants AgentFlow itself to know
//! about an MCP server outside any skill — say `github` or
//! `filesystem` — so the doctor (and future surface) can probe it
//! once per host instead of once per skill.
//!
//! This module introduces a top-level `~/.agentflow/mcp.toml` file
//! that lists named MCP servers using the **same** [`McpServerConfig`]
//! shape skill manifests already use. Resolution order mirrors the
//! LLM models config:
//!
//! 1. `AGENTFLOW_MCP_CONFIG` env override
//! 2. `~/.agentflow/mcp.toml`
//! 3. None — empty config, doctor reports zero configured servers
//!
//! The file format:
//!
//! ```toml
//! [[mcp_servers]]
//! name = "filesystem"
//! command = "npx"
//! args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
//! env = { READ_ONLY = "1" }
//!
//! [[mcp_servers]]
//! name = "github"
//! command = "uvx"
//! args = ["mcp-server-github"]
//! ```
//!
//! No `enabled` flag yet — operators who want to keep a config entry
//! around but disable it can prefix the section header with `#`. We
//! can add structured enable/disable when there's concrete demand.

use agentflow_skills::McpServerConfig;
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Environment variable that overrides the default config path.
pub const MCP_CONFIG_ENV: &str = "AGENTFLOW_MCP_CONFIG";

/// Parsed `mcp.toml`. The on-disk shape is `[[mcp_servers]]`
/// tables (one per server); the wrapper struct here exists so future
/// top-level fields (e.g. global timeouts, defaults) can land without
/// breaking existing files.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpConfigFile {
  /// Configured MCP servers, in file order.
  #[serde(default)]
  pub mcp_servers: Vec<McpServerConfig>,
}

/// How `mcp.toml` was located.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpConfigSource {
  /// `AGENTFLOW_MCP_CONFIG` env var pointed at this path.
  EnvOverride(PathBuf),
  /// User-level `~/.agentflow/mcp.toml` resolved.
  UserConfig(PathBuf),
  /// No config file present. Loader returns an empty
  /// [`McpConfigFile`]; doctor reports zero configured servers.
  None,
}

impl McpConfigSource {
  /// Path on disk, or `None` for the implicit-empty source.
  pub fn path(&self) -> Option<&Path> {
    match self {
      McpConfigSource::EnvOverride(p) | McpConfigSource::UserConfig(p) => Some(p.as_path()),
      McpConfigSource::None => None,
    }
  }

  /// Operator-readable description (mirrors `LLMConfigSource::display_path`).
  pub fn display_path(&self) -> String {
    match self {
      McpConfigSource::EnvOverride(p) => format!("{} (from {MCP_CONFIG_ENV})", p.display()),
      McpConfigSource::UserConfig(p) => p.display().to_string(),
      McpConfigSource::None => "<no mcp.toml configured>".to_string(),
    }
  }
}

impl McpConfigFile {
  /// Resolve the on-disk source for the MCP config using AgentFlow's
  /// default search precedence.
  ///
  /// This is the same precedence model the LLM config uses, so
  /// operators don't have to learn a new resolution order per file.
  pub fn resolve_default_source() -> McpConfigSource {
    let env_value = std::env::var_os(MCP_CONFIG_ENV);
    let home = dirs::home_dir();
    Self::resolve_default_source_from(home.as_deref(), env_value)
  }

  /// Pure version of [`Self::resolve_default_source`] for tests.
  pub fn resolve_default_source_from(
    home: Option<&Path>,
    env_override: Option<std::ffi::OsString>,
  ) -> McpConfigSource {
    if let Some(value) = env_override.filter(|v| !v.is_empty()) {
      return McpConfigSource::EnvOverride(PathBuf::from(value));
    }
    if let Some(home) = home {
      let user_path = home.join(".agentflow").join("mcp.toml");
      if user_path.exists() {
        return McpConfigSource::UserConfig(user_path);
      }
    }
    McpConfigSource::None
  }

  /// Load + validate the resolved config. Absent file ⇒ empty config.
  pub fn load_default() -> Result<(Self, McpConfigSource)> {
    let source = Self::resolve_default_source();
    let config = match source.path() {
      Some(path) => Self::load_from_path(path)?,
      None => Self::default(),
    };
    Ok((config, source))
  }

  /// Parse + validate the file at `path`.
  pub fn load_from_path(path: &Path) -> Result<Self> {
    let raw = std::fs::read_to_string(path)
      .with_context(|| format!("failed to read MCP config at {}", path.display()))?;
    let config: McpConfigFile = toml::from_str(&raw)
      .with_context(|| format!("failed to parse MCP config at {}", path.display()))?;
    config.validate()?;
    Ok(config)
  }

  /// Validate cross-entry invariants. Per-entry shape checks (empty
  /// command, etc.) are enforced here rather than via
  /// `serde(deny_unknown_fields)` so the error messages name the
  /// offending server.
  pub fn validate(&self) -> Result<()> {
    let mut seen: HashSet<&str> = HashSet::new();
    for server in &self.mcp_servers {
      if server.name.trim().is_empty() {
        return Err(anyhow!(
          "MCP config entry has empty `name`; every server needs a unique identifier"
        ));
      }
      if !seen.insert(server.name.as_str()) {
        return Err(anyhow!(
          "duplicate MCP server name '{}' — names must be unique within mcp.toml",
          server.name
        ));
      }
      if server.command.trim().is_empty() {
        return Err(anyhow!(
          "MCP server '{}' has empty `command`; nothing to spawn",
          server.name
        ));
      }
    }
    Ok(())
  }

  /// Look up a configured server by name.
  pub fn get(&self, name: &str) -> Option<&McpServerConfig> {
    self.mcp_servers.iter().find(|s| s.name == name)
  }
}

// ── CLI subcommand handlers ──────────────────────────────────────────────────

/// `agentflow mcp config path` — print the resolved file path so
/// shell automation can `cat $(agentflow mcp config path)`.
pub fn run_path() -> Result<()> {
  let source = McpConfigFile::resolve_default_source();
  println!("{}", source.display_path());
  Ok(())
}

/// `agentflow mcp config validate` — parse + cross-validate, report
/// each issue with the offending server name.
pub fn run_validate() -> Result<()> {
  let (config, source) = McpConfigFile::load_default()?;
  println!("✅ MCP config OK — {}", source.display_path());
  println!("   {} server(s) configured", config.mcp_servers.len());
  Ok(())
}

/// `agentflow mcp config list` — print the configured server names +
/// commands so operators can see what doctor / future tools will
/// probe.
///
/// `format` accepts:
/// - `"text"` (default) — human-readable bullet list.
/// - `"json"` — legacy bare body `{source, servers}`. Preserved
///   for back-compat with existing automation; new tooling
///   should prefer `json-envelope`.
/// - `"json-envelope"` — canonical `CliJsonEnvelope` wrapping
///   the same body the legacy `json` mode emits, so scripts get
///   the closed `agentflow.cli/1` wire shape (P10.11.3).
pub fn run_list(format: &str) -> Result<()> {
  let (config, source) = McpConfigFile::load_default()?;
  let payload = serde_json::json!({
    "source": source.display_path(),
    "servers": config.mcp_servers,
  });
  match format {
    "json" => {
      println!("{}", serde_json::to_string_pretty(&payload)?);
      return Ok(());
    }
    "json-envelope" => {
      let envelope = crate::json_envelope::CliJsonEnvelope::ok("mcp config list", &payload);
      println!("{}", serde_json::to_string_pretty(&envelope)?);
      return Ok(());
    }
    _ => {}
  }

  println!("MCP config: {}", source.display_path());
  if config.mcp_servers.is_empty() {
    println!("(no servers configured)");
    return Ok(());
  }
  for server in &config.mcp_servers {
    let args_part = if server.args.is_empty() {
      String::new()
    } else {
      format!(" {}", server.args.join(" "))
    };
    println!("  • {:20}  {}{}", server.name, server.command, args_part);
  }
  Ok(())
}

/// `agentflow mcp config show <name>` — print one server's full
/// config (incl. env + timeouts) as JSON.
pub fn run_show(name: &str) -> Result<()> {
  let (config, _) = McpConfigFile::load_default()?;
  let server = config.get(name).ok_or_else(|| {
    anyhow!(
      "MCP server '{}' not found in mcp.toml (known: {})",
      name,
      config
        .mcp_servers
        .iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
    )
  })?;
  println!("{}", serde_json::to_string_pretty(server)?);
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;
  use tempfile::TempDir;

  fn write_config(home: &Path, content: &str) -> PathBuf {
    let dir = home.join(".agentflow");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("mcp.toml");
    fs::write(&path, content).unwrap();
    path
  }

  #[test]
  fn resolve_default_source_returns_none_when_no_file_and_no_env() {
    let tmp = TempDir::new().unwrap();
    let source = McpConfigFile::resolve_default_source_from(Some(tmp.path()), None);
    assert_eq!(source, McpConfigSource::None);
    assert!(source.path().is_none());
  }

  #[test]
  fn resolve_default_source_honors_env_override_above_user_file() {
    let tmp = TempDir::new().unwrap();
    let user_path = write_config(tmp.path(), "");
    let override_path = tmp.path().join("override.toml");
    fs::write(&override_path, "").unwrap();

    let source = McpConfigFile::resolve_default_source_from(
      Some(tmp.path()),
      Some(override_path.clone().into_os_string()),
    );
    assert_eq!(source, McpConfigSource::EnvOverride(override_path));
    // User file would also have resolved if env wasn't set; that's
    // the precedence we're locking in.
    assert!(user_path.exists());
  }

  #[test]
  fn resolve_default_source_finds_user_file_when_env_unset() {
    let tmp = TempDir::new().unwrap();
    let user_path = write_config(tmp.path(), "");
    let source = McpConfigFile::resolve_default_source_from(Some(tmp.path()), None);
    assert_eq!(source, McpConfigSource::UserConfig(user_path));
  }

  #[test]
  fn parses_minimal_config_with_two_servers() {
    let tmp = TempDir::new().unwrap();
    let path = write_config(
      tmp.path(),
      r#"
[[mcp_servers]]
name = "filesystem"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]

[[mcp_servers]]
name = "github"
command = "uvx"
args = ["mcp-server-github"]
"#,
    );
    let config = McpConfigFile::load_from_path(&path).unwrap();
    assert_eq!(config.mcp_servers.len(), 2);
    assert_eq!(config.mcp_servers[0].name, "filesystem");
    assert_eq!(config.mcp_servers[1].command, "uvx");
    // Per-server timeouts and env optional, default values consistent
    // with skill.toml.
    assert!(config.mcp_servers[0].env.is_empty());
    assert!(config.mcp_servers[0].timeout_secs.is_none());
  }

  #[test]
  fn parses_full_config_with_env_and_timeout() {
    let tmp = TempDir::new().unwrap();
    let path = write_config(
      tmp.path(),
      r#"
[[mcp_servers]]
name = "github"
command = "uvx"
args = ["mcp-server-github"]
timeout_secs = 30
max_concurrent_calls = 4

[mcp_servers.env]
GITHUB_PERSONAL_ACCESS_TOKEN = "${GITHUB_TOKEN}"
"#,
    );
    let config = McpConfigFile::load_from_path(&path).unwrap();
    let s = &config.mcp_servers[0];
    assert_eq!(s.timeout_secs, Some(30));
    assert_eq!(s.max_concurrent_calls, Some(4));
    assert_eq!(
      s.env
        .get("GITHUB_PERSONAL_ACCESS_TOKEN")
        .map(String::as_str),
      Some("${GITHUB_TOKEN}")
    );
  }

  #[test]
  fn validate_rejects_duplicate_server_names() {
    let config = McpConfigFile {
      mcp_servers: vec![
        McpServerConfig {
          name: "filesystem".into(),
          command: "npx".into(),
          ..Default::default()
        },
        McpServerConfig {
          name: "filesystem".into(),
          command: "uvx".into(),
          ..Default::default()
        },
      ],
    };
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("duplicate"));
    assert!(err.to_string().contains("filesystem"));
  }

  #[test]
  fn validate_rejects_empty_command() {
    let config = McpConfigFile {
      mcp_servers: vec![McpServerConfig {
        name: "broken".into(),
        command: "  ".into(),
        ..Default::default()
      }],
    };
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("empty `command`"));
    assert!(err.to_string().contains("broken"));
  }

  #[test]
  fn validate_rejects_empty_name() {
    let config = McpConfigFile {
      mcp_servers: vec![McpServerConfig {
        name: "".into(),
        command: "npx".into(),
        ..Default::default()
      }],
    };
    let err = config.validate().unwrap_err();
    assert!(err.to_string().contains("empty `name`"));
  }

  #[test]
  fn load_default_returns_empty_when_no_source() {
    // We can't easily isolate the global HOME for this test, but we
    // can call the lower-level helper to confirm an absent path
    // resolves to None and loads empty.
    let tmp = TempDir::new().unwrap();
    let source = McpConfigFile::resolve_default_source_from(Some(tmp.path()), None);
    assert_eq!(source, McpConfigSource::None);
    // Loader path absent ⇒ empty config (mirrors the production
    // `load_default` short-circuit).
    let config = McpConfigFile::default();
    assert!(config.mcp_servers.is_empty());
    assert!(config.validate().is_ok());
  }

  #[test]
  fn get_finds_server_by_name() {
    let config = McpConfigFile {
      mcp_servers: vec![McpServerConfig {
        name: "filesystem".into(),
        command: "npx".into(),
        ..Default::default()
      }],
    };
    assert!(config.get("filesystem").is_some());
    assert!(config.get("missing").is_none());
  }
}

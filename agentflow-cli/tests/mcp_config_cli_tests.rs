//! End-to-end smoke for `agentflow mcp config` (P3.4-PR.2).
//!
//! Unit-level coverage of the parser + validator + resolver lives in
//! `agentflow-cli/src/commands/mcp/config.rs::tests`. These tests
//! exercise the CLI wiring itself: subcommand discovery, exit codes,
//! and output shape against a fixture `mcp.toml` injected via the
//! `AGENTFLOW_MCP_CONFIG` env override.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn write_fixture(content: &str) -> (TempDir, std::path::PathBuf) {
  let tmp = TempDir::new().expect("tempdir");
  let path = tmp.path().join("mcp.toml");
  std::fs::write(&path, content).expect("write fixture");
  (tmp, path)
}

#[test]
fn config_subcommand_help_lists_known_subcommands() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args(["mcp", "config", "--help"]);
  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("path"))
    .stdout(predicate::str::contains("validate"))
    .stdout(predicate::str::contains("list"))
    .stdout(predicate::str::contains("show"));
}

#[test]
fn config_list_against_fixture_shows_server_names() {
  let (_tmp, path) = write_fixture(
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
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .env("AGENTFLOW_MCP_CONFIG", &path)
    .args(["mcp", "config", "list"]);
  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("filesystem"))
    .stdout(predicate::str::contains("github"))
    .stdout(predicate::str::contains("npx"))
    .stdout(predicate::str::contains("uvx"));
}

#[test]
fn config_list_json_format_emits_structured_payload() {
  let (_tmp, path) = write_fixture(
    r#"
[[mcp_servers]]
name = "filesystem"
command = "npx"
"#,
  );
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .env("AGENTFLOW_MCP_CONFIG", &path)
    .args(["mcp", "config", "list", "--format", "json"]);
  let out = cmd.assert().success();
  let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
  // Don't lock down exact whitespace — just verify the schema-stable
  // top-level keys are there and the server round-tripped.
  let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
  assert!(parsed.get("source").is_some(), "envelope must carry source");
  let servers = parsed.get("servers").and_then(|v| v.as_array()).unwrap();
  assert_eq!(servers.len(), 1);
  assert_eq!(
    servers[0].get("name").and_then(|v| v.as_str()),
    Some("filesystem")
  );
}

#[test]
fn config_show_prints_one_server_as_json() {
  let (_tmp, path) = write_fixture(
    r#"
[[mcp_servers]]
name = "github"
command = "uvx"
args = ["mcp-server-github"]
timeout_secs = 30
"#,
  );
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .env("AGENTFLOW_MCP_CONFIG", &path)
    .args(["mcp", "config", "show", "github"]);
  let out = cmd.assert().success();
  let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
  let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
  assert_eq!(
    parsed.get("name").and_then(|v| v.as_str()),
    Some("github")
  );
  assert_eq!(
    parsed.get("timeout_secs").and_then(|v| v.as_u64()),
    Some(30)
  );
}

#[test]
fn config_show_errors_when_server_name_unknown() {
  let (_tmp, path) = write_fixture(
    r#"
[[mcp_servers]]
name = "filesystem"
command = "npx"
"#,
  );
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .env("AGENTFLOW_MCP_CONFIG", &path)
    .args(["mcp", "config", "show", "missing-server"]);
  cmd
    .assert()
    .failure()
    .stderr(predicate::str::contains("missing-server"))
    // The error helpfully lists known servers so operators don't
    // have to grep their config.
    .stderr(predicate::str::contains("filesystem"));
}

#[test]
fn config_validate_succeeds_on_valid_fixture() {
  let (_tmp, path) = write_fixture(
    r#"
[[mcp_servers]]
name = "filesystem"
command = "npx"
"#,
  );
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .env("AGENTFLOW_MCP_CONFIG", &path)
    .args(["mcp", "config", "validate"]);
  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("OK"))
    .stdout(predicate::str::contains("1 server"));
}

#[test]
fn config_validate_rejects_duplicate_server_names() {
  // Cross-entry invariants surface as a non-zero exit with a clear
  // message — that's the doctor-friendly contract.
  let (_tmp, path) = write_fixture(
    r#"
[[mcp_servers]]
name = "filesystem"
command = "npx"

[[mcp_servers]]
name = "filesystem"
command = "uvx"
"#,
  );
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .env("AGENTFLOW_MCP_CONFIG", &path)
    .args(["mcp", "config", "validate"]);
  cmd
    .assert()
    .failure()
    .stderr(predicate::str::contains("duplicate"))
    .stderr(predicate::str::contains("filesystem"));
}

#[test]
fn config_path_prints_env_override_path() {
  let (_tmp, path) = write_fixture("");
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .env("AGENTFLOW_MCP_CONFIG", &path)
    .args(["mcp", "config", "path"]);
  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains(path.to_str().unwrap()));
}

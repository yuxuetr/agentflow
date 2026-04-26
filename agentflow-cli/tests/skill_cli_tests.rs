use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

fn mcp_basic_skill_path() -> String {
  format!(
    "{}/../agentflow-skills/examples/skills/mcp-basic",
    env!("CARGO_MANIFEST_DIR")
  )
}

fn write_mock_models_config(home: &Path) {
  let config_dir = home.join(".agentflow");
  fs::create_dir_all(&config_dir).unwrap();
  fs::write(
    config_dir.join("models.yml"),
    r#"
models:
  mock-model:
    vendor: mock
    type: text
    model_id: mock-model
providers:
  mock:
    api_key_env: MOCK_API_KEY
"#,
  )
  .unwrap();
}

fn write_mock_mcp_skill(dir: &Path) {
  let server = format!(
    "{}/../agentflow-skills/examples/skills/mcp-basic/server.py",
    env!("CARGO_MANIFEST_DIR")
  );
  fs::write(
    dir.join("skill.toml"),
    format!(
      r#"
[skill]
name = "mock_mcp_runner"
version = "0.1.0"
description = "Mock ReAct skill for MCP CLI tests"

[persona]
role = "Use MCP tools when needed."

[model]
name = "mock-model"
max_iterations = 4

[[mcp_servers]]
name = "local-demo"
command = "python3"
args = [{server:?}]
"#
    ),
  )
  .unwrap();
}

fn mock_react_responses() -> String {
  serde_json::to_string(&vec![
    r#"{"thought":"call echo","action":{"tool":"mcp_local_demo_echo","params":{"text":"from run"}}}"#,
    r#"{"thought":"observed echo","answer":"MCP said: mcp-basic: from run"}"#,
  ])
  .unwrap()
}

#[test]
fn skill_validate_checks_mcp_server_config() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["skill", "validate", &mcp_basic_skill_path()])
    .assert()
    .success()
    .stdout(predicate::str::contains("MCP Servers (1)"))
    .stdout(predicate::str::contains("discovered MCP tools: 2"));
}

#[test]
fn skill_list_tools_shows_mcp_tools_and_schema() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["skill", "list-tools", &mcp_basic_skill_path()])
    .assert()
    .success()
    .stdout(predicate::str::contains("mcp_local_demo_echo"))
    .stdout(predicate::str::contains("mcp_local_demo_status"))
    .stdout(predicate::str::contains("text (string): Text to echo."));
}

#[test]
fn skill_run_can_call_mcp_tool_with_mock_llm() {
  let home = TempDir::new().unwrap();
  write_mock_models_config(home.path());
  let skill = TempDir::new().unwrap();
  write_mock_mcp_skill(skill.path());

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", mock_react_responses())
    .args([
      "skill",
      "run",
      skill.path().to_str().unwrap(),
      "--message",
      "echo through MCP",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains(
      "Agent: MCP said: mcp-basic: from run",
    ));
}

#[test]
fn skill_chat_can_call_mcp_tool_with_mock_llm() {
  let home = TempDir::new().unwrap();
  write_mock_models_config(home.path());
  let skill = TempDir::new().unwrap();
  write_mock_mcp_skill(skill.path());

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", mock_react_responses())
    .args(["skill", "chat", skill.path().to_str().unwrap()])
    .write_stdin("echo through MCP\n/exit\n")
    .assert()
    .success()
    .stdout(predicate::str::contains("MCP said: mcp-basic: from run"));
}

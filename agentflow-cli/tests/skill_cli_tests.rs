use assert_cmd::Command;
use predicates::prelude::*;

fn mcp_basic_skill_path() -> String {
  format!(
    "{}/../agentflow-skills/examples/skills/mcp-basic",
    env!("CARGO_MANIFEST_DIR")
  )
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

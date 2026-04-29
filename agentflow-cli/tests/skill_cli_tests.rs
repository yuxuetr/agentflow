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

fn write_mock_mcp_skill_with_model(dir: &Path, model: &str) {
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
name = {model:?}
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

fn write_broken_mcp_skill(dir: &Path) {
  fs::write(
    dir.join("skill.toml"),
    r#"
[skill]
name = "broken_mcp"
version = "0.1.0"
description = "Broken MCP skill for CLI error tests"

[persona]
role = "Use MCP tools when needed."

[security]
mcp_command_allowlist = ["agentflow-no-such-mcp-server-command"]

[[mcp_servers]]
name = "broken-demo"
command = "agentflow-no-such-mcp-server-command"
"#,
  )
  .unwrap();
}

fn write_skill_registry_index(root: &Path) {
  let skill_dir = root.join("skills").join("sample-skill");
  fs::create_dir_all(&skill_dir).unwrap();
  fs::write(
    skill_dir.join("skill.toml"),
    r#"
[skill]
name = "sample-skill"
version = "1.2.3"
description = "Sample skill for registry tests"

[persona]
role = "Use the sample skill."
"#,
  )
  .unwrap();

  fs::write(
    root.join("skills.index.toml"),
    r#"
schema_version = 1
name = "org-shared"
description = "Shared skills for the organization"

[[skills]]
name = "sample-skill"
version = "1.2.3"
path = "skills/sample-skill"
manifest = "skill.toml"
aliases = ["sample"]
channel = "stable"
"#,
  )
  .unwrap();
}

fn write_skill_marketplace(root: &Path) {
  fs::write(
    root.join("marketplace.toml"),
    r#"
schema_version = 1
name = "org-marketplace"
description = "Shared marketplace for registry tests"

[[indexes]]
name = "org"
kind = "organization"
source = "skills.index.toml"
description = "Organization index"

[[featured]]
skill = "sample-skill"
index = "org"
reason = "Used by tests"
"#,
  )
  .unwrap();
}

fn mock_react_responses() -> String {
  serde_json::to_string(&vec![
    r#"{"thought":"call echo","action":{"tool":"mcp_local_demo_echo","params":{"text":"from run","api_key":"should-not-print"}}}"#,
    r#"{"thought":"observed echo","answer":"MCP said: mcp-basic: from run token=answer-secret"}"#,
  ])
  .unwrap()
}

#[test]
fn skill_init_creates_valid_skill_scaffold() {
  let skill = TempDir::new().unwrap();
  let skill_dir = skill.path().join("my-generated-skill");

  let mut init = Command::cargo_bin("agentflow").unwrap();
  init
    .args([
      "skill",
      "init",
      skill_dir.to_str().unwrap(),
      "--description",
      "Generated skill for CLI tests",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Created skill scaffold"))
    .stdout(predicate::str::contains("SKILL.md"))
    .stdout(predicate::str::contains("tests/smoke.sh"));

  assert!(skill_dir.join("SKILL.md").is_file());
  assert!(skill_dir.join("README.md").is_file());
  assert!(skill_dir.join("scripts").join("hello.py").is_file());
  assert!(skill_dir.join("references").join("example.md").is_file());
  assert!(skill_dir.join("tests").join("smoke.sh").is_file());

  let skill_md = fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
  assert!(skill_md.contains("name: my-generated-skill"));
  assert!(skill_md.contains("allowed-tools: script"));

  let mut validate = Command::cargo_bin("agentflow").unwrap();
  validate
    .args(["skill", "validate", skill_dir.to_str().unwrap()])
    .assert()
    .success()
    .stdout(predicate::str::contains("Skill is valid"));

  let mut list_tools = Command::cargo_bin("agentflow").unwrap();
  list_tools
    .args(["skill", "list-tools", skill_dir.to_str().unwrap()])
    .assert()
    .success()
    .stdout(predicate::str::contains("script"))
    .stdout(predicate::str::contains("source: script"));

  let mut test = Command::cargo_bin("agentflow").unwrap();
  test
    .args(["skill", "test", skill_dir.to_str().unwrap()])
    .assert()
    .success()
    .stdout(predicate::str::contains("manifest: valid"))
    .stdout(predicate::str::contains("tools: discovered"))
    .stdout(predicate::str::contains(
      "script hello.py: hello from skill-test",
    ))
    .stdout(predicate::str::contains("Skill test passed"));
}

#[test]
fn skill_init_refuses_non_empty_directory_without_force() {
  let skill = TempDir::new().unwrap();
  fs::write(skill.path().join("existing.txt"), "keep").unwrap();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["skill", "init", skill.path().to_str().unwrap()])
    .assert()
    .failure()
    .stderr(predicate::str::contains("already exists and is not empty"));
}

#[test]
fn skill_index_validate_list_and_resolve_work_for_shared_skills() {
  let root = TempDir::new().unwrap();
  write_skill_registry_index(root.path());
  let index = root.path().join("skills.index.toml");

  let mut validate = Command::cargo_bin("agentflow").unwrap();
  validate
    .args(["skill", "index", "validate", index.to_str().unwrap()])
    .assert()
    .success()
    .stdout(predicate::str::contains("Registry index: org-shared"))
    .stdout(predicate::str::contains("Skill registry index is valid"));

  let mut list = Command::cargo_bin("agentflow").unwrap();
  list
    .args(["skill", "index", "list", index.to_str().unwrap()])
    .assert()
    .success()
    .stdout(predicate::str::contains("sample-skill @ 1.2.3"))
    .stdout(predicate::str::contains("lock: version only"))
    .stdout(predicate::str::contains("aliases: sample"))
    .stdout(predicate::str::contains("channel: stable"));

  let mut resolve = Command::cargo_bin("agentflow").unwrap();
  resolve
    .args([
      "skill",
      "index",
      "resolve",
      index.to_str().unwrap(),
      "sample",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("sample-skill"))
    .stdout(predicate::str::contains("version: 1.2.3"))
    .stdout(predicate::str::contains("skills/sample-skill"))
    .stdout(predicate::str::contains("manifest:"));
}

#[test]
fn skill_install_copies_local_registry_skill_and_respects_force() {
  let index = format!(
    "{}/../agentflow-skills/examples/skills.index.toml",
    env!("CARGO_MANIFEST_DIR")
  );
  let install_root = TempDir::new().unwrap();
  let installed_skill = install_root.path().join("mcp-basic");

  let mut install = Command::cargo_bin("agentflow").unwrap();
  install
    .args([
      "skill",
      "install",
      &index,
      "mcp-demo",
      "--dir",
      install_root.path().to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains(
      "Installed skill: mcp-basic @ 1.0.0",
    ))
    .stdout(predicate::str::contains(
      "Validate with: agentflow skill validate",
    ));

  assert!(installed_skill.join("SKILL.md").is_file());
  assert!(installed_skill.join("README.md").is_file());
  assert!(installed_skill.join("server.py").is_file());

  let mut validate = Command::cargo_bin("agentflow").unwrap();
  validate
    .args(["skill", "validate", installed_skill.to_str().unwrap()])
    .assert()
    .success()
    .stdout(predicate::str::contains("Skill is valid"));

  let mut duplicate = Command::cargo_bin("agentflow").unwrap();
  duplicate
    .args([
      "skill",
      "install",
      &index,
      "mcp-demo",
      "--dir",
      install_root.path().to_str().unwrap(),
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("already exists"))
    .stderr(predicate::str::contains("mcp-demo"))
    .stderr(predicate::str::contains("skills.index.toml"));

  let mut forced = Command::cargo_bin("agentflow").unwrap();
  forced
    .args([
      "skill",
      "install",
      &index,
      "mcp-demo",
      "--dir",
      install_root.path().to_str().unwrap(),
      "--force",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains(
      "Installed skill: mcp-basic @ 1.0.0",
    ));
}

#[test]
fn skill_marketplace_lists_and_resolves_install_command() {
  let root = TempDir::new().unwrap();
  write_skill_registry_index(root.path());
  write_skill_marketplace(root.path());
  let marketplace = root.path().join("marketplace.toml");

  let mut validate = Command::cargo_bin("agentflow").unwrap();
  validate
    .args([
      "skill",
      "marketplace",
      "validate",
      marketplace.to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Skill marketplace is valid"));

  let mut list = Command::cargo_bin("agentflow").unwrap();
  list
    .args([
      "skill",
      "marketplace",
      "list",
      marketplace.to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("sample-skill @ 1.2.3"))
    .stdout(predicate::str::contains("agentflow skill install"))
    .stdout(predicate::str::contains("skills.index.toml"));

  let mut resolve = Command::cargo_bin("agentflow").unwrap();
  resolve
    .args([
      "skill",
      "marketplace",
      "resolve",
      marketplace.to_str().unwrap(),
      "sample",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("sample-skill"))
    .stdout(predicate::str::contains("install: agentflow skill install"));
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
fn skill_test_runs_validation_and_tool_discovery_for_mcp_skill() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["skill", "test", &mcp_basic_skill_path()])
    .assert()
    .success()
    .stdout(predicate::str::contains("manifest: valid"))
    .stdout(predicate::str::contains("tools: discovered 2"))
    .stdout(predicate::str::contains("mcp_local_demo_echo"))
    .stdout(predicate::str::contains("regressions: 0 passed"))
    .stdout(predicate::str::contains("Skill test passed"));
}

#[test]
fn skill_test_dry_run_skips_regressions_and_smoke() {
  let skill = TempDir::new().unwrap();
  let skill_dir = skill.path().join("dry-run-skill");

  let mut init = Command::cargo_bin("agentflow").unwrap();
  init
    .args(["skill", "init", skill_dir.to_str().unwrap()])
    .assert()
    .success();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["skill", "test", skill_dir.to_str().unwrap(), "--dry-run"])
    .assert()
    .success()
    .stdout(predicate::str::contains("manifest: valid"))
    .stdout(predicate::str::contains("tools: discovered"))
    .stdout(predicate::str::contains(
      "dry-run: skipped regressions and smoke tests",
    ))
    .stdout(predicate::str::contains("script hello.py").not());
}

#[test]
fn skill_inspect_summarizes_manifest_without_running_agent() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["skill", "inspect", &mcp_basic_skill_path()])
    .assert()
    .success()
    .stdout(predicate::str::contains("Skill: mcp-basic"))
    .stdout(predicate::str::contains("Model:"))
    .stdout(predicate::str::contains("MCP Servers:"))
    .stdout(predicate::str::contains("local-demo"))
    .stdout(predicate::str::contains("Security:"))
    .stdout(predicate::str::contains("Status: valid"));
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
fn skill_validate_mcp_failure_names_server_and_reason() {
  let skill = TempDir::new().unwrap();
  write_broken_mcp_skill(skill.path());

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["skill", "validate", skill.path().to_str().unwrap()])
    .assert()
    .failure()
    .stderr(predicate::str::contains("MCP server validation failed"))
    .stderr(predicate::str::contains("server 'broken-demo'"))
    .stderr(predicate::str::contains(
      "agentflow-no-such-mcp-server-command",
    ));
}

#[test]
fn skill_list_tools_mcp_failure_names_server_and_tool_naming_rule() {
  let skill = TempDir::new().unwrap();
  write_broken_mcp_skill(skill.path());

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["skill", "list-tools", skill.path().to_str().unwrap()])
    .assert()
    .failure()
    .stderr(predicate::str::contains(
      "Failed to build skill tool registry",
    ))
    .stderr(predicate::str::contains("server 'broken-demo'"))
    .stderr(predicate::str::contains("mcp_<server>_<tool>"));
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
      "--trace",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains(
      "Agent: MCP said: mcp-basic: from run token=[REDACTED]",
    ))
    .stdout(predicate::str::contains("Runtime Trace"))
    .stdout(predicate::str::contains("\"type\": \"tool_call\""))
    .stdout(predicate::str::contains(
      "\"tool\": \"mcp_local_demo_echo\"",
    ))
    .stdout(predicate::str::contains("[REDACTED]"))
    .stdout(predicate::str::contains("should-not-print").not())
    .stdout(predicate::str::contains("answer-secret").not());
}

#[test]
fn skill_run_model_override_replaces_manifest_model() {
  let home = TempDir::new().unwrap();
  write_mock_models_config(home.path());
  let skill = TempDir::new().unwrap();
  write_mock_mcp_skill_with_model(skill.path(), "not-configured-model");

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
      "--model",
      "mock-model",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Model: mock-model"))
    .stdout(predicate::str::contains("Agent: MCP said:"));
}

#[test]
fn skill_run_memory_override_replaces_manifest_memory() {
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
      "--memory",
      "none",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Memory: none"))
    .stdout(predicate::str::contains("Agent: MCP said:"));
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
    .stdout(predicate::str::contains(
      "MCP said: mcp-basic: from run token=[REDACTED]",
    ))
    .stdout(predicate::str::contains("answer-secret").not());
}

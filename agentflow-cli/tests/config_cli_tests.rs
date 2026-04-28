use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn write_config(home: &TempDir) {
  let config_dir = home.path().join(".agentflow");
  fs::create_dir_all(&config_dir).unwrap();
  fs::write(
    config_dir.join("models.yml"),
    r#"
models:
  test-model:
    vendor: openai
    type: text
    model_id: test-model
providers:
  openai:
    api_key_env: OPENAI_API_KEY
    api_key: should-not-print
    base_url: https://api.openai.example/v1
defaults:
  timeout_seconds: 30
"#,
  )
  .unwrap();
}

#[test]
fn config_show_redacts_secret_values_but_keeps_env_names() {
  let home = TempDir::new().unwrap();
  write_config(&home);

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["config", "show", "providers"])
    .env("HOME", home.path())
    .assert()
    .success()
    .stdout(predicate::str::contains("api_key_env: OPENAI_API_KEY"))
    .stdout(predicate::str::contains("api_key: '[REDACTED]'"))
    .stdout(predicate::str::contains("should-not-print").not());
}

#[test]
fn config_validate_reports_missing_env_names_without_values() {
  let home = TempDir::new().unwrap();
  write_config(&home);

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["config", "validate"])
    .env("HOME", home.path())
    .env_remove("OPENAI_API_KEY")
    .assert()
    .success()
    .stdout(predicate::str::contains(
      "Status: valid with missing secrets",
    ))
    .stdout(predicate::str::contains("OPENAI_API_KEY"))
    .stdout(predicate::str::contains("should-not-print").not());
}

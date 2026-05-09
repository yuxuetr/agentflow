use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn write_config(home: &TempDir) {
  write_config_file(home, "models.yml", "test-model");
}

fn write_config_file(home: &TempDir, file_name: &str, model_name: &str) {
  let config_dir = home.path().join(".agentflow");
  fs::create_dir_all(&config_dir).unwrap();
  fs::write(
    config_dir.join(file_name),
    format!(
      r#"
models:
  {model_name}:
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
"#
    ),
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
    .stdout(predicate::str::contains("source: UserModelsYml"))
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
      "Configuration source: UserModelsYml",
    ))
    .stdout(predicate::str::contains(
      "Status: valid with missing secrets",
    ))
    .stdout(predicate::str::contains("OPENAI_API_KEY"))
    .stdout(predicate::str::contains("should-not-print").not());
}

#[test]
fn config_commands_support_legacy_models_yaml() {
  let home = TempDir::new().unwrap();
  write_config_file(&home, "models.yaml", "legacy-model");

  let mut show = Command::cargo_bin("agentflow").unwrap();
  show
    .args(["config", "show", "models"])
    .env("HOME", home.path())
    .assert()
    .success()
    .stdout(predicate::str::contains("source: UserModelsYaml"))
    .stdout(predicate::str::contains("legacy-model"));

  let mut validate = Command::cargo_bin("agentflow").unwrap();
  validate
    .args(["config", "validate"])
    .env("HOME", home.path())
    .env_remove("OPENAI_API_KEY")
    .assert()
    .success()
    .stdout(predicate::str::contains(
      "Configuration source: UserModelsYaml",
    ));
}

#[test]
fn config_commands_prefer_models_yml_over_models_yaml() {
  let home = TempDir::new().unwrap();
  write_config_file(&home, "models.yml", "preferred-model");
  write_config_file(&home, "models.yaml", "legacy-model");

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["config", "show", "models"])
    .env("HOME", home.path())
    .assert()
    .success()
    .stderr(predicate::str::contains("Both"))
    .stdout(predicate::str::contains("source: UserModelsYml"))
    .stdout(predicate::str::contains("preferred-model"))
    .stdout(predicate::str::contains("legacy-model").not());
}

#[test]
fn config_commands_use_env_override_path() {
  let home = TempDir::new().unwrap();
  write_config_file(&home, "models.yml", "home-model");
  let override_path = home.path().join("override-models.yml");
  fs::write(
    &override_path,
    r#"
models:
  override-model:
    vendor: mock
    type: text
"#,
  )
  .unwrap();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["config", "show", "models"])
    .env("HOME", home.path())
    .env("AGENTFLOW_MODELS_CONFIG", &override_path)
    .assert()
    .success()
    .stdout(predicate::str::contains("source: EnvOverride"))
    .stdout(predicate::str::contains("override-model"))
    .stdout(predicate::str::contains("home-model").not());
}

#[test]
fn config_validate_counts_env_file_keys_without_printing_values() {
  let home = TempDir::new().unwrap();
  write_config(&home);
  fs::write(
    home.path().join(".agentflow").join(".env"),
    "OPENAI_API_KEY=super-secret-value\n",
  )
  .unwrap();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["config", "validate"])
    .env("HOME", home.path())
    .env_remove("OPENAI_API_KEY")
    .assert()
    .success()
    .stdout(predicate::str::contains("Status: valid"))
    .stdout(predicate::str::contains("super-secret-value").not());
}

#[test]
fn llm_models_reads_user_model_config() {
  let home = TempDir::new().unwrap();
  write_config(&home);

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["llm", "models", "--provider", "openai", "--detailed"])
    .env("HOME", home.path())
    .assert()
    .success()
    .stdout(predicate::str::contains("test-model"))
    .stdout(predicate::str::contains("Vendor: openai"));
}

#[test]
fn llm_chat_is_retired_with_agent_first_guidance() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["llm", "chat", "--model", "mock-model"])
    .assert()
    .failure()
    .stderr(predicate::str::contains(
      "`agentflow llm chat` has been retired",
    ))
    .stderr(predicate::str::contains("agentflow skill chat"))
    .stderr(predicate::str::contains("skill_agent"));
}

#[test]
fn llm_help_hides_chat_subcommand() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["llm", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("models"))
    .stdout(predicate::str::contains("chat").not());
}

#[test]
fn top_level_help_exposes_diagnostics_and_feature_gated_commands() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .arg("--help")
    .assert()
    .success()
    .stdout(predicate::str::contains("doctor"))
    .stdout(predicate::str::contains("plugin"))
    .stdout(predicate::str::contains("rag"));
}

#[test]
fn doctor_reports_missing_config_without_failing() {
  let home = TempDir::new().unwrap();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .arg("doctor")
    .env("HOME", home.path())
    .assert()
    .success()
    .stdout(predicate::str::contains("AgentFlow doctor"))
    .stdout(predicate::str::contains("source: BuiltInDefault"));
}

#[test]
fn doctor_json_reports_enabled_feature_flags() {
  let home = TempDir::new().unwrap();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["doctor", "--format", "json"])
    .env("HOME", home.path())
    .assert()
    .success()
    .stdout(predicate::str::contains("\"features\""))
    .stdout(predicate::str::contains("\"models_config_source\""))
    .stdout(predicate::str::contains("\"rag\""))
    .stdout(predicate::str::contains("\"plugin\""));
}

#[cfg(not(feature = "rag"))]
#[test]
fn rag_command_explains_missing_feature_in_default_build() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .arg("rag")
    .assert()
    .failure()
    .stderr(predicate::str::contains("not available in this binary"))
    .stderr(predicate::str::contains("--features rag"));
}

#[cfg(not(feature = "plugin"))]
#[test]
fn plugin_command_explains_missing_feature_in_default_build() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .arg("plugin")
    .assert()
    .failure()
    .stderr(predicate::str::contains("not available in this binary"))
    .stderr(predicate::str::contains("--features plugin"));
}

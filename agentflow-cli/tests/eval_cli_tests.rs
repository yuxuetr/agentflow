//! `agentflow eval run` end-to-end CLI tests.
//!
//! Drives the bundled `agentflow-agents/eval_datasets/ci_offline/`
//! fixture against the mock LLM provider so the suite is hermetic — no
//! API key, no network, no DB. Slice 3 of P4.4.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// Write a `~/.agentflow/models.yml` registering the two mock-model ids
/// the `ci_offline` fixture uses. Mirrors the helper in
/// `skill_cli_tests.rs` but with the eval-specific ids.
fn write_eval_mock_models_config(home: &Path) {
  let config_dir = home.join(".agentflow");
  fs::create_dir_all(&config_dir).unwrap();
  fs::write(
    config_dir.join("models.yml"),
    r#"
models:
  mock-eval-hello:
    vendor: mock
    type: text
    model_id: mock-eval-hello
  mock-eval-budget:
    vendor: mock
    type: text
    model_id: mock-eval-budget
providers:
  mock:
    api_key_env: MOCK_API_KEY
"#,
  )
  .unwrap();
}

/// Two canned ReAct responses sufficient for the two-case fixture: the
/// agent plans, then emits a final answer matching the case's `contains`
/// assertion. Mock provider serves responses round-robin.
fn mock_responses() -> String {
  serde_json::to_string(&vec![
    r#"{"thought":"answer directly","answer":"hello there"}"#,
    r#"{"thought":"answer directly","answer":"done"}"#,
  ])
  .unwrap()
}

fn fixture_path() -> String {
  format!(
    "{}/../agentflow-agents/eval_datasets/ci_offline",
    env!("CARGO_MANIFEST_DIR")
  )
}

#[test]
fn cli_eval_run_text_summary_emits_passed_count() {
  let home = TempDir::new().unwrap();
  write_eval_mock_models_config(home.path());

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["eval", "run", &fixture_path()])
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", mock_responses())
    .assert()
    .success()
    .stdout(predicate::str::contains("Dataset: ci-offline"))
    .stdout(predicate::str::contains("2 total, 2 passed, 0 failed"));
}

#[test]
fn cli_eval_run_json_envelope_has_expected_top_level_keys() {
  let home = TempDir::new().unwrap();
  write_eval_mock_models_config(home.path());

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  let output = cmd
    .args(["eval", "run", &fixture_path(), "--format", "json"])
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", mock_responses())
    .output()
    .unwrap();
  assert!(
    output.status.success(),
    "stderr: {}",
    String::from_utf8_lossy(&output.stderr)
  );
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(report["schema_version"], 1);
  assert_eq!(report["dataset"], "ci-offline");
  assert_eq!(report["dataset_version"], "0.1.0");
  assert_eq!(report["summary"]["total"], 2);
  assert_eq!(report["summary"]["passed"], 2);
  assert_eq!(report["summary"]["failed"], 0);
  let cases = report["cases"].as_array().unwrap();
  assert_eq!(cases.len(), 2);
  // Each case carries a trace_id so operators can hand it to `agentflow
  // trace replay`.
  for case in cases {
    assert!(case["trace_id"].is_string());
    assert_eq!(case["status"], "passed");
  }
}

#[test]
fn cli_eval_run_filter_skips_non_matching_cases() {
  let home = TempDir::new().unwrap();
  write_eval_mock_models_config(home.path());

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  let output = cmd
    .args([
      "eval",
      "run",
      &fixture_path(),
      "--format",
      "json",
      "--filter",
      "hello-*",
    ])
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", mock_responses())
    .output()
    .unwrap();
  assert!(output.status.success());
  let report: Value = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(report["summary"]["total"], 2);
  assert_eq!(report["summary"]["passed"], 1);
  assert_eq!(report["summary"]["skipped"], 1);
  let cases = report["cases"].as_array().unwrap();
  let by_id: std::collections::HashMap<&str, &Value> = cases
    .iter()
    .map(|c| (c["id"].as_str().unwrap(), c))
    .collect();
  assert_eq!(by_id["hello-world"]["status"], "passed");
  assert_eq!(by_id["step-budget"]["status"], "skipped");
}

#[test]
fn cli_eval_run_exits_nonzero_when_case_fails() {
  let home = TempDir::new().unwrap();
  write_eval_mock_models_config(home.path());

  // Force a failure by feeding mock responses whose `answer` doesn't
  // contain the needle the case expects.
  let bad_responses = serde_json::to_string(&vec![
    r#"{"thought":"oops","answer":"totally unrelated"}"#,
    r#"{"thought":"oops","answer":"also unrelated"}"#,
  ])
  .unwrap();
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  let output = cmd
    .args(["eval", "run", &fixture_path()])
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", bad_responses)
    .output()
    .unwrap();
  assert_eq!(output.status.code(), Some(1));
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(stdout.contains("failed"), "stdout: {stdout}");
}

#[test]
fn cli_eval_run_fail_on_status_never_returns_zero_even_on_failure() {
  let home = TempDir::new().unwrap();
  write_eval_mock_models_config(home.path());

  let bad_responses = serde_json::to_string(&vec![
    r#"{"thought":"oops","answer":"totally unrelated"}"#,
    r#"{"thought":"oops","answer":"also unrelated"}"#,
  ])
  .unwrap();
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["eval", "run", &fixture_path(), "--fail-on-status", "never"])
    .env("HOME", home.path())
    .env("AGENTFLOW_MOCK_RESPONSES", bad_responses)
    .assert()
    .success();
}

#[test]
fn cli_eval_run_help_lists_format_filter_fail_on_status_flags() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args(["eval", "run", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("--format"))
    .stdout(predicate::str::contains("--filter"))
    .stdout(predicate::str::contains("--fail-on-status"));
}

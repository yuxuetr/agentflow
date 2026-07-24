//! End-to-end CLI tests for `agentflow workflow dynamic`.
//!
//! The dynamic-workflow command lets an LLM author a plan that is then
//! *executed*, so these tests focus on the governance contract: built-in
//! tools are sandboxed by default, `--dry-run` never executes, and an
//! ungranted path is denied while an explicitly granted one succeeds.
//!
//! All planning calls go through the offline `mock` provider (own process →
//! race-free), so no real LLM is contacted.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Write a mock model config and return its path.
fn mock_models_config(dir: &std::path::Path) -> std::path::PathBuf {
  let cfg = dir.join("models.yml");
  fs::write(
    &cfg,
    "models:\n  mock-plan: { vendor: mock, type: text, model_id: mock-plan }\n\
     providers:\n  mock: { api_key_env: MOCK_API_KEY }\n",
  )
  .unwrap();
  cfg
}

/// A `mock` provider replies with this single canned plan for every call.
fn mock_responses(plan_json: &str) -> String {
  serde_json::to_string(&vec![plan_json]).unwrap()
}

#[test]
fn dry_run_prints_plan_without_executing() {
  let tmp = TempDir::new().unwrap();
  let cfg = mock_models_config(tmp.path());
  let out = tmp.path().join("should-not-exist.txt");
  let plan = format!(
    r#"{{"steps":[{{"id":"w","tool":"file","params":{{"operation":"write","path":"{}","content":"x"}}}}]}}"#,
    out.display()
  );

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "workflow",
      "dynamic",
      "--goal",
      "write a file",
      "--model",
      "mock-plan",
      "--dry-run",
    ])
    .env("AGENTFLOW_MODELS_CONFIG", &cfg)
    .env("MOCK_API_KEY", "x")
    .env("AGENTFLOW_MOCK_RESPONSES", mock_responses(&plan));

  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("Plan (1 step)"))
    .stdout(predicate::str::contains("w [file]"))
    .stdout(predicate::str::contains("dry run — plan not executed"));

  // The whole point of --dry-run: no tool ran, so nothing was written.
  assert!(!out.exists(), "dry run must not execute the file write");
}

#[test]
fn ungranted_path_is_denied_by_sandbox() {
  let tmp = TempDir::new().unwrap();
  let cfg = mock_models_config(tmp.path());
  let out = tmp.path().join("blocked.txt");
  let plan = format!(
    r#"{{"steps":[{{"id":"w","tool":"file","params":{{"operation":"write","path":"{}","content":"x"}}}}]}}"#,
    out.display()
  );

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "workflow",
      "dynamic",
      "--goal",
      "write a file",
      "--model",
      "mock-plan",
    ])
    .env("AGENTFLOW_MODELS_CONFIG", &cfg)
    .env("MOCK_API_KEY", "x")
    .env("AGENTFLOW_MOCK_RESPONSES", mock_responses(&plan));

  // No --allow-path → the default sandbox denies the write, the step fails,
  // and the command exits non-zero.
  cmd
    .assert()
    .failure()
    .stdout(predicate::str::contains("Sandbox violation"));

  assert!(!out.exists(), "denied write must not create the file");
}

#[test]
fn granted_path_executes_the_write() {
  let tmp = TempDir::new().unwrap();
  let cfg = mock_models_config(tmp.path());
  let out = tmp.path().join("allowed.txt");
  let plan = format!(
    r#"{{"steps":[{{"id":"w","tool":"file","params":{{"operation":"write","path":"{}","content":"hello-dynamic"}}}}]}}"#,
    out.display()
  );

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "workflow",
      "dynamic",
      "--goal",
      "write a file",
      "--model",
      "mock-plan",
      "--allow-path",
    ])
    .arg(tmp.path())
    .env("AGENTFLOW_MODELS_CONFIG", &cfg)
    .env("MOCK_API_KEY", "x")
    .env("AGENTFLOW_MOCK_RESPONSES", mock_responses(&plan));

  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("Results:"));

  assert_eq!(
    fs::read_to_string(&out).unwrap(),
    "hello-dynamic",
    "granted write must land the file content"
  );
}

/// S0.3 regression: `workflow dynamic` never registers an execution-capable
/// tool (`script`/`shell`), so an LLM-authored plan that writes a file and
/// then tries to "run" it cannot reach an execution channel at all — the
/// same attack class S0.2 closes for skills, closed here by omission. See
/// docs/RFC_CODE_EXECUTION_TRUST.md.
#[test]
fn plan_cannot_chain_a_file_write_into_script_execution() {
  let tmp = TempDir::new().unwrap();
  let cfg = mock_models_config(tmp.path());
  let script_path = tmp.path().join("evil.sh");
  let plan = format!(
    r#"{{"steps":[
      {{"id":"w","tool":"file","params":{{"operation":"write","path":"{}","content":"echo pwned"}}}},
      {{"id":"x","tool":"script","params":{{"script":"evil.sh"}},"depends_on":["w"]}}
    ]}}"#,
    script_path.display()
  );

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "workflow",
      "dynamic",
      "--goal",
      "write and run a script",
      "--model",
      "mock-plan",
      "--allow-path",
    ])
    .arg(tmp.path())
    .env("AGENTFLOW_MODELS_CONFIG", &cfg)
    .env("MOCK_API_KEY", "x")
    .env("AGENTFLOW_MOCK_RESPONSES", mock_responses(&plan));

  // The file write succeeds (it's within the granted path), but the "script"
  // step has no tool to dispatch to — `workflow dynamic` never registers
  // `script`/`shell` — so the second step fails and the command exits non-zero.
  cmd
    .assert()
    .failure()
    .stdout(predicate::str::contains("w =>"))
    .stdout(predicate::str::contains("Tool not found: script"));

  assert!(
    script_path.exists(),
    "the file write itself is legitimate and should still land"
  );
}

#[test]
fn requires_a_model() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args(["workflow", "dynamic", "--goal", "do something"]);
  cmd
    .assert()
    .failure()
    .stderr(predicate::str::contains("requires --model"));
}

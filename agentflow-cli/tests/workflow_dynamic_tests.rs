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

/// T1.3: an unset `--approve` under a non-`dev` `--profile` must wire the
/// interactive CLI approval provider by default (an LLM-authored plan is
/// adversarial by construction), rather than the pre-T1.3 behavior of
/// running every tool call unsupervised regardless of profile. Closing
/// stdin makes the approval read hit EOF immediately, which
/// `CliApprovalProvider` treats as a deny — deterministic and fast, no
/// hang, no timeout needed.
#[test]
fn local_profile_without_approve_flag_defaults_to_requiring_cli_approval() {
  let tmp = TempDir::new().unwrap();
  let cfg = mock_models_config(tmp.path());
  let out = tmp.path().join("should-be-denied.txt");
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
      "--profile",
      "local",
      "--allow-path",
    ])
    .arg(tmp.path())
    .env("AGENTFLOW_MODELS_CONFIG", &cfg)
    .env("MOCK_API_KEY", "x")
    .env("AGENTFLOW_MOCK_RESPONSES", mock_responses(&plan))
    .write_stdin("");

  // No --approve passed: the profile-aware default must still route the
  // call through the interactive CLI approval provider (proven by the
  // prompt appearing) and, since stdin is closed, the call is denied and
  // the step — and the whole command — fails.
  cmd
    .assert()
    .failure()
    .stderr(predicate::str::contains("Harness approval request"));

  assert!(
    !out.exists(),
    "a call denied by the default approval gate must not have executed"
  );
}

/// T1.3: an explicit `--approve auto-allow` must still win over the
/// profile-aware default on `production` — the safer default only
/// changes what happens when `--approve` is *omitted*, never overrides
/// an operator's explicit choice.
#[test]
fn production_profile_with_explicit_auto_allow_still_executes_unattended() {
  let tmp = TempDir::new().unwrap();
  let cfg = mock_models_config(tmp.path());
  let out = tmp.path().join("auto-allowed.txt");
  let plan = format!(
    r#"{{"steps":[{{"id":"w","tool":"file","params":{{"operation":"write","path":"{}","content":"hello-auto-allow"}}}}]}}"#,
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
      "--profile",
      "production",
      "--approve",
      "auto-allow",
      "--allow-path",
    ])
    .arg(tmp.path())
    .env("AGENTFLOW_MODELS_CONFIG", &cfg)
    .env("MOCK_API_KEY", "x")
    .env("AGENTFLOW_MOCK_RESPONSES", mock_responses(&plan));

  cmd.assert().success();

  assert_eq!(
    fs::read_to_string(&out).unwrap(),
    "hello-auto-allow",
    "an explicit --approve auto-allow must still execute without interactive input"
  );
}

/// W2.1: `--replan <n>` exposes `DynamicWorkflowAgent::run_with_replan` —
/// pre-fix `workflow dynamic` only ever called `compile_plan_to_flow` once
/// and a failed step failed the whole run. Round 1's plan targets a tool
/// that doesn't exist (an immediate, deterministic failure); the queued
/// round-2 mock response is a working plan, proving the CLI actually asks
/// the planner for a revision and executes it rather than giving up after
/// round 1.
#[test]
fn replan_recovers_from_a_failed_step_and_reports_revisions() {
  let tmp = TempDir::new().unwrap();
  let cfg = mock_models_config(tmp.path());
  let out = tmp.path().join("recovered.txt");
  let failing_plan = r#"{"steps":[{"id":"w","tool":"nonexistent_tool","params":{}}]}"#.to_string();
  let fixed_plan = format!(
    r#"{{"steps":[{{"id":"w","tool":"file","params":{{"operation":"write","path":"{}","content":"recovered"}}}}]}}"#,
    out.display()
  );
  let responses = serde_json::to_string(&vec![failing_plan, fixed_plan]).unwrap();

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
    .args(["--replan", "1"])
    .env("AGENTFLOW_MODELS_CONFIG", &cfg)
    .env("MOCK_API_KEY", "x")
    .env("AGENTFLOW_MOCK_RESPONSES", responses);

  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("Revisions: 1"));

  assert_eq!(
    fs::read_to_string(&out).unwrap(),
    "recovered",
    "the revised (round 2) plan must be the one that actually executed"
  );
}

/// W2.1: `--replan` only helps recover from a failure that happens during
/// execution — under `--dry-run` nothing ever executes, so there is no
/// failure for a revision to respond to. The command must still just show
/// the single round-0 plan (proven by asserting the file the round-0 plan
/// would have written never gets created), not silently attempt to honor
/// `--replan` some other way.
#[test]
fn dry_run_ignores_replan_and_shows_single_round_plan() {
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
    .args(["--replan", "3"])
    .env("AGENTFLOW_MODELS_CONFIG", &cfg)
    .env("MOCK_API_KEY", "x")
    .env("AGENTFLOW_MOCK_RESPONSES", mock_responses(&plan));

  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("Plan (1 step)"))
    .stdout(predicate::str::contains(
      "--replan has no effect without execution",
    ));

  assert!(!out.exists(), "dry run must not execute the file write");
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

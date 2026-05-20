//! Hermetic CLI coverage for `agentflow workflow run --server` flag
//! validation (P10.11.4).
//!
//! These tests do NOT spin up an HTTP server — the validation runs
//! before any network call. Using an obviously-unreachable URL
//! exercises the early-bail path; if the validation didn't fire,
//! the CLI would still bail but with a "connection refused" error
//! instead of the expected per-flag message. Pinning the specific
//! message is what keeps the contract crisp.

use assert_cmd::Command;

fn cli_bin() -> Command {
  Command::cargo_bin("agentflow").expect("agentflow binary built")
}

/// A tiny workflow file the CLI can read to satisfy the
/// `workflow_file` positional argument. The validation runs BEFORE
/// the file is opened (in the actual `run_via_server` path the
/// file is opened after validation succeeds), so a missing /
/// invalid file would still surface only after the flag check —
/// which keeps these tests truly hermetic.
const WORKFLOW_YAML: &str = r#"
name: P10.11.4 Validation Demo
nodes:
  - id: render
    type: template
    parameters:
      template: "hello"
"#;

fn write_workflow() -> (tempfile::TempDir, std::path::PathBuf) {
  let dir = tempfile::tempdir().expect("tempdir");
  let path = dir.path().join("wf.yml");
  std::fs::write(&path, WORKFLOW_YAML).expect("write workflow");
  (dir, path)
}

#[test]
fn cli_workflow_run_server_rejects_run_dir() {
  let (_dir, path) = write_workflow();
  let assert = cli_bin()
    .args([
      "workflow",
      "run",
      path.to_str().unwrap(),
      "--server",
      "http://127.0.0.1:1",
      "--run-dir",
      "/tmp/x",
    ])
    .assert()
    .failure();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(
    stderr.contains("--run-dir is local-only"),
    "stderr must explain the rejection: {stderr}"
  );
}

#[test]
fn cli_workflow_run_server_rejects_output() {
  let (_dir, path) = write_workflow();
  let assert = cli_bin()
    .args([
      "workflow",
      "run",
      path.to_str().unwrap(),
      "--server",
      "http://127.0.0.1:1",
      "--output",
      "result.json",
    ])
    .assert()
    .failure();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(
    stderr.contains("--output 'result.json' is local-only"),
    "stderr should quote the path: {stderr}"
  );
  // Operators must see the in-band alternatives explicitly.
  assert!(
    stderr.contains("json-envelope") || stderr.contains("workflow logs"),
    "stderr must point at alternatives: {stderr}"
  );
}

#[test]
fn cli_workflow_run_server_rejects_watch() {
  let (_dir, path) = write_workflow();
  let assert = cli_bin()
    .args([
      "workflow",
      "run",
      path.to_str().unwrap(),
      "--server",
      "http://127.0.0.1:1",
      "--watch",
    ])
    .assert()
    .failure();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(stderr.contains("--watch is local-only"), "{stderr}");
  assert!(
    stderr.contains("workflow logs") && stderr.contains("--follow"),
    "stderr should point at the streaming alternative: {stderr}"
  );
}

#[test]
fn cli_workflow_run_server_rejects_dry_run() {
  let (_dir, path) = write_workflow();
  let assert = cli_bin()
    .args([
      "workflow",
      "run",
      path.to_str().unwrap(),
      "--server",
      "http://127.0.0.1:1",
      "--dry-run",
    ])
    .assert()
    .failure();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(stderr.contains("--dry-run is local-only"), "{stderr}");
  // Operators get pointed at `workflow validate` as the proper
  // dry-run-before-submit pattern in server mode.
  assert!(stderr.contains("workflow validate"), "{stderr}");
}

#[test]
fn cli_workflow_run_server_rejects_model_with_future_api_note() {
  let (_dir, path) = write_workflow();
  let assert = cli_bin()
    .args([
      "workflow",
      "run",
      path.to_str().unwrap(),
      "--server",
      "http://127.0.0.1:1",
      "--model",
      "gpt-4o",
    ])
    .assert()
    .failure();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(
    stderr.contains("--model is not yet wired"),
    "stderr must explain the rejection: {stderr}"
  );
  // Future-API addition messages name the tracking ID so curious
  // operators can find the follow-up work.
  assert!(stderr.contains("P10.11.4"), "{stderr}");
}

#[test]
fn cli_workflow_run_server_rejects_execution_mode_when_overridden() {
  let (_dir, path) = write_workflow();
  let assert = cli_bin()
    .args([
      "workflow",
      "run",
      path.to_str().unwrap(),
      "--server",
      "http://127.0.0.1:1",
      "--execution-mode",
      "concurrent",
    ])
    .assert()
    .failure();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(
    stderr.contains("--execution-mode 'concurrent'"),
    "stderr must quote the override: {stderr}"
  );
}

#[test]
fn cli_workflow_run_server_rejects_max_concurrency_when_changed() {
  let (_dir, path) = write_workflow();
  let assert = cli_bin()
    .args([
      "workflow",
      "run",
      path.to_str().unwrap(),
      "--server",
      "http://127.0.0.1:1",
      "--max-concurrency",
      "16",
    ])
    .assert()
    .failure();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(stderr.contains("--max-concurrency 16"), "{stderr}");
}

#[test]
fn cli_workflow_run_server_rejects_input_pairs() {
  let (_dir, path) = write_workflow();
  let assert = cli_bin()
    .args([
      "workflow",
      "run",
      path.to_str().unwrap(),
      "--server",
      "http://127.0.0.1:1",
      "--input",
      "key",
      "value",
    ])
    .assert()
    .failure();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(stderr.contains("--input is not yet wired"), "{stderr}");
  assert!(stderr.contains("workflow YAML"), "{stderr}");
}

#[test]
fn cli_workflow_run_server_rejects_timeout_when_overridden() {
  let (_dir, path) = write_workflow();
  let assert = cli_bin()
    .args([
      "workflow",
      "run",
      path.to_str().unwrap(),
      "--server",
      "http://127.0.0.1:1",
      "--timeout",
      "120s",
    ])
    .assert()
    .failure();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(stderr.contains("--timeout '120s'"), "{stderr}");
}

#[test]
fn cli_workflow_run_server_rejects_max_retries_when_nonzero() {
  let (_dir, path) = write_workflow();
  let assert = cli_bin()
    .args([
      "workflow",
      "run",
      path.to_str().unwrap(),
      "--server",
      "http://127.0.0.1:1",
      "--max-retries",
      "3",
    ])
    .assert()
    .failure();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(stderr.contains("--max-retries 3"), "{stderr}");
}

/// Sanity: passing NO local-only flags must NOT trip the validator.
/// Without a real server at 127.0.0.1:1 the CLI proceeds to the
/// HTTP call and fails with a connection error — that's the
/// "validation passed" signal we're asserting on, because any
/// validation failure would surface a different (per-flag) message
/// instead.
#[test]
fn cli_workflow_run_server_baseline_proceeds_past_validation() {
  let (_dir, path) = write_workflow();
  let assert = cli_bin()
    .args([
      "workflow",
      "run",
      path.to_str().unwrap(),
      "--server",
      "http://127.0.0.1:1",
    ])
    .assert()
    .failure();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  // The validator stayed silent — so none of the per-flag messages
  // appear. Pinning their absence is what proves the baseline
  // doesn't spuriously trip.
  for forbidden in [
    "--run-dir",
    "--output",
    "--watch",
    "--dry-run",
    "--model",
    "--execution-mode",
    "--max-concurrency",
    "--input",
    "--timeout",
    "--max-retries",
  ] {
    assert!(
      !stderr.contains(&format!("{forbidden} is local-only"))
        && !stderr.contains(&format!("{forbidden} is not yet wired")),
      "validator must not trip on `{forbidden}` when it wasn't set: {stderr}"
    );
  }
}

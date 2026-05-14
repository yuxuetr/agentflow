//! End-to-end CLI tests for `agentflow workflow resume-plan`.
//!
//! Uses an on-disk checkpoint fixture that mimics a workflow run with
//! one `AgentNode` carrying an `agent_resume` contract. The fixture
//! covers the three resume decisions (replay / skip / requires_manual)
//! plus the `--force-replay` opt-in.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;
use std::fs;
use tempfile::TempDir;

fn write_checkpoint(
  dir: &std::path::Path,
  run_id: &str,
  tool_records: Vec<serde_json::Value>,
) -> std::path::PathBuf {
  // Mirrors the file layout used by CheckpointManager:
  //   <dir>/<workflow_id>/checkpoint_latest.json
  let workflow_dir = dir.join(run_id);
  fs::create_dir_all(&workflow_dir).unwrap();
  let path = workflow_dir.join("checkpoint_latest.json");
  let body = json!({
    "workflow_id": run_id,
    "last_completed_node": "agent_node",
    "state": {
      "agent_node": {
        "agent_resume": {
          "tool_calls": tool_records
        }
      }
    },
    "created_at": "2026-05-14T00:00:00Z",
    "status": "Running",
    "metadata": {}
  });
  fs::write(&path, serde_json::to_string(&body).unwrap()).unwrap();
  path
}

fn tool_record(
  call_id: &str,
  tool: &str,
  step_index: usize,
  side_effect: &str,
  replay_policy: &str,
  result_step: Option<usize>,
) -> serde_json::Value {
  let mut record = json!({
    "call_id": call_id,
    "tool": tool,
    "step_index": step_index,
    "side_effect_class": side_effect,
    "replay_policy": replay_policy,
  });
  if let Some(idx) = result_step {
    record["result_step_index"] = json!(idx);
  }
  record
}

#[test]
fn resume_plan_classifies_idempotent_call_as_replay() {
  let tmp = TempDir::new().unwrap();
  let checkpoint_dir = tmp.path();
  write_checkpoint(
    checkpoint_dir,
    "run-idem",
    vec![tool_record(
      "call-1",
      "http",
      2,
      "idempotent",
      "replay_allowed",
      None,
    )],
  );

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args([
    "workflow",
    "resume-plan",
    "run-idem",
    "--checkpoint-dir",
    checkpoint_dir.to_str().unwrap(),
    "--format",
    "json",
  ]);
  let output = cmd.output().unwrap();
  assert!(output.status.success(), "command failed: {output:?}");
  let stdout = String::from_utf8(output.stdout).unwrap();
  let plan: serde_json::Value = serde_json::from_str(&stdout).unwrap();
  assert_eq!(plan["tool_calls"][0]["decision"], "replay");
  assert_eq!(plan["tool_calls"][0]["idempotency"], "idempotent");
  assert_eq!(plan["summary"]["to_replay"], 1);
  assert_eq!(plan["summary"]["requires_manual"], 0);
  assert_eq!(plan["force_replay"], false);
  assert_eq!(plan["schema_version"], 1);
}

#[test]
fn resume_plan_marks_non_idempotent_call_requires_manual() {
  let tmp = TempDir::new().unwrap();
  write_checkpoint(
    tmp.path(),
    "run-mut",
    vec![tool_record(
      "call-1",
      "send_email",
      2,
      "mutating",
      "manual_required",
      None,
    )],
  );

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args([
    "workflow",
    "resume-plan",
    "run-mut",
    "--checkpoint-dir",
    tmp.path().to_str().unwrap(),
  ]);
  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("requires_manual"))
    .stdout(predicate::str::contains("send_email"))
    .stderr(predicate::str::contains(
      "tool call(s) require manual recovery",
    ));
}

#[test]
fn resume_plan_denies_unknown_call_without_force_replay() {
  let tmp = TempDir::new().unwrap();
  write_checkpoint(
    tmp.path(),
    "run-unknown",
    vec![tool_record(
      "call-1",
      "mystery_tool",
      2,
      "unknown",
      "manual_required",
      None,
    )],
  );

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args([
    "workflow",
    "resume-plan",
    "run-unknown",
    "--checkpoint-dir",
    tmp.path().to_str().unwrap(),
    "--format",
    "json",
  ]);
  let output = cmd.output().unwrap();
  assert!(output.status.success());
  let plan: serde_json::Value =
    serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
  assert_eq!(plan["tool_calls"][0]["idempotency"], "unknown");
  assert_eq!(plan["tool_calls"][0]["decision"], "requires_manual");
  assert!(
    plan["tool_calls"][0]["reason"]
      .as_str()
      .unwrap()
      .contains("--force-replay")
  );
}

#[test]
fn resume_plan_allows_unknown_call_with_force_replay() {
  let tmp = TempDir::new().unwrap();
  write_checkpoint(
    tmp.path(),
    "run-unknown-forced",
    vec![tool_record(
      "call-1",
      "mystery_tool",
      2,
      "unknown",
      "manual_required",
      None,
    )],
  );

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args([
    "workflow",
    "resume-plan",
    "run-unknown-forced",
    "--checkpoint-dir",
    tmp.path().to_str().unwrap(),
    "--force-replay",
    "--format",
    "json",
  ]);
  let output = cmd.output().unwrap();
  assert!(output.status.success());
  let plan: serde_json::Value =
    serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
  assert_eq!(plan["tool_calls"][0]["decision"], "replay");
  assert_eq!(plan["force_replay"], true);
  assert_eq!(plan["summary"]["to_replay"], 1);
}

#[test]
fn resume_plan_uses_recorded_result_for_skip_decision() {
  let tmp = TempDir::new().unwrap();
  write_checkpoint(
    tmp.path(),
    "run-skip",
    vec![tool_record(
      "call-1",
      "search",
      2,
      "idempotent",
      "reuse_recorded_result",
      Some(3),
    )],
  );

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args([
    "workflow",
    "resume-plan",
    "run-skip",
    "--checkpoint-dir",
    tmp.path().to_str().unwrap(),
    "--format",
    "json",
  ]);
  let output = cmd.output().unwrap();
  let plan: serde_json::Value =
    serde_json::from_str(&String::from_utf8(output.stdout).unwrap()).unwrap();
  assert_eq!(plan["tool_calls"][0]["decision"], "skip");
  assert_eq!(plan["tool_calls"][0]["has_recorded_result"], true);
}

#[test]
fn resume_plan_text_format_renders_table_and_summary() {
  let tmp = TempDir::new().unwrap();
  write_checkpoint(
    tmp.path(),
    "run-text",
    vec![
      tool_record("call-1", "http", 1, "idempotent", "replay_allowed", None),
      tool_record(
        "call-2",
        "send_email",
        2,
        "mutating",
        "manual_required",
        None,
      ),
    ],
  );

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args([
    "workflow",
    "resume-plan",
    "run-text",
    "--checkpoint-dir",
    tmp.path().to_str().unwrap(),
  ]);
  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("Resume plan for run: run-text"))
    .stdout(predicate::str::contains(
      "total=2 replay=1 skip=0 requires_manual=1",
    ))
    .stdout(predicate::str::contains("replay"))
    .stdout(predicate::str::contains("requires_manual"));
}

#[test]
fn resume_plan_fails_clearly_when_checkpoint_missing() {
  let tmp = TempDir::new().unwrap();
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args([
    "workflow",
    "resume-plan",
    "ghost-run",
    "--checkpoint-dir",
    tmp.path().to_str().unwrap(),
  ]);
  cmd
    .assert()
    .failure()
    .stderr(predicate::str::contains("no checkpoint found"));
}

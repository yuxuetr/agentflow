//! End-to-end CLI tests for `agentflow harness …`.
//!
//! `run` requires a working LLM provider, so the live invocation path is
//! exercised in `agentflow-harness/tests/runtime_react_smoke.rs`. Here
//! we cover the persistence-side subcommands that operate on a JSONL
//! session log without ever calling out to an LLM: `list`, `inspect`,
//! `resume`, plus argument validation on `run`.

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;
use std::fs;
use tempfile::TempDir;

const RUNTIME_VERSION: &str = "harness/1";

fn write_session(run_dir: &std::path::Path, session_id: &str) -> std::path::PathBuf {
  let dir = run_dir.join("harness").join("sessions");
  fs::create_dir_all(&dir).unwrap();
  let path = dir.join(format!("{session_id}.jsonl"));
  let started = json!({
    "seq": 0,
    "session_id": session_id,
    "ts": "2026-05-14T12:00:00Z",
    "kind": "session_started",
    "payload": {
      "workspace_root": "/tmp/ws",
      "runtime": "react",
      "profile": "local",
      "model": "mock-model",
      "skills": [],
      "context_item_count": 0,
      "context_token_estimate": 0
    }
  });
  let stopped = json!({
    "seq": 1,
    "session_id": session_id,
    "ts": "2026-05-14T12:00:05Z",
    "kind": "stopped",
    "payload": {
      "reason": "completed",
      "final_answer": "all done"
    }
  });
  fs::write(
    &path,
    format!(
      "{}\n{}\n",
      serde_json::to_string(&started).unwrap(),
      serde_json::to_string(&stopped).unwrap()
    ),
  )
  .unwrap();
  path
}

#[test]
fn harness_run_requires_model_or_skill() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args(["harness", "run", "hello"]);
  cmd.assert().failure().stderr(predicate::str::contains(
    "either --skill or --model is required",
  ));
}

#[test]
fn harness_chat_requires_model_or_skill() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args(["harness", "chat"]);
  cmd.write_stdin("exit\n");
  cmd.assert().failure().stderr(predicate::str::contains(
    "either --skill or --model is required",
  ));
}

/// End-to-end: the chat REPL reads multiple lines from stdin and runs one
/// Harness turn per line against a single session — proving interactive
/// multi-turn. Uses the offline mock provider (own process → race-free).
#[test]
fn harness_chat_repl_runs_multi_turn_with_mock() {
  let tmp = tempfile::tempdir().unwrap();
  let cfg = tmp.path().join("models.yml");
  std::fs::write(
    &cfg,
    "models:\n  mock-chat: { vendor: mock, type: text, model_id: mock-chat }\n\
     providers:\n  mock: { api_key_env: MOCK_API_KEY }\n",
  )
  .unwrap();
  let run_dir = tmp.path().join("runs");

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args([
    "harness",
    "chat",
    "--model",
    "mock-chat",
    "--approve",
    "none",
    "--no-default-context",
    "--run-dir",
  ])
  .arg(&run_dir)
  .env("AGENTFLOW_MODELS_CONFIG", &cfg)
  .env("MOCK_API_KEY", "x")
  .env(
    "AGENTFLOW_MOCK_RESPONSES",
    r#"["{\"thought\":\"t1\",\"answer\":\"reply one\"}","{\"thought\":\"t2\",\"answer\":\"reply two\"}"]"#,
  )
  .write_stdin("first message\nsecond message\nexit\n");

  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("reply one"))
    .stdout(predicate::str::contains("reply two"));
}

#[test]
fn harness_run_rejects_unknown_approve_mode() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args([
    "harness",
    "run",
    "hello",
    "--model",
    "mock-model",
    "--approve",
    "bogus",
  ]);
  // clap value_parser rejects with a non-zero exit and a "possible values" hint.
  cmd
    .assert()
    .failure()
    .stderr(predicate::str::contains("possible values"))
    .stderr(predicate::str::contains("auto-allow"));
}

#[test]
fn harness_run_help_lists_approve_flag() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args(["harness", "run", "--help"]);
  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("--approve"))
    .stdout(predicate::str::contains("HookedTool"))
    .stdout(predicate::str::contains("F-A2-11"));
}

#[test]
fn harness_run_help_lists_context_engineering_flags() {
  // The Phase 0/2a context-engineering knobs must be reachable from the
  // production CLI: a real-tokenizer context budget that compacts
  // (not drops) over-budget context, and an agent prompt-memory budget.
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args(["harness", "run", "--help"]);
  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("--context-budget"))
    .stdout(predicate::str::contains("compacted"))
    .stdout(predicate::str::contains("--token-budget"))
    // §6: harness-driven turn loop with between-turn context refresh.
    .stdout(predicate::str::contains("--context-refresh"))
    .stdout(predicate::str::contains("context_refresh"));
}

#[test]
fn harness_list_text_output_lists_sessions_under_run_dir() {
  let run_dir = TempDir::new().unwrap();
  write_session(run_dir.path(), "sess-list-a");
  write_session(run_dir.path(), "sess-list-b");

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args([
    "harness",
    "list",
    "--run-dir",
    run_dir.path().to_str().unwrap(),
  ]);
  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("sess-list-a"))
    .stdout(predicate::str::contains("sess-list-b"))
    .stdout(predicate::str::contains("SESSION_ID"));
}

#[test]
fn harness_list_json_output_emits_session_array() {
  let run_dir = TempDir::new().unwrap();
  write_session(run_dir.path(), "sess-json-1");

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args([
    "harness",
    "list",
    "--run-dir",
    run_dir.path().to_str().unwrap(),
    "--output",
    "json",
  ]);
  let output = cmd.output().unwrap();
  assert!(
    output.status.success(),
    "list --output json failed: {output:?}"
  );
  let stdout = String::from_utf8(output.stdout).unwrap();
  let payload: serde_json::Value = serde_json::from_str(&stdout).unwrap();
  let sessions = payload["sessions"].as_array().unwrap();
  assert_eq!(sessions.len(), 1);
  assert_eq!(sessions[0]["session_id"], "sess-json-1");
  assert!(sessions[0]["event_count"].as_u64().unwrap() >= 2);
}

#[test]
fn harness_list_reports_empty_directory_gracefully() {
  let run_dir = TempDir::new().unwrap();
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args([
    "harness",
    "list",
    "--run-dir",
    run_dir.path().to_str().unwrap(),
  ]);
  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("no sessions found"));
}

#[test]
fn harness_inspect_text_output_summarises_session() {
  let run_dir = TempDir::new().unwrap();
  write_session(run_dir.path(), "sess-inspect-1");

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args([
    "harness",
    "inspect",
    "sess-inspect-1",
    "--run-dir",
    run_dir.path().to_str().unwrap(),
  ]);
  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("Session: sess-inspect-1"))
    .stdout(predicate::str::contains("runtime: react"))
    .stdout(predicate::str::contains("session_started: 1"))
    .stdout(predicate::str::contains("stopped: 1"))
    .stdout(predicate::str::contains("final answer: all done"));
}

#[test]
fn harness_inspect_unknown_session_fails_clearly() {
  let run_dir = TempDir::new().unwrap();
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args([
    "harness",
    "inspect",
    "ghost",
    "--run-dir",
    run_dir.path().to_str().unwrap(),
  ]);
  cmd
    .assert()
    .failure()
    .stderr(predicate::str::contains("no events found"));
}

#[test]
fn harness_resume_text_output_replays_lines() {
  let run_dir = TempDir::new().unwrap();
  write_session(run_dir.path(), "sess-resume-1");

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args([
    "harness",
    "resume",
    "sess-resume-1",
    "--run-dir",
    run_dir.path().to_str().unwrap(),
  ]);
  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("Stored events: 2"))
    .stdout(predicate::str::contains("session_started"))
    .stdout(predicate::str::contains("stopped"));
}

#[test]
fn harness_resume_stream_json_emits_per_event_lines() {
  let run_dir = TempDir::new().unwrap();
  write_session(run_dir.path(), "sess-resume-stream");

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args([
    "harness",
    "resume",
    "sess-resume-stream",
    "--run-dir",
    run_dir.path().to_str().unwrap(),
    "--output",
    "stream-json",
  ]);
  let output = cmd.output().unwrap();
  assert!(output.status.success());
  let stdout = String::from_utf8(output.stdout).unwrap();
  let lines: Vec<&str> = stdout.lines().collect();
  // Two harness events + one summary trailer.
  assert_eq!(
    lines.len(),
    3,
    "stream-json output should have 3 lines: {stdout}"
  );
  let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
  assert_eq!(first["kind"], "session_started");
  let summary: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
  assert_eq!(summary["type"], "harness_resume_summary");
}

#[test]
fn harness_command_help_is_listed() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args(["harness", "--help"]);
  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("run"))
    .stdout(predicate::str::contains("resume"))
    .stdout(predicate::str::contains("list"))
    .stdout(predicate::str::contains("inspect"));
  // Sanity: keep schema version referenced anywhere in the help/source
  // so we never accidentally rename the constant without picking it up.
  assert_eq!(RUNTIME_VERSION, "harness/1");
}

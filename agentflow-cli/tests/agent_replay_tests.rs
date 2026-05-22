//! Hermetic CLI coverage for `agentflow agent replay --diff` (P10.8.1).
//!
//! Seeds two temp JSONL files representing baseline and current
//! `agentflow_agents::AgentEvent` streams, then drives the CLI binary
//! end-to-end and asserts the exit code + a couple of marker strings
//! in the output. These complement the comparator's pure unit tests
//! (in `commands::agent::replay::tests`) — they don't re-cover the
//! comparator boundaries, just that the wiring from CLI args through
//! to stdout actually works.

use std::path::Path;

use assert_cmd::Command;

fn cli_bin() -> Command {
  Command::cargo_bin("agentflow").expect("agentflow binary built")
}

/// Write a JSONL trace to `<dir>/<name>.jsonl` from raw event values.
/// The events are arranged so the diff scenarios below are obvious in
/// the source.
fn write_jsonl(dir: &Path, name: &str, events: &[serde_json::Value]) -> std::path::PathBuf {
  let path = dir.join(format!("{name}.jsonl"));
  let body = events
    .iter()
    .map(|e| serde_json::to_string(e).expect("serialize event"))
    .collect::<Vec<_>>()
    .join("\n");
  std::fs::write(&path, body).expect("write JSONL");
  path
}

fn step_completed_observe(index: usize, ts: &str, input: &str) -> serde_json::Value {
  serde_json::json!({
    "event": "step_completed",
    "session_id": "test",
    "step": {
      "index": index,
      "kind": { "type": "observe", "input": input },
      "timestamp": ts,
      "duration_ms": null,
    }
  })
}

fn step_completed_tool_call(
  index: usize,
  ts: &str,
  tool: &str,
  params: serde_json::Value,
) -> serde_json::Value {
  serde_json::json!({
    "event": "step_completed",
    "session_id": "test",
    "step": {
      "index": index,
      "kind": { "type": "tool_call", "tool": tool, "params": params },
      "timestamp": ts,
      "duration_ms": null,
    }
  })
}

fn run_stopped(ts: &str, reason_payload: serde_json::Value) -> serde_json::Value {
  serde_json::json!({
    "event": "run_stopped",
    "session_id": "test",
    "reason": reason_payload,
    "timestamp": ts,
  })
}

#[test]
fn cli_agent_replay_identical_traces_succeeds() {
  let dir = tempfile::tempdir().expect("tempdir");
  let events = vec![
    step_completed_observe(0, "2026-05-21T10:00:00Z", "hello"),
    step_completed_tool_call(
      1,
      "2026-05-21T10:00:01Z",
      "search",
      serde_json::json!({"q": "rust"}),
    ),
    run_stopped(
      "2026-05-21T10:00:02Z",
      serde_json::json!({"reason": "final_answer"}),
    ),
  ];
  let baseline = write_jsonl(dir.path(), "baseline", &events);
  let current = write_jsonl(dir.path(), "current", &events);

  let assert = cli_bin()
    .args([
      "agent",
      "replay",
      current.to_str().unwrap(),
      "--diff",
      baseline.to_str().unwrap(),
    ])
    .assert()
    .success();
  let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
  assert!(
    stdout.contains("match: 2 step(s) agree"),
    "expected match summary; got:\n{stdout}"
  );
}

#[test]
fn cli_agent_replay_tool_name_divergence_exits_non_zero() {
  let dir = tempfile::tempdir().expect("tempdir");
  let baseline_events = vec![
    step_completed_observe(0, "2026-05-21T10:00:00Z", "go"),
    step_completed_tool_call(1, "2026-05-21T10:00:01Z", "search", serde_json::json!({})),
    run_stopped(
      "2026-05-21T10:00:02Z",
      serde_json::json!({"reason": "final_answer"}),
    ),
  ];
  let current_events = vec![
    step_completed_observe(0, "2026-05-21T10:00:00Z", "go"),
    step_completed_tool_call(1, "2026-05-21T10:00:01Z", "browse", serde_json::json!({})),
    run_stopped(
      "2026-05-21T10:00:02Z",
      serde_json::json!({"reason": "final_answer"}),
    ),
  ];
  let baseline = write_jsonl(dir.path(), "baseline", &baseline_events);
  let current = write_jsonl(dir.path(), "current", &current_events);

  let assert = cli_bin()
    .args([
      "agent",
      "replay",
      current.to_str().unwrap(),
      "--diff",
      baseline.to_str().unwrap(),
    ])
    .assert()
    .failure();
  let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
  assert!(
    stdout.contains("step 1 tool name: baseline=search, current=browse"),
    "expected tool-name divergence line; got:\n{stdout}"
  );
}

#[test]
fn cli_agent_replay_stop_reason_divergence_exits_non_zero() {
  let dir = tempfile::tempdir().expect("tempdir");
  let baseline_events = vec![run_stopped(
    "2026-05-21T10:00:02Z",
    serde_json::json!({"reason": "final_answer"}),
  )];
  let current_events = vec![run_stopped(
    "2026-05-21T10:00:02Z",
    serde_json::json!({"reason": "max_steps", "max_steps": 10}),
  )];
  let baseline = write_jsonl(dir.path(), "baseline", &baseline_events);
  let current = write_jsonl(dir.path(), "current", &current_events);

  let assert = cli_bin()
    .args([
      "agent",
      "replay",
      current.to_str().unwrap(),
      "--diff",
      baseline.to_str().unwrap(),
    ])
    .assert()
    .failure();
  let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
  assert!(
    stdout.contains("stop reason: baseline=final_answer, current=max_steps"),
    "expected stop-reason divergence line; got:\n{stdout}"
  );
}

#[test]
fn cli_agent_replay_json_envelope_format_wraps_canonical_shape() {
  let dir = tempfile::tempdir().expect("tempdir");
  let events = vec![
    step_completed_observe(0, "2026-05-21T10:00:00Z", "hi"),
    run_stopped(
      "2026-05-21T10:00:02Z",
      serde_json::json!({"reason": "final_answer"}),
    ),
  ];
  let baseline = write_jsonl(dir.path(), "baseline", &events);
  let current = write_jsonl(dir.path(), "current", &events);

  let assert = cli_bin()
    .args([
      "agent",
      "replay",
      current.to_str().unwrap(),
      "--diff",
      baseline.to_str().unwrap(),
      "--format",
      "json-envelope",
    ])
    .assert()
    .success();
  let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
  let envelope: serde_json::Value = serde_json::from_str(&stdout).expect("envelope must be JSON");
  assert_eq!(envelope["version"], "agentflow.cli/1");
  assert_eq!(envelope["command"], "agent replay --diff");
  assert!(envelope["result"]["divergences"].is_array());
  assert!(envelope["result"]["variances"].is_array());
  assert_eq!(envelope["errors"].as_array().unwrap().len(), 0);
}

#[test]
fn cli_agent_replay_strict_tokens_promotes_delta_to_divergence() {
  let dir = tempfile::tempdir().expect("tempdir");
  let baseline_events = vec![
    serde_json::json!({
      "event": "llm_call_completed",
      "session_id": "test",
      "step_index": 0,
      "model": "m",
      "prompt_tokens": 100,
      "completion_tokens": 50,
      "total_tokens": 150,
      "duration_ms": 100,
      "timestamp": "2026-05-21T10:00:00Z",
    }),
    run_stopped(
      "2026-05-21T10:00:02Z",
      serde_json::json!({"reason": "final_answer"}),
    ),
  ];
  let current_events = vec![
    serde_json::json!({
      "event": "llm_call_completed",
      "session_id": "test",
      "step_index": 0,
      "model": "m",
      "prompt_tokens": 105,
      "completion_tokens": 48,
      "total_tokens": 153,
      "duration_ms": 100,
      "timestamp": "2026-05-21T10:00:00Z",
    }),
    run_stopped(
      "2026-05-21T10:00:02Z",
      serde_json::json!({"reason": "final_answer"}),
    ),
  ];
  let baseline = write_jsonl(dir.path(), "baseline", &baseline_events);
  let current = write_jsonl(dir.path(), "current", &current_events);

  // Without --strict-tokens: 5/+(-2) token delta is a variance,
  // gate passes.
  cli_bin()
    .args([
      "agent",
      "replay",
      current.to_str().unwrap(),
      "--diff",
      baseline.to_str().unwrap(),
    ])
    .assert()
    .success();

  // With --strict-tokens: same delta is a divergence, gate fails.
  let strict = cli_bin()
    .args([
      "agent",
      "replay",
      current.to_str().unwrap(),
      "--diff",
      baseline.to_str().unwrap(),
      "--strict-tokens",
    ])
    .assert()
    .failure();
  let stdout = String::from_utf8_lossy(&strict.get_output().stdout).to_string();
  assert!(
    stdout.contains("step 0 tokens (strict)"),
    "expected strict-mode token divergence line; got:\n{stdout}"
  );
}

#[test]
fn cli_agent_replay_malformed_jsonl_errors_with_line_number() {
  let dir = tempfile::tempdir().expect("tempdir");
  let baseline = dir.path().join("baseline.jsonl");
  std::fs::write(
    &baseline,
    "{\"event\":\"run_started\",\"session_id\":\"s\",\"model\":\"m\",\"timestamp\":\"2026-05-21T10:00:00Z\"}\n",
  )
  .unwrap();
  let current = dir.path().join("current.jsonl");
  std::fs::write(&current, "not-json-at-all\n").unwrap();

  let assert = cli_bin()
    .args([
      "agent",
      "replay",
      current.to_str().unwrap(),
      "--diff",
      baseline.to_str().unwrap(),
    ])
    .assert()
    .failure();
  let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
  assert!(
    stderr.contains("line 1"),
    "expected line-number diagnostic in stderr; got:\n{stderr}"
  );
}

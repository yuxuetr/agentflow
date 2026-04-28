use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;
use std::fs;
use tempfile::TempDir;

#[test]
fn trace_replay_prints_agent_and_mcp_timeline() {
  let traces = TempDir::new().unwrap();
  fs::write(
    traces.path().join("wf-replay.json"),
    serde_json::to_string_pretty(&json!({
      "workflow_id": "wf-replay",
      "workflow_name": "Replay Test",
      "started_at": "2026-04-27T00:00:00Z",
      "completed_at": "2026-04-27T00:00:01Z",
      "status": {"type": "completed"},
      "nodes": [
        {
          "node_id": "agent",
          "node_type": "agent",
          "started_at": "2026-04-27T00:00:00Z",
          "completed_at": "2026-04-27T00:00:01Z",
          "duration_ms": 1000,
          "status": "completed",
          "agent_details": {
            "session_id": "session-1",
            "answer": "done",
            "stop_reason": {"reason": "final_answer"},
            "steps": [
              {"index": 0, "kind": {"type": "observe", "input": "hello"}},
              {"index": 1, "kind": {"type": "tool_call", "tool": "mcp_echo"}},
              {"index": 2, "kind": {"type": "tool_result", "tool": "mcp_echo", "content": "ok", "is_error": false}}
            ],
            "events": [],
            "tool_calls": [
              {
                "tool": "mcp_echo",
                "params": {"message": "hello", "api_key": "should-not-print"},
                "is_error": false,
                "duration_ms": 12,
                "is_mcp": true
              }
            ]
          }
        }
      ],
      "metadata": {
        "user_id": null,
        "session_id": null,
        "tags": [],
        "environment": "test"
      }
    }))
    .unwrap(),
  )
  .unwrap();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "trace",
      "replay",
      "wf-replay",
      "--dir",
      traces.path().to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Trace Replay: wf-replay"))
    .stdout(predicate::str::contains("Workflow: Replay Test"))
    .stdout(predicate::str::contains("step 1: tool_call mcp_echo"))
    .stdout(predicate::str::contains("tool: mcp_echo source=mcp"))
    .stdout(predicate::str::contains("[REDACTED]"))
    .stdout(predicate::str::contains("should-not-print").not());
}

#[test]
fn trace_tui_filters_mcp_timeline_with_details() {
  let traces = TempDir::new().unwrap();
  fs::write(
    traces.path().join("wf-tui.json"),
    serde_json::to_string_pretty(&json!({
      "workflow_id": "wf-tui",
      "workflow_name": "TUI Test",
      "started_at": "2026-04-27T00:00:00Z",
      "completed_at": "2026-04-27T00:00:01Z",
      "status": {"type": "completed"},
      "nodes": [
        {
          "node_id": "agent",
          "node_type": "agent",
          "started_at": "2026-04-27T00:00:00Z",
          "completed_at": "2026-04-27T00:00:01Z",
          "duration_ms": 1000,
          "status": "completed",
          "agent_details": {
            "session_id": "session-1",
            "answer": "done",
            "stop_reason": {"reason": "final_answer"},
            "steps": [
              {"index": 0, "kind": {"type": "observe", "input": "hello"}},
              {"index": 1, "kind": {"type": "tool_call", "tool": "mcp_echo"}}
            ],
            "events": [],
            "tool_calls": [
              {
                "tool": "local_echo",
                "params": {"message": "local"},
                "is_error": false,
                "duration_ms": 4,
                "is_mcp": false
              },
              {
                "tool": "mcp_echo",
                "params": {"message": "hello", "api_key": "should-not-print"},
                "is_error": false,
                "duration_ms": 12,
                "is_mcp": true
              }
            ]
          }
        }
      ],
      "metadata": {
        "user_id": null,
        "session_id": null,
        "tags": [],
        "environment": "test"
      }
    }))
    .unwrap(),
  )
  .unwrap();

  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd
    .args([
      "trace",
      "tui",
      "wf-tui",
      "--dir",
      traces.path().to_str().unwrap(),
      "--filter",
      "mcp",
      "--details",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("AgentFlow Trace TUI"))
    .stdout(predicate::str::contains("Filter: mcp"))
    .stdout(predicate::str::contains("tool mcp_echo source=mcp"))
    .stdout(predicate::str::contains("params:"))
    .stdout(predicate::str::contains("[REDACTED]"))
    .stdout(predicate::str::contains("local_echo").not())
    .stdout(predicate::str::contains("should-not-print").not());
}

//! Hermetic CLI coverage for `agentflow harness replay` (P10.10.2).
//!
//! Seeds a temp run-dir with a small synthesised JSONL session log
//! that matches the on-disk shape `agentflow_harness::JsonlEventSink`
//! writes, then drives the CLI binary with `--speed instant` so the
//! test stays fast (no sleeps) while still exercising the full
//! load → filter → render → output path.

use std::path::Path;

use assert_cmd::Command;
use serde_json::Value;

fn cli_bin() -> Command {
  Command::cargo_bin("agentflow").expect("agentflow binary built")
}

const SESSION_ID: &str = "p10-10-2-replay";

/// Synthesise a minimal session JSONL log under
/// `<run_dir>/harness/sessions/<session_id>.jsonl`. Each line is one
/// `HarnessEvent` shaped to match `agentflow_harness`'s wire format
/// (kind + payload tagged enum). The ts values are 1 second apart
/// so a hypothetical 1x replay would take ~3 s; tests use
/// `--speed instant` to short-circuit the sleeps.
fn seed_session(run_dir: &Path) {
  let sessions_dir = run_dir.join("harness").join("sessions");
  std::fs::create_dir_all(&sessions_dir).expect("mkdir -p sessions");
  let log = sessions_dir.join(format!("{SESSION_ID}.jsonl"));
  // Each event:
  //   {seq, session_id, ts, kind, payload}
  // The discriminator is `kind` + `payload`, snake_case (see
  // `HarnessEventBody` serde attributes).
  let lines = [
    serde_json::json!({
      "seq": 0,
      "session_id": SESSION_ID,
      "ts": "2026-05-20T10:00:00Z",
      "kind": "session_started",
      "payload": {
        "workspace_root": "/tmp",
        "runtime": "react",
        "profile": "local",
        "model": "gpt-4o-mini",
        "context_item_count": 3,
        "context_token_estimate": 120
      }
    }),
    serde_json::json!({
      "seq": 1,
      "session_id": SESSION_ID,
      "ts": "2026-05-20T10:00:01Z",
      "kind": "step_started",
      "payload": { "step_index": 0, "step_type": "plan" }
    }),
    serde_json::json!({
      "seq": 2,
      "session_id": SESSION_ID,
      "ts": "2026-05-20T10:00:02Z",
      "kind": "stopped",
      "payload": { "reason": "completed" }
    }),
  ];
  let body = lines
    .iter()
    .map(|v| serde_json::to_string(v).unwrap())
    .collect::<Vec<_>>()
    .join("\n");
  std::fs::write(&log, body).expect("write session log");
}

#[test]
fn cli_harness_replay_instant_speed_streams_all_events() {
  let dir = tempfile::tempdir().expect("tempdir");
  let run_dir = dir.path().to_path_buf();
  seed_session(&run_dir);

  let assert = cli_bin()
    .args([
      "harness",
      "replay",
      SESSION_ID,
      "--run-dir",
      run_dir.to_str().unwrap(),
      "--speed",
      "instant",
    ])
    .assert()
    .success();
  let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
  // text mode: header + 3 event lines.
  assert!(
    stdout.contains(&format!("Session: {SESSION_ID}")),
    "missing session header: {stdout}"
  );
  for kind in ["session_started", "step_started", "stopped"] {
    assert!(
      stdout.contains(kind),
      "missing event kind '{kind}': {stdout}"
    );
  }
  // The seq prefix should appear for every event. Pinning all
  // three catches a regression that drops some via faulty filter
  // logic.
  for seq in ["[0000]", "[0001]", "[0002]"] {
    assert!(stdout.contains(seq), "missing seq '{seq}': {stdout}");
  }
}

#[test]
fn cli_harness_replay_stream_json_emits_one_event_per_line() {
  let dir = tempfile::tempdir().expect("tempdir");
  let run_dir = dir.path().to_path_buf();
  seed_session(&run_dir);

  let assert = cli_bin()
    .args([
      "harness",
      "replay",
      SESSION_ID,
      "--run-dir",
      run_dir.to_str().unwrap(),
      "--speed",
      "instant",
      "--output",
      "stream-json",
    ])
    .assert()
    .success();
  let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
  let lines: Vec<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
  // Each line is a complete JSON event (header went to stderr).
  assert_eq!(
    lines.len(),
    3,
    "expected 3 JSONL lines on stdout, got {}: {stdout}",
    lines.len()
  );
  for line in &lines {
    let parsed: Value =
      serde_json::from_str(line).unwrap_or_else(|e| panic!("invalid JSON `{line}`: {e}"));
    assert!(parsed.get("seq").is_some());
    assert!(parsed.get("kind").is_some());
  }
  // Header on stderr proves the stream-json contract: stdout
  // stays pure JSONL for `jq -c` consumption.
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(
    stderr.contains("replaying session"),
    "header must land on stderr in stream-json mode, got: {stderr}"
  );
}

#[test]
fn cli_harness_replay_filter_kind_restricts_output() {
  let dir = tempfile::tempdir().expect("tempdir");
  let run_dir = dir.path().to_path_buf();
  seed_session(&run_dir);

  let assert = cli_bin()
    .args([
      "harness",
      "replay",
      SESSION_ID,
      "--run-dir",
      run_dir.to_str().unwrap(),
      "--speed",
      "instant",
      "--filter-kind",
      "stopped",
    ])
    .assert()
    .success();
  let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
  assert!(
    stdout.contains("stopped"),
    "filter must include match: {stdout}"
  );
  // The filter excludes the other two kinds, so neither
  // `session_started` nor `step_started` should appear in event
  // lines. The header line includes "1 of 3 events" so we can't
  // grep for "step_started" naively; check the per-event seq
  // prefixes instead — only seq 2 should appear as an event row.
  assert!(
    stdout.contains("[0002]"),
    "stopped event seq must be present: {stdout}"
  );
  assert!(
    !stdout.contains("[0000]") && !stdout.contains("[0001]"),
    "filtered events must NOT appear: {stdout}",
  );
}

#[test]
fn cli_harness_replay_from_seq_skips_earlier_events() {
  let dir = tempfile::tempdir().expect("tempdir");
  let run_dir = dir.path().to_path_buf();
  seed_session(&run_dir);

  let assert = cli_bin()
    .args([
      "harness",
      "replay",
      SESSION_ID,
      "--run-dir",
      run_dir.to_str().unwrap(),
      "--speed",
      "instant",
      "--from-seq",
      "1",
    ])
    .assert()
    .success();
  let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
  // --from-seq is inclusive. Seq 1 and 2 visible; seq 0 hidden.
  assert!(stdout.contains("[0001]"), "{stdout}");
  assert!(stdout.contains("[0002]"), "{stdout}");
  assert!(
    !stdout.contains("[0000]"),
    "seq 0 must be filtered: {stdout}"
  );
}

#[test]
fn cli_harness_replay_rejects_bare_integer_speed() {
  let dir = tempfile::tempdir().expect("tempdir");
  let run_dir = dir.path().to_path_buf();
  seed_session(&run_dir);

  let assert = cli_bin()
    .args([
      "harness",
      "replay",
      SESSION_ID,
      "--run-dir",
      run_dir.to_str().unwrap(),
      "--speed",
      "2",
    ])
    .assert()
    .failure();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(stderr.contains("must end in 'x'"), "{stderr}");
}

#[test]
fn cli_harness_replay_rejects_json_envelope_format() {
  // Replay produces an open-ended stream — envelope format is
  // for bounded payloads. Pin the rejection so a future
  // refactor that loosened the check would surface.
  let dir = tempfile::tempdir().expect("tempdir");
  let run_dir = dir.path().to_path_buf();
  seed_session(&run_dir);

  let assert = cli_bin()
    .args([
      "harness",
      "replay",
      SESSION_ID,
      "--run-dir",
      run_dir.to_str().unwrap(),
      "--speed",
      "instant",
      "--output",
      "stream-json",
    ])
    .assert()
    .success();
  let _ = assert; // stream-json + instant is the happy path
  // Now the rejection path: --output text and stream-json are
  // allowed by clap; json + json-envelope are clap-rejected at
  // parse time. Verify the clap message names the allowed set
  // so operators see what to switch to.
  let bad = cli_bin()
    .args([
      "harness",
      "replay",
      SESSION_ID,
      "--run-dir",
      run_dir.to_str().unwrap(),
      "--speed",
      "instant",
      "--output",
      "json",
    ])
    .assert()
    .failure();
  let stderr = String::from_utf8(bad.get_output().stderr.clone()).unwrap();
  assert!(
    stderr.contains("text") && stderr.contains("stream-json"),
    "clap error must name the allowed values: {stderr}",
  );
}

#[test]
fn cli_harness_replay_errors_when_session_id_unknown() {
  let dir = tempfile::tempdir().expect("tempdir");
  let run_dir = dir.path().to_path_buf();
  // Don't seed — the session id doesn't exist.
  let assert = cli_bin()
    .args([
      "harness",
      "replay",
      "never-existed",
      "--run-dir",
      run_dir.to_str().unwrap(),
      "--speed",
      "instant",
    ])
    .assert()
    .failure();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(
    stderr.contains("no events found for session 'never-existed'"),
    "stderr must explain the missing session: {stderr}",
  );
}

//! Frozen-fixture round-trip tests for the Harness Mode envelope.
//!
//! These fixtures are the v1 contract for `HarnessEvent`,
//! `ApprovalRequest`, and `ApprovalDecision`. Adding new fixtures is
//! fine; **changing** an existing one is a wire-breaking change that
//! must come with a `HARNESS_ENVELOPE_SCHEMA_VERSION` bump and a
//! `docs/STABILITY.md` entry.

use std::path::PathBuf;

use agentflow_harness::{
  ApprovalOutcome, ApprovalRisk, ApprovalScope, HARNESS_ENVELOPE_SCHEMA_VERSION, HarnessEvent,
  HarnessEventBody, HarnessProfile, HarnessRuntimeKind, StopReason,
};

fn fixture(name: &str) -> String {
  let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  path.push("tests");
  path.push("fixtures");
  path.push(name);
  std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {path:?}: {err}"))
}

fn parse(name: &str) -> HarnessEvent {
  let raw = fixture(name);
  serde_json::from_str(&raw).unwrap_or_else(|err| panic!("decode {name}: {err}\n{raw}"))
}

fn round_trip_value(event: &HarnessEvent) -> serde_json::Value {
  serde_json::to_value(event).expect("serialize harness event")
}

#[test]
fn schema_version_constant_is_stable() {
  assert_eq!(HARNESS_ENVELOPE_SCHEMA_VERSION, "harness/1");
}

#[test]
fn session_started_fixture_decodes() {
  let event = parse("session_started.json");
  assert_eq!(event.seq, 0);
  assert_eq!(event.session_id, "sess-fixture-1");
  match &event.body {
    HarnessEventBody::SessionStarted(payload) => {
      assert_eq!(payload.workspace_root, "/tmp/agentflow-fixture");
      assert_eq!(payload.runtime, HarnessRuntimeKind::React);
      assert_eq!(payload.profile, HarnessProfile::Local);
      assert_eq!(payload.model, "step-1");
      assert_eq!(payload.skills, vec!["code-review".to_string()]);
      assert_eq!(payload.context_item_count, 3);
      assert_eq!(payload.context_token_estimate, 1024);
    }
    other => panic!("expected SessionStarted, got {other:?}"),
  }
  let re = round_trip_value(&event);
  assert_eq!(re["kind"], "session_started");
  assert_eq!(re["payload"]["runtime"], "react");
}

#[test]
fn approval_requested_fixture_preserves_request_fields() {
  let event = parse("approval_requested.json");
  match &event.body {
    HarnessEventBody::ApprovalRequested(payload) => {
      let request = &payload.request;
      assert_eq!(request.tool, "shell");
      assert_eq!(request.permissions.len(), 1);
      assert_eq!(request.risk, ApprovalRisk::High);
      assert!(request.expires_at.is_none());
      assert_eq!(request.params_summary["cmd"], "rm /tmp/x");
    }
    other => panic!("expected ApprovalRequested, got {other:?}"),
  }
}

#[test]
fn approval_decided_fixture_preserves_decision_fields() {
  let event = parse("approval_decided.json");
  match &event.body {
    HarnessEventBody::ApprovalDecided(payload) => {
      let decision = &payload.decision;
      assert_eq!(decision.request_id, "req-1");
      assert_eq!(decision.decision, ApprovalOutcome::Allow);
      assert_eq!(decision.scope, ApprovalScope::Session);
      assert_eq!(decision.decided_by, "user");
      assert_eq!(decision.reason.as_deref(), Some("trusted cleanup script"));
    }
    other => panic!("expected ApprovalDecided, got {other:?}"),
  }
}

#[test]
fn stopped_fixture_completed_round_trips() {
  let event = parse("stopped_completed.json");
  match &event.body {
    HarnessEventBody::Stopped(payload) => {
      assert_eq!(payload.reason, StopReason::Completed);
      assert_eq!(payload.final_answer.as_deref(), Some("All TODOs reviewed."));
      assert!(payload.error.is_none());
    }
    other => panic!("expected Stopped, got {other:?}"),
  }
}

#[test]
fn unknown_optional_fields_are_ignored() {
  // Adding an unknown additive field must not break decoders. This guards
  // the "additive new fields are non-breaking" promise documented in
  // docs/STABILITY.md.
  let raw = r#"{
    "seq": 1,
    "session_id": "sess",
    "ts": "2026-05-14T00:00:00Z",
    "kind": "step_started",
    "payload": {"step_index": 0, "step_type": "plan", "future_only_field": 42}
  }"#;
  let event: HarnessEvent = serde_json::from_str(raw).expect("decode additive payload");
  match event.body {
    HarnessEventBody::StepStarted(payload) => {
      assert_eq!(payload.step_index, 0);
      assert_eq!(payload.step_type, "plan");
    }
    other => panic!("expected StepStarted, got {other:?}"),
  }
}

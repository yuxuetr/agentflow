use agentflow_agents::{AgentEvent, AgentStep, AgentStepKind};
use serde_json::{Value, json};

fn fixture_value(raw: &str) -> Value {
  serde_json::from_str(raw).expect("fixture should be valid JSON")
}

#[test]
fn agent_step_golden_fixtures_round_trip() {
  let fixture = fixture_value(include_str!("fixtures/agent_steps/compatibility_steps.json"));
  let steps: Vec<AgentStep> = serde_json::from_value(fixture.clone()).unwrap();

  assert!(steps.iter().any(|step| matches!(step.kind, AgentStepKind::ToolCall { .. })));
  assert!(steps.iter().any(|step| matches!(step.kind, AgentStepKind::ToolResult { .. })));
  assert!(steps.iter().any(|step| matches!(step.kind, AgentStepKind::Handoff { .. })));
  assert!(
    steps
      .iter()
      .any(|step| matches!(step.kind, AgentStepKind::BlackboardOp { .. }))
  );
  assert!(
    steps
      .iter()
      .any(|step| matches!(step.kind, AgentStepKind::DebateProposal { .. }))
  );
  assert!(
    steps
      .iter()
      .any(|step| matches!(step.kind, AgentStepKind::DebateVerdict { .. }))
  );

  assert_eq!(serde_json::to_value(steps).unwrap(), fixture);
}

#[test]
fn agent_event_golden_fixtures_round_trip() {
  let fixture = fixture_value(include_str!("fixtures/agent_events/compatibility_events.json"));
  let events: Vec<AgentEvent> = serde_json::from_value(fixture.clone()).unwrap();

  assert!(
    events
      .iter()
      .any(|event| matches!(event, AgentEvent::ToolCapabilityDecision { .. }))
  );
  assert!(events.iter().any(|event| matches!(event, AgentEvent::HandoffOccurred { .. })));
  assert!(events.iter().any(|event| matches!(event, AgentEvent::BlackboardWritten { .. })));
  assert!(
    events
      .iter()
      .any(|event| matches!(event, AgentEvent::DebateRoundStarted { .. }))
  );
  assert!(
    events
      .iter()
      .any(|event| matches!(event, AgentEvent::DebateVerdictRendered { .. }))
  );

  assert_eq!(serde_json::to_value(events).unwrap(), fixture);
}

#[test]
fn agent_step_accepts_additive_fields_inside_known_variants() {
  let mut value = json!({
    "index": 2,
    "kind": {
      "type": "tool_call",
      "tool": "search",
      "params": {"query": "agentflow"},
      "future_variant_field": "ignored"
    },
    "timestamp": "2026-05-10T00:00:02Z",
    "duration_ms": null,
    "future_step_field": "ignored"
  });

  let step: AgentStep = serde_json::from_value(value.clone()).unwrap();
  assert!(matches!(step.kind, AgentStepKind::ToolCall { .. }));

  value
    .as_object_mut()
    .unwrap()
    .remove("future_step_field");
  value["kind"]
    .as_object_mut()
    .unwrap()
    .remove("future_variant_field");
  assert_eq!(serde_json::to_value(step).unwrap(), value);
}

#[test]
fn agent_event_accepts_additive_fields_inside_known_variants() {
  let mut value = json!({
    "event": "tool_call_started",
    "session_id": "compat-session",
    "step_index": 2,
    "tool": "search",
    "params": {"query": "agentflow"},
    "source": "builtin",
    "permissions": ["network"],
    "timestamp": "2026-05-10T00:00:02Z",
    "future_event_field": "ignored"
  });

  let event: AgentEvent = serde_json::from_value(value.clone()).unwrap();
  assert!(matches!(event, AgentEvent::ToolCallStarted { .. }));

  value
    .as_object_mut()
    .unwrap()
    .remove("future_event_field");
  assert_eq!(serde_json::to_value(event).unwrap(), value);
}

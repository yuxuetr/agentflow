use agentflow_agents::{
  nodes::{
    AgentNodeResumeContract, AgentNodeToolReplayPolicy, agent_node::AgentNodeToolSideEffectClass,
  },
  runtime::{AgentRunResult, AgentStep, AgentStepKind, AgentStopReason},
};
use serde_json::json;

fn partial_result_with_tool_call(params: serde_json::Value) -> AgentRunResult {
  AgentRunResult {
    session_id: "resume-session".to_string(),
    answer: None,
    stop_reason: AgentStopReason::Error {
      message: "interrupted before tool result".to_string(),
    },
    steps: vec![AgentStep::new(
      0,
      AgentStepKind::ToolCall {
        tool: "lookup".to_string(),
        params,
      },
    )],
    events: Vec::new(),
  }
}

#[test]
fn resume_contract_allows_idempotent_replay_and_rejects_mutating_replay() {
  let idempotent = AgentNodeResumeContract::from_result(
    "agent",
    "react",
    &partial_result_with_tool_call(json!({
      "query": "status",
      "_agentflow": {
        "side_effect_class": "idempotent",
        "idempotency_key": "lookup-status"
      }
    })),
  );

  assert_eq!(idempotent.tool_calls.len(), 1);
  assert_eq!(
    idempotent.tool_calls[0].side_effect_class,
    AgentNodeToolSideEffectClass::Idempotent
  );
  assert_eq!(
    idempotent.tool_calls[0].replay_policy,
    AgentNodeToolReplayPolicy::ReplayAllowed
  );
  assert_eq!(
    idempotent.tool_calls[0].idempotency_key.as_deref(),
    Some("lookup-status")
  );

  let mutating = AgentNodeResumeContract::from_result(
    "agent",
    "react",
    &partial_result_with_tool_call(json!({
      "path": "/tmp/output.txt",
      "_agentflow": {
        "side_effect_class": "mutating"
      }
    })),
  );

  assert_eq!(mutating.tool_calls.len(), 1);
  assert_eq!(
    mutating.tool_calls[0].side_effect_class,
    AgentNodeToolSideEffectClass::Mutating
  );
  assert_eq!(
    mutating.tool_calls[0].replay_policy,
    AgentNodeToolReplayPolicy::ManualRequired
  );
}

use agentflow_tracing::{format_trace_replay, ExecutionTrace, ReplayOptions};

#[test]
fn hybrid_trace_replay_fixture_shows_workflow_agent_tool_chain() {
  let trace: ExecutionTrace =
    serde_json::from_str(include_str!("fixtures/hybrid_trace_replay.json"))
      .expect("hybrid trace fixture should deserialize");

  let replay = format_trace_replay(
    &trace,
    ReplayOptions {
      max_field_chars: 200,
      include_json: false,
    },
  );

  assert!(replay.contains("Trace Replay: hybrid-fixture"));
  assert!(replay.contains("[1] node prepare (template) - completed"));
  assert!(replay.contains("[2] node review_agent (skill_agent) - completed"));
  assert!(replay.contains("agent: session=session-hybrid"));
  assert!(replay.contains("step 1: tool_call mcp_fixture_echo"));
  assert!(replay.contains("tool: mcp_fixture_echo source=mcp error=false"));
  assert!(replay.contains("permissions: mcp,network"));
  assert!(replay.contains("duration: 12ms"));
}

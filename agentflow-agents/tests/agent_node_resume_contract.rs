use std::sync::Arc;

use agentflow_agents::{
  nodes::{
    AgentNodeResumeContract, AgentNodeToolReplayPolicy, agent_node::AgentNodeToolSideEffectClass,
  },
  runtime::{AgentRunResult, AgentStep, AgentStepKind, AgentStopReason},
};
use agentflow_tools::{Tool, ToolError, ToolIdempotency, ToolMetadata, ToolOutput, ToolRegistry};
use async_trait::async_trait;
use serde_json::{Value, json};

fn partial_result_with_tool_call(params: serde_json::Value) -> AgentRunResult {
  partial_result_with_named_tool_call("lookup", params)
}

fn partial_result_with_named_tool_call(tool: &str, params: serde_json::Value) -> AgentRunResult {
  AgentRunResult {
    session_id: "resume-session".to_string(),
    answer: None,
    stop_reason: AgentStopReason::Error {
      message: "interrupted before tool result".to_string(),
    },
    steps: vec![AgentStep::new(
      0,
      AgentStepKind::ToolCall {
        tool: tool.to_string(),
        params,
      },
    )],
    events: Vec::new(),
  }
}

/// Stub tool whose registry-declared idempotency is configurable per test.
/// Lets us assert that `from_result_with_tools` consults
/// `Tool::idempotency()` when the params hint is absent.
struct StubTool {
  name: &'static str,
  idempotency: ToolIdempotency,
}

#[async_trait]
impl Tool for StubTool {
  fn name(&self) -> &str {
    self.name
  }
  fn description(&self) -> &str {
    "stub tool"
  }
  fn parameters_schema(&self) -> Value {
    json!({"type": "object"})
  }
  fn metadata(&self) -> ToolMetadata {
    ToolMetadata::builtin_named(self.name).with_idempotency(self.idempotency)
  }
  fn idempotency(&self, _params: &Value) -> ToolIdempotency {
    self.idempotency
  }
  async fn execute(&self, _params: Value) -> Result<ToolOutput, ToolError> {
    Ok(ToolOutput::success("ok"))
  }
}

fn registry_with(tool: StubTool) -> ToolRegistry {
  let mut registry = ToolRegistry::new();
  registry.register(Arc::new(tool));
  registry
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

// ── Registry-driven side-effect bridge ───────────────────────────────────────
//
// The bridge lets `Tool::idempotency()` reach the resume planner without
// requiring every call site to embed `_agentflow.side_effect_class` in
// params. These tests pin the contract documented on
// `AgentNodeResumeContract::from_result_with_tools`:
//
// 1. Params hint always wins (operator intent is sacred).
// 2. Tools registered as `ToolIdempotency::Idempotent` get
//    `ReplayAllowed` automatically — no params hint needed.
// 3. Tools registered as `ToolIdempotency::NonIdempotent` get
//    `ManualRequired` automatically.
// 4. `ToolIdempotency::Unknown` and unregistered tools stay
//    `External` / `ManualRequired` (operator must opt in).

#[test]
fn registry_idempotent_tool_replays_without_params_hint() {
  let result = partial_result_with_named_tool_call("search", json!({"query": "agentflow"}));
  let tools = registry_with(StubTool {
    name: "search",
    idempotency: ToolIdempotency::Idempotent,
  });

  let contract = AgentNodeResumeContract::from_result_with_tools("agent", "react", &result, &tools);

  assert_eq!(
    contract.tool_calls[0].side_effect_class,
    AgentNodeToolSideEffectClass::Idempotent,
    "registry-declared Idempotent should bridge into Idempotent side_effect_class"
  );
  assert_eq!(
    contract.tool_calls[0].replay_policy,
    AgentNodeToolReplayPolicy::ReplayAllowed,
    "idempotent registry tool must be replay-allowed even without params hint"
  );
}

#[test]
fn registry_non_idempotent_tool_requires_manual_recovery() {
  let result = partial_result_with_named_tool_call(
    "send_email",
    json!({"to": "ops@example.test", "body": "deploy started"}),
  );
  let tools = registry_with(StubTool {
    name: "send_email",
    idempotency: ToolIdempotency::NonIdempotent,
  });

  let contract = AgentNodeResumeContract::from_result_with_tools("agent", "react", &result, &tools);

  assert_eq!(
    contract.tool_calls[0].side_effect_class,
    AgentNodeToolSideEffectClass::Mutating
  );
  assert_eq!(
    contract.tool_calls[0].replay_policy,
    AgentNodeToolReplayPolicy::ManualRequired,
    "non-idempotent registry tool must gate replay even without params hint"
  );
}

#[test]
fn registry_unknown_tool_defaults_to_external() {
  let result = partial_result_with_named_tool_call("mystery", json!({}));
  let tools = registry_with(StubTool {
    name: "mystery",
    idempotency: ToolIdempotency::Unknown,
  });

  let contract = AgentNodeResumeContract::from_result_with_tools("agent", "react", &result, &tools);

  assert_eq!(
    contract.tool_calls[0].side_effect_class,
    AgentNodeToolSideEffectClass::External,
    "Unknown idempotency must keep the pre-bridge default (External)"
  );
  assert_eq!(
    contract.tool_calls[0].replay_policy,
    AgentNodeToolReplayPolicy::ManualRequired
  );
}

#[test]
fn unregistered_tool_defaults_to_external() {
  // Tool name not present in the registry — the bridge should fall
  // through to the original External default, not panic or pick up
  // the empty registry's first entry.
  let result = partial_result_with_named_tool_call("ghost", json!({}));
  let tools = ToolRegistry::new();

  let contract = AgentNodeResumeContract::from_result_with_tools("agent", "react", &result, &tools);

  assert_eq!(
    contract.tool_calls[0].side_effect_class,
    AgentNodeToolSideEffectClass::External
  );
}

#[test]
fn params_hint_overrides_registry_metadata() {
  // Tool is registered as NonIdempotent, but params explicitly mark
  // the call as idempotent (e.g. the agent / operator vetted that
  // these specific args are safe to repeat). The params hint wins.
  let result = partial_result_with_named_tool_call(
    "send_email",
    json!({
      "to": "ops@example.test",
      "_agentflow": {"side_effect_class": "idempotent"}
    }),
  );
  let tools = registry_with(StubTool {
    name: "send_email",
    idempotency: ToolIdempotency::NonIdempotent,
  });

  let contract = AgentNodeResumeContract::from_result_with_tools("agent", "react", &result, &tools);

  assert_eq!(
    contract.tool_calls[0].side_effect_class,
    AgentNodeToolSideEffectClass::Idempotent
  );
  assert_eq!(
    contract.tool_calls[0].replay_policy,
    AgentNodeToolReplayPolicy::ReplayAllowed
  );
}

#[test]
fn legacy_from_result_keeps_pre_bridge_behavior() {
  // `from_result` (no registry) must keep the exact pre-bridge
  // behaviour: no params hint ⇒ External / ManualRequired. This
  // protects all existing callers and serialized contracts.
  let result = partial_result_with_named_tool_call("search", json!({"query": "agentflow"}));

  let contract = AgentNodeResumeContract::from_result("agent", "react", &result);

  assert_eq!(
    contract.tool_calls[0].side_effect_class,
    AgentNodeToolSideEffectClass::External
  );
  assert_eq!(
    contract.tool_calls[0].replay_policy,
    AgentNodeToolReplayPolicy::ManualRequired
  );
}

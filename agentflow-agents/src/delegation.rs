//! Applying a [`DelegationSpec`] end-to-end (L5.1): narrow a subagent's
//! tool registry, run it, and validate its answer against the spec's
//! expected output schema.
//!
//! This is a standalone primitive rather than being woven into any one
//! supervisor pattern's dispatch loop (Handoff / Blackboard / Debate, in
//! `crate::supervisor`) — before this, every one of those patterns had the
//! *caller* hand-build each subagent's `ToolRegistry` with no per-subagent
//! capability isolation. Any supervisor, or a direct caller, can now build
//! a subagent via [`build_delegated_agent`] and drive it via
//! [`run_delegated`]. Wiring a `DelegationSpec` field directly into e.g.
//! `HandoffSupervisorBuilder::add_agent` — so a supervisor applies the
//! narrowing itself instead of the caller doing it up front — is a
//! natural, smaller follow-up once this primitive exists.

use std::sync::Arc;

use agentflow_agent_spi::RuntimeLimits;
pub use agentflow_agent_spi::delegation::{DelegationSpec, SchemaValidation, validate_output};
use agentflow_memory::MemoryStore;
use agentflow_tools::ToolRegistry;

use crate::react::{ReActAgent, ReActConfig, ReActError};
use crate::runtime::{AgentContext, AgentRunResult};

/// Build a subagent whose tool registry is narrowed per `spec`
/// ([`DelegationSpec::narrow_registry`]) — the L5.1 "per-subagent
/// capability narrowing" primitive applied to a concrete `ReActAgent`.
pub fn build_delegated_agent(
  spec: &DelegationSpec,
  parent_registry: &ToolRegistry,
  config: ReActConfig,
  memory: Box<dyn MemoryStore>,
) -> ReActAgent {
  let narrowed = spec.narrow_registry(parent_registry);
  ReActAgent::new(config, memory, Arc::new(narrowed))
}

/// Outcome of running a subagent under a [`DelegationSpec`]: its raw
/// [`AgentRunResult`] plus whether the answer validated against the
/// spec's `expected_output_schema`, if any.
#[derive(Debug, Clone)]
pub struct DelegationOutcome {
  pub result: AgentRunResult,
  pub schema_validation: SchemaValidation,
}

/// Run `agent` against `spec`'s goal + input context (folded into a single
/// task string — the subagent's persona decides how to use the two
/// halves), applying `spec`'s `timeout_ms` / `budget_tokens` on top of
/// [`RuntimeLimits::react_defaults`], then validate the answer against
/// `spec.expected_output_schema`.
///
/// `session_id` / `model` mirror [`AgentContext::new`]'s parameters — the
/// caller picks them (typically the parent supervisor's session and
/// configured model).
pub async fn run_delegated(
  spec: &DelegationSpec,
  agent: &mut ReActAgent,
  session_id: &str,
  model: &str,
) -> Result<DelegationOutcome, ReActError> {
  let task = if spec.input_context.trim().is_empty() {
    spec.goal.clone()
  } else {
    format!("{}\n\nContext:\n{}", spec.goal, spec.input_context)
  };

  let mut limits = RuntimeLimits::react_defaults();
  if let Some(timeout_ms) = spec.timeout_ms {
    limits.timeout_ms = Some(timeout_ms);
  }
  if let Some(budget_tokens) = spec.budget_tokens {
    limits.token_budget = Some(budget_tokens);
  }
  let context = AgentContext::new(session_id, &task, model).with_limits(limits);

  let result = agent.run_with_context(context).await?;
  let schema_validation = result
    .answer
    .as_deref()
    .map(|answer| validate_output(spec, answer))
    .unwrap_or(SchemaValidation::NotRequired);
  Ok(DelegationOutcome {
    result,
    schema_validation,
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use agentflow_memory::SessionMemory;
  use agentflow_tools::builtin::{FileTool, HttpTool};
  use agentflow_tools::{Tool, ToolError, ToolOutput};
  use async_trait::async_trait;
  use serde_json::{Value, json};

  struct EchoTool;

  #[async_trait]
  impl Tool for EchoTool {
    fn name(&self) -> &str {
      "echo"
    }
    fn description(&self) -> &str {
      "Echo test input"
    }
    fn parameters_schema(&self) -> Value {
      json!({"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]})
    }
    async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
      Ok(ToolOutput::success(format!(
        "echo: {}",
        params["text"].as_str().unwrap_or_default()
      )))
    }
  }

  /// Removes `AGENTFLOW_MOCK_RESPONSES` on drop so a test can't leak it
  /// into whichever test acquires `LLM_TEST_LOCK` next — it takes priority
  /// over `AGENTFLOW_MOCK_RESPONSE` in the mock provider, so a stale queue
  /// silently hijacks every subsequent mock-model test in the process.
  struct EnvVarGuard(&'static str);
  impl Drop for EnvVarGuard {
    fn drop(&mut self) {
      // SAFETY: the LLM_TEST_LOCK guard serializes these process-wide mutations.
      unsafe {
        std::env::remove_var(self.0);
      }
    }
  }

  async fn init_mock_model(model: &str) {
    let config_path = std::env::temp_dir().join(format!(
      "agentflow-agents-mock-{}.yml",
      uuid::Uuid::new_v4()
    ));
    std::fs::write(
      &config_path,
      format!(
        r#"
models:
  {model}:
    vendor: mock
    type: text
    model_id: {model}
providers:
  mock:
    api_key_env: MOCK_API_KEY
"#
      ),
    )
    .unwrap();

    agentflow_llm::AgentFlow::init_with_config(config_path.to_str().unwrap())
      .await
      .unwrap();
  }

  fn parent_registry() -> ToolRegistry {
    let policy = Arc::new(agentflow_tools::sandbox::SandboxPolicy::permissive());
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(EchoTool));
    registry.register(Arc::new(HttpTool::new(policy.clone()).expect("http tool")));
    registry.register(Arc::new(FileTool::new(policy)));
    registry
  }

  #[test]
  fn build_delegated_agent_narrows_to_allowed_tools_only() {
    let spec = DelegationSpec::new("say hi").with_allowed_tools(["echo"]);
    let agent = build_delegated_agent(
      &spec,
      &parent_registry(),
      ReActConfig::new("mock-model"),
      Box::new(SessionMemory::default_window()),
    );
    let names: Vec<String> = agent
      .tools()
      .list()
      .iter()
      .map(|t| t.name().to_string())
      .collect();
    assert_eq!(names, vec!["echo".to_string()]);
  }

  #[test]
  fn build_delegated_agent_with_no_narrowing_keeps_every_tool() {
    let spec = DelegationSpec::new("say hi");
    let agent = build_delegated_agent(
      &spec,
      &parent_registry(),
      ReActConfig::new("mock-model"),
      Box::new(SessionMemory::default_window()),
    );
    assert_eq!(agent.tools().list().len(), 3);
  }

  #[tokio::test]
  async fn run_delegated_validates_answer_against_expected_output_schema() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-delegation-{}", uuid::Uuid::new_v4());
    // SAFETY: LLM_TEST_LOCK serializes mutation of process-wide mock env vars.
    unsafe {
      std::env::set_var(
        "AGENTFLOW_MOCK_RESPONSES",
        serde_json::to_string(&vec![
          r#"{"thought":"done","answer":"{\"summary\":\"ok\"}"}"#,
        ])
        .unwrap(),
      );
    }
    let _responses = EnvVarGuard("AGENTFLOW_MOCK_RESPONSES");
    init_mock_model(&model).await;

    let spec = DelegationSpec::new("summarize").with_expected_output_schema(json!({
      "type": "object",
      "properties": { "summary": { "type": "string" } },
      "required": ["summary"]
    }));
    let mut agent = build_delegated_agent(
      &spec,
      &ToolRegistry::new(),
      ReActConfig::new(&model).with_max_iterations(2),
      Box::new(SessionMemory::default_window()),
    );

    let outcome = run_delegated(&spec, &mut agent, "session-delegated", &model)
      .await
      .unwrap();
    assert_eq!(
      outcome.result.answer.as_deref(),
      Some(r#"{"summary":"ok"}"#)
    );
    assert_eq!(outcome.schema_validation, SchemaValidation::Valid);
  }

  #[tokio::test]
  async fn run_delegated_reports_invalid_when_answer_violates_schema() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-delegation-invalid-{}", uuid::Uuid::new_v4());
    // SAFETY: LLM_TEST_LOCK serializes mutation of process-wide mock env vars.
    unsafe {
      std::env::set_var(
        "AGENTFLOW_MOCK_RESPONSES",
        serde_json::to_string(&vec![r#"{"thought":"done","answer":"just plain text"}"#]).unwrap(),
      );
    }
    let _responses = EnvVarGuard("AGENTFLOW_MOCK_RESPONSES");
    init_mock_model(&model).await;

    let spec = DelegationSpec::new("summarize").with_expected_output_schema(json!({
      "type": "object",
      "properties": { "summary": { "type": "string" } },
      "required": ["summary"]
    }));
    let mut agent = build_delegated_agent(
      &spec,
      &ToolRegistry::new(),
      ReActConfig::new(&model).with_max_iterations(2),
      Box::new(SessionMemory::default_window()),
    );

    let outcome = run_delegated(&spec, &mut agent, "session-delegated-invalid", &model)
      .await
      .unwrap();
    assert!(!outcome.schema_validation.is_valid());
  }

  /// L5.1 literal regression scenario: a subagent narrowed to `["echo"]`
  /// attempts to call a tool outside its spec (`http`) — it must be
  /// denied, and the denial must be visible in the run's trace (a
  /// `ToolPolicyDecision` event / `ToolResult` step reporting the error),
  /// not silently ignored.
  #[tokio::test]
  async fn narrowed_subagent_denied_tool_outside_spec_is_visible_in_trace() {
    let _guard = crate::LLM_TEST_LOCK.lock().await;
    let model = format!("mock-delegation-denied-{}", uuid::Uuid::new_v4());
    // SAFETY: LLM_TEST_LOCK serializes mutation of process-wide mock env vars.
    unsafe {
      std::env::set_var(
        "AGENTFLOW_MOCK_RESPONSES",
        serde_json::to_string(&vec![
          r#"{"thought":"try http anyway","action":{"tool":"http","params":{"url":"https://example.com"}}}"#,
          r#"{"thought":"blocked, answering directly","answer":"could not fetch"}"#,
        ])
        .unwrap(),
      );
    }
    let _responses = EnvVarGuard("AGENTFLOW_MOCK_RESPONSES");
    init_mock_model(&model).await;

    let spec = DelegationSpec::new("fetch a page").with_allowed_tools(["echo"]);
    let mut agent = build_delegated_agent(
      &spec,
      &parent_registry(),
      ReActConfig::new(&model).with_max_iterations(4),
      Box::new(SessionMemory::default_window()),
    );

    let outcome = run_delegated(&spec, &mut agent, "session-denied", &model)
      .await
      .unwrap();

    assert!(
      outcome.result.steps.iter().any(
        |step| matches!(&step.kind, crate::runtime::AgentStepKind::ToolResult {
          tool,
          is_error: true,
          ..
        } if tool == "http")
      ),
      "the denied http call must show up as a failed ToolResult step in the trace: {:#?}",
      outcome.result.steps
    );
  }
}

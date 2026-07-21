//! Custom verification strategy example.
//!
//! Plugs a small user-defined `VerificationStrategy` into a `ReActAgent`.
//! Unlike a `ReflectionStrategy` (see `custom_reflection.rs`), which only
//! observes a decision the loop already made, a `VerificationStrategy` can
//! *reject* a candidate final answer and send the loop back around for
//! another attempt.
//!
//! It uses the **mock LLM provider**, so it does not need any API key or
//! network access. The mock script: one tool call, then a first candidate
//! answer the verifier rejects for missing a citation, then a revised
//! answer the verifier accepts.
//!
//! Run:
//!
//! ```sh
//! cargo run -p agentflow-agents --example custom_verification
//! ```

use std::fs;
use std::sync::Arc;

use agentflow_agents::verification::{
  VerificationContext, VerificationError, VerificationOutcome, VerificationStrategy,
};
use agentflow_agents::{
  AgentContext, AgentRuntime,
  react::{ReActAgent, ReActConfig},
};
use agentflow_memory::SessionMemory;
use agentflow_tools::{Tool, ToolError, ToolOutput, ToolRegistry};
use async_trait::async_trait;
use serde_json::{Value, json};

// ── A user-defined verification strategy ───────────────────────────────────

/// Rejects a candidate answer that has no `[n]`-style citation marker, and
/// approves once one is present.
#[derive(Debug, Default, Clone)]
struct RequireCitation;

#[async_trait]
impl VerificationStrategy for RequireCitation {
  fn name(&self) -> &'static str {
    "require-citation"
  }

  async fn verify(
    &self,
    context: &VerificationContext,
  ) -> Result<VerificationOutcome, VerificationError> {
    if context.answer.contains('[') {
      return Ok(VerificationOutcome::Approved);
    }
    Ok(VerificationOutcome::Rejected {
      feedback: "Cite your source with a `[n]` marker before answering.".to_string(),
    })
  }
}

// ── A trivial deterministic tool the mock script can call ──────────────────

struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
  fn name(&self) -> &str {
    "echo"
  }

  fn description(&self) -> &str {
    "Echo the supplied text with a deterministic prefix."
  }

  fn parameters_schema(&self) -> Value {
    json!({
      "type": "object",
      "properties": {
        "text": { "type": "string", "description": "Text to echo." }
      },
      "required": ["text"]
    })
  }

  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
    let text = params["text"].as_str().unwrap_or_default();
    Ok(ToolOutput::success(format!("echo: {text}")))
  }
}

// ── Mock-provider scaffolding (matches `custom_reflection.rs`) ─────────────

async fn init_mock_model(model: &str) -> anyhow::Result<()> {
  // SAFETY: this standalone example wires up the mock provider before the
  // ReAct agent is constructed; nothing else mutates the env in parallel.
  unsafe {
    std::env::set_var(
      "AGENTFLOW_MOCK_RESPONSES",
      serde_json::to_string(&[
        r#"{"thought":"call echo","action":{"tool":"echo","params":{"text":"sdk"}}}"#,
        r#"{"thought":"echo returned the expected text","answer":"echo: sdk"}"#,
        r#"{"thought":"adding a citation as the verifier asked","answer":"echo: sdk [1]"}"#,
      ])?,
    );
  }

  let config_path = std::env::temp_dir().join(format!(
    "agentflow-custom-verification-{}.yml",
    uuid::Uuid::new_v4()
  ));
  fs::write(
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
  )?;

  agentflow_llm::AgentFlow::init_with_config(config_path.to_str().unwrap()).await?;
  Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  let model = "mock-custom-verification";
  init_mock_model(model).await?;

  let mut registry = ToolRegistry::new();
  registry.register(Arc::new(EchoTool));
  let registry = Arc::new(registry);

  // Agent — note `with_verification_strategy` accepts any
  // `Arc<dyn VerificationStrategy>`.
  let mut agent = ReActAgent::new(
    ReActConfig::new(model)
      .with_persona("Use the echo tool when the user asks to echo something.")
      .with_max_iterations(6),
    Box::new(SessionMemory::default_window()),
    registry,
  )
  .with_verification_strategy(Arc::new(RequireCitation));

  let result = AgentRuntime::run(
    &mut agent,
    AgentContext::new("custom-verification-example", "Echo the word sdk.", model),
  )
  .await?;

  println!("Answer: {}", result.answer.as_deref().unwrap_or("<none>"));
  println!("Stop reason: {:?}", result.stop_reason);
  println!("--- Steps ---");
  for step in &result.steps {
    println!("  {:>2}: {}", step.index, step_kind_label(step));
  }

  Ok(())
}

fn step_kind_label(step: &agentflow_agents::AgentStep) -> String {
  match &step.kind {
    agentflow_agents::AgentStepKind::Observe { input } => format!("observe: {input}"),
    agentflow_agents::AgentStepKind::Plan { thought } => format!("plan: {thought}"),
    agentflow_agents::AgentStepKind::ToolCall { tool, .. } => format!("tool_call: {tool}"),
    agentflow_agents::AgentStepKind::ToolResult { tool, content, .. } => {
      format!("tool_result: {tool} -> {content}")
    }
    agentflow_agents::AgentStepKind::Verify {
      approved, attempt, ..
    } => format!("verify: attempt {attempt} approved={approved}"),
    agentflow_agents::AgentStepKind::FinalAnswer { answer } => format!("final: {answer}"),
    other => format!("{other:?}"),
  }
}

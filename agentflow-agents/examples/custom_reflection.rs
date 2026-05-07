//! Custom reflection strategy example.
//!
//! Plugs a small user-defined `ReflectionStrategy` into a `ReActAgent`.
//! The strategy records every reflection trigger into a shared
//! `Arc<Mutex<Vec<_>>>`, so the example also doubles as a smoke-test for the
//! reflection extension point.
//!
//! It uses the **mock LLM provider**, so it does not need any API key or
//! network access. The mock returns a hand-crafted ReAct script: one tool
//! call followed by a final answer.
//!
//! Run:
//!
//! ```sh
//! cargo run -p agentflow-agents --example custom_reflection
//! ```

use std::fs;
use std::sync::{Arc, Mutex};

use agentflow_agents::reflection::{
  Reflection, ReflectionContext, ReflectionError, ReflectionStrategy, ReflectionTrigger,
};
use agentflow_agents::{
  AgentContext, AgentRuntime,
  react::{ReActAgent, ReActConfig},
};
use agentflow_memory::SessionMemory;
use agentflow_tools::{Tool, ToolError, ToolOutput, ToolRegistry};
use async_trait::async_trait;
use serde_json::{Value, json};

// ── A user-defined reflection strategy ─────────────────────────────────────

/// Records every reflection trigger and emits a one-line summary that the
/// runtime stores as a `Reflect` step.
#[derive(Debug, Clone, Default)]
struct LoggingReflection {
  /// Shared log so the test driver can assert what was observed.
  observed: Arc<Mutex<Vec<ReflectionTrigger>>>,
}

#[async_trait]
impl ReflectionStrategy for LoggingReflection {
  fn name(&self) -> &'static str {
    "logging"
  }

  async fn reflect(
    &self,
    context: &ReflectionContext,
  ) -> Result<Option<Reflection>, ReflectionError> {
    if let Ok(mut observed) = self.observed.lock() {
      observed.push(context.trigger.clone());
    }

    let content = match context.trigger {
      ReflectionTrigger::Final => format!(
        "[logging] final answer at step {}: {}",
        context.step_index,
        context.answer.as_deref().unwrap_or("<none>")
      ),
      ReflectionTrigger::Failure => format!(
        "[logging] failure at step {}: {}",
        context.step_index,
        context.error.as_deref().unwrap_or("<unknown>")
      ),
      ReflectionTrigger::Step => format!("[logging] step {} observed", context.step_index),
    };

    Ok(Some(Reflection::new(
      self.name(),
      context.trigger.clone(),
      content,
    )))
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

// ── Mock-provider scaffolding (matches `agent_native_react.rs`) ────────────

async fn init_mock_model(model: &str) -> anyhow::Result<()> {
  // SAFETY: this standalone example wires up the mock provider before the
  // ReAct agent is constructed; nothing else mutates the env in parallel.
  unsafe {
    std::env::set_var(
      "AGENTFLOW_MOCK_RESPONSES",
      serde_json::to_string(&[
        r#"{"thought":"call echo","action":{"tool":"echo","params":{"text":"sdk"}}}"#,
        r#"{"thought":"echo returned the expected text","answer":"echo: sdk"}"#,
      ])?,
    );
  }

  let config_path = std::env::temp_dir().join(format!(
    "agentflow-custom-reflection-{}.yml",
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
  let model = "mock-custom-reflection";
  init_mock_model(model).await?;

  // Tools
  let mut registry = ToolRegistry::new();
  registry.register(Arc::new(EchoTool));
  let registry = Arc::new(registry);

  // Reflection strategy + shared log
  let reflection = LoggingReflection::default();
  let observed = reflection.observed.clone();

  // Agent — note `with_reflection_strategy` accepts any `Arc<dyn ReflectionStrategy>`.
  let mut agent = ReActAgent::new(
    ReActConfig::new(model)
      .with_persona("Use the echo tool when the user asks to echo something.")
      .with_max_iterations(4),
    Box::new(SessionMemory::default_window()),
    registry,
  )
  .with_reflection_strategy(Arc::new(reflection));

  let result = AgentRuntime::run(
    &mut agent,
    AgentContext::new("custom-reflection-example", "Echo the word sdk.", model),
  )
  .await?;

  println!("Answer: {}", result.answer.as_deref().unwrap_or("<none>"));
  println!("Stop reason: {:?}", result.stop_reason);
  println!(
    "Triggers observed by LoggingReflection: {:?}",
    observed.lock().unwrap()
  );
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
    agentflow_agents::AgentStepKind::Reflect { content } => format!("reflect: {content}"),
    agentflow_agents::AgentStepKind::FinalAnswer { answer } => format!("final: {answer}"),
    other => format!("{other:?}"),
  }
}

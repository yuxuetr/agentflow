//! Agent-native ReAct runtime example.
//!
//! This example runs a ReAct agent directly, without embedding it in a DAG.
//! It uses the mock LLM provider, so it does not need an external API key.
//!
//! Run:
//! ```sh
//! cargo run -p agentflow-agents --example agent_native_react
//! ```

use std::fs;
use std::sync::Arc;

use agentflow_agents::react::{ReActAgent, ReActConfig};
use agentflow_agents::{AgentContext, AgentRuntime};
use agentflow_memory::SessionMemory;
use agentflow_tools::{Tool, ToolError, ToolOutput, ToolRegistry};
use async_trait::async_trait;
use serde_json::{json, Value};

struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
  fn name(&self) -> &str {
    "echo"
  }

  fn description(&self) -> &str {
    "Echo input text with a deterministic prefix."
  }

  fn parameters_schema(&self) -> Value {
    json!({
      "type": "object",
      "properties": {
        "text": {
          "type": "string",
          "description": "Text to echo."
        }
      },
      "required": ["text"]
    })
  }

  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
    let text = params["text"].as_str().unwrap_or_default();
    Ok(ToolOutput::success(format!("echo: {text}")))
  }
}

async fn init_mock_model(model: &str) -> anyhow::Result<()> {
  std::env::set_var(
    "AGENTFLOW_MOCK_RESPONSES",
    serde_json::to_string(&vec![
      r#"{"thought":"use the echo tool","action":{"tool":"echo","params":{"text":"agent-native"}}}"#,
      r#"{"thought":"the tool returned the expected text","answer":"final answer: echo: agent-native"}"#,
    ])?,
  );

  let config_path = std::env::temp_dir().join(format!(
    "agentflow-agent-native-react-{}.yml",
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
  let model = "mock-agent-native-react";
  init_mock_model(model).await?;

  let mut registry = ToolRegistry::new();
  registry.register(Arc::new(EchoTool));

  let mut agent = ReActAgent::new(
    ReActConfig::new(model)
      .with_persona("Use tools when they directly answer the request.")
      .with_max_iterations(4),
    Box::new(SessionMemory::default_window()),
    Arc::new(registry),
  );

  let result = AgentRuntime::run(
    &mut agent,
    AgentContext::new(
      "agent-native-example",
      "Echo the phrase agent-native.",
      model,
    ),
  )
  .await?;

  println!("Answer:");
  println!("{}", result.answer.unwrap_or_default());
  println!();
  println!("Stop reason:");
  println!("{}", serde_json::to_string_pretty(&result.stop_reason)?);
  println!();
  println!("Runtime steps:");
  println!("{}", serde_json::to_string_pretty(&result.steps)?);

  Ok(())
}

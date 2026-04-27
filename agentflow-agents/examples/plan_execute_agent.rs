//! Agent-native Plan-and-Execute runtime example.
//!
//! This example uses the mock LLM provider and a deterministic local tool.
//!
//! Run:
//! ```sh
//! cargo run -p agentflow-agents --example plan_execute_agent
//! ```

use std::fs;
use std::sync::Arc;

use agentflow_agents::{AgentContext, AgentRuntime, PlanExecuteAgent, PlanExecuteConfig};
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
    "AGENTFLOW_MOCK_RESPONSE",
    r#"{"plan":[{"id":"1","description":"Echo the requested phrase","tool":"echo","params":{"text":"plan-execute"}}]}"#,
  );

  let config_path = std::env::temp_dir().join(format!(
    "agentflow-plan-execute-example-{}.yml",
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
  let model = "mock-plan-execute-example";
  init_mock_model(model).await?;

  let mut registry = ToolRegistry::new();
  registry.register(Arc::new(EchoTool));

  let mut agent = PlanExecuteAgent::new(
    PlanExecuteConfig::new(model).with_max_steps(4),
    Box::new(SessionMemory::default_window()),
    Arc::new(registry),
  );

  let result = AgentRuntime::run(
    &mut agent,
    AgentContext::new(
      "plan-execute-example",
      "Echo the phrase plan-execute.",
      model,
    ),
  )
  .await?;

  println!("Answer:");
  println!("{}", result.answer.unwrap_or_default());
  println!();
  println!("Runtime steps:");
  println!("{}", serde_json::to_string_pretty(&result.steps)?);

  Ok(())
}

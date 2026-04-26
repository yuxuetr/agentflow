use std::fs;
use std::sync::Arc;

use agentflow_agents::agentflow_memory::SessionMemory;
use agentflow_agents::agentflow_tools::{Tool, ToolError, ToolOutput, ToolRegistry};
use agentflow_agents::react::{ReActAgent, ReActConfig};
use agentflow_agents::{AgentContext, AgentFlow, AgentStepKind, FinalReflection};
use async_trait::async_trait;
use serde_json::{json, Value};

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
    json!({
      "type": "object",
      "properties": {
        "text": {"type": "string"}
      },
      "required": ["text"]
    })
  }

  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
    Ok(ToolOutput::success(format!(
      "echo: {}",
      params["text"].as_str().unwrap_or_default()
    )))
  }
}

async fn init_mock_model(model: &str, responses: &[&str]) {
  std::env::set_var(
    "AGENTFLOW_MOCK_RESPONSES",
    serde_json::to_string(responses).unwrap(),
  );

  let config_path = std::env::temp_dir().join(format!(
    "agentflow-agents-golden-{}.yml",
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
  )
  .unwrap();

  AgentFlow::init_with_config(config_path.to_str().unwrap())
    .await
    .unwrap();
}

fn normalize_runtime_json(value: &mut Value) {
  match value {
    Value::Object(map) => {
      if map.contains_key("timestamp") {
        map.insert("timestamp".to_string(), json!("<timestamp>"));
      }
      if matches!(map.get("duration_ms"), Some(Value::Number(_))) {
        map.insert("duration_ms".to_string(), json!(0));
      }
      for value in map.values_mut() {
        normalize_runtime_json(value);
      }
    }
    Value::Array(items) => {
      for value in items {
        normalize_runtime_json(value);
      }
    }
    _ => {}
  }
}

#[tokio::test]
async fn react_runtime_trace_matches_golden_fixture() {
  let model = "mock-golden-runtime";
  init_mock_model(
    &model,
    &[
      r#"{"thought":"use tool","action":{"tool":"echo","params":{"text":"hi"}}}"#,
      r#"{"thought":"done","answer":"final: echo: hi"}"#,
    ],
  )
  .await;

  let mut registry = ToolRegistry::new();
  registry.register(Arc::new(EchoTool));
  let mut agent = ReActAgent::new(
    ReActConfig::new(model).with_max_iterations(4),
    Box::new(SessionMemory::default_window()),
    Arc::new(registry),
  )
  .with_reflection_strategy(Arc::new(FinalReflection));

  let result = agent
    .run_with_context(AgentContext::new("golden-session", "say hi", model))
    .await
    .unwrap();

  assert!(matches!(
    result.steps.get(2).map(|step| &step.kind),
    Some(AgentStepKind::ToolCall { tool, .. }) if tool == "echo"
  ));

  let mut actual = serde_json::to_value(result).unwrap();
  normalize_runtime_json(&mut actual);

  let expected: Value =
    serde_json::from_str(include_str!("fixtures/agent_runtime_react_trace.json")).unwrap();
  assert_eq!(actual, expected);
}

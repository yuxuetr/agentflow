//! Wrap a DAG [`Flow`](agentflow_core::Flow) as an agent-callable tool.

use std::sync::Arc;

use agentflow_core::{async_node::AsyncNodeInputs, Flow, FlowValue};
use agentflow_tools::{Tool, ToolError, ToolOutput, ToolOutputPart};
use async_trait::async_trait;
use serde_json::Value;

/// Tool adapter that lets an agent call a workflow as a normal tool.
pub struct WorkflowTool {
  name: String,
  description: String,
  parameters_schema: Value,
  flow: Arc<Flow>,
}

impl WorkflowTool {
  pub fn new(name: impl Into<String>, description: impl Into<String>, flow: Flow) -> Self {
    Self::with_schema(
      name,
      description,
      default_workflow_parameters_schema(),
      flow,
    )
  }

  pub fn with_schema(
    name: impl Into<String>,
    description: impl Into<String>,
    parameters_schema: Value,
    flow: Flow,
  ) -> Self {
    Self {
      name: name.into(),
      description: description.into(),
      parameters_schema,
      flow: Arc::new(flow),
    }
  }

  pub fn from_shared(
    name: impl Into<String>,
    description: impl Into<String>,
    parameters_schema: Value,
    flow: Arc<Flow>,
  ) -> Self {
    Self {
      name: name.into(),
      description: description.into(),
      parameters_schema,
      flow,
    }
  }

  pub fn flow_handle(&self) -> Arc<Flow> {
    self.flow.clone()
  }
}

#[async_trait]
impl Tool for WorkflowTool {
  fn name(&self) -> &str {
    &self.name
  }

  fn description(&self) -> &str {
    &self.description
  }

  fn parameters_schema(&self) -> Value {
    self.parameters_schema.clone()
  }

  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
    let inputs = params_to_inputs(params)?;
    let results =
      self
        .flow
        .execute_from_inputs(inputs)
        .await
        .map_err(|e| ToolError::ExecutionFailed {
          message: format!("Workflow tool '{}' failed: {}", self.name, e),
        })?;
    let has_node_error = results.values().any(Result::is_err);
    let value = serde_json::to_value(&results).map_err(ToolError::SerdeError)?;
    let content = serde_json::to_string_pretty(&value).map_err(ToolError::SerdeError)?;
    let parts = vec![ToolOutputPart::Resource {
      uri: format!("workflow://{}", self.name),
      mime_type: Some("application/json".to_string()),
      text: Some(content.clone()),
    }];

    if has_node_error {
      Ok(ToolOutput::error_parts(content, parts))
    } else {
      Ok(ToolOutput::success_parts(content, parts))
    }
  }
}

fn default_workflow_parameters_schema() -> Value {
  serde_json::json!({
    "type": "object",
    "description": "Initial workflow inputs keyed by input name.",
    "additionalProperties": true
  })
}

fn params_to_inputs(params: Value) -> Result<AsyncNodeInputs, ToolError> {
  let Value::Object(map) = params else {
    return Err(ToolError::InvalidParams {
      message: "Workflow tool parameters must be a JSON object".to_string(),
    });
  };

  let mut inputs = AsyncNodeInputs::new();
  for (key, value) in map {
    let flow_value =
      serde_json::from_value::<FlowValue>(value.clone()).unwrap_or_else(|_| FlowValue::Json(value));
    inputs.insert(key, flow_value);
  }
  Ok(inputs)
}

#[cfg(test)]
mod tests {
  use super::*;
  use agentflow_core::{
    async_node::{AsyncNodeInputs, AsyncNodeResult},
    error::AgentFlowError,
    flow::{GraphNode, NodeType},
  };
  use async_trait::async_trait;
  use serde_json::json;

  struct EchoNode;

  #[async_trait]
  impl agentflow_core::AsyncNode for EchoNode {
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
      let mut outputs = AsyncNodeInputs::new();
      outputs.insert(
        "echo".to_string(),
        inputs
          .get("text")
          .cloned()
          .unwrap_or_else(|| FlowValue::Json(json!(""))),
      );
      Ok(outputs)
    }
  }

  struct FailingNode;

  #[async_trait]
  impl agentflow_core::AsyncNode for FailingNode {
    async fn execute(&self, _inputs: &AsyncNodeInputs) -> AsyncNodeResult {
      Err(AgentFlowError::NodeExecutionFailed {
        message: "boom".to_string(),
      })
    }
  }

  fn single_node_flow(id: &str, node: Arc<dyn agentflow_core::AsyncNode>) -> Flow {
    Flow::new(vec![GraphNode {
      id: id.to_string(),
      node_type: NodeType::Standard(node),
      dependencies: Vec::new(),
      input_mapping: None,
      run_if: None,
      initial_inputs: AsyncNodeInputs::new(),
    }])
  }

  fn use_writable_home() {
    let home = std::env::temp_dir().join(format!(
      "agentflow-workflow-tool-test-{}",
      uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("HOME", home);
  }

  #[tokio::test]
  async fn workflow_tool_executes_flow_and_returns_json_resource() {
    use_writable_home();
    let tool = WorkflowTool::new(
      "echo_workflow",
      "Run echo workflow",
      single_node_flow("echo", Arc::new(EchoNode)),
    );

    let output = tool.execute(json!({"text": "hello"})).await.unwrap();

    assert!(!output.is_error);
    assert!(output.content.contains("\"echo\""));
    assert!(output.content.contains("hello"));
    assert!(matches!(
      output.parts.as_slice(),
      [ToolOutputPart::Resource {
        uri,
        mime_type: Some(mime_type),
        ..
      }] if uri == "workflow://echo_workflow" && mime_type == "application/json"
    ));
  }

  #[tokio::test]
  async fn workflow_tool_marks_node_errors_as_tool_error_output() {
    use_writable_home();
    let tool = WorkflowTool::new(
      "failing_workflow",
      "Run failing workflow",
      single_node_flow("fail", Arc::new(FailingNode)),
    );

    let output = tool.execute(json!({})).await.unwrap();

    assert!(output.is_error);
    assert!(output.content.contains("boom"));
  }

  #[tokio::test]
  async fn workflow_tool_rejects_non_object_params() {
    let tool = WorkflowTool::new(
      "echo_workflow",
      "Run echo workflow",
      single_node_flow("echo", Arc::new(EchoNode)),
    );

    let err = tool.execute(json!("bad")).await.unwrap_err();

    assert!(matches!(err, ToolError::InvalidParams { .. }));
  }
}

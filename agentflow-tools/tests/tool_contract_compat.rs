use std::sync::Arc;

use agentflow_tools::{
  Tool, ToolDefinition, ToolError, ToolIdempotency, ToolMetadata, ToolOutput, ToolOutputPart,
  ToolPermission, ToolPermissionSet, ToolRegistry, ToolSource,
};
use async_trait::async_trait;
use serde_json::{Value, json};

fn fixture_value(raw: &str) -> Value {
  serde_json::from_str(raw).expect("fixture should be valid JSON")
}

struct LookupTool;

#[async_trait]
impl Tool for LookupTool {
  fn name(&self) -> &str {
    "lookup"
  }

  fn description(&self) -> &str {
    "Lookup a value"
  }

  fn parameters_schema(&self) -> Value {
    json!({
      "type": "object",
      "properties": {
        "query": {
          "type": "string"
        }
      },
      "required": ["query"]
    })
  }

  fn metadata(&self) -> ToolMetadata {
    ToolMetadata::builtin_named("lookup").with_idempotency(ToolIdempotency::Idempotent)
  }

  async fn execute(&self, _params: Value) -> Result<ToolOutput, ToolError> {
    Ok(ToolOutput::success("ok"))
  }
}

#[test]
fn tool_definition_fixture_round_trips() {
  let fixture = fixture_value(include_str!("fixtures/tool_contracts/tool_definition.json"));
  let definition: ToolDefinition = serde_json::from_value(fixture.clone()).unwrap();

  assert_eq!(definition.name, "mcp_demo_echo");
  assert_eq!(definition.metadata.source, ToolSource::Mcp);
  assert_eq!(definition.metadata.idempotency, ToolIdempotency::Unknown);
  assert_eq!(definition.metadata.mcp_server_name.as_deref(), Some("demo"));
  assert_eq!(definition.metadata.mcp_tool_name.as_deref(), Some("echo"));
  assert_eq!(
    definition.metadata.permissions.permissions,
    vec![ToolPermission::Mcp, ToolPermission::Network]
  );
  assert_eq!(serde_json::to_value(definition).unwrap(), fixture);
}

#[test]
fn tool_metadata_and_permission_set_fixture_round_trips() {
  let fixture = fixture_value(include_str!("fixtures/tool_contracts/tool_metadata.json"));
  let metadata: ToolMetadata = serde_json::from_value(fixture.clone()).unwrap();

  assert_eq!(metadata.source, ToolSource::Script);
  assert_eq!(metadata.idempotency, ToolIdempotency::NonIdempotent);
  assert_eq!(
    metadata.permissions,
    ToolPermissionSet::new([ToolPermission::FilesystemRead, ToolPermission::ProcessExec])
  );
  assert_eq!(serde_json::to_value(metadata).unwrap(), fixture);
}

#[test]
fn typed_tool_output_parts_fixture_round_trips() {
  let fixture = fixture_value(include_str!("fixtures/tool_contracts/tool_output_parts.json"));
  let parts: Vec<ToolOutputPart> = serde_json::from_value(fixture.clone()).unwrap();

  assert!(matches!(parts[0], ToolOutputPart::Text { .. }));
  assert!(matches!(parts[1], ToolOutputPart::Image { .. }));
  assert!(matches!(parts[2], ToolOutputPart::Resource { .. }));
  assert_eq!(serde_json::to_value(parts).unwrap(), fixture);
}

#[test]
fn openai_tools_array_fixture_remains_stable() {
  let mut registry = ToolRegistry::new();
  registry.register(Arc::new(LookupTool));

  assert_eq!(
    serde_json::to_value(registry.openai_tools_array()).unwrap(),
    fixture_value(include_str!("fixtures/tool_contracts/openai_tools_array.json"))
  );
}

#[test]
fn tool_contracts_accept_additive_fields_where_supported() {
  let mut value = json!({
    "name": "mcp_demo_echo",
    "description": "Echo text through a demo MCP server",
    "parameters": {"type": "object"},
    "metadata": {
      "source": "mcp",
      "permissions": {"permissions": ["mcp", "network"]},
      "idempotency": "unknown",
      "mcp_server_name": "demo",
      "mcp_tool_name": "echo",
      "future_metadata_field": "ignored"
    },
    "future_definition_field": "ignored"
  });

  let definition: ToolDefinition = serde_json::from_value(value.clone()).unwrap();
  value.as_object_mut().unwrap().remove("future_definition_field");
  value["metadata"]
    .as_object_mut()
    .unwrap()
    .remove("future_metadata_field");

  assert_eq!(serde_json::to_value(definition).unwrap(), value);
}

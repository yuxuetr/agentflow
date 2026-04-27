use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::{Tool, ToolError, ToolOutput, ToolPermission};

/// Central registry for all available tools.
///
/// Register tools once at startup; the [`ReActAgent`] uses the registry
/// to look up and dispatch tool calls from LLM responses.
pub struct ToolRegistry {
  tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
  pub fn new() -> Self {
    Self {
      tools: HashMap::new(),
    }
  }

  /// Register a tool.  A previously registered tool with the same name
  /// is silently replaced.
  pub fn register(&mut self, tool: Arc<dyn Tool>) {
    self.tools.insert(tool.name().to_string(), tool);
  }

  /// Look up a tool by name.
  pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
    self.tools.get(name).cloned()
  }

  /// List all registered tools.
  pub fn list(&self) -> Vec<Arc<dyn Tool>> {
    self.tools.values().cloned().collect()
  }

  /// List tools whose metadata includes a specific permission.
  pub fn list_by_permission(&self, permission: ToolPermission) -> Vec<Arc<dyn Tool>> {
    self
      .tools
      .values()
      .filter(|tool| tool.metadata().permissions.allows(&permission))
      .cloned()
      .collect()
  }

  /// Check whether a registered tool declares a permission.
  pub fn tool_has_permission(&self, name: &str, permission: &ToolPermission) -> bool {
    self
      .tools
      .get(name)
      .map(|tool| tool.metadata().permissions.allows(permission))
      .unwrap_or(false)
  }

  /// Build an OpenAI-style `tools` array for use in API calls.
  pub fn openai_tools_array(&self) -> Vec<Value> {
    self
      .tools
      .values()
      .map(|t| {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": t.name(),
                "description": t.description(),
                "parameters": t.parameters_schema()
            }
        })
      })
      .collect()
  }

  /// Build a compact plain-text description of all tools for use in
  /// system prompts (prompt-based tool calling).
  pub fn prompt_tools_description(&self) -> String {
    let mut lines: Vec<String> = self
      .tools
      .values()
      .map(|t| t.prompt_description())
      .collect();
    lines.sort(); // deterministic ordering
    lines.join("\n")
  }

  /// Execute a named tool with the given JSON parameters.
  pub async fn execute(&self, name: &str, params: Value) -> Result<ToolOutput, ToolError> {
    let tool = self.tools.get(name).ok_or_else(|| ToolError::NotFound {
      name: name.to_string(),
    })?;
    tool.execute(params).await
  }
}

impl Default for ToolRegistry {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{ToolDefinition, ToolMetadata, ToolOutput, ToolPermissionSet, ToolSource};
  use async_trait::async_trait;
  use serde_json::json;

  struct StaticTool {
    name: &'static str,
    metadata: ToolMetadata,
  }

  #[async_trait]
  impl Tool for StaticTool {
    fn name(&self) -> &str {
      self.name
    }

    fn description(&self) -> &str {
      "static test tool"
    }

    fn parameters_schema(&self) -> Value {
      json!({"type": "object"})
    }

    fn metadata(&self) -> ToolMetadata {
      self.metadata.clone()
    }

    async fn execute(&self, _params: Value) -> Result<ToolOutput, ToolError> {
      Ok(ToolOutput::success("ok"))
    }
  }

  #[test]
  fn registry_can_filter_tools_by_permission() {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(StaticTool {
      name: "http",
      metadata: ToolMetadata::builtin_named("http"),
    }));
    registry.register(Arc::new(StaticTool {
      name: "workflow",
      metadata: ToolMetadata::workflow(),
    }));

    let network_tools = registry.list_by_permission(ToolPermission::Network);
    assert_eq!(network_tools.len(), 1);
    assert_eq!(network_tools[0].name(), "http");
    assert!(registry.tool_has_permission("workflow", &ToolPermission::Workflow));
    assert!(!registry.tool_has_permission("workflow", &ToolPermission::Network));
  }

  #[test]
  fn tool_definition_serializes_permissions() {
    let definition = ToolDefinition {
      name: "mcp_demo_echo".to_string(),
      description: "demo".to_string(),
      parameters: json!({"type": "object"}),
      metadata: ToolMetadata::mcp("demo", "echo"),
    };
    let value = serde_json::to_value(definition).unwrap();

    assert_eq!(value["metadata"]["source"], json!("mcp"));
    assert_eq!(
      value["metadata"]["permissions"]["permissions"],
      json!(["mcp", "network"])
    );
  }

  #[test]
  fn permission_sets_are_stable_and_deduplicated() {
    let set = ToolPermissionSet::new([
      ToolPermission::Network,
      ToolPermission::Network,
      ToolPermission::Mcp,
    ]);

    assert_eq!(
      set.permissions,
      vec![ToolPermission::Mcp, ToolPermission::Network]
    );
  }

  #[test]
  fn default_metadata_is_builtin_without_permissions() {
    let metadata = ToolMetadata::default();

    assert_eq!(metadata.source, ToolSource::Builtin);
    assert!(metadata.permissions.permissions.is_empty());
  }
}

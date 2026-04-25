use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::{Tool, ToolError, ToolOutput};

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

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::ToolError;

/// Result of a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
  /// The output content as a string (stdout, response body, etc.)
  pub content: String,
  /// Whether this output represents an error condition
  pub is_error: bool,
  /// Structured content parts returned by tools that support typed output.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub parts: Vec<ToolOutputPart>,
}

/// Typed output content returned by tools.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolOutputPart {
  Text {
    text: String,
  },
  Image {
    data: String,
    mime_type: String,
  },
  Resource {
    uri: String,
    mime_type: Option<String>,
    text: Option<String>,
  },
}

impl ToolOutput {
  pub fn success(content: impl Into<String>) -> Self {
    Self {
      content: content.into(),
      is_error: false,
      parts: Vec::new(),
    }
  }

  pub fn error(content: impl Into<String>) -> Self {
    Self {
      content: content.into(),
      is_error: true,
      parts: Vec::new(),
    }
  }

  pub fn success_parts(content: impl Into<String>, parts: Vec<ToolOutputPart>) -> Self {
    Self {
      content: content.into(),
      is_error: false,
      parts,
    }
  }

  pub fn error_parts(content: impl Into<String>, parts: Vec<ToolOutputPart>) -> Self {
    Self {
      content: content.into(),
      is_error: true,
      parts,
    }
  }

  pub fn with_parts(mut self, parts: Vec<ToolOutputPart>) -> Self {
    self.parts = parts;
    self
  }
}

/// A parsed tool call from an LLM response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
  /// The tool name to invoke
  pub tool: String,
  /// Parameters for the tool (matches the tool's JSON schema)
  pub params: Value,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn string_output_constructors_have_no_parts_by_default() {
    let output = ToolOutput::success("ok");

    assert_eq!(output.content, "ok");
    assert!(!output.is_error);
    assert!(output.parts.is_empty());
  }

  #[test]
  fn typed_output_preserves_parts_and_compatible_content() {
    let output = ToolOutput::success_parts(
      "hello",
      vec![ToolOutputPart::Text {
        text: "hello".to_string(),
      }],
    );

    assert_eq!(output.content, "hello");
    assert_eq!(
      output.parts,
      vec![ToolOutputPart::Text {
        text: "hello".to_string(),
      }]
    );
  }
}

/// OpenAI-compatible function definition for use in prompts or API calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
  pub name: String,
  pub description: String,
  pub parameters: Value,
  #[serde(default)]
  pub metadata: ToolMetadata,
}

/// Stable source classification for tools registered in AgentFlow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSource {
  Builtin,
  Script,
  Mcp,
  Workflow,
}

impl ToolSource {
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::Builtin => "builtin",
      Self::Script => "script",
      Self::Mcp => "mcp",
      Self::Workflow => "workflow",
    }
  }
}

/// Metadata used for inspection, tracing, and tool registry diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolMetadata {
  pub source: ToolSource,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub mcp_server_name: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub mcp_tool_name: Option<String>,
}

impl ToolMetadata {
  pub fn builtin() -> Self {
    Self {
      source: ToolSource::Builtin,
      mcp_server_name: None,
      mcp_tool_name: None,
    }
  }

  pub fn script() -> Self {
    Self {
      source: ToolSource::Script,
      mcp_server_name: None,
      mcp_tool_name: None,
    }
  }

  pub fn mcp(server_name: impl Into<String>, tool_name: impl Into<String>) -> Self {
    Self {
      source: ToolSource::Mcp,
      mcp_server_name: Some(server_name.into()),
      mcp_tool_name: Some(tool_name.into()),
    }
  }

  pub fn workflow() -> Self {
    Self {
      source: ToolSource::Workflow,
      mcp_server_name: None,
      mcp_tool_name: None,
    }
  }
}

impl Default for ToolMetadata {
  fn default() -> Self {
    Self::builtin()
  }
}

/// Core trait that every tool must implement
#[async_trait]
pub trait Tool: Send + Sync {
  /// Unique, machine-readable name (e.g. "shell", "file", "http")
  fn name(&self) -> &str;

  /// Human-readable description shown to the LLM
  fn description(&self) -> &str;

  /// JSON Schema for the tool's parameters object
  fn parameters_schema(&self) -> Value;

  /// Metadata for registry inspection and diagnostics.
  fn metadata(&self) -> ToolMetadata {
    ToolMetadata::builtin()
  }

  /// Execute the tool and return its output
  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError>;

  /// Convenience: build the full tool definition
  fn definition(&self) -> ToolDefinition {
    ToolDefinition {
      name: self.name().to_string(),
      description: self.description().to_string(),
      parameters: self.parameters_schema(),
      metadata: self.metadata(),
    }
  }

  /// Format the definition as a compact string for LLM system prompts
  fn prompt_description(&self) -> String {
    format!(
      "- **{}**: {}\n  Parameters: {}",
      self.name(),
      self.description(),
      serde_json::to_string(&self.parameters_schema()).unwrap_or_else(|_| "{}".to_string())
    )
  }
}

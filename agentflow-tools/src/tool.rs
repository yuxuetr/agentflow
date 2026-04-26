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

  /// Execute the tool and return its output
  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError>;

  /// Convenience: build the full tool definition
  fn definition(&self) -> ToolDefinition {
    ToolDefinition {
      name: self.name().to_string(),
      description: self.description().to_string(),
      parameters: self.parameters_schema(),
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

//! Tool calling interface
//!
//! This module provides the interface for listing and calling tools on MCP servers.

use crate::error::{JsonRpcErrorCode, MCPError, MCPResult, ResultExt};
use crate::protocol::types::{JsonRpcRequest, JsonRpcResponse};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::MCPClient;

/// Tool definition from server
///
/// Describes a tool that can be called on the server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
  /// Tool name (must be unique)
  pub name: String,
  /// Human-readable description
  #[serde(skip_serializing_if = "Option::is_none")]
  pub description: Option<String>,
  /// JSON Schema for input parameters
  pub input_schema: Value,
}

/// Content type in tool results
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Content {
  /// Text content
  Text {
    /// The text content
    text: String,
  },
  /// Image content
  Image {
    /// Image data (base64 or URL)
    data: String,
    /// MIME type (e.g., "image/png")
    #[serde(rename = "mimeType")]
    mime_type: String,
  },
  /// Resource reference
  Resource {
    /// Resource URI
    uri: String,
    /// Optional MIME type
    #[serde(skip_serializing_if = "Option::is_none", rename = "mimeType")]
    mime_type: Option<String>,
    /// Optional text content
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
  },
}

impl Content {
  /// Create text content
  pub fn text(text: impl Into<String>) -> Self {
    Self::Text {
      text: text.into(),
    }
  }

  /// Create image content
  pub fn image(data: impl Into<String>, mime_type: impl Into<String>) -> Self {
    Self::Image {
      data: data.into(),
      mime_type: mime_type.into(),
    }
  }

  /// Create resource content
  pub fn resource(uri: impl Into<String>) -> Self {
    Self::Resource {
      uri: uri.into(),
      mime_type: None,
      text: None,
    }
  }

  /// Create resource content with MIME type
  pub fn resource_with_type(uri: impl Into<String>, mime_type: impl Into<String>) -> Self {
    Self::Resource {
      uri: uri.into(),
      mime_type: Some(mime_type.into()),
      text: None,
    }
  }
}

/// Tool call result from server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
  /// Result content (array of Content)
  #[serde(default)]
  pub content: Vec<Content>,
  /// Whether the call resulted in an error
  #[serde(skip_serializing_if = "Option::is_none")]
  pub is_error: Option<bool>,
}

impl CallToolResult {
  /// Check if the result is an error
  pub fn is_error(&self) -> bool {
    self.is_error.unwrap_or(false)
  }

  /// Get text content from result
  pub fn text_content(&self) -> Vec<&str> {
    self
      .content
      .iter()
      .filter_map(|c| match c {
        Content::Text { text } => Some(text.as_str()),
        _ => None,
      })
      .collect()
  }

  /// Get first text content
  pub fn first_text(&self) -> Option<&str> {
    self.text_content().first().copied()
  }
}

/// Tool calling methods for MCPClient
impl MCPClient {
  /// List available tools from the server
  ///
  /// # Errors
  ///
  /// Returns an error if:
  /// - Client is not connected
  /// - Request fails
  /// - Server returns an error
  ///
  /// # Example
  ///
  /// ```no_run
  /// # use agentflow_mcp::client::ClientBuilder;
  /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
  /// let mut client = ClientBuilder::new()
  ///   .with_stdio(vec!["node".to_string(), "server.js".to_string()])
  ///   .build()
  ///   .await?;
  ///
  /// client.connect().await?;
  /// let tools = client.list_tools().await?;
  ///
  /// for tool in tools {
  ///   println!("Tool: {} - {:?}", tool.name, tool.description);
  /// }
  /// # Ok(())
  /// # }
  /// ```
  pub async fn list_tools(&mut self) -> MCPResult<Vec<Tool>> {
    // Check connection
    if !self.is_connected().await {
      return Err(MCPError::connection("Client is not connected"));
    }

    // Build request
    let request = JsonRpcRequest::new(self.next_request_id(), "tools/list", None);

    // Send request
    let response = self
      .send_request(request)
      .await
      .context("Failed to send tools/list request")?;

    // Parse response
    let response: JsonRpcResponse = serde_json::from_value(response)
      .map_err(|e| MCPError::from(e).context("Failed to parse tools/list response"))?;

    // Check for errors
    if let Some(error) = response.error {
      return Err(MCPError::protocol(
        format!("tools/list failed: {} - {}", error.code, error.message),
        JsonRpcErrorCode::InternalError,
      ));
    }

    // Parse result
    let result = response
      .result
      .ok_or_else(|| MCPError::protocol("Missing result in tools/list response", JsonRpcErrorCode::InvalidRequest))?;

    // Extract tools array
    let tools_array = result
      .get("tools")
      .and_then(|v| v.as_array())
      .ok_or_else(|| MCPError::protocol("Missing or invalid 'tools' field in response", JsonRpcErrorCode::InvalidRequest))?;

    // Parse tools
    let tools: Vec<Tool> = tools_array
      .iter()
      .map(|v| {
        serde_json::from_value(v.clone())
          .map_err(|e| MCPError::from(e).context("Failed to parse tool definition"))
      })
      .collect::<MCPResult<Vec<Tool>>>()?;

    Ok(tools)
  }

  /// Call a tool on the server
  ///
  /// # Arguments
  ///
  /// * `name` - Tool name
  /// * `arguments` - Tool arguments (must match tool's input schema)
  ///
  /// # Errors
  ///
  /// Returns an error if:
  /// - Client is not connected
  /// - Tool does not exist
  /// - Arguments are invalid
  /// - Tool execution fails
  ///
  /// # Example
  ///
  /// ```no_run
  /// # use agentflow_mcp::client::ClientBuilder;
  /// # use serde_json::json;
  /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
  /// # let mut client = ClientBuilder::new()
  /// #   .with_stdio(vec!["node".to_string(), "server.js".to_string()])
  /// #   .build().await?;
  /// # client.connect().await?;
  /// let result = client.call_tool(
  ///   "add_numbers",
  ///   json!({"a": 5, "b": 3})
  /// ).await?;
  ///
  /// if let Some(text) = result.first_text() {
  ///   println!("Result: {}", text);
  /// }
  /// # Ok(())
  /// # }
  /// ```
  pub async fn call_tool(
    &mut self,
    name: impl Into<String>,
    arguments: Value,
  ) -> MCPResult<CallToolResult> {
    let name = name.into();

    // Check connection
    if !self.is_connected().await {
      return Err(MCPError::connection("Client is not connected"));
    }

    // Build params
    let params = serde_json::json!({
      "name": name,
      "arguments": arguments
    });

    // Build request
    let request = JsonRpcRequest::new(self.next_request_id(), "tools/call", Some(params));

    // Send request
    let response = self
      .send_request(request)
      .await
      .context(format!("Failed to send tools/call request for '{}'", name))?;

    // Parse response
    let response: JsonRpcResponse = serde_json::from_value(response).map_err(|e| {
      MCPError::from(e).context(format!("Failed to parse tools/call response for '{}'", name))
    })?;

    // Check for errors
    if let Some(error) = response.error {
      return Err(MCPError::tool(
        format!("Tool '{}' execution failed: {} - {}", name, error.code, error.message),
        Some(name),
      ));
    }

    // Parse result
    let result = response.result.ok_or_else(|| {
      MCPError::protocol(
        format!("Missing result in tools/call response for '{}'", name),
        JsonRpcErrorCode::InvalidRequest,
      )
    })?;

    // Parse tool result
    let tool_result: CallToolResult = serde_json::from_value(result).map_err(|e| {
      MCPError::from(e).context(format!("Failed to parse tool result for '{}'", name))
    })?;

    Ok(tool_result)
  }

  /// Call a tool with JSON Schema validation
  ///
  /// This validates the arguments against the tool's input schema before calling.
  ///
  /// # Arguments
  ///
  /// * `tool` - Tool definition (from list_tools)
  /// * `arguments` - Tool arguments
  ///
  /// # Errors
  ///
  /// Returns an error if:
  /// - Arguments don't match schema
  /// - Tool call fails
  ///
  /// # Example
  ///
  /// ```no_run
  /// # use agentflow_mcp::client::ClientBuilder;
  /// # use serde_json::json;
  /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
  /// # let mut client = ClientBuilder::new()
  /// #   .with_stdio(vec!["node".to_string(), "server.js".to_string()])
  /// #   .build().await?;
  /// # client.connect().await?;
  /// let tools = client.list_tools().await?;
  /// let tool = tools.iter().find(|t| t.name == "add_numbers").unwrap();
  ///
  /// let result = client.call_tool_validated(
  ///   tool,
  ///   json!({"a": 5, "b": 3})
  /// ).await?;
  /// # Ok(())
  /// # }
  /// ```
  pub async fn call_tool_validated(
    &mut self,
    tool: &Tool,
    arguments: Value,
  ) -> MCPResult<CallToolResult> {
    // Validate arguments against schema
    self
      .validate_tool_arguments(tool, &arguments)
      .context(format!("Validation failed for tool '{}'", tool.name))?;

    // Call tool
    self.call_tool(&tool.name, arguments).await
  }

  /// Validate tool arguments against JSON Schema
  ///
  /// This performs basic validation. For production use, consider using
  /// a full JSON Schema validator like `jsonschema` crate.
  fn validate_tool_arguments(&self, tool: &Tool, arguments: &Value) -> MCPResult<()> {
    // Basic validation: check if arguments is an object
    if !arguments.is_object() {
      return Err(MCPError::validation(
        format!(
          "Tool '{}' expects arguments to be an object, got {}",
          tool.name,
          arguments.type_name()
        ),
        None,
      ));
    }

    // TODO: Implement full JSON Schema validation
    // For now, we rely on the server to validate
    Ok(())
  }
}

/// Extension trait for Value to get type name
trait ValueExt {
  fn type_name(&self) -> &'static str;
}

impl ValueExt for Value {
  fn type_name(&self) -> &'static str {
    match self {
      Value::Null => "null",
      Value::Bool(_) => "boolean",
      Value::Number(_) => "number",
      Value::String(_) => "string",
      Value::Array(_) => "array",
      Value::Object(_) => "object",
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_tool_deserialization() {
    let json = serde_json::json!({
      "name": "test_tool",
      "description": "A test tool",
      "inputSchema": {
        "type": "object",
        "properties": {
          "param": { "type": "string" }
        }
      }
    });

    let tool: Tool = serde_json::from_value(json).unwrap();
    assert_eq!(tool.name, "test_tool");
    assert_eq!(tool.description, Some("A test tool".to_string()));
  }

  #[test]
  fn test_content_text() {
    let content = Content::text("Hello, world!");
    match content {
      Content::Text { text } => assert_eq!(text, "Hello, world!"),
      _ => panic!("Expected Text content"),
    }
  }

  #[test]
  fn test_content_image() {
    let content = Content::image("base64data", "image/png");
    match content {
      Content::Image { data, mime_type } => {
        assert_eq!(data, "base64data");
        assert_eq!(mime_type, "image/png");
      }
      _ => panic!("Expected Image content"),
    }
  }

  #[test]
  fn test_content_resource() {
    let content = Content::resource("file:///path/to/file.txt");
    match content {
      Content::Resource { uri, .. } => {
        assert_eq!(uri, "file:///path/to/file.txt");
      }
      _ => panic!("Expected Resource content"),
    }
  }

  #[test]
  fn test_call_tool_result_text_content() {
    let result = CallToolResult {
      content: vec![
        Content::text("First"),
        Content::text("Second"),
        Content::image("data", "image/png"),
      ],
      is_error: None,
    };

    let text_content = result.text_content();
    assert_eq!(text_content.len(), 2);
    assert_eq!(text_content[0], "First");
    assert_eq!(text_content[1], "Second");
  }

  #[test]
  fn test_call_tool_result_first_text() {
    let result = CallToolResult {
      content: vec![Content::text("Hello")],
      is_error: None,
    };

    assert_eq!(result.first_text(), Some("Hello"));
  }

  #[test]
  fn test_call_tool_result_is_error() {
    let result1 = CallToolResult {
      content: vec![],
      is_error: Some(true),
    };
    assert!(result1.is_error());

    let result2 = CallToolResult {
      content: vec![],
      is_error: None,
    };
    assert!(!result2.is_error());
  }
}

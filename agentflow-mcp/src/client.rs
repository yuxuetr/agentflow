//! MCP client implementation for AgentFlow

use crate::error::{MCPError, MCPResult};
use crate::tools::{ToolCall, ToolDefinition, ToolResult};
use crate::transport::{Transport, TransportClient};
use serde_json::{json, Value};
use uuid::Uuid;

/// MCP client for calling tools on external servers
pub struct MCPClient {
  transport: TransportClient,
  #[allow(dead_code)]
  session_id: String,
}

impl MCPClient {
  /// Create a new MCP client with the given transport
  pub fn new(transport: Transport) -> Self {
    Self {
      transport: TransportClient::new(transport),
      session_id: Uuid::new_v4().to_string(),
    }
  }

  /// Create a stdio-based MCP client
  pub fn stdio(command: Vec<String>) -> Self {
    Self::new(Transport::stdio(command))
  }

  /// Create an HTTP-based MCP client
  pub fn http<S: Into<String>>(base_url: S) -> Self {
    Self::new(Transport::http(base_url))
  }

  /// Connect to the MCP server
  pub async fn connect(&mut self) -> MCPResult<()> {
    self.transport.connect().await?;
    self.initialize().await?;
    Ok(())
  }

  /// Initialize the MCP session
  async fn initialize(&mut self) -> MCPResult<()> {
    let init_request = json!({
        "jsonrpc": "2.0",
        "id": self.next_request_id(),
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "clientInfo": {
                "name": "agentflow-mcp",
                "version": "0.1.0"
            }
        }
    });

    let response = self.transport.send_message(init_request).await?;

    if response.get("error").is_some() {
      return Err(MCPError::Protocol {
        message: format!("Initialization failed: {:?}", response["error"]),
      });
    }

    // Send initialized notification
    let initialized_notification = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });

    self
      .transport
      .send_message(initialized_notification)
      .await?;
    Ok(())
  }

  /// List available tools from the server
  pub async fn list_tools(&mut self) -> MCPResult<Vec<ToolDefinition>> {
    let request = json!({
        "jsonrpc": "2.0",
        "id": self.next_request_id(),
        "method": "tools/list"
    });

    let response = self.transport.send_message(request).await?;

    if let Some(error) = response.get("error") {
      return Err(MCPError::Protocol {
        message: format!("Failed to list tools: {:?}", error),
      });
    }

    let tools = response["result"]["tools"]
      .as_array()
      .ok_or_else(|| MCPError::Protocol {
        message: "Invalid tools list response".to_string(),
      })?;

    let mut tool_definitions = Vec::new();
    for tool in tools {
      let definition: ToolDefinition = serde_json::from_value(tool.clone())?;
      tool_definitions.push(definition);
    }

    Ok(tool_definitions)
  }

  /// Call a tool on the server
  pub async fn call_tool(&mut self, tool_call: ToolCall) -> MCPResult<ToolResult> {
    let request = json!({
        "jsonrpc": "2.0",
        "id": self.next_request_id(),
        "method": "tools/call",
        "params": {
            "name": tool_call.name,
            "arguments": tool_call.parameters
        }
    });

    let response = self.transport.send_message(request).await?;

    if let Some(error) = response.get("error") {
      return Err(MCPError::ToolExecution {
        message: format!("Tool call failed: {:?}", error),
      });
    }

    let result: ToolResult = serde_json::from_value(response["result"].clone())?;
    Ok(result)
  }

  /// Call a tool by name with parameters
  pub async fn call_tool_simple<S: Into<String>>(
    &mut self,
    name: S,
    parameters: Value,
  ) -> MCPResult<ToolResult> {
    let tool_call = ToolCall::new(name, parameters);
    self.call_tool(tool_call).await
  }

  /// Disconnect from the server
  pub async fn disconnect(&mut self) -> MCPResult<()> {
    self.transport.disconnect().await
  }

  fn next_request_id(&self) -> String {
    Uuid::new_v4().to_string()
  }
}

impl Drop for MCPClient {
  fn drop(&mut self) {
    // Transport client handles cleanup in its own Drop implementation
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  

  #[tokio::test]
  async fn test_client_creation() {
    let client = MCPClient::stdio(vec!["echo".to_string(), "test".to_string()]);
    assert!(!client.session_id.is_empty());
  }

  #[tokio::test]
  async fn test_tool_call_creation() {
    let call = ToolCall::new("test_tool", json!({"param": "value"}));
    assert_eq!(call.name, "test_tool");
    assert_eq!(call.parameters["param"], "value");
  }
}

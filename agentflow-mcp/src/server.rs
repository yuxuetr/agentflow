//! MCP server implementation for exposing AgentFlow capabilities

use crate::error::{MCPError, MCPResult};
use crate::tools::{ToolCall, ToolDefinition, ToolResult};
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Handler trait for MCP server implementations (simplified for now)
pub trait MCPServerHandler: Send + Sync {
  /// List available tools
  fn list_tools(&self) -> Vec<ToolDefinition>;

  /// Execute a tool call (synchronous for simplicity)
  fn call_tool(&self, tool_call: ToolCall) -> MCPResult<ToolResult>;

  /// Get server capabilities
  fn get_capabilities(&self) -> Value {
    json!({
        "tools": {}
    })
  }

  /// Get server information
  fn get_server_info(&self) -> Value {
    json!({
        "name": "agentflow-mcp-server",
        "version": "0.1.0"
    })
  }
}

/// MCP server for exposing AgentFlow functionality
pub struct MCPServer {
  handler: Box<dyn MCPServerHandler>,
}

impl MCPServer {
  pub fn new(handler: Box<dyn MCPServerHandler>) -> Self {
    Self { handler }
  }

  /// Run the server using stdio transport
  pub async fn run_stdio(&self) -> MCPResult<()> {
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
      line.clear();
      let bytes_read = reader.read_line(&mut line).await?;

      if bytes_read == 0 {
        break; // EOF
      }

      let request: Value = match serde_json::from_str(line.trim()) {
        Ok(req) => req,
        Err(e) => {
          tracing::error!("Failed to parse request: {}", e);
          continue;
        }
      };

      let response = self.handle_request(request).await;

      match response {
        Ok(Some(resp)) => {
          let response_str = serde_json::to_string(&resp)?;
          stdout.write_all(response_str.as_bytes()).await?;
          stdout.write_all(b"\n").await?;
          stdout.flush().await?;
        }
        Ok(None) => {
          // No response needed (notification)
        }
        Err(e) => {
          tracing::error!("Error handling request: {}", e);
        }
      }
    }

    Ok(())
  }

  /// Handle an MCP request
  async fn handle_request(&self, request: Value) -> MCPResult<Option<Value>> {
    let method = request["method"]
      .as_str()
      .ok_or_else(|| MCPError::Protocol {
        message: "Missing method in request".to_string(),
      })?;

    let id = request.get("id");

    match method {
      "initialize" => {
        let response = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": self.handler.get_capabilities(),
                "serverInfo": self.handler.get_server_info()
            }
        });
        Ok(Some(response))
      }

      "notifications/initialized" => {
        // Initialization complete notification - no response needed
        Ok(None)
      }

      "tools/list" => {
        let tools = self.handler.list_tools();
        let response = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "tools": tools
            }
        });
        Ok(Some(response))
      }

      "tools/call" => {
        let params = request["params"].clone();
        let tool_call: ToolCall = serde_json::from_value(params)?;

        match self.handler.call_tool(tool_call) {
          Ok(result) => {
            let response = json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result
            });
            Ok(Some(response))
          }
          Err(e) => {
            let error_response = json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32603,
                    "message": format!("Tool execution failed: {}", e)
                }
            });
            Ok(Some(error_response))
          }
        }
      }

      _ => {
        let error_response = json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": format!("Method not found: {}", method)
            }
        });
        Ok(Some(error_response))
      }
    }
  }
}

/// Example server handler for AgentFlow workflows
pub struct AgentFlowServerHandler {
  tools: HashMap<String, ToolDefinition>,
}

impl AgentFlowServerHandler {
  pub fn new() -> Self {
    let mut tools = HashMap::new();

    // Example: Workflow execution tool
    tools.insert(
      "run_workflow".to_string(),
      ToolDefinition {
        name: "run_workflow".to_string(),
        description: "Execute an AgentFlow workflow".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "workflow_path": {
                    "type": "string",
                    "description": "Path to the workflow YAML file"
                },
                "inputs": {
                    "type": "object",
                    "description": "Input parameters for the workflow"
                }
            },
            "required": ["workflow_path"]
        }),
      },
    );

    Self { tools }
  }
}

impl MCPServerHandler for AgentFlowServerHandler {
  fn list_tools(&self) -> Vec<ToolDefinition> {
    self.tools.values().cloned().collect()
  }

  fn call_tool(&self, tool_call: ToolCall) -> MCPResult<ToolResult> {
    match tool_call.name.as_str() {
      "run_workflow" => {
        // This would integrate with the actual AgentFlow workflow runner
        let workflow_path = tool_call.parameters["workflow_path"]
          .as_str()
          .ok_or_else(|| MCPError::ToolExecution {
            message: "Missing workflow_path parameter".to_string(),
          })?;

        // Placeholder implementation
        let result = ToolResult::success(vec![crate::tools::ToolContent::Text {
          text: format!("Would execute workflow: {}", workflow_path),
        }]);

        Ok(result)
      }
      _ => Err(MCPError::ToolExecution {
        message: format!("Unknown tool: {}", tool_call.name),
      }),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_handler_creation() {
    let handler = AgentFlowServerHandler::new();
    let tools = handler.list_tools();
    assert!(!tools.is_empty());
    assert_eq!(tools[0].name, "run_workflow");
  }
}

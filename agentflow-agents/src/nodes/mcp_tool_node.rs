//! Generic MCP Tool Node - Execute tools via Model Context Protocol
//!
//! **Status**: Experimental - MCP integration is in early development stage
//!
//! This node allows workflow integration with Model Context Protocol (MCP) servers
//! to execute external tools and access resources.

use agentflow_core::{AsyncNode, AsyncNodeInputs, AsyncNodeResult, AgentFlowError, FlowValue};
use agentflow_mcp::{MCPClient, ToolCall};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Generic MCP tool execution node
///
/// # Example
/// ```yaml
/// nodes:
///   - type: mcp_tool
///     name: search_docs
///     tool_name: "search"
///     server_command: ["node", "mcp-server.js"]
///     parameters:
///       query: "{{ search_query }}"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPToolNode {
    /// Name of the tool to call
    pub tool_name: String,
    /// MCP server command (for stdio transport)
    pub server_command: Vec<String>,
    /// Static parameters for the tool call
    pub parameters: Option<Value>,
    /// Input mappings: maps parameter names to input keys
    pub input_mapping: Option<HashMap<String, String>>,
}

impl MCPToolNode {
    /// Create a new MCP tool node
    pub fn new<S: Into<String>>(tool_name: S, server_command: Vec<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            server_command,
            parameters: None,
            input_mapping: None,
        }
    }

    /// Set static parameters
    pub fn with_parameters(mut self, parameters: Value) -> Self {
        self.parameters = Some(parameters);
        self
    }

    /// Set input mapping (maps parameter names to input keys)
    pub fn with_input_mapping(mut self, mapping: HashMap<String, String>) -> Self {
        self.input_mapping = Some(mapping);
        self
    }

    /// Resolve parameters by combining static parameters with dynamic inputs
    fn resolve_parameters(&self, inputs: &AsyncNodeInputs) -> Result<Value, AgentFlowError> {
        let mut resolved_params = self.parameters.clone().unwrap_or_else(|| json!({}));

        // Apply input mappings
        if let Some(mappings) = &self.input_mapping {
            for (param_name, input_key) in mappings {
                if let Some(input_value) = inputs.get(input_key) {
                    // Convert FlowValue to JSON Value
                    let json_value = match input_value {
                        FlowValue::Json(v) => v.clone(),
                        FlowValue::File(path) => json!({ "file_path": path }),
                        FlowValue::Url(url) => json!({ "url": url }),
                    };

                    if let Some(obj) = resolved_params.as_object_mut() {
                        obj.insert(param_name.clone(), json_value);
                    }
                }
            }
        }

        Ok(resolved_params)
    }
}

#[async_trait]
impl AsyncNode for MCPToolNode {
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        println!("🔧 MCPToolNode: Executing tool '{}'", self.tool_name);

        // Resolve parameters from inputs
        let parameters = self.resolve_parameters(inputs)?;

        // Create and connect to MCP client
        let mut client = MCPClient::stdio(self.server_command.clone());

        client.connect().await.map_err(|e| AgentFlowError::ExecutionError {
            message: format!("Failed to connect to MCP server: {}", e),
        })?;

        // Execute tool call
        let tool_call = ToolCall::new(&self.tool_name, parameters);
        let result = client.call_tool(tool_call).await.map_err(|e| {
            AgentFlowError::ExecutionError {
                message: format!("MCP tool call '{}' failed: {}", self.tool_name, e),
            }
        })?;

        // Disconnect from server
        client.disconnect().await.map_err(|e| AgentFlowError::ExecutionError {
            message: format!("Failed to disconnect from MCP server: {}", e),
        })?;

        println!("✅ MCP tool '{}' executed successfully", self.tool_name);

        // Extract result content
        let mut result_content = json!({});

        if let Some(text_content) = result.get_text() {
            result_content["text"] = json!(text_content);
        }

        // Include all content types
        result_content["content"] = serde_json::to_value(&result.content)
            .unwrap_or_else(|_| json!([]));
        result_content["is_error"] = json!(result.is_error.unwrap_or(false));

        // Return output as FlowValue
        let mut outputs = HashMap::new();
        outputs.insert(
            "result".to_string(),
            FlowValue::Json(json!({
                "tool_name": self.tool_name,
                "tool_result": result_content,
            })),
        );

        Ok(outputs)
    }
}
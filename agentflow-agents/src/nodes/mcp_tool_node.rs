//! Generic MCP Tool Node - Execute tools via Model Context Protocol

use agentflow_core::{AsyncNode, SharedState, AgentFlowError};
use agentflow_mcp::{MCPClient, ToolCall};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

/// Generic MCP tool execution node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPToolNode {
    /// Name of the tool to call
    pub tool_name: String,
    /// MCP server command (for stdio transport)
    pub server_command: Vec<String>,
    /// Static parameters for the tool call
    pub parameters: Option<Value>,
    /// Template parameters that will be resolved from shared state
    pub parameter_templates: Option<HashMap<String, String>>,
    /// Node identifier
    pub node_id: Option<String>,
}

impl MCPToolNode {
    /// Create a new MCP tool node
    pub fn new<S: Into<String>>(tool_name: S, server_command: Vec<String>) -> Self {
        Self {
            tool_name: tool_name.into(),
            server_command,
            parameters: None,
            parameter_templates: None,
            node_id: None,
        }
    }

    /// Set static parameters
    pub fn with_parameters(mut self, parameters: Value) -> Self {
        self.parameters = Some(parameters);
        self
    }

    /// Set parameter templates that resolve from shared state
    pub fn with_parameter_templates(mut self, templates: HashMap<String, String>) -> Self {
        self.parameter_templates = Some(templates);
        self
    }

    /// Set node identifier
    pub fn with_node_id<S: Into<String>>(mut self, id: S) -> Self {
        self.node_id = Some(id.into());
        self
    }

    /// Resolve template parameters using shared state
    fn resolve_parameters(&self, shared: &SharedState) -> Result<Value, AgentFlowError> {
        let mut resolved_params = self.parameters.clone().unwrap_or_else(|| json!({}));

        if let Some(templates) = &self.parameter_templates {
            for (param_key, template) in templates {
                let resolved_value = self.resolve_template(template, shared)?;
                if let Some(obj) = resolved_params.as_object_mut() {
                    obj.insert(param_key.clone(), resolved_value);
                }
            }
        }

        Ok(resolved_params)
    }

    /// Resolve a single template string
    fn resolve_template(&self, template: &str, shared: &SharedState) -> Result<Value, AgentFlowError> {
        // Simple template resolution: {{key}} -> shared.get("key")
        if template.starts_with("{{") && template.ends_with("}}") {
            let key = template.trim_start_matches("{{").trim_end_matches("}}").trim();
            
            if let Some(value) = shared.get(key) {
                // Extract nested value if needed (e.g., summary.text)
                if let Some(dot_pos) = key.find('.') {
                    let (main_key, sub_key) = key.split_at(dot_pos);
                    let sub_key = &sub_key[1..]; // Remove the dot
                    
                    if let Some(main_value) = shared.get(main_key) {
                        if let Some(sub_value) = main_value.get(sub_key) {
                            return Ok(sub_value.clone());
                        }
                    }
                }
                
                Ok(value.clone())
            } else {
                Err(AgentFlowError::AsyncExecutionError {
                    message: format!("Template parameter '{}' not found in shared state", key),
                })
            }
        } else {
            // Literal value
            Ok(json!(template))
        }
    }
}

#[async_trait]
impl AsyncNode for MCPToolNode {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value, AgentFlowError> {
        let resolved_params = self.resolve_parameters(shared)?;
        
        Ok(json!({
            "tool_name": self.tool_name,
            "server_command": self.server_command,
            "parameters": resolved_params
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value, AgentFlowError> {
        let tool_name = prep_result["tool_name"].as_str().unwrap();
        let server_command: Vec<String> = serde_json::from_value(
            prep_result["server_command"].clone()
        ).map_err(|e| AgentFlowError::AsyncExecutionError {
            message: format!("Invalid server command: {}", e),
        })?;
        let parameters = prep_result["parameters"].clone();

        println!("🔧 Executing MCP tool: {} via {:?}", tool_name, server_command);

        // Create and connect to MCP client
        let mut client = MCPClient::stdio(server_command);
        client.connect().await.map_err(|e| AgentFlowError::AsyncExecutionError {
            message: format!("Failed to connect to MCP server: {}", e),
        })?;

        // Execute tool call
        let tool_call = ToolCall::new(tool_name, parameters);
        let result = client.call_tool(tool_call).await.map_err(|e| {
            AgentFlowError::AsyncExecutionError {
                message: format!("MCP tool call failed: {}", e),
            }
        })?;

        // Disconnect from server
        client.disconnect().await.map_err(|e| AgentFlowError::AsyncExecutionError {
            message: format!("Failed to disconnect from MCP server: {}", e),
        })?;

        println!("✅ MCP tool '{}' executed successfully", tool_name);

        // Extract result content
        let mut result_content = json!({});
        
        if let Some(text_content) = result.get_text() {
            result_content["text"] = json!(text_content);
        }

        // Include all content types
        result_content["content"] = serde_json::to_value(&result.content).unwrap_or_else(|_| json!([]));
        result_content["is_error"] = json!(result.is_error.unwrap_or(false));

        Ok(json!({
            "tool_result": result_content,
            "tool_name": tool_name
        }))
    }

    async fn post_async(
        &self,
        shared: &SharedState,
        _prep_result: Value,
        exec_result: Value,
    ) -> Result<Option<String>, AgentFlowError> {
        // Store result in shared state
        let node_key = self.node_id.as_deref().unwrap_or(&self.tool_name);
        shared.insert(node_key.to_string(), exec_result);

        println!("🔧 MCPToolNode: Stored result for '{}' in shared state", node_key);

        // No automatic next node - let workflow configuration handle routing
        Ok(None)
    }

    fn get_node_id(&self) -> Option<String> {
        self.node_id.clone().or_else(|| Some(format!("mcp_{}", self.tool_name)))
    }
}
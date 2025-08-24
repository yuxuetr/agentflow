//! Tool definitions and utilities for MCP integration

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Represents an MCP tool definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// Tool call request  
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub parameters: Value,
}

/// Tool call result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub content: Vec<ToolContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// Tool content types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    #[serde(rename = "resource")]
    Resource { resource: ResourceReference },
}

/// Resource reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceReference {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// Registry for known MCP tools and their configurations
#[derive(Debug, Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, ToolConfiguration>,
}

/// Configuration for a specific tool/server combination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfiguration {
    pub tool_name: String,
    pub server_command: Vec<String>,
    pub description: String,
    pub input_schema: Value,
    pub default_parameters: Option<Value>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a new tool configuration
    pub fn register_tool(&mut self, key: String, config: ToolConfiguration) {
        self.tools.insert(key, config);
    }

    /// Get a tool configuration
    pub fn get_tool(&self, key: &str) -> Option<&ToolConfiguration> {
        self.tools.get(key)
    }

    /// List all registered tools
    pub fn list_tools(&self) -> Vec<&String> {
        self.tools.keys().collect()
    }

    /// Create default registry with common tools
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        
        // MarkMap tool
        registry.register_tool(
            "markmap".to_string(),
            ToolConfiguration {
                tool_name: "markdown-to-mindmap".to_string(),
                server_command: vec![
                    "npx".to_string(),
                    "-y".to_string(), 
                    "@jinzcdev/markmap-mcp-server".to_string(),
                ],
                description: "Convert Markdown to interactive mind map".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "markdown": {
                            "type": "string",
                            "description": "Markdown content to convert"
                        },
                        "open": {
                            "type": "boolean", 
                            "description": "Auto-open in browser",
                            "default": false
                        }
                    },
                    "required": ["markdown"]
                }),
                default_parameters: Some(serde_json::json!({
                    "open": false
                })),
            },
        );

        registry
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Helper functions for working with tools
impl ToolCall {
    pub fn new<S: Into<String>>(name: S, parameters: Value) -> Self {
        Self {
            name: name.into(),
            parameters,
        }
    }

    /// Merge default parameters with provided parameters
    pub fn with_defaults(mut self, defaults: &Value) -> Self {
        if let (Some(params), Some(defaults)) = (self.parameters.as_object_mut(), defaults.as_object()) {
            for (key, default_value) in defaults {
                if !params.contains_key(key) {
                    params.insert(key.clone(), default_value.clone());
                }
            }
        }
        self
    }
}

impl ToolResult {
    pub fn success(content: Vec<ToolContent>) -> Self {
        Self {
            content,
            is_error: None,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            content: vec![ToolContent::Text { text: message }],
            is_error: Some(true),
        }
    }

    /// Extract text content from result
    pub fn get_text(&self) -> Option<String> {
        self.content
            .iter()
            .find_map(|content| match content {
                ToolContent::Text { text } => Some(text.clone()),
                _ => None,
            })
    }
}
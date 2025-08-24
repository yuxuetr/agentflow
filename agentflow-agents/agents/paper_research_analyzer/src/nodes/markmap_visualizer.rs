//! MarkMap Visualizer Node - Convert mind map markdown to visual mind map using MCP

use agentflow_agents::{AsyncNode, SharedState, AgentFlowError};
use agentflow_mcp::{MCPClient, ToolCall};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::Path;

pub struct MarkMapVisualizerNode {
    export_format: String,  // "png", "svg", "html"
    auto_open: bool,
    output_dir: Option<String>,
}

impl MarkMapVisualizerNode {
    pub fn new(export_format: String) -> Self {
        Self {
            export_format,
            auto_open: false,
            output_dir: None,
        }
    }

    pub fn with_auto_open(mut self, auto_open: bool) -> Self {
        self.auto_open = auto_open;
        self
    }

    pub fn with_output_dir<S: Into<String>>(mut self, output_dir: S) -> Self {
        self.output_dir = Some(output_dir.into());
        self
    }
}

#[async_trait]
impl AsyncNode for MarkMapVisualizerNode {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value, AgentFlowError> {
        // Get mind map markdown from shared state
        let mind_map_data = shared.get("mind_map").ok_or_else(|| 
            AgentFlowError::AsyncExecutionError { 
                message: "Mind map not available in shared state".to_string() 
            }
        )?;

        // Extract markdown content
        let mind_map_md = if let Some(md) = mind_map_data.get("mind_map") {
            md.as_str().unwrap_or("")
        } else {
            mind_map_data.as_str().unwrap_or("")
        };

        if mind_map_md.is_empty() {
            return Err(AgentFlowError::AsyncExecutionError {
                message: "Mind map markdown is empty".to_string(),
            });
        }

        Ok(json!({
            "mind_map_markdown": mind_map_md,
            "export_format": self.export_format,
            "auto_open": self.auto_open,
            "output_dir": self.output_dir
        }))
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value, AgentFlowError> {
        let mind_map_md = prep_result["mind_map_markdown"].as_str().unwrap();
        
        println!("🎨 Converting mind map to visual format: {}", self.export_format);

        // Create MCP client for MarkMap server
        let server_command = vec![
            "npx".to_string(),
            "-y".to_string(),
            "@jinzcdev/markmap-mcp-server".to_string(),
        ];

        let mut client = MCPClient::stdio(server_command);
        client.connect().await.map_err(|e| AgentFlowError::AsyncExecutionError {
            message: format!("Failed to connect to MarkMap MCP server: {}", e),
        })?;

        // Prepare tool call parameters
        let mut tool_params = json!({
            "markdown": mind_map_md,
            "open": self.auto_open
        });

        // Add export format if specified
        if !self.export_format.is_empty() && self.export_format != "html" {
            tool_params["export"] = json!(self.export_format);
        }

        // Execute the markdown-to-mindmap tool
        let tool_call = ToolCall::new("markdown-to-mindmap", tool_params);
        let result = client.call_tool(tool_call).await.map_err(|e| {
            AgentFlowError::AsyncExecutionError {
                message: format!("MarkMap tool call failed: {}", e),
            }
        })?;

        // Disconnect from server
        client.disconnect().await.map_err(|e| AgentFlowError::AsyncExecutionError {
            message: format!("Failed to disconnect from MarkMap server: {}", e),
        })?;

        // Extract file path from result
        let output_path = result.get_text().unwrap_or_else(|| "mind_map.html".to_string());
        
        println!("✅ Mind map visualization created: {}", output_path);

        // Move file to output directory if specified
        let final_path = if let Some(output_dir) = &self.output_dir {
            let file_name = Path::new(&output_path).file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("mind_map.html");
            
            let target_path = Path::new(output_dir).join(file_name);
            
            // Create output directory if it doesn't exist
            if let Some(parent) = target_path.parent() {
                tokio::fs::create_dir_all(parent).await.map_err(|e| {
                    AgentFlowError::AsyncExecutionError {
                        message: format!("Failed to create output directory: {}", e),
                    }
                })?;
            }
            
            // Move the file
            tokio::fs::rename(&output_path, &target_path).await.map_err(|e| {
                AgentFlowError::AsyncExecutionError {
                    message: format!("Failed to move mind map file: {}", e),
                }
            })?;
            
            target_path.to_string_lossy().to_string()
        } else {
            output_path
        };

        Ok(json!({
            "mind_map_visual_path": final_path,
            "format": self.export_format,
            "auto_opened": self.auto_open,
            "source_markdown": mind_map_md
        }))
    }

    async fn post_async(
        &self,
        shared: &SharedState,
        _prep_result: Value,
        exec_result: Value,
    ) -> Result<Option<String>, AgentFlowError> {
        println!("🎨 MarkMapVisualizerNode: Storing visual mind map result");
        shared.insert("mind_map_visual".to_string(), exec_result);

        // Continue to next node in workflow - check for translation
        if shared.get("has_translation").and_then(|v| v.as_bool()).unwrap_or(false) {
            Ok(Some("translator".to_string()))
        } else {
            Ok(Some("compiler".to_string()))
        }
    }

    fn get_node_id(&self) -> Option<String> {
        Some("markmap_visualizer".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;

    #[tokio_test::test]
    async fn test_markmap_node_creation() {
        let node = MarkMapVisualizerNode::new("png".to_string())
            .with_auto_open(false)
            .with_output_dir("./output");
            
        assert_eq!(node.export_format, "png");
        assert!(!node.auto_open);
        assert_eq!(node.output_dir.as_ref().unwrap(), "./output");
    }

    #[test]
    fn test_node_id() {
        let node = MarkMapVisualizerNode::new("svg".to_string());
        assert_eq!(node.get_node_id().unwrap(), "markmap_visualizer");
    }
}
use agentflow_core::{
    async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
    error::AgentFlowError,
    value::FlowValue,
};
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct FileNode;

#[async_trait]
impl AsyncNode for FileNode {
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        let operation = get_string_input(inputs, "operation")?;
        let path = get_string_input(inputs, "path")?;

        match operation {
            "read" => {
                let content = tokio::fs::read_to_string(path).await.map_err(|e| {
                    AgentFlowError::AsyncExecutionError { message: format!("Failed to read file '{}': {}", path, e) }
                })?;
                let mut outputs = HashMap::new();
                outputs.insert("content".to_string(), FlowValue::Json(json!(content)));
                Ok(outputs)
            }
            "write" => {
                let content = get_string_input(inputs, "content")?;
                if let Some(parent) = Path::new(path).parent() {
                    tokio::fs::create_dir_all(parent).await.map_err(|e| {
                        AgentFlowError::AsyncExecutionError { message: format!("Failed to create directory '{}': {}", parent.display(), e) }
                    })?;
                }
                tokio::fs::write(path, content).await.map_err(|e| {
                    AgentFlowError::AsyncExecutionError { message: format!("Failed to write file '{}': {}", path, e) }
                })?;
                let mut outputs = HashMap::new();
                outputs.insert("path".to_string(), FlowValue::Json(json!(path)));
                Ok(outputs)
            }
            _ => Err(AgentFlowError::NodeInputError { message: format!("Unsupported file operation: {}", operation) })
        }
    }
}

fn get_string_input<'a>(inputs: &'a AsyncNodeInputs, key: &str) -> Result<&'a str, AgentFlowError> {
    inputs.get(key)
        .and_then(|v| match v {
            FlowValue::Json(serde_json::Value::String(s)) => Some(s.as_str()),
            _ => None,
        })
        .ok_or_else(|| AgentFlowError::NodeInputError { message: format!("Required string input '{}' is missing or has wrong type", key) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_file_node_write_and_read() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        let file_path_str = file_path.to_str().unwrap();

        // Write to file
        let write_node = FileNode::default();
        let mut write_inputs = AsyncNodeInputs::new();
        write_inputs.insert("operation".to_string(), FlowValue::Json(json!("write")));
        write_inputs.insert("path".to_string(), FlowValue::Json(json!(file_path_str)));
        write_inputs.insert("content".to_string(), FlowValue::Json(json!("hello world")));

        let write_result = write_node.execute(&write_inputs).await;
        assert!(write_result.is_ok());

        // Read from file
        let read_node = FileNode::default();
        let mut read_inputs = AsyncNodeInputs::new();
        read_inputs.insert("operation".to_string(), FlowValue::Json(json!("read")));
        read_inputs.insert("path".to_string(), FlowValue::Json(json!(file_path_str)));

        let read_result = read_node.execute(&read_inputs).await;
        assert!(read_result.is_ok());

        let outputs = read_result.unwrap();
        let content = outputs.get("content").unwrap();
        if let FlowValue::Json(Value::String(s)) = content {
            assert_eq!(s, "hello world");
        }
    }
}
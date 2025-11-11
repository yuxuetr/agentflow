//! Integration tests for checkpoint recovery

use agentflow_core::{
    async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
    checkpoint::CheckpointConfig,
    flow::{Flow, GraphNode, NodeType},
    value::FlowValue,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;

/// Simple test node
#[derive(Clone)]
struct SimpleNode {
    id: String,
    output_value: String,
}

impl SimpleNode {
    fn new(id: &str, output_value: &str) -> Self {
        Self {
            id: id.to_string(),
            output_value: output_value.to_string(),
        }
    }
}

#[async_trait]
impl AsyncNode for SimpleNode {
    async fn execute(&self, _inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        let mut outputs = HashMap::new();
        outputs.insert(
            "result".to_string(),
            FlowValue::Json(serde_json::json!(self.output_value)),
        );
        Ok(outputs)
    }
}

#[tokio::test]
async fn test_checkpointing_enabled() {
    let temp_dir = TempDir::new().unwrap();
    let config = CheckpointConfig::default()
        .with_checkpoint_dir(temp_dir.path())
        .with_auto_cleanup(false);

    let nodes = vec![GraphNode {
        id: "node1".to_string(),
        node_type: NodeType::Standard(Arc::new(SimpleNode::new("node1", "test_output"))),
        dependencies: vec![],
        input_mapping: None,
        run_if: None,
        initial_inputs: HashMap::new(),
    }];

    let flow = Flow::new(nodes).with_checkpointing(config).unwrap();
    let result = flow.run().await;

    assert!(result.is_ok());
    let state = result.unwrap();
    assert_eq!(state.len(), 1);
    assert!(state.contains_key("node1"));
}

#[tokio::test]
async fn test_checkpoint_saves_state() {
    let temp_dir = TempDir::new().unwrap();
    let config = CheckpointConfig::default()
        .with_checkpoint_dir(temp_dir.path())
        .with_auto_cleanup(false);

    let nodes = vec![
        GraphNode {
            id: "node1".to_string(),
            node_type: NodeType::Standard(Arc::new(SimpleNode::new("node1", "output1"))),
            dependencies: vec![],
            input_mapping: None,
            run_if: None,
            initial_inputs: HashMap::new(),
        },
        GraphNode {
            id: "node2".to_string(),
            node_type: NodeType::Standard(Arc::new(SimpleNode::new("node2", "output2"))),
            dependencies: vec!["node1".to_string()],
            input_mapping: None,
            run_if: None,
            initial_inputs: HashMap::new(),
        },
    ];

    let flow = Flow::new(nodes).with_checkpointing(config).unwrap();
    let result = flow.run().await;

    assert!(result.is_ok());
    let state = result.unwrap();
    assert_eq!(state.len(), 2);

    // Verify checkpoint directory was created
    assert!(temp_dir.path().exists());

    // Verify checkpoint files exist (they will have UUID-based directory names)
    let entries: Vec<_> = std::fs::read_dir(temp_dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();

    // At least one workflow directory should exist
    assert!(!entries.is_empty(), "No checkpoint directories created");
}

#[tokio::test]
async fn test_default_checkpointing() {
    let nodes = vec![GraphNode {
        id: "node1".to_string(),
        node_type: NodeType::Standard(Arc::new(SimpleNode::new("node1", "test"))),
        dependencies: vec![],
        input_mapping: None,
        run_if: None,
        initial_inputs: HashMap::new(),
    }];

    let flow = Flow::new(nodes).with_default_checkpointing();
    assert!(flow.is_ok(), "Default checkpointing should succeed");

    let result = flow.unwrap().run().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_checkpoint_after_each_node() {
    let temp_dir = TempDir::new().unwrap();
    let config = CheckpointConfig::default()
        .with_checkpoint_dir(temp_dir.path())
        .with_auto_cleanup(false);

    // Create a 3-node workflow
    let nodes = vec![
        GraphNode {
            id: "step1".to_string(),
            node_type: NodeType::Standard(Arc::new(SimpleNode::new("step1", "first"))),
            dependencies: vec![],
            input_mapping: None,
            run_if: None,
            initial_inputs: HashMap::new(),
        },
        GraphNode {
            id: "step2".to_string(),
            node_type: NodeType::Standard(Arc::new(SimpleNode::new("step2", "second"))),
            dependencies: vec!["step1".to_string()],
            input_mapping: None,
            run_if: None,
            initial_inputs: HashMap::new(),
        },
        GraphNode {
            id: "step3".to_string(),
            node_type: NodeType::Standard(Arc::new(SimpleNode::new("step3", "third"))),
            dependencies: vec!["step2".to_string()],
            input_mapping: None,
            run_if: None,
            initial_inputs: HashMap::new(),
        },
    ];

    let flow = Flow::new(nodes).with_checkpointing(config).unwrap();
    let result = flow.run().await;

    assert!(result.is_ok());

    // Verify all nodes completed
    let state = result.unwrap();
    assert_eq!(state.len(), 3);
    assert!(state.contains_key("step1"));
    assert!(state.contains_key("step2"));
    assert!(state.contains_key("step3"));
}

#[tokio::test]
async fn test_workflow_without_checkpointing() {
    // Ensure normal workflows still work without checkpointing
    let nodes = vec![GraphNode {
        id: "node1".to_string(),
        node_type: NodeType::Standard(Arc::new(SimpleNode::new("node1", "test"))),
        dependencies: vec![],
        input_mapping: None,
        run_if: None,
        initial_inputs: HashMap::new(),
    }];

    let flow = Flow::new(nodes);
    let result = flow.run().await;

    assert!(result.is_ok());
    let state = result.unwrap();
    assert_eq!(state.len(), 1);
}

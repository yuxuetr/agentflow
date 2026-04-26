//! Integration tests for checkpoint recovery

use agentflow_core::{
  async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
  checkpoint::{CheckpointConfig, CheckpointManager},
  flow::{Flow, GraphNode, NodeType},
  value::FlowValue,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::TempDir;

fn use_writable_home() {
  let home = std::env::temp_dir().join(format!(
    "agentflow-checkpoint-test-{}",
    uuid::Uuid::new_v4()
  ));
  std::fs::create_dir_all(&home).unwrap();
  std::env::set_var("HOME", home);
}

/// Simple test node
#[derive(Clone)]
struct SimpleNode {
  _id: String,
  output_value: String,
}

#[derive(Clone)]
struct AgentLikeNode;

#[derive(Clone)]
struct CountingAgentLikeNode {
  calls: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct FlakyNode {
  calls: Arc<AtomicUsize>,
}

#[async_trait]
impl AsyncNode for AgentLikeNode {
  async fn execute(&self, _inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let mut outputs = HashMap::new();
    outputs.insert(
      "response".to_string(),
      FlowValue::Json(serde_json::json!("done")),
    );
    outputs.insert(
      "agent_result".to_string(),
      FlowValue::Json(serde_json::json!({
        "session_id": "session-1",
        "answer": "done",
        "stop_reason": {"reason": "final_answer"},
        "steps": [
          {"index": 0, "kind": {"type": "observe", "input": "hello"}},
          {"index": 1, "kind": {"type": "final_answer", "answer": "done"}}
        ],
        "events": []
      })),
    );
    Ok(outputs)
  }
}

#[async_trait]
impl AsyncNode for CountingAgentLikeNode {
  async fn execute(&self, _inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let call_count = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
    let mut outputs = HashMap::new();
    outputs.insert(
      "response".to_string(),
      FlowValue::Json(serde_json::json!("done")),
    );
    outputs.insert(
      "agent_result".to_string(),
      FlowValue::Json(serde_json::json!({
        "session_id": "session-1",
        "answer": "done",
        "stop_reason": {"reason": "final_answer"},
        "steps": [
          {"index": 0, "kind": {"type": "observe", "input": "hello"}},
          {"index": 1, "kind": {"type": "tool_call", "tool": "expensive_tool", "params": {"call_count": call_count}}},
          {"index": 2, "kind": {"type": "tool_result", "tool": "expensive_tool", "content": "cached by checkpoint", "is_error": false}},
          {"index": 3, "kind": {"type": "final_answer", "answer": "done"}}
        ],
        "events": []
      })),
    );
    Ok(outputs)
  }
}

#[async_trait]
impl AsyncNode for FlakyNode {
  async fn execute(&self, _inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let call_count = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
    if call_count == 1 {
      return Err(agentflow_core::error::AgentFlowError::NodeExecutionFailed {
        message: "transient downstream failure".to_string(),
      });
    }

    let mut outputs = HashMap::new();
    outputs.insert(
      "result".to_string(),
      FlowValue::Json(serde_json::json!("recovered")),
    );
    Ok(outputs)
  }
}

impl SimpleNode {
  fn new(id: &str, output_value: &str) -> Self {
    Self {
      _id: id.to_string(),
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
  use_writable_home();
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
  use_writable_home();
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
async fn test_checkpoint_preserves_agent_node_step_history() {
  use_writable_home();
  let temp_dir = TempDir::new().unwrap();
  let config = CheckpointConfig::default()
    .with_checkpoint_dir(temp_dir.path())
    .with_auto_cleanup(false);
  let manager = CheckpointManager::new(config.clone()).unwrap();

  let nodes = vec![GraphNode {
    id: "agent".to_string(),
    node_type: NodeType::Standard(Arc::new(AgentLikeNode)),
    dependencies: vec![],
    input_mapping: None,
    run_if: None,
    initial_inputs: HashMap::new(),
  }];

  let flow = Flow::new(nodes).with_checkpointing(config).unwrap();
  let result = flow.run().await.unwrap();
  assert!(result.contains_key("agent"));

  let workflow_dir = std::fs::read_dir(temp_dir.path())
    .unwrap()
    .filter_map(|entry| entry.ok())
    .find(|entry| entry.path().is_dir())
    .expect("workflow checkpoint directory");
  let workflow_id = workflow_dir.file_name().to_string_lossy().into_owned();
  let checkpoint = manager
    .load_latest_checkpoint(&workflow_id)
    .await
    .unwrap()
    .expect("latest checkpoint");

  let agent_result = &checkpoint.state["agent"]["agent_result"];
  assert_eq!(agent_result["session_id"], "session-1");
  assert_eq!(agent_result["steps"][0]["kind"]["type"], "observe");
  assert_eq!(agent_result["steps"][1]["kind"]["type"], "final_answer");
}

#[tokio::test]
async fn test_resume_continues_after_agent_node_without_reexecuting_it() {
  use_writable_home();
  let temp_dir = TempDir::new().unwrap();
  let config = CheckpointConfig::default()
    .with_checkpoint_dir(temp_dir.path())
    .with_auto_cleanup(false);
  let manager = CheckpointManager::new(config.clone()).unwrap();
  let agent_calls = Arc::new(AtomicUsize::new(0));
  let flaky_calls = Arc::new(AtomicUsize::new(0));

  let nodes = vec![
    GraphNode {
      id: "agent".to_string(),
      node_type: NodeType::Standard(Arc::new(CountingAgentLikeNode {
        calls: agent_calls.clone(),
      })),
      dependencies: vec![],
      input_mapping: None,
      run_if: None,
      initial_inputs: HashMap::new(),
    },
    GraphNode {
      id: "downstream".to_string(),
      node_type: NodeType::Standard(Arc::new(FlakyNode {
        calls: flaky_calls.clone(),
      })),
      dependencies: vec!["agent".to_string()],
      input_mapping: None,
      run_if: None,
      initial_inputs: HashMap::new(),
    },
  ];

  let flow = Flow::new(nodes).with_checkpointing(config).unwrap();
  let first_run = flow.run().await.unwrap();
  assert!(first_run["downstream"].is_err());
  assert_eq!(agent_calls.load(Ordering::SeqCst), 1);
  assert_eq!(flaky_calls.load(Ordering::SeqCst), 1);

  let workflow_dir = std::fs::read_dir(temp_dir.path())
    .unwrap()
    .filter_map(|entry| entry.ok())
    .find(|entry| entry.path().is_dir())
    .expect("workflow checkpoint directory");
  let workflow_id = workflow_dir.file_name().to_string_lossy().into_owned();
  let failed_checkpoint = manager
    .load_latest_checkpoint(&workflow_id)
    .await
    .unwrap()
    .expect("latest checkpoint");
  assert_eq!(failed_checkpoint.last_completed_node, "agent");
  assert!(failed_checkpoint.state.contains_key("agent"));
  assert!(!failed_checkpoint.state.contains_key("downstream"));

  let resumed = flow.resume(&workflow_id).await.unwrap();
  assert_eq!(agent_calls.load(Ordering::SeqCst), 1);
  assert_eq!(flaky_calls.load(Ordering::SeqCst), 2);
  assert_eq!(
    resumed["downstream"].as_ref().unwrap()["result"],
    FlowValue::Json(serde_json::json!("recovered"))
  );

  let agent_result = match &resumed["agent"].as_ref().unwrap()["agent_result"] {
    FlowValue::Json(value) => value,
    other => panic!("expected JSON agent_result, got {other:?}"),
  };
  assert_eq!(agent_result["steps"][1]["kind"]["type"], "tool_call");
  assert_eq!(agent_result["steps"][2]["kind"]["type"], "tool_result");
}

#[tokio::test]
async fn test_default_checkpointing() {
  use_writable_home();
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
  use_writable_home();
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
  use_writable_home();
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

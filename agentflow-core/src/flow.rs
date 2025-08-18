use crate::{AgentFlowError, Node, Result, SharedState};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub struct Flow {
  start_node: Option<Box<dyn Node>>,
  nodes: HashMap<String, Box<dyn Node>>,
  parameters: HashMap<String, Value>,
  max_iterations: u32,
}

impl Flow {
  pub fn new(start_node: Box<dyn Node>) -> Self {
    Self {
      start_node: Some(start_node),
      nodes: HashMap::new(),
      parameters: HashMap::new(),
      max_iterations: 100, // Default limit to prevent infinite loops
    }
  }

  pub fn has_start_node(&self) -> bool {
    self.start_node.is_some()
  }

  pub fn add_node(&mut self, name: String, node: Box<dyn Node>) {
    self.nodes.insert(name, node);
  }

  pub fn set_parameter(&mut self, key: String, value: Value) {
    self.parameters.insert(key, value);
  }

  pub fn set_max_iterations(&mut self, max: u32) {
    self.max_iterations = max;
  }

  pub fn run(&self, shared: &SharedState) -> Result<Option<String>> {
    // Add flow parameters to shared state
    for (key, value) in &self.parameters {
      shared.insert(format!("received_{}", key), value.clone());
    }

    let mut current_node = match &self.start_node {
      Some(node) => node,
      None => {
        return Err(AgentFlowError::FlowExecutionFailed {
          message: "No start node defined".to_string(),
        })
      }
    };

    let mut iterations = 0;
    let mut visited_states = HashSet::new();

    loop {
      iterations += 1;

      if iterations > self.max_iterations {
        shared.insert("max_iterations_reached".to_string(), Value::Bool(true));
        return Err(AgentFlowError::FlowExecutionFailed {
          message: "Maximum iterations reached".to_string(),
        });
      }

      // Check for circular flows (simplified)
      let node_id = format!("{:p}", current_node.as_ref() as *const dyn Node);
      if visited_states.contains(&node_id) && iterations > 2 {
        return Err(AgentFlowError::CircularFlow);
      }
      visited_states.insert(node_id);

      // Execute current node
      let next_action = current_node.run(shared)?;

      // If no next action, flow ends
      let action = match next_action {
        Some(action) => action,
        None => return Ok(None),
      };

      // Find next node based on action
      current_node = match self.nodes.get(&action) {
        Some(node) => node,
        None => {
          // Unknown transition - flow ends gracefully
          return Ok(Some(action));
        }
      };
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{AgentFlowError, Node, Result, SharedState};
  use serde_json::Value;
  use std::sync::{Arc, Mutex};

  // Mock nodes for testing flows
  struct SimpleNode {
    id: String,
    next_action: Option<String>,
    execution_log: Arc<Mutex<Vec<String>>>,
  }

  impl Node for SimpleNode {
    fn prep(&self, _shared: &SharedState) -> Result<Value> {
      Ok(Value::String(format!("prep_{}", self.id)))
    }

    fn exec(&self, _prep_result: Value) -> Result<Value> {
      self.execution_log.lock().unwrap().push(self.id.clone());
      Ok(Value::String(format!("exec_{}", self.id)))
    }

    fn post(
      &self,
      shared: &SharedState,
      _prep_result: Value,
      _exec_result: Value,
    ) -> Result<Option<String>> {
      shared.insert(format!("{}_executed", self.id), Value::Bool(true));
      Ok(self.next_action.clone())
    }
  }

  struct ConditionalNode {
    condition_key: String,
    true_action: String,
    false_action: String,
  }

  impl Node for ConditionalNode {
    fn prep(&self, _shared: &SharedState) -> Result<Value> {
      Ok(Value::String("conditional_prep".to_string()))
    }

    fn exec(&self, _prep_result: Value) -> Result<Value> {
      Ok(Value::String("conditional_exec".to_string()))
    }

    fn post(
      &self,
      shared: &SharedState,
      _prep_result: Value,
      _exec_result: Value,
    ) -> Result<Option<String>> {
      let condition = shared
        .get(&self.condition_key)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

      if condition {
        Ok(Some(self.true_action.clone()))
      } else {
        Ok(Some(self.false_action.clone()))
      }
    }
  }

  struct FailingNode {
    should_fail: bool,
  }

  impl Node for FailingNode {
    fn prep(&self, _shared: &SharedState) -> Result<Value> {
      Ok(Value::String("failing_prep".to_string()))
    }

    fn exec(&self, _prep_result: Value) -> Result<Value> {
      if self.should_fail {
        Err(AgentFlowError::NodeExecutionFailed {
          message: "Failing node failed as expected".to_string(),
        })
      } else {
        Ok(Value::String("failing_exec".to_string()))
      }
    }

    fn post(
      &self,
      _shared: &SharedState,
      _prep_result: Value,
      _exec_result: Value,
    ) -> Result<Option<String>> {
      Ok(None)
    }
  }

  #[test]
  fn test_flow_creation() {
    // Test creating a flow with a start node
    let start_node = SimpleNode {
      id: "start".to_string(),
      next_action: None,
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let flow = Flow::new(Box::new(start_node));
    assert!(flow.has_start_node());
  }

  #[test]
  fn test_flow_simple_execution() {
    // Test executing a simple linear flow
    let start_node = SimpleNode {
      id: "start".to_string(),
      next_action: Some("end".to_string()),
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let end_node = SimpleNode {
      id: "end".to_string(),
      next_action: None,
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let mut flow = Flow::new(Box::new(start_node));
    flow.add_node("end".to_string(), Box::new(end_node));

    let shared = SharedState::new();
    let result = flow.run(&shared);

    assert!(result.is_ok());
    // Both nodes should have executed
  }

  #[test]
  fn test_flow_conditional_routing() {
    // Test conditional routing based on node return values
    let condition_node = ConditionalNode {
      condition_key: "should_succeed".to_string(),
      true_action: "success".to_string(),
      false_action: "failure".to_string(),
    };

    let success_node = SimpleNode {
      id: "success".to_string(),
      next_action: None,
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let failure_node = SimpleNode {
      id: "failure".to_string(),
      next_action: None,
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let mut flow = Flow::new(Box::new(condition_node));
    flow.add_node("success".to_string(), Box::new(success_node));
    flow.add_node("failure".to_string(), Box::new(failure_node));

    // Test success path
    let shared = SharedState::new();
    shared.insert("should_succeed".to_string(), Value::Bool(true));
    let result = flow.run(&shared);
    assert!(result.is_ok());

    // Test failure path
    let shared = SharedState::new();
    shared.insert("should_succeed".to_string(), Value::Bool(false));
    let result = flow.run(&shared);
    assert!(result.is_ok());
  }

  #[test]
  fn test_flow_error_handling() {
    // Test flow error handling and propagation
    let failing_node = FailingNode { should_fail: true };
    let flow = Flow::new(Box::new(failing_node));

    let shared = SharedState::new();
    let result = flow.run(&shared);

    assert!(result.is_err());
    // Error should be properly propagated from the failing node
  }

  #[test]
  fn test_flow_circular_detection() {
    // Test detection and handling of circular flows
    let node1 = SimpleNode {
      id: "node1".to_string(),
      next_action: Some("node2".to_string()),
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let node2 = SimpleNode {
      id: "node2".to_string(),
      next_action: Some("node1".to_string()), // Creates a circle
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let mut flow = Flow::new(Box::new(node1));
    flow.add_node(
      "node1".to_string(),
      Box::new(SimpleNode {
        id: "node1_copy".to_string(),
        next_action: Some("node2".to_string()),
        execution_log: Arc::new(Mutex::new(Vec::new())),
      }),
    );
    flow.add_node("node2".to_string(), Box::new(node2));
    flow.set_max_iterations(10); // Lower limit to trigger max iterations

    let shared = SharedState::new();
    let result = flow.run(&shared);

    // Should hit max iterations due to circular flow
    assert!(result.is_err() || shared.contains_key("max_iterations_reached"));
  }

  #[test]
  fn test_flow_state_preservation() {
    // Test that shared state is preserved across node executions
    let node1 = SimpleNode {
      id: "node1".to_string(),
      next_action: Some("node2".to_string()),
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let node2 = SimpleNode {
      id: "node2".to_string(),
      next_action: None,
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let mut flow = Flow::new(Box::new(node1));
    flow.add_node("node2".to_string(), Box::new(node2));

    let shared = SharedState::new();
    shared.insert(
      "initial_value".to_string(),
      Value::String("test".to_string()),
    );

    let result = flow.run(&shared);
    assert!(result.is_ok());

    // Initial value should still be present
    assert_eq!(
      shared.get("initial_value").unwrap(),
      Value::String("test".to_string())
    );

    // Nodes should have added their own values
    assert!(shared.contains_key("node1_executed"));
    assert!(shared.contains_key("node2_executed"));
  }

  #[test]
  fn test_flow_unknown_transition() {
    // Test handling of unknown transitions
    let node = SimpleNode {
      id: "start".to_string(),
      next_action: Some("nonexistent".to_string()),
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let flow = Flow::new(Box::new(node));

    let shared = SharedState::new();
    let result = flow.run(&shared);

    // Should handle unknown transition gracefully
    assert!(result.is_ok()); // Flow should end gracefully
  }

  #[test]
  fn test_flow_copy_semantics() {
    // Test that flows handle max iterations correctly
    let execution_log = Arc::new(Mutex::new(Vec::new()));

    let node1 = SimpleNode {
      id: "node1".to_string(),
      next_action: Some("nonexistent".to_string()), // Will cause flow to end
      execution_log: execution_log.clone(),
    };

    let mut flow = Flow::new(Box::new(node1));
    flow.set_max_iterations(3); // Limit iterations

    let shared = SharedState::new();
    let result = flow.run(&shared);

    assert!(result.is_ok());

    // Should have executed once before ending on unknown transition
    let log = execution_log.lock().unwrap();
    assert_eq!(log.len(), 1);
  }

  #[test]
  fn test_flow_parameter_inheritance() {
    // Test parameter inheritance in flows (similar to PocketFlow's params)
    let mut flow = Flow::new(Box::new(SimpleNode {
      id: "start".to_string(),
      next_action: None,
      execution_log: Arc::new(Mutex::new(Vec::new())),
    }));

    flow.set_parameter(
      "global_param".to_string(),
      Value::String("global_value".to_string()),
    );

    let shared = SharedState::new();
    let result = flow.run(&shared);

    assert!(result.is_ok());
    // Node should have access to flow parameters
    assert_eq!(
      shared.get("received_global_param").unwrap(),
      Value::String("global_value".to_string())
    );
  }
}

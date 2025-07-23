use crate::{SharedState, AgentFlowError, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

pub trait Node: Send + Sync {
  fn prep(&self, shared: &SharedState) -> Result<Value>;
  fn exec(&self, prep_result: Value) -> Result<Value>;
  fn post(&self, shared: &SharedState, prep_result: Value, exec_result: Value) -> Result<Option<String>>;
  
  fn run(&self, shared: &SharedState) -> Result<Option<String>> {
    let prep_result = self.prep(shared)?;
    let exec_result = self.exec(prep_result.clone())?;
    self.post(shared, prep_result, exec_result)
  }
  
  fn run_with_retries(&self, shared: &SharedState, max_retries: u32, wait_duration: Duration) -> Result<Option<String>> {
    let mut _last_error = None;
    
    for attempt in 1..=max_retries {
      match self.run(shared) {
        Ok(result) => return Ok(result),
        Err(e) => {
          _last_error = Some(e);
          if attempt < max_retries {
            std::thread::sleep(wait_duration);
          }
        }
      }
    }
    
    Err(AgentFlowError::RetryExhausted { attempts: max_retries })
  }
}

pub struct BaseNode {
  pub id: Uuid,
  successors: HashMap<String, Box<dyn Node>>,
}

impl BaseNode {
  pub fn new() -> Self {
    Self {
      id: Uuid::new_v4(),
      successors: HashMap::new(),
    }
  }
  
  pub fn add_successor(&mut self, action: String, node: Box<dyn Node>) {
    self.successors.insert(action, node);
  }
  
  pub fn has_successor(&self, action: &str) -> bool {
    self.successors.contains_key(action)
  }
  
  pub fn get_successor(&self, action: &str) -> Option<&Box<dyn Node>> {
    self.successors.get(action)
  }
}

impl Default for BaseNode {
  fn default() -> Self {
    Self::new()
  }
}

// Implement >> operator for chaining nodes (similar to PocketFlow)
impl<'a> std::ops::Shr<Box<dyn Node>> for &'a mut BaseNode {
  type Output = &'a mut BaseNode;

  fn shr(self, rhs: Box<dyn Node>) -> Self::Output {
    self.add_successor("default".to_string(), rhs);
    self
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{SharedState, AgentFlowError, Result};
  use serde_json::Value;
  use std::sync::{Arc, Mutex};
  use std::time::Duration;

  // Mock node for testing
  struct MockNode {
    prep_result: Option<String>,
    exec_result: Option<String>, 
    post_action: Option<String>,
    should_fail: bool,
    call_count: Arc<Mutex<u32>>,
  }

  impl Node for MockNode {
    fn prep(&self, _shared: &SharedState) -> Result<Value> {
      let mut count = self.call_count.lock().unwrap();
      *count += 1;
      
      if let Some(ref result) = self.prep_result {
        Ok(Value::String(result.clone()))
      } else {
        Ok(Value::String("default_prep".to_string()))
      }
    }

    fn exec(&self, _prep_result: Value) -> Result<Value> {
      if self.should_fail {
        return Err(AgentFlowError::NodeExecutionFailed { 
          message: "Mock node failed".to_string() 
        });
      }
      
      if let Some(ref result) = self.exec_result {
        Ok(Value::String(result.clone()))
      } else {
        Ok(Value::String("default_exec".to_string()))
      }
    }

    fn post(&self, shared: &SharedState, _prep_result: Value, exec_result: Value) -> Result<Option<String>> {
      // Store the execution result in shared state
      shared.insert("output".to_string(), exec_result);
      Ok(self.post_action.clone())
    }
  }

  struct RetryNode {
    attempts: Arc<Mutex<u32>>,
    fail_until_attempt: u32,
  }

  impl Node for RetryNode {
    fn prep(&self, _shared: &SharedState) -> Result<Value> {
      Ok(Value::String("prep".to_string()))
    }

    fn exec(&self, _prep_result: Value) -> Result<Value> {
      let mut attempts = self.attempts.lock().unwrap();
      *attempts += 1;
      
      if *attempts < self.fail_until_attempt {
        Err(AgentFlowError::NodeExecutionFailed { 
          message: format!("Attempt {} failed", *attempts) 
        })
      } else {
        Ok(Value::String("success".to_string()))
      }
    }

    fn post(&self, _shared: &SharedState, _prep_result: Value, _exec_result: Value) -> Result<Option<String>> {
      Ok(None)
    }
  }

  struct CounterNode {
    counter: Arc<Mutex<u32>>,
  }

  impl Node for CounterNode {
    fn prep(&self, _shared: &SharedState) -> Result<Value> {
      let mut counter = self.counter.lock().unwrap();
      *counter += 1;
      Ok(Value::Number(serde_json::Number::from(*counter)))
    }

    fn exec(&self, prep_result: Value) -> Result<Value> {
      Ok(prep_result)
    }

    fn post(&self, _shared: &SharedState, _prep_result: Value, _exec_result: Value) -> Result<Option<String>> {
      Ok(None)
    }
  }

  #[test]
  fn test_node_lifecycle() {
    // Test the basic node lifecycle: prep -> exec -> post
    let node = MockNode {
      prep_result: Some("prep_data".to_string()),
      exec_result: Some("exec_result".to_string()),
      post_action: Some("next".to_string()),
      should_fail: false,
      call_count: Arc::new(Mutex::new(0)),
    };

    let shared = SharedState::new();
    let result = node.run(&shared);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some("next".to_string()));
    
    // Verify the node was called
    let count = *node.call_count.lock().unwrap();
    assert_eq!(count, 1);
  }

  #[test]
  fn test_node_error_handling() {
    // Test node error handling
    let node = MockNode {
      prep_result: Some("prep_data".to_string()),
      exec_result: None,
      post_action: None,
      should_fail: true,
      call_count: Arc::new(Mutex::new(0)),
    };

    let shared = SharedState::new();
    let result = node.run(&shared);

    assert!(result.is_err());
  }

  #[test]
  fn test_node_retry_mechanism() {
    // Test retry mechanism with max_retries
    let node = RetryNode {
      attempts: Arc::new(Mutex::new(0)),
      fail_until_attempt: 3, // Fail first 2 attempts, succeed on 3rd
    };

    let shared = SharedState::new();
    let result = node.run_with_retries(&shared, 5, Duration::from_millis(10));

    assert!(result.is_ok());
    
    let attempts = *node.attempts.lock().unwrap();
    assert_eq!(attempts, 3);
  }

  #[test]
  fn test_node_retry_exhaustion() {
    // Test retry exhaustion
    let node = RetryNode {
      attempts: Arc::new(Mutex::new(0)),
      fail_until_attempt: 10, // Always fail
    };

    let shared = SharedState::new();
    let result = node.run_with_retries(&shared, 3, Duration::from_millis(1));

    assert!(result.is_err());
    
    let attempts = *node.attempts.lock().unwrap();
    assert_eq!(attempts, 3); // Should have tried exactly max_retries times
  }

  #[test]
  fn test_base_node_successor_chaining() {
    // Test successor chaining with >> operator
    let node2 = CounterNode { counter: Arc::new(Mutex::new(0)) };

    // Test manual successor addition
    let mut base_node = BaseNode::new();
    base_node.add_successor("default".to_string(), Box::new(node2));

    assert!(base_node.has_successor("default"));
    assert!(!base_node.has_successor("nonexistent"));
  }

  #[test]
  fn test_node_chaining_operator() {
    // Test >> operator for node chaining
    let node2 = CounterNode { counter: Arc::new(Mutex::new(0)) };
    let mut base_node = BaseNode::new();
    
    // Use the >> operator
    let _ = &mut base_node >> Box::new(node2);
    
    assert!(base_node.has_successor("default"));
  }

  #[test]
  fn test_conditional_transitions() {
    // Test conditional transitions with action routing
    let mut node = BaseNode::new();
    let success_node = CounterNode { counter: Arc::new(Mutex::new(0)) };
    let error_node = CounterNode { counter: Arc::new(Mutex::new(0)) };

    node.add_successor("success".to_string(), Box::new(success_node));
    node.add_successor("error".to_string(), Box::new(error_node));

    // Test getting successors
    assert!(node.get_successor("success").is_some());
    assert!(node.get_successor("error").is_some());
    assert!(node.get_successor("nonexistent").is_none());
  }

  #[test]
  fn test_node_shared_state_interaction() {
    // Test how nodes interact with shared state
    let node = MockNode {
      prep_result: Some("test_key".to_string()),
      exec_result: Some("test_value".to_string()),
      post_action: Some("continue".to_string()),
      should_fail: false,
      call_count: Arc::new(Mutex::new(0)),
    };

    let shared = SharedState::new();
    shared.insert("input".to_string(), Value::String("initial_value".to_string()));

    let result = node.run(&shared);
    assert!(result.is_ok());

    // Verify the node modified shared state as expected
    assert!(shared.contains_key("output")); // Node should have added this
  }

  #[test]
  fn test_node_parameter_passing() {
    // Test parameter passing between prep, exec, and post
    let node = MockNode {
      prep_result: Some("prep_output".to_string()),
      exec_result: Some("exec_output".to_string()),
      post_action: Some("done".to_string()),
      should_fail: false,
      call_count: Arc::new(Mutex::new(0)),
    };

    let shared = SharedState::new();
    let result = node.run(&shared);

    assert!(result.is_ok());
    // Implementation should ensure prep result is passed to exec,
    // and both prep and exec results are passed to post
  }
}
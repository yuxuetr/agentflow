// Async Flow implementation - tests first, implementation follows

use crate::observability::{ExecutionEvent, MetricsCollector};
#[warn(unused_imports)]
use crate::{AgentFlowError, AsyncNode, Result, SharedState};
// use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::time::Duration;
use uuid::Uuid;

pub struct AsyncFlow {
  pub id: Uuid,
  start_node: Option<Box<dyn AsyncNode>>,
  nodes: HashMap<String, Box<dyn AsyncNode>>,
  parallel_nodes: Vec<Box<dyn AsyncNode>>, // For parallel execution
  batch_size: Option<usize>,
  timeout: Option<Duration>,
  max_concurrent_batches: Option<usize>,
  metrics_collector: Option<Arc<MetricsCollector>>,
  flow_name: Option<String>,
}

impl AsyncFlow {
  pub fn new(start_node: Box<dyn AsyncNode>) -> Self {
    Self {
      id: Uuid::new_v4(),
      start_node: Some(start_node),
      nodes: HashMap::new(),
      parallel_nodes: Vec::new(),
      batch_size: None,
      timeout: None,
      max_concurrent_batches: None,
      metrics_collector: None,
      flow_name: None,
    }
  }

  pub fn new_parallel(nodes: Vec<Box<dyn AsyncNode>>) -> Self {
    Self {
      id: Uuid::new_v4(),
      start_node: None,
      nodes: HashMap::new(),
      parallel_nodes: nodes,
      batch_size: None,
      timeout: None,
      max_concurrent_batches: None,
      metrics_collector: None,
      flow_name: None,
    }
  }

  pub fn new_empty() -> Self {
    Self {
      id: Uuid::new_v4(),
      start_node: None,
      nodes: HashMap::new(),
      parallel_nodes: Vec::new(),
      batch_size: None,
      timeout: None,
      max_concurrent_batches: None,
      metrics_collector: None,
      flow_name: None,
    }
  }

  pub fn has_start_node(&self) -> bool {
    self.start_node.is_some()
  }

  pub fn add_node(&mut self, id: String, node: Box<dyn AsyncNode>) {
    self.nodes.insert(id, node);
  }

  pub fn set_metrics_collector(&mut self, collector: Arc<MetricsCollector>) {
    self.metrics_collector = Some(collector);
  }

  pub fn set_flow_name(&mut self, name: String) {
    self.flow_name = Some(name);
  }

  pub fn enable_tracing(&mut self, flow_name: String) {
    self.set_flow_name(flow_name);
    if self.metrics_collector.is_none() {
      self.metrics_collector = Some(Arc::new(MetricsCollector::new()));
    }
  }

  pub async fn run(&self, shared: &SharedState) -> Result<Value> {
    self.run_async(shared).await
  }

  pub async fn run_async(&self, shared: &SharedState) -> Result<Value> {
    let flow_name = self.flow_name.as_deref().unwrap_or("unnamed_flow");
    let start_time = Instant::now();

    // Record flow start event
    if let Some(ref collector) = self.metrics_collector {
      let event = ExecutionEvent {
        node_id: flow_name.to_string(),
        event_type: "flow_start".to_string(),
        timestamp: start_time,
        duration_ms: None,
        metadata: HashMap::new(),
      };
      collector.record_event(event);
      collector.increment_counter(&format!("{}.execution_count", flow_name), 1.0);
    }

    let result = self.run_async_internal(shared).await;
    let duration = start_time.elapsed();

    // Record flow completion event
    if let Some(ref collector) = self.metrics_collector {
      let event = ExecutionEvent {
        node_id: flow_name.to_string(),
        event_type: if result.is_ok() {
          "flow_success"
        } else {
          "flow_error"
        }
        .to_string(),
        timestamp: start_time,
        duration_ms: Some(duration.as_millis() as u64),
        metadata: HashMap::new(),
      };
      collector.record_event(event);

      collector.increment_counter(
        &format!("{}.duration_ms", flow_name),
        duration.as_millis() as f64,
      );
      if result.is_ok() {
        collector.increment_counter(&format!("{}.success_count", flow_name), 1.0);
      } else {
        collector.increment_counter(&format!("{}.error_count", flow_name), 1.0);
      }
    }

    result
  }

  async fn run_async_internal(&self, shared: &SharedState) -> Result<Value> {
    // Handle parallel execution mode
    if !self.parallel_nodes.is_empty() {
      let node_refs: Vec<&dyn AsyncNode> = self.parallel_nodes.iter().map(|n| n.as_ref()).collect();
      let results = self.run_parallel(node_refs, shared).await?;

      // Return success indicator for parallel execution
      return Ok(Value::String(format!(
        "parallel_completed_{}",
        results.len()
      )));
    }

    // Standard sequential flow execution
    let mut current_node = match &self.start_node {
      Some(node) => node,
      None => {
        return Err(AgentFlowError::FlowExecutionFailed {
          message: "No start node defined".to_string(),
        })
      }
    };

    #[allow(unused_assignments)]
    let mut last_action: Option<String> = None;
    let mut execution_count = 0;
    const MAX_EXECUTIONS: usize = 100; // Prevent infinite loops

    loop {
      // Prevent infinite loops
      execution_count += 1;
      if execution_count > MAX_EXECUTIONS {
        return Err(AgentFlowError::FlowExecutionFailed {
          message: format!(
            "Flow execution exceeded maximum iterations ({})",
            MAX_EXECUTIONS
          ),
        });
      }

      // Execute current node with observability
      let action = match self.timeout {
        Some(timeout_duration) => {
          match tokio::time::timeout(
            timeout_duration,
            current_node.run_async_with_observability(shared, self.metrics_collector.clone()),
          )
          .await
          {
            Ok(result) => result?,
            Err(_) => {
              return Err(AgentFlowError::TimeoutExceeded {
                duration_ms: timeout_duration.as_millis() as u64,
              });
            }
          }
        }
        None => {
          current_node
            .run_async_with_observability(shared, self.metrics_collector.clone())
            .await?
        }
      };

      last_action = action.clone();

      // Find next node based on the action returned
      let next_node_id = match action {
        Some(action_str) => {
          // Look for the action in the nodes map
          if self.nodes.contains_key(&action_str) {
            Some(action_str)
          } else {
            // No more nodes to execute
            None
          }
        }
        None => None, // No action means end of flow
      };

      match next_node_id {
        Some(node_id) => {
          current_node =
            self
              .nodes
              .get(&node_id)
              .ok_or_else(|| AgentFlowError::FlowExecutionFailed {
                message: format!("Node '{}' not found in flow", node_id),
              })?;
        }
        None => break, // End of flow
      }
    }

    // Return the last action as the flow result
    Ok(Value::String(last_action.unwrap_or_default()))
  }

  pub async fn run_parallel(
    &self,
    nodes: Vec<&dyn AsyncNode>,
    shared: &SharedState,
  ) -> Result<Vec<Value>> {
    if nodes.is_empty() {
      return Ok(Vec::new());
    }

    // Create futures for all nodes with observability
    let futures = nodes.iter().map(|node| async move {
      match self.timeout {
        Some(timeout_duration) => {
          match tokio::time::timeout(
            timeout_duration,
            node.run_async_with_observability(shared, self.metrics_collector.clone()),
          )
          .await
          {
            Ok(result) => result.map(|r| Value::String(r.unwrap_or_default())),
            Err(_) => Err(AgentFlowError::TimeoutExceeded {
              duration_ms: timeout_duration.as_millis() as u64,
            }),
          }
        }
        None => node
          .run_async_with_observability(shared, self.metrics_collector.clone())
          .await
          .map(|r| Value::String(r.unwrap_or_default())),
      }
    });

    // Execute all futures concurrently using join_all (similar to asyncio.gather)
    let results = futures::future::join_all(futures).await;

    // Process results - collect successes and first error
    let mut success_results = Vec::new();
    let mut first_error = None;

    for result in results {
      match result {
        Ok(value) => success_results.push(value),
        Err(e) => {
          if first_error.is_none() {
            first_error = Some(e);
          }
        }
      }
    }

    // Return first error if any occurred, otherwise return all successful results
    match first_error {
      Some(error) => Err(error),
      None => Ok(success_results),
    }
  }

  pub async fn run_batch(
    &self,
    nodes: Vec<&dyn AsyncNode>,
    shared: &SharedState,
    batch_size: usize,
  ) -> Result<Vec<Value>> {
    if nodes.is_empty() {
      return Ok(Vec::new());
    }

    let effective_batch_size = if batch_size == 0 { 1 } else { batch_size };
    let mut all_results = Vec::new();

    // Process nodes in batches
    for chunk in nodes.chunks(effective_batch_size) {
      let batch_results = self.run_parallel(chunk.to_vec(), shared).await?;
      all_results.extend(batch_results);

      // Optional: Add small delay between batches to prevent overwhelming resources
      if chunk.len() == effective_batch_size && !all_results.is_empty() {
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
      }
    }

    Ok(all_results)
  }

  // Enhanced batch processing with concurrent batch execution
  pub async fn run_concurrent_batches(
    &self,
    nodes: Vec<&dyn AsyncNode>,
    shared: &SharedState,
  ) -> Result<Vec<Value>> {
    let batch_size = self.batch_size.unwrap_or(5);
    let max_concurrent = self.max_concurrent_batches.unwrap_or(3);

    if nodes.is_empty() {
      return Ok(Vec::new());
    }

    // Split nodes into batches
    let batches: Vec<Vec<&dyn AsyncNode>> = nodes
      .chunks(batch_size)
      .map(|chunk| chunk.to_vec())
      .collect();

    let mut all_results = Vec::new();

    // Process batches with concurrency limit
    for batch_group in batches.chunks(max_concurrent) {
      let batch_futures = batch_group
        .iter()
        .map(|batch| self.run_parallel(batch.clone(), shared));

      let batch_group_results = futures::future::join_all(batch_futures).await;

      // Collect results from this batch group
      for batch_result in batch_group_results {
        match batch_result {
          Ok(batch_values) => all_results.extend(batch_values),
          Err(e) => return Err(e),
        }
      }
    }

    Ok(all_results)
  }

  pub fn set_batch_size(&mut self, size: usize) {
    self.batch_size = Some(size);
  }

  pub fn set_timeout(&mut self, timeout_duration: Duration) {
    self.timeout = Some(timeout_duration);
  }

  pub fn set_max_concurrent_batches(&mut self, max_batches: usize) {
    self.max_concurrent_batches = Some(max_batches);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{AgentFlowError, AsyncNode, Result, SharedState};
  use async_trait::async_trait;
  use serde_json::Value;
  use std::sync::{Arc, Mutex};
  use std::time::{Duration, Instant};
  use tokio::time::sleep;

  // Mock async nodes for flow testing
  struct SimpleAsyncNode {
    id: String,
    next_action: Option<String>,
    delay_ms: u64,
    execution_log: Arc<Mutex<Vec<(String, Instant)>>>,
  }

  struct ConditionalAsyncNode {
    condition_key: String,
    true_action: String,
    false_action: String,
    delay_ms: u64,
  }

  struct BatchProcessingNode {
    batch_size: usize,
    processing_delay_ms: u64,
    execution_log: Arc<Mutex<Vec<String>>>,
  }

  struct NestedFlowNode {
    inner_flow: AsyncFlow,
    execution_log: Arc<Mutex<Vec<String>>>,
  }

  // AsyncNode implementations for test nodes

  #[async_trait]
  impl AsyncNode for SimpleAsyncNode {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
      // Check if we should fail
      if let Some(Value::Bool(true)) = shared.get("should_fail") {
        return Err(AgentFlowError::AsyncExecutionError {
          message: format!("Node {} configured to fail", self.id),
        });
      }
      Ok(Value::String(format!("prep_{}", self.id)))
    }

    async fn exec_async(&self, _prep_result: Value) -> Result<Value> {
      if self.delay_ms > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
      }
      {
        self
          .execution_log
          .lock()
          .unwrap()
          .push((self.id.clone(), std::time::Instant::now()));
      }
      Ok(Value::String(format!("exec_{}", self.id)))
    }

    async fn post_async(
      &self,
      shared: &SharedState,
      _prep_result: Value,
      exec_result: Value,
    ) -> Result<Option<String>> {
      shared.insert(format!("{}_executed", self.id), Value::Bool(true));
      shared.insert("output".to_string(), exec_result);
      Ok(self.next_action.clone())
    }
  }

  #[async_trait]
  impl AsyncNode for ConditionalAsyncNode {
    async fn prep_async(&self, _shared: &SharedState) -> Result<Value> {
      Ok(Value::String("conditional_prep".to_string()))
    }

    async fn exec_async(&self, _prep_result: Value) -> Result<Value> {
      if self.delay_ms > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
      }
      Ok(Value::String("conditional_exec".to_string()))
    }

    async fn post_async(
      &self,
      shared: &SharedState,
      _prep_result: Value,
      _exec_result: Value,
    ) -> Result<Option<String>> {
      let condition_value = shared
        .get(&self.condition_key)
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

      if condition_value {
        Ok(Some(self.true_action.clone()))
      } else {
        Ok(Some(self.false_action.clone()))
      }
    }
  }

  #[async_trait]
  impl AsyncNode for BatchProcessingNode {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
      // Get batch items from shared state
      match shared.get("batch_items") {
        Some(Value::Array(items)) => Ok(Value::Array(items.clone())),
        _ => Ok(Value::Array(Vec::new())),
      }
    }

    async fn exec_async(&self, prep_result: Value) -> Result<Value> {
      if let Value::Array(items) = prep_result {
        if self.processing_delay_ms > 0 {
          tokio::time::sleep(std::time::Duration::from_millis(self.processing_delay_ms)).await;
        }

        // Process items in batches
        let mut batch_count = 0;
        for _batch in items.chunks(self.batch_size) {
          batch_count += 1;
          {
            self
              .execution_log
              .lock()
              .unwrap()
              .push(format!("batch_{}_executed", batch_count));
          }

          // Small delay between batches
          if self.processing_delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.processing_delay_ms)).await;
          }
        }

        Ok(Value::Number(serde_json::Number::from(batch_count)))
      } else {
        Ok(Value::Number(serde_json::Number::from(0)))
      }
    }

    async fn post_async(
      &self,
      shared: &SharedState,
      _prep_result: Value,
      exec_result: Value,
    ) -> Result<Option<String>> {
      // Store the number of processed batches in shared state
      shared.insert("processed_batches".to_string(), exec_result);
      Ok(None)
    }
  }

  #[async_trait]
  impl AsyncNode for NestedFlowNode {
    async fn prep_async(&self, _shared: &SharedState) -> Result<Value> {
      Ok(Value::String("nested_prep".to_string()))
    }

    async fn exec_async(&self, _prep_result: Value) -> Result<Value> {
      // Execute the inner flow
      let shared = SharedState::new();
      let inner_result = self.inner_flow.run_async(&shared).await?;

      {
        self
          .execution_log
          .lock()
          .unwrap()
          .push("nested_executed".to_string());
      }

      Ok(inner_result)
    }

    async fn post_async(
      &self,
      _shared: &SharedState,
      _prep_result: Value,
      _exec_result: Value,
    ) -> Result<Option<String>> {
      Ok(None)
    }
  }

  #[tokio::test]
  async fn test_async_flow_creation() {
    // Test creating an async flow
    let start_node = SimpleAsyncNode {
      id: "start".to_string(),
      next_action: None,
      delay_ms: 0,
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let flow = AsyncFlow::new(Box::new(start_node));
    assert!(flow.has_start_node());
  }

  #[tokio::test]
  async fn test_async_flow_simple_execution() {
    // Test simple async flow execution
    let execution_log = Arc::new(Mutex::new(Vec::new()));

    let start_node = SimpleAsyncNode {
      id: "start".to_string(),
      next_action: Some("end".to_string()),
      delay_ms: 10,
      execution_log: execution_log.clone(),
    };

    let end_node = SimpleAsyncNode {
      id: "end".to_string(),
      next_action: None,
      delay_ms: 10,
      execution_log: execution_log.clone(),
    };

    let mut flow = AsyncFlow::new(Box::new(start_node));
    flow.add_node("end".to_string(), Box::new(end_node));

    let shared = SharedState::new();
    let result = flow.run_async(&shared).await;

    assert!(result.is_ok());

    let log = execution_log.lock().unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[0].0, "start");
    assert_eq!(log[1].0, "end");
  }

  #[tokio::test]
  async fn test_async_flow_parallel_execution() {
    // Test parallel execution within flow
    let execution_log = Arc::new(Mutex::new(Vec::new()));

    let parallel_nodes = (0..3)
      .map(|i| SimpleAsyncNode {
        id: format!("parallel_{}", i),
        next_action: None,
        delay_ms: 50,
        execution_log: execution_log.clone(),
      })
      .collect::<Vec<_>>();

    let flow = AsyncFlow::new_parallel(
      parallel_nodes
        .into_iter()
        .map(|n| Box::new(n) as Box<dyn AsyncNode>)
        .collect(),
    );

    let shared = SharedState::new();
    let start = Instant::now();

    let result = flow.run_async(&shared).await;
    let elapsed = start.elapsed();

    assert!(result.is_ok());
    assert!(elapsed < Duration::from_millis(100)); // Should be ~50ms, not ~150ms

    let log = execution_log.lock().unwrap();
    assert_eq!(log.len(), 3);
  }

  #[tokio::test]
  async fn test_async_flow_batch_processing() {
    // Test batch processing capabilities
    let batch_node = BatchProcessingNode {
      batch_size: 5,
      processing_delay_ms: 10,
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let mut flow = AsyncFlow::new(Box::new(batch_node));
    flow.set_batch_size(5);

    let shared = SharedState::new();
    // Add 15 items to process in batches of 5
    let items: Vec<Value> = (0..15)
      .map(|i| Value::Number(serde_json::Number::from(i)))
      .collect();
    shared.insert("batch_items".to_string(), Value::Array(items));

    let result = flow.run_async(&shared).await;
    assert!(result.is_ok());

    // Should have processed 3 batches
    let processed_batches = shared
      .get("processed_batches")
      .and_then(|v| v.as_u64())
      .unwrap_or(0);
    assert_eq!(processed_batches, 3);
  }

  #[tokio::test]
  async fn test_async_flow_nested_flows() {
    // Test nested flow execution
    let inner_execution_log = Arc::new(Mutex::new(Vec::new()));
    let outer_execution_log = Arc::new(Mutex::new(Vec::new()));

    // Create inner flow
    let inner_node = SimpleAsyncNode {
      id: "inner".to_string(),
      next_action: None,
      delay_ms: 10,
      execution_log: inner_execution_log.clone(),
    };
    let inner_flow = AsyncFlow::new(Box::new(inner_node));

    // Create outer flow with nested flow node
    let nested_node = NestedFlowNode {
      inner_flow,
      execution_log: outer_execution_log.clone(),
    };
    let outer_flow = AsyncFlow::new(Box::new(nested_node));

    let shared = SharedState::new();
    let result = outer_flow.run_async(&shared).await;

    assert!(result.is_ok());

    let inner_log = inner_execution_log.lock().unwrap();
    let outer_log = outer_execution_log.lock().unwrap();

    assert_eq!(inner_log.len(), 1);
    assert_eq!(outer_log.len(), 1);
  }

  #[tokio::test]
  async fn test_async_flow_conditional_routing() {
    // Test conditional async routing
    let condition_node = ConditionalAsyncNode {
      condition_key: "should_succeed".to_string(),
      true_action: "success".to_string(),
      false_action: "failure".to_string(),
      delay_ms: 10,
    };

    let success_node = SimpleAsyncNode {
      id: "success".to_string(),
      next_action: None,
      delay_ms: 10,
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let failure_node = SimpleAsyncNode {
      id: "failure".to_string(),
      next_action: None,
      delay_ms: 10,
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let mut flow = AsyncFlow::new(Box::new(condition_node));
    flow.add_node("success".to_string(), Box::new(success_node));
    flow.add_node("failure".to_string(), Box::new(failure_node));

    // Test success path
    let shared = SharedState::new();
    shared.insert("should_succeed".to_string(), Value::Bool(true));
    let result = flow.run_async(&shared).await;
    assert!(result.is_ok());
    assert!(shared.contains_key("success_executed"));

    // Test failure path
    let shared = SharedState::new();
    shared.insert("should_succeed".to_string(), Value::Bool(false));
    let result = flow.run_async(&shared).await;
    assert!(result.is_ok());
    assert!(shared.contains_key("failure_executed"));
  }

  #[tokio::test]
  async fn test_async_flow_timeout_control() {
    // Test flow-level timeout control
    let slow_node = SimpleAsyncNode {
      id: "slow".to_string(),
      next_action: None,
      delay_ms: 1000, // 1 second
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let mut flow = AsyncFlow::new(Box::new(slow_node));
    flow.set_timeout(Duration::from_millis(100)); // 100ms timeout

    let shared = SharedState::new();
    let result = flow.run_async(&shared).await;

    assert!(result.is_err());
    // Should be a timeout error
    assert!(matches!(
      result.unwrap_err(),
      AgentFlowError::TimeoutExceeded { .. }
    ));
  }

  #[tokio::test]
  async fn test_async_flow_error_propagation() {
    // Test error propagation in async flows
    let failing_node = SimpleAsyncNode {
      id: "failing".to_string(),
      next_action: None,
      delay_ms: 0,
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };
    // Configure node to fail

    let flow = AsyncFlow::new(Box::new(failing_node));

    let shared = SharedState::new();
    shared.insert("should_fail".to_string(), Value::Bool(true));

    let result = flow.run_async(&shared).await;
    assert!(result.is_err());
  }

  #[tokio::test]
  async fn test_async_flow_cancellation() {
    // Test flow cancellation
    let long_running_node = SimpleAsyncNode {
      id: "long_running".to_string(),
      next_action: None,
      delay_ms: 5000, // 5 seconds
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let flow = AsyncFlow::new(Box::new(long_running_node));
    let shared = SharedState::new();

    // Start flow but cancel after 50ms
    let handle = tokio::spawn(async move { flow.run_async(&shared).await });

    tokio::time::sleep(Duration::from_millis(50)).await;
    handle.abort();

    let result = handle.await;
    assert!(result.is_err()); // Should be cancelled
  }

  #[tokio::test]
  async fn test_async_flow_backpressure() {
    // Test backpressure handling in batch processing
    let batch_node = BatchProcessingNode {
      batch_size: 2,
      processing_delay_ms: 100,
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let mut flow = AsyncFlow::new(Box::new(batch_node));
    flow.set_max_concurrent_batches(2); // Limit concurrent batches

    let shared = SharedState::new();
    // Add many items to test backpressure
    let items: Vec<Value> = (0..20)
      .map(|i| Value::Number(serde_json::Number::from(i)))
      .collect();
    shared.insert("batch_items".to_string(), Value::Array(items));

    let start = Instant::now();
    let result = flow.run_async(&shared).await;
    let elapsed = start.elapsed();

    assert!(result.is_ok());

    // With backpressure, should take longer than processing all at once
    // but less than sequential processing
    assert!(elapsed > Duration::from_millis(500)); // At least some batching delay
    assert!(elapsed < Duration::from_millis(2000)); // But not fully sequential
  }

  #[tokio::test]
  async fn test_async_flow_state_preservation() {
    // Test that shared state is preserved across async node executions
    let node1 = SimpleAsyncNode {
      id: "node1".to_string(),
      next_action: Some("node2".to_string()),
      delay_ms: 10,
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let node2 = SimpleAsyncNode {
      id: "node2".to_string(),
      next_action: None,
      delay_ms: 10,
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let mut flow = AsyncFlow::new(Box::new(node1));
    flow.add_node("node2".to_string(), Box::new(node2));

    let shared = SharedState::new();
    shared.insert(
      "initial_value".to_string(),
      Value::String("test".to_string()),
    );

    let result = flow.run_async(&shared).await;
    assert!(result.is_ok());

    // Initial value should still be present
    assert_eq!(
      shared.get("initial_value").unwrap(),
      Value::String("test".to_string())
    );

    // Both nodes should have executed
    assert!(shared.contains_key("node1_executed"));
    assert!(shared.contains_key("node2_executed"));
  }
}

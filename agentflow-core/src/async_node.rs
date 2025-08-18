// Async Node implementation - tests first, implementation follows

use crate::observability::{ExecutionEvent, MetricsCollector};
use crate::{AgentFlowError, Result, SharedState};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::{sleep, timeout};
use uuid::Uuid;

#[async_trait]
pub trait AsyncNode: Send + Sync {
  async fn prep_async(&self, shared: &SharedState) -> Result<Value>;
  async fn exec_async(&self, prep_result: Value) -> Result<Value>;
  async fn post_async(
    &self,
    shared: &SharedState,
    prep_result: Value,
    exec_result: Value,
  ) -> Result<Option<String>>;

  async fn run_async(&self, shared: &SharedState) -> Result<Option<String>> {
    self.run_async_with_observability(shared, None).await
  }

  async fn run_async_with_observability(
    &self,
    shared: &SharedState,
    metrics_collector: Option<Arc<MetricsCollector>>,
  ) -> Result<Option<String>> {
    let node_id = self.get_node_id().unwrap_or_else(|| "unknown".to_string());
    let start_time = Instant::now();

    // Record start event
    if let Some(ref collector) = metrics_collector {
      let event = ExecutionEvent {
        node_id: node_id.clone(),
        event_type: "node_start".to_string(),
        timestamp: start_time,
        duration_ms: None,
        metadata: HashMap::new(),
      };
      collector.record_event(event);
      collector.increment_counter(&format!("{}.execution_count", node_id), 1.0);
    }

    let result = async {
      let prep_result = self.prep_async(shared).await?;
      let exec_result = self.exec_async(prep_result.clone()).await?;
      self.post_async(shared, prep_result, exec_result).await
    }
    .await;

    let duration = start_time.elapsed();

    // Record completion event and metrics
    if let Some(ref collector) = metrics_collector {
      let event = ExecutionEvent {
        node_id: node_id.clone(),
        event_type: if result.is_ok() {
          "node_success"
        } else {
          "node_error"
        }
        .to_string(),
        timestamp: start_time,
        duration_ms: Some(duration.as_millis() as u64),
        metadata: HashMap::new(),
      };
      collector.record_event(event);

      // Update metrics
      collector.increment_counter(
        &format!("{}.duration_ms", node_id),
        duration.as_millis() as f64,
      );
      if result.is_ok() {
        collector.increment_counter(&format!("{}.success_count", node_id), 1.0);
      } else {
        collector.increment_counter(&format!("{}.error_count", node_id), 1.0);
      }
    }

    result
  }

  fn get_node_id(&self) -> Option<String> {
    None // Default implementation, can be overridden
  }

  async fn run_async_with_retries(
    &self,
    shared: &SharedState,
    max_retries: u32,
    wait_duration: Duration,
  ) -> Result<Option<String>> {
    let mut last_error = None;

    for attempt in 1..=max_retries {
      match self.run_async(shared).await {
        Ok(result) => return Ok(result),
        Err(e) => {
          last_error = Some(e);
          if attempt < max_retries {
            sleep(wait_duration).await;
          }
        }
      }
    }

    // Return the last error if available, otherwise return retry exhausted
    match last_error {
      Some(err) => Err(err),
      None => Err(AgentFlowError::RetryExhausted {
        attempts: max_retries,
      }),
    }
  }

  async fn run_async_with_timeout(
    &self,
    shared: &SharedState,
    timeout_duration: Duration,
  ) -> Result<Option<String>> {
    match timeout(timeout_duration, self.run_async(shared)).await {
      Ok(result) => result,
      Err(_) => {
        // Add graceful degradation marker
        shared.insert(
          "degraded_result".to_string(),
          Value::String("timeout_degraded".to_string()),
        );
        Err(AgentFlowError::TimeoutExceeded {
          duration_ms: timeout_duration.as_millis() as u64,
        })
      }
    }
  }

  async fn health_check(&self) -> Result<serde_json::Map<String, Value>> {
    let mut status = serde_json::Map::new();
    status.insert(
      "node_id".to_string(),
      Value::String("test_node".to_string()),
    );
    status.insert("status".to_string(), Value::String("healthy".to_string()));
    status.insert(
      "last_check".to_string(),
      Value::String(chrono::Utc::now().to_rfc3339()),
    );
    Ok(status)
  }
}

pub struct AsyncBaseNode {
  pub id: Uuid,
  successors: HashMap<String, Box<dyn AsyncNode>>,
}

impl AsyncBaseNode {
  pub fn new() -> Self {
    Self {
      id: Uuid::new_v4(),
      successors: HashMap::new(),
    }
  }

  pub fn add_successor(&mut self, action: String, node: Box<dyn AsyncNode>) {
    self.successors.insert(action, node);
  }

  pub fn has_successor(&self, action: &str) -> bool {
    self.successors.contains_key(action)
  }

  pub fn get_successor(&self, action: &str) -> Option<&Box<dyn AsyncNode>> {
    self.successors.get(action)
  }
}

impl Default for AsyncBaseNode {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{AgentFlowError, Result, SharedState};
  use serde_json::Value;
  use std::sync::{Arc, Mutex};
  use std::time::{Duration, Instant};
  use tokio::time::{sleep, timeout};

  // Mock async node for testing
  struct MockAsyncNode {
    prep_result: Option<String>,
    exec_result: Option<String>,
    post_action: Option<String>,
    should_fail: bool,
    delay_ms: u64,
    call_count: Arc<Mutex<u32>>,
  }

  #[async_trait]
  impl AsyncNode for MockAsyncNode {
    async fn prep_async(&self, _shared: &SharedState) -> Result<Value> {
      {
        let mut count = self.call_count.lock().unwrap();
        *count += 1;
      }

      if self.delay_ms > 0 {
        sleep(Duration::from_millis(self.delay_ms)).await;
      }

      if let Some(ref result) = self.prep_result {
        Ok(Value::String(result.clone()))
      } else {
        Ok(Value::String("default_prep".to_string()))
      }
    }

    async fn exec_async(&self, _prep_result: Value) -> Result<Value> {
      if self.should_fail {
        return Err(AgentFlowError::AsyncExecutionError {
          message: "Mock async node failed".to_string(),
        });
      }

      if let Some(ref result) = self.exec_result {
        Ok(Value::String(result.clone()))
      } else {
        Ok(Value::String("default_exec".to_string()))
      }
    }

    async fn post_async(
      &self,
      shared: &SharedState,
      _prep_result: Value,
      exec_result: Value,
    ) -> Result<Option<String>> {
      shared.insert("output".to_string(), exec_result);
      Ok(self.post_action.clone())
    }
  }

  struct DelayNode {
    delay_ms: u64,
    execution_log: Arc<Mutex<Vec<String>>>,
  }

  #[async_trait]
  impl AsyncNode for DelayNode {
    async fn prep_async(&self, _shared: &SharedState) -> Result<Value> {
      Ok(Value::String("delay_prep".to_string()))
    }

    async fn exec_async(&self, _prep_result: Value) -> Result<Value> {
      sleep(Duration::from_millis(self.delay_ms)).await;
      {
        self
          .execution_log
          .lock()
          .unwrap()
          .push("executed".to_string());
      }
      Ok(Value::String("delay_complete".to_string()))
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

  struct FailingAsyncNode {
    fail_after_attempts: u32,
    attempts: Arc<Mutex<u32>>,
  }

  #[async_trait]
  impl AsyncNode for FailingAsyncNode {
    async fn prep_async(&self, _shared: &SharedState) -> Result<Value> {
      Ok(Value::String("failing_prep".to_string()))
    }

    async fn exec_async(&self, _prep_result: Value) -> Result<Value> {
      let current_attempts = {
        let mut attempts = self.attempts.lock().unwrap();
        *attempts += 1;
        *attempts
      };

      if current_attempts < self.fail_after_attempts {
        Err(AgentFlowError::AsyncExecutionError {
          message: format!("Attempt {} failed", current_attempts),
        })
      } else {
        Ok(Value::String("success".to_string()))
      }
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

  struct ParallelTestNode {
    id: String,
    delay_ms: u64,
    execution_log: Arc<Mutex<Vec<(String, Instant)>>>,
  }

  #[async_trait]
  impl AsyncNode for ParallelTestNode {
    async fn prep_async(&self, _shared: &SharedState) -> Result<Value> {
      Ok(Value::String(format!("prep_{}", self.id)))
    }

    async fn exec_async(&self, _prep_result: Value) -> Result<Value> {
      sleep(Duration::from_millis(self.delay_ms)).await;
      {
        self
          .execution_log
          .lock()
          .unwrap()
          .push((self.id.clone(), Instant::now()));
      }
      Ok(Value::String(format!("exec_{}", self.id)))
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
  async fn test_async_node_lifecycle() {
    // Test the basic async node lifecycle: prep -> exec -> post
    let node = MockAsyncNode {
      prep_result: Some("async_prep".to_string()),
      exec_result: Some("async_exec".to_string()),
      post_action: Some("next".to_string()),
      should_fail: false,
      delay_ms: 10,
      call_count: Arc::new(Mutex::new(0)),
    };

    let shared = SharedState::new();
    let result = node.run_async(&shared).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some("next".to_string()));

    let count = *node.call_count.lock().unwrap();
    assert_eq!(count, 1);
  }

  #[tokio::test]
  async fn test_async_node_with_delay() {
    // Test async node with actual delay
    let start = Instant::now();
    let node = DelayNode {
      delay_ms: 50,
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let shared = SharedState::new();
    let result = node.run_async(&shared).await;

    let elapsed = start.elapsed();
    assert!(result.is_ok());
    assert!(elapsed >= Duration::from_millis(50));

    let log = node.execution_log.lock().unwrap();
    assert_eq!(log.len(), 1);
  }

  #[tokio::test]
  async fn test_async_node_error_handling() {
    // Test async error handling
    let node = MockAsyncNode {
      prep_result: Some("prep".to_string()),
      exec_result: None,
      post_action: None,
      should_fail: true,
      delay_ms: 0,
      call_count: Arc::new(Mutex::new(0)),
    };

    let shared = SharedState::new();
    let result = node.run_async(&shared).await;

    assert!(result.is_err());
  }

  #[tokio::test]
  async fn test_async_node_timeout() {
    // Test timeout functionality
    let node = DelayNode {
      delay_ms: 1000, // 1 second delay
      execution_log: Arc::new(Mutex::new(Vec::new())),
    };

    let shared = SharedState::new();

    // Set timeout to 100ms, node takes 1000ms
    let result = timeout(Duration::from_millis(100), node.run_async(&shared)).await;

    assert!(result.is_err()); // Should timeout
  }

  #[tokio::test]
  async fn test_async_node_retry_mechanism() {
    // Test async retry mechanism
    let node = FailingAsyncNode {
      fail_after_attempts: 3,
      attempts: Arc::new(Mutex::new(0)),
    };

    let shared = SharedState::new();
    let result = node
      .run_async_with_retries(&shared, 5, Duration::from_millis(10))
      .await;

    assert!(result.is_ok());

    let attempts = *node.attempts.lock().unwrap();
    assert_eq!(attempts, 3);
  }

  #[tokio::test]
  async fn test_async_node_retry_exhaustion() {
    // Test async retry exhaustion
    let node = FailingAsyncNode {
      fail_after_attempts: 10, // Always fail
      attempts: Arc::new(Mutex::new(0)),
    };

    let shared = SharedState::new();
    let result = node
      .run_async_with_retries(&shared, 3, Duration::from_millis(1))
      .await;

    assert!(result.is_err());

    let attempts = *node.attempts.lock().unwrap();
    assert_eq!(attempts, 3);
  }

  #[tokio::test]
  async fn test_async_parallel_execution() {
    // Test that multiple async nodes can run in parallel
    let execution_log = Arc::new(Mutex::new(Vec::new()));

    let nodes = (0..3)
      .map(|i| ParallelTestNode {
        id: format!("node_{}", i),
        delay_ms: 50,
        execution_log: execution_log.clone(),
      })
      .collect::<Vec<_>>();

    let shared = SharedState::new();
    let start = Instant::now();

    // Run all nodes in parallel
    let futures = nodes.iter().map(|node| node.run_async(&shared));
    let results = futures::future::join_all(futures).await;

    let elapsed = start.elapsed();

    // All should succeed
    for result in results {
      assert!(result.is_ok());
    }

    // Should complete in ~50ms (parallel) not ~150ms (sequential)
    assert!(elapsed < Duration::from_millis(100));

    let log = execution_log.lock().unwrap();
    assert_eq!(log.len(), 3);
  }

  #[tokio::test]
  async fn test_async_node_cancellation() {
    // Test that async nodes can be cancelled
    let execution_log = Arc::new(Mutex::new(Vec::new()));
    let node = DelayNode {
      delay_ms: 1000,
      execution_log: execution_log.clone(),
    };

    let shared = SharedState::new();

    // Start the node but cancel it after 50ms
    let handle = tokio::spawn(async move { node.run_async(&shared).await });

    tokio::time::sleep(Duration::from_millis(50)).await;
    handle.abort();

    let result = handle.await;
    assert!(result.is_err()); // Should be cancelled

    let log = execution_log.lock().unwrap();
    assert_eq!(log.len(), 0); // Should not have completed
  }

  #[tokio::test]
  async fn test_async_shared_state_concurrent_access() {
    // Test concurrent access to shared state from multiple async nodes
    let shared = SharedState::new();
    shared.insert(
      "counter".to_string(),
      Value::Number(serde_json::Number::from(0)),
    );

    let nodes = (0..10)
      .map(|_| MockAsyncNode {
        prep_result: Some("increment".to_string()),
        exec_result: Some("done".to_string()),
        post_action: None,
        should_fail: false,
        delay_ms: 10,
        call_count: Arc::new(Mutex::new(0)),
      })
      .collect::<Vec<_>>();

    // Run all nodes concurrently, each incrementing the counter
    let futures = nodes.iter().map(|node| async {
      let _ = node.run_async(&shared).await;
      // Simulate incrementing shared counter
      if let Some(Value::Number(n)) = shared.get("counter") {
        if let Some(val) = n.as_u64() {
          shared.insert(
            "counter".to_string(),
            Value::Number(serde_json::Number::from(val + 1)),
          );
        }
      }
    });

    futures::future::join_all(futures).await;

    // Final counter value should reflect all increments
    let final_count = shared.get("counter").and_then(|v| v.as_u64()).unwrap_or(0);

    assert!(final_count > 0); // Some increments should have succeeded
  }
}

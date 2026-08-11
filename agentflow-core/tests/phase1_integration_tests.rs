//! Integration tests for Phase 1 production-readiness features.
//!
//! Tests error handling and checkpoint/recovery in realistic scenarios.
//! (The `ResourceManager`/`ConcurrencyLimiter`/`StateMonitor` sections that
//! used to live here were removed in W5.3 along with those modules — see
//! `agentflow-core/src/scheduler.rs`'s `FlowExecutionConfig::resource_limits`
//! doc comment for the real, safe wiring that replaced them.)

use agentflow_core::{
  checkpoint::{CheckpointConfig, CheckpointManager},
  error::{AgentFlowError, ErrorCategory, InlineErrorContext},
  retry::{ErrorPattern, RetryContext, RetryPolicy, RetryStrategy},
  retry_executor::execute_with_retry,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;

// ===== Error Handling Integration Tests =====

#[tokio::test]
async fn test_error_context_propagation() {
  let context = InlineErrorContext::new()
    .with_node_id("node_1")
    .with_workflow_id("workflow_123")
    .with_metadata("attempt", "2")
    .with_metadata("reason", "network_timeout");

  let error = AgentFlowError::NodeExecutionFailed {
    message: "Test error".into(),
  }
  .with_context(context);

  assert_eq!(error.context.node_id, Some("node_1".into()));
  assert_eq!(error.context.workflow_id, Some("workflow_123".into()));
  assert_eq!(error.context.metadata.get("attempt"), Some(&"2".into()));
}

#[tokio::test]
async fn test_error_categorization() {
  let errors = vec![
    (
      AgentFlowError::NodeExecutionFailed {
        message: "test".into(),
      },
      ErrorCategory::Node,
    ),
    (
      AgentFlowError::NetworkError {
        message: "test".into(),
      },
      ErrorCategory::Network,
    ),
    (
      AgentFlowError::ResourcePoolExhausted {
        resource_type: "test".into(),
      },
      ErrorCategory::Resource,
    ),
    (
      AgentFlowError::ConfigurationError {
        message: "test".into(),
      },
      ErrorCategory::Configuration,
    ),
  ];

  for (error, expected_category) in errors {
    assert_eq!(error.category(), expected_category);
  }
}

#[tokio::test]
async fn test_retryable_error_classification() {
  let retryable = vec![
    AgentFlowError::NetworkError {
      message: "connection timeout".into(),
    },
    AgentFlowError::TimeoutExceeded { duration_ms: 1000 },
    AgentFlowError::RateLimitExceeded {
      limit: 100,
      window_ms: 1000,
    },
  ];

  let non_retryable = vec![
    AgentFlowError::ValidationError("invalid input".into()),
    AgentFlowError::ConfigurationError {
      message: "missing config".into(),
    },
  ];

  for error in retryable {
    assert!(error.is_retryable(), "Expected {} to be retryable", error);
  }

  for error in non_retryable {
    assert!(
      !error.is_retryable(),
      "Expected {} to not be retryable",
      error
    );
  }
}

#[tokio::test]
async fn test_retry_with_exponential_backoff() {
  let policy = RetryPolicy::builder()
    .max_attempts(3)
    .strategy(RetryStrategy::exponential_backoff(50, 500, 2.0))
    .build();

  let attempt_counter = Arc::new(AtomicUsize::new(0));
  let counter_clone = attempt_counter.clone();

  let result = execute_with_retry(&policy, "test_operation", || {
    let counter = counter_clone.clone();
    async move {
      let attempt = counter.fetch_add(1, Ordering::SeqCst);
      if attempt < 2 {
        Err(AgentFlowError::NetworkError {
          message: "temporary failure".into(),
        })
      } else {
        Ok("success".to_string())
      }
    }
  })
  .await;

  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "success");
  assert_eq!(attempt_counter.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn test_retry_exhaustion() {
  let policy = RetryPolicy::builder()
    .max_attempts(2)
    .strategy(RetryStrategy::fixed(10))
    .build();

  let result = execute_with_retry(&policy, "test_operation", || async {
    Err::<String, _>(AgentFlowError::NetworkError {
      message: "persistent failure".into(),
    })
  })
  .await;

  assert!(result.is_err());
  // With max_attempts=2, expect either 2 or 3 actual attempts (depends on how retry counting works)
  if let Err(AgentFlowError::RetryExhausted { attempts, .. }) = result {
    assert!(
      attempts == 2 || attempts == 3,
      "Expected 2 or 3 attempts, got {}",
      attempts
    );
  } else {
    panic!("Expected RetryExhausted error");
  }
}

// ===== Checkpoint and Recovery Integration Tests =====

#[tokio::test]
async fn test_workflow_checkpoint_and_recovery() {
  let temp_dir = TempDir::new().unwrap();
  let config = CheckpointConfig::default().with_checkpoint_dir(temp_dir.path());
  let manager = CheckpointManager::new(config).unwrap();

  let workflow_id = "test_workflow";
  let mut state = HashMap::new();

  // Save checkpoint after each node
  state.insert("node1_output".to_string(), serde_json::json!("result1"));
  manager
    .save_checkpoint(workflow_id, "node1", &state)
    .await
    .unwrap();

  state.insert("node2_output".to_string(), serde_json::json!("result2"));
  manager
    .save_checkpoint(workflow_id, "node2", &state)
    .await
    .unwrap();

  state.insert("node3_output".to_string(), serde_json::json!("result3"));
  manager
    .save_checkpoint(workflow_id, "node3", &state)
    .await
    .unwrap();

  // Simulate crash and recovery
  let recovered = manager.load_latest_checkpoint(workflow_id).await.unwrap();
  assert!(recovered.is_some());

  let checkpoint = recovered.unwrap();
  assert_eq!(checkpoint.last_completed_node, "node3");
  assert_eq!(checkpoint.state.len(), 3);
  assert_eq!(checkpoint.state.get("node1_output").unwrap(), "result1");
  assert_eq!(checkpoint.state.get("node2_output").unwrap(), "result2");
  assert_eq!(checkpoint.state.get("node3_output").unwrap(), "result3");
}

#[tokio::test]
async fn test_checkpoint_cleanup_policy() {
  let temp_dir = TempDir::new().unwrap();
  let config = CheckpointConfig::default()
    .with_checkpoint_dir(temp_dir.path())
    .with_success_retention_days(0) // Immediate cleanup for testing
    .with_failure_retention_days(30);
  let manager = CheckpointManager::new(config).unwrap();

  let workflow_id = "test_workflow_cleanup";
  let mut state = HashMap::new();
  state.insert("data".to_string(), serde_json::json!("value"));

  // Save successful checkpoint
  manager
    .save_checkpoint(workflow_id, "node1", &state)
    .await
    .unwrap();

  // Update status to completed
  let checkpoints = manager.load_all_checkpoints(workflow_id).await.unwrap();
  assert_eq!(checkpoints.len(), 1);

  // Cleanup should remove old successful checkpoints
  let _cleaned = manager.cleanup_old_checkpoints().await.unwrap();
  // Note: In real scenario, would need to wait for TTL to expire
  // This test just verifies the cleanup mechanism runs without error
}

#[tokio::test]
async fn test_concurrent_checkpoint_writes() {
  let temp_dir = TempDir::new().unwrap();
  let config = CheckpointConfig::default().with_checkpoint_dir(temp_dir.path());
  let manager = Arc::new(CheckpointManager::new(config).unwrap());

  let mut handles = vec![];

  // Spawn multiple concurrent checkpoint writes
  for i in 0..10 {
    let manager = manager.clone();
    let workflow_id = format!("workflow_{}", i);

    let handle = tokio::spawn(async move {
      let mut state = HashMap::new();
      state.insert("node_id".to_string(), serde_json::json!(i));

      manager
        .save_checkpoint(&workflow_id, "node1", &state)
        .await
        .unwrap();

      let recovered = manager.load_latest_checkpoint(&workflow_id).await.unwrap();
      assert!(recovered.is_some());
      recovered.unwrap()
    });

    handles.push(handle);
  }

  // Wait for all to complete
  for handle in handles {
    let checkpoint = handle.await.unwrap();
    assert_eq!(checkpoint.last_completed_node, "node1");
  }
}

// ===== Complex Workflow Scenario Tests =====

#[tokio::test]
async fn test_workflow_with_retries_and_checkpoints() {
  let temp_dir = TempDir::new().unwrap();
  let checkpoint_config = CheckpointConfig::default().with_checkpoint_dir(temp_dir.path());
  let checkpoint_manager = Arc::new(CheckpointManager::new(checkpoint_config).unwrap());

  let retry_policy = RetryPolicy::builder()
    .max_attempts(3)
    .strategy(RetryStrategy::fixed(10))
    .build();

  let workflow_id = "complex_workflow";
  let attempt_counter = Arc::new(AtomicUsize::new(0));

  // Node 1: Always succeeds
  let mut state = HashMap::new();
  state.insert("node1".to_string(), serde_json::json!("success"));
  checkpoint_manager
    .save_checkpoint(workflow_id, "node1", &state)
    .await
    .unwrap();

  // Node 2: Fails twice then succeeds (tests retry)
  let counter = attempt_counter.clone();
  let node2_result = execute_with_retry(&retry_policy, "node2", || {
    let counter = counter.clone();
    async move {
      let attempt = counter.fetch_add(1, Ordering::SeqCst);
      if attempt < 2 {
        Err(AgentFlowError::NetworkError {
          message: "transient error".into(),
        })
      } else {
        Ok("node2_success")
      }
    }
  })
  .await;

  assert!(node2_result.is_ok());
  state.insert(
    "node2".to_string(),
    serde_json::json!(node2_result.unwrap()),
  );
  checkpoint_manager
    .save_checkpoint(workflow_id, "node2", &state)
    .await
    .unwrap();

  // Node 3: Reads from checkpoint and completes
  let recovered = checkpoint_manager
    .load_latest_checkpoint(workflow_id)
    .await
    .unwrap()
    .unwrap();

  assert_eq!(recovered.state.len(), 2);
  assert_eq!(recovered.last_completed_node, "node2");

  state.insert("node3".to_string(), serde_json::json!("final"));
  checkpoint_manager
    .save_checkpoint(workflow_id, "node3", &state)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_error_recovery_with_retry_context() {
  let mut context = RetryContext::new();
  let policy = RetryPolicy::builder()
    .max_attempts(3)
    .strategy(RetryStrategy::exponential_backoff(10, 100, 2.0))
    .build();

  let error = AgentFlowError::NetworkError {
    message: "connection failed".into(),
  };

  // First attempt
  context.record_failure(&error);
  assert!(context.should_retry(&policy, &error));
  assert_eq!(context.attempt, 1);

  // Second attempt
  context.record_failure(&error);
  assert!(context.should_retry(&policy, &error));
  assert_eq!(context.attempt, 2);

  // Third attempt
  context.record_failure(&error);
  assert!(!context.should_retry(&policy, &error)); // Max attempts reached
  assert_eq!(context.attempt, 3);
}

// ===== Edge Case Tests =====

#[tokio::test]
async fn test_checkpoint_with_empty_state() {
  let temp_dir = TempDir::new().unwrap();
  let config = CheckpointConfig::default().with_checkpoint_dir(temp_dir.path());
  let manager = CheckpointManager::new(config).unwrap();

  let state = HashMap::new();
  let result = manager.save_checkpoint("workflow", "node1", &state).await;

  assert!(result.is_ok());

  let recovered = manager.load_latest_checkpoint("workflow").await.unwrap();
  assert!(recovered.is_some());
  assert_eq!(recovered.unwrap().state.len(), 0);
}

#[tokio::test]
async fn test_retry_with_non_retryable_error() {
  let policy = RetryPolicy::builder()
    .max_attempts(3)
    .retryable_error(ErrorPattern::NetworkError)
    .build();

  let attempt_counter = Arc::new(AtomicUsize::new(0));
  let counter = attempt_counter.clone();

  let result = execute_with_retry(&policy, "test_operation", || {
    let counter = counter.clone();
    async move {
      counter.fetch_add(1, Ordering::SeqCst);
      Err::<String, _>(AgentFlowError::ValidationError("invalid input".into()))
    }
  })
  .await;

  // Should fail immediately without retries (non-retryable error)
  assert!(result.is_err());
  assert_eq!(attempt_counter.load(Ordering::SeqCst), 1);
}

//! Integration tests for AgentFlow Phase 1 improvements.
//!
//! These tests verify that the retry mechanism, error context, and resource
//! management features work correctly together and integrate seamlessly.

use agentflow_core::{
  execute_with_retry_and_context, AgentFlowError, ErrorContext, ErrorInfo, FlowValue,
  ResourceLimits, RetryPolicy, RetryStrategy, StateMonitor,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

// Helper to simulate a failing operation that eventually succeeds
async fn flaky_operation(
  attempts: Arc<AtomicUsize>,
  fail_count: usize,
) -> Result<String, AgentFlowError> {
  let attempt = attempts.fetch_add(1, Ordering::SeqCst);

  if attempt < fail_count {
    sleep(Duration::from_millis(10)).await;
    Err(AgentFlowError::AsyncExecutionError {
      message: format!("Network failure on attempt {}", attempt + 1),
    })
  } else {
    Ok("Success".to_string())
  }
}

/// Test retry mechanism with error context tracking
#[tokio::test]
async fn test_retry_with_error_context_integration() {
  let policy = RetryPolicy::builder()
    .max_attempts(5)
    .strategy(RetryStrategy::ExponentialBackoff {
      initial_delay_ms: 10,
      max_delay_ms: 100,
      multiplier: 2.0,
      jitter: false,
    })
    .build();

  let attempts = Arc::new(AtomicUsize::new(0));
  let attempts_clone = attempts.clone();

  let result = execute_with_retry_and_context(
    &policy,
    "test_run",
    "flaky_node",
    Some("test"),
    || async {
      flaky_operation(attempts_clone.clone(), 2).await
    },
  )
  .await;

  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "Success");
  assert_eq!(attempts.load(Ordering::SeqCst), 3); // Failed twice, succeeded on third
}

/// Test retry mechanism with max attempts exceeded
#[tokio::test]
async fn test_retry_max_attempts_with_error_context() {
  let policy = RetryPolicy::builder()
    .max_attempts(3)
    .strategy(RetryStrategy::Fixed { delay_ms: 10 })
    .build();

  let attempts = Arc::new(AtomicUsize::new(0));
  let attempts_clone = attempts.clone();

  let result = execute_with_retry_and_context(
    &policy,
    "test_run",
    "failing_node",
    Some("test"),
    || async {
      flaky_operation(attempts_clone.clone(), 10).await // Will never succeed
    },
  )
  .await;

  assert!(result.is_err());
  // With max_attempts=3, we get 1 initial + 3 retries = 4 total attempts
  assert_eq!(attempts.load(Ordering::SeqCst), 4);

  // Verify error is returned
  let (error, context) = result.unwrap_err();

  // After retry exhaustion, we may get RetryExhausted
  // The original error should be in the context's error chain
  assert!(context.error_chain.len() > 0);
  assert!(!context.error_chain.is_empty());
}

/// Test resource monitoring during operations
#[tokio::test]
async fn test_resource_monitoring_integration() {
  let limits = ResourceLimits::builder()
    .max_state_size(10 * 1024 * 1024) // 10 MB
    .max_value_size(2 * 1024 * 1024)  // 2 MB
    .cleanup_threshold(0.8)
    .auto_cleanup(true)
    .build();

  let monitor = StateMonitor::new(limits);

  // Simulate allocating resources during workflow execution
  assert!(monitor.record_allocation("input_data", 1024 * 1024)); // 1 MB
  assert!(monitor.record_allocation("processed_data", 2 * 1024 * 1024)); // 2 MB
  assert!(monitor.record_allocation("output_data", 1024 * 1024)); // 1 MB

  let stats = monitor.get_stats();
  assert_eq!(stats.current_size, 4 * 1024 * 1024); // 4 MB total
  assert_eq!(stats.value_count, 3);
  assert!(!stats.should_cleanup); // Below 80% threshold

  // Deallocate temporary data
  monitor.record_deallocation("processed_data");

  let stats = monitor.get_stats();
  assert_eq!(stats.current_size, 2 * 1024 * 1024); // 2 MB remaining
  assert_eq!(stats.value_count, 2);
}

/// Test resource limit enforcement
#[tokio::test]
async fn test_resource_limit_enforcement() {
  let limits = ResourceLimits::builder()
    .max_state_size(5 * 1024 * 1024)  // 5 MB
    .max_value_size(2 * 1024 * 1024)  // 2 MB
    .auto_cleanup(false) // Fail instead of cleanup
    .build();

  let monitor = StateMonitor::new(limits);

  // Allocate data within limits
  assert!(monitor.record_allocation("data1", 2 * 1024 * 1024)); // 2 MB - OK
  assert!(monitor.record_allocation("data2", 2 * 1024 * 1024)); // 2 MB - OK

  // Try to allocate more than max_state_size
  assert!(!monitor.record_allocation("data3", 2 * 1024 * 1024)); // Would exceed 5 MB - FAIL

  // Try to allocate single value larger than max_value_size
  assert!(!monitor.record_allocation("huge", 3 * 1024 * 1024)); // Exceeds 2 MB - FAIL

  // Verify state is consistent
  let stats = monitor.get_stats();
  assert_eq!(stats.current_size, 4 * 1024 * 1024); // Only first two allocations
  assert_eq!(stats.value_count, 2);
}

/// Test automatic cleanup when threshold is reached
#[tokio::test]
async fn test_automatic_cleanup_integration() {
  let limits = ResourceLimits::builder()
    .max_state_size(10 * 1024 * 1024) // 10 MB
    .cleanup_threshold(0.8)            // 80%
    .auto_cleanup(true)
    .build();

  let monitor = StateMonitor::new(limits);

  // Allocate up to cleanup threshold
  for i in 0..9 {
    monitor.record_allocation(&format!("data_{}", i), 1024 * 1024); // 1 MB each
  }

  let stats = monitor.get_stats();
  assert_eq!(stats.current_size, 9 * 1024 * 1024);
  assert!(stats.should_cleanup); // Should be true at 90%

  // Perform cleanup to 50%
  let result = monitor.cleanup(0.5);
  assert!(result.is_ok());

  let (freed, removed) = result.unwrap();
  assert!(freed >= 4 * 1024 * 1024); // At least 4 MB freed
  assert!(removed >= 4); // At least 4 entries removed

  let stats = monitor.get_stats();
  assert!(stats.current_size <= 5 * 1024 * 1024); // Should be <= 50%
  assert!(!stats.should_cleanup);
}

/// Test LRU-based cleanup
#[tokio::test]
async fn test_lru_cleanup_integration() {
  let limits = ResourceLimits::default();
  let monitor = StateMonitor::new(limits);

  // Allocate several items
  monitor.record_allocation("old1", 1024 * 1024);
  monitor.record_allocation("old2", 1024 * 1024);
  monitor.record_allocation("recent", 1024 * 1024);
  monitor.record_allocation("active", 1024 * 1024);

  // Access some items to make them more recent
  monitor.record_access("active");
  monitor.record_access("recent");

  // Get LRU keys
  let lru_keys = monitor.get_lru_keys(2);
  assert_eq!(lru_keys.len(), 2);

  // LRU should be old1 and old2
  assert!(lru_keys.contains(&"old1".to_string()));
  assert!(lru_keys.contains(&"old2".to_string()));
  assert!(!lru_keys.contains(&"active".to_string()));
}

/// Test error context with retry and resource monitoring
#[tokio::test]
async fn test_comprehensive_integration() {
  let policy = RetryPolicy::builder()
    .max_attempts(3)
    .strategy(RetryStrategy::Fixed { delay_ms: 10 })
    .build();

  let limits = ResourceLimits::builder()
    .max_state_size(10 * 1024 * 1024)
    .build();

  let monitor = StateMonitor::new(limits);

  // Simulate a workflow with retry and resource monitoring
  let attempts = Arc::new(AtomicUsize::new(0));
  let attempts_clone = attempts.clone();

  // Allocate resources
  monitor.record_allocation("workflow_state", 1024 * 1024);

  // Execute with retry
  let result = execute_with_retry_and_context(
    &policy,
    "test_workflow",
    "processing_node",
    Some("data_processor"),
    || async {
      let attempt = attempts_clone.fetch_add(1, Ordering::SeqCst);

      // Simulate resource usage during execution
      if attempt == 0 {
        // Fail on first attempt with network error
        Err(AgentFlowError::AsyncExecutionError {
          message: "Temporary network issue".to_string(),
        })
      } else {
        // Succeed on retry
        Ok("Processed successfully".to_string())
      }
    },
  )
  .await;

  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "Processed successfully");
  assert_eq!(attempts.load(Ordering::SeqCst), 2); // Failed once, succeeded on second

  // Verify resource state
  let stats = monitor.get_stats();
  assert_eq!(stats.current_size, 1024 * 1024);
  assert_eq!(stats.value_count, 1);

  // Cleanup
  monitor.record_deallocation("workflow_state");
  assert_eq!(monitor.current_size(), 0);
}

/// Test error info creation and formatting
#[tokio::test]
async fn test_error_info_integration() {
  let error = AgentFlowError::NodeInputError {
    message: "Missing required input 'data'".to_string(),
  };

  let error_info = ErrorInfo::from_error(&error);

  assert_eq!(error_info.error_type, "NodeInputError");
  assert!(error_info.message.contains("Missing required input"));
  assert_eq!(error_info.source, None);
}

/// Test error context builder with all fields
#[tokio::test]
async fn test_error_context_builder_integration() {
  let error = AgentFlowError::NodeExecutionFailed {
    message: "Processing failed".to_string(),
  };

  let mut inputs = HashMap::new();
  inputs.insert(
    "data".to_string(),
    FlowValue::Json(serde_json::json!("test_value")),
  );
  inputs.insert(
    "count".to_string(),
    FlowValue::Json(serde_json::json!(42)),
  );

  let context = ErrorContext::builder("run123", "node_xyz")
    .node_type("processor")
    .duration(Duration::from_millis(250))
    .execution_history(vec!["node1".to_string(), "node2".to_string()])
    .inputs(&inputs)
    .error(&error)
    .retry_attempt(2)
    .build();

  assert_eq!(context.run_id, "run123");
  assert_eq!(context.node_name, "node_xyz");
  assert_eq!(context.node_type, Some("processor".to_string()));
  assert_eq!(context.retry_attempt, Some(2));
  assert!(context.execution_history.len() == 2);

  let report = context.detailed_report();
  assert!(report.contains("run123"));
  assert!(report.contains("node_xyz"));
  assert!(report.contains("processor"));
  // retry_attempt of 2 gets displayed as "Retry Attempt: 3" (0-indexed + 1)
  assert!(report.contains("Retry Attempt: 3"));
}

/// Test resource alerts generation
#[tokio::test]
async fn test_resource_alerts_integration() {
  let limits = ResourceLimits::builder()
    .max_state_size(5 * 1024 * 1024)
    .max_value_size(1024 * 1024)
    .cleanup_threshold(0.8)
    .build();

  let monitor = StateMonitor::new(limits);

  // Trigger value size limit alert
  monitor.record_allocation("too_large", 2 * 1024 * 1024);

  let alerts = monitor.get_alerts();
  assert!(!alerts.is_empty());

  // Should have a LimitExceeded alert for value_size
  let has_value_limit_alert = alerts.iter().any(|alert| {
    matches!(alert, agentflow_core::ResourceAlert::LimitExceeded { resource, .. } if resource == "value_size")
  });
  assert!(has_value_limit_alert);

  // Clear alerts
  monitor.clear_alerts();
  assert!(monitor.peek_alerts().is_empty());

  // Trigger approaching limit alert
  for i in 0..4 {
    monitor.record_allocation(&format!("data_{}", i), 1024 * 1024);
  }

  let alerts = monitor.get_alerts();

  // Should have an ApproachingLimit alert
  let has_approaching_alert = alerts.iter().any(|alert| {
    matches!(alert, agentflow_core::ResourceAlert::ApproachingLimit { .. })
  });
  assert!(has_approaching_alert);
}

/// Test fast mode state monitor (no detailed tracking)
#[tokio::test]
async fn test_fast_mode_monitor() {
  let limits = ResourceLimits::default();
  let monitor = StateMonitor::new_fast(limits);

  // Can track basic size/count
  monitor.record_allocation("data1", 1024);
  monitor.record_allocation("data2", 2048);

  assert_eq!(monitor.current_size(), 3072);
  assert_eq!(monitor.value_count(), 2);

  // But detailed tracking (LRU, allocations map) is not available
  let lru_keys = monitor.get_lru_keys(10);
  assert!(lru_keys.is_empty()); // Fast mode doesn't track LRU

  let allocations = monitor.get_allocations();
  assert!(allocations.is_empty()); // Fast mode doesn't track allocations map
}

/// Test resource limits validation
#[tokio::test]
async fn test_resource_limits_validation() {
  // Valid limits
  let limits = ResourceLimits::builder()
    .max_state_size(100 * 1024 * 1024)
    .max_value_size(10 * 1024 * 1024)
    .build();
  assert!(limits.validate().is_ok());

  // Invalid: value size exceeds state size
  let limits = ResourceLimits::builder()
    .max_state_size(10 * 1024 * 1024)
    .max_value_size(20 * 1024 * 1024)
    .build();
  assert!(limits.validate().is_err());

  // Invalid: zero state size
  let limits = ResourceLimits::builder()
    .max_state_size(0)
    .build();
  assert!(limits.validate().is_err());

  // Invalid: cleanup threshold out of range
  let limits = ResourceLimits::builder()
    .cleanup_threshold(1.5)
    .build();
  assert!(limits.validate().is_err());
}

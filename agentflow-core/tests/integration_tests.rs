//! Integration tests for AgentFlow Phase 1 improvements.
//!
//! These tests verify that the retry mechanism and `ResourceLimits`
//! validation work correctly. (The `StateMonitor`/`ResourceAlert` LRU
//! tracking + eviction tests that used to live here were removed in
//! W5.3 along with `state_monitor.rs` itself — its eviction model was
//! unsafe for `Flow`'s actual state pool semantics, see
//! `agentflow-core/src/scheduler.rs`'s `FlowExecutionConfig::resource_limits`
//! doc comment for the real, safe wiring that replaced it.)

use agentflow_core::{
  AgentFlowError, ResourceLimits, RetryPolicy, RetryStrategy, execute_with_retry,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Test retry mechanism in isolation (the resource-monitoring half of
/// this test's original name was removed in W5.3).
#[tokio::test]
async fn test_comprehensive_integration() {
  let policy = RetryPolicy::builder()
    .max_attempts(3)
    .strategy(RetryStrategy::Fixed { delay_ms: 10 })
    .build();

  let attempts = Arc::new(AtomicUsize::new(0));
  let attempts_clone = attempts.clone();

  let result = execute_with_retry(&policy, "processing_node", || async {
    let attempt = attempts_clone.fetch_add(1, Ordering::SeqCst);

    if attempt == 0 {
      Err(AgentFlowError::AsyncExecutionError {
        message: "Temporary network issue".to_string(),
      })
    } else {
      Ok("Processed successfully".to_string())
    }
  })
  .await;

  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "Processed successfully");
  assert_eq!(attempts.load(Ordering::SeqCst), 2); // Failed once, succeeded on second
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
  let limits = ResourceLimits::builder().max_state_size(0).build();
  assert!(limits.validate().is_err());

  // Invalid: cleanup threshold out of range
  let limits = ResourceLimits::builder().cleanup_threshold(1.5).build();
  assert!(limits.validate().is_err());
}

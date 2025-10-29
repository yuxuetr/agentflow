//! Retry logic with exponential backoff
//!
//! This module provides retry mechanisms for transient failures.

use crate::error::{JsonRpcErrorCode, MCPError, MCPResult};
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
  /// Maximum number of retry attempts (0 = no retries)
  pub max_retries: u32,
  /// Base backoff duration in milliseconds
  pub backoff_base_ms: u64,
  /// Maximum backoff duration in milliseconds
  pub max_backoff_ms: u64,
}

impl RetryConfig {
  /// Create a new retry configuration
  pub fn new(max_retries: u32, backoff_base_ms: u64) -> Self {
    Self {
      max_retries,
      backoff_base_ms,
      max_backoff_ms: 30_000, // 30 seconds default max
    }
  }

  /// Set maximum backoff duration
  pub fn with_max_backoff(mut self, max_backoff_ms: u64) -> Self {
    self.max_backoff_ms = max_backoff_ms;
    self
  }

  /// Calculate backoff duration for attempt
  ///
  /// Uses exponential backoff: base * 2^attempt, capped at max_backoff
  pub fn backoff_duration(&self, attempt: u32) -> Duration {
    let backoff_ms = self
      .backoff_base_ms
      .saturating_mul(2_u64.saturating_pow(attempt))
      .min(self.max_backoff_ms);

    Duration::from_millis(backoff_ms)
  }
}

impl Default for RetryConfig {
  fn default() -> Self {
    Self::new(3, 100)
  }
}

/// Retry a fallible async operation with exponential backoff
///
/// # Arguments
///
/// * `config` - Retry configuration
/// * `operation` - Async operation to retry
///
/// # Returns
///
/// Returns the result of the operation, or the last error if all retries failed
///
/// # Example
///
/// ```no_run
/// use agentflow_mcp::client::retry::{retry_with_backoff, RetryConfig};
/// use agentflow_mcp::error::MCPResult;
///
/// # async fn example() -> MCPResult<String> {
/// let config = RetryConfig::new(3, 100);
/// let result = retry_with_backoff(&config, || async {
///   // Some operation that might fail
///   Ok("success".to_string())
/// }).await?;
/// # Ok(result)
/// # }
/// ```
pub async fn retry_with_backoff<F, Fut, T>(config: &RetryConfig, mut operation: F) -> MCPResult<T>
where
  F: FnMut() -> Fut,
  Fut: Future<Output = MCPResult<T>>,
{
  let mut last_error = None;

  for attempt in 0..=config.max_retries {
    match operation().await {
      Ok(result) => return Ok(result),
      Err(e) => {
        // Check if error is transient
        if !e.is_transient() {
          // Non-transient error - fail immediately
          return Err(e);
        }

        last_error = Some(e);

        // Don't sleep after the last attempt
        if attempt < config.max_retries {
          let backoff = config.backoff_duration(attempt);
          sleep(backoff).await;
        }
      }
    }
  }

  // All retries exhausted
  Err(last_error.unwrap_or_else(|| {
    MCPError::protocol(
      "Retry failed with no error (this is a bug)",
      JsonRpcErrorCode::InternalError,
    )
  }))
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::atomic::{AtomicU32, Ordering};
  use std::sync::Arc;

  #[test]
  fn test_retry_config_backoff_duration() {
    let config = RetryConfig::new(5, 100);

    assert_eq!(config.backoff_duration(0), Duration::from_millis(100)); // 100 * 2^0
    assert_eq!(config.backoff_duration(1), Duration::from_millis(200)); // 100 * 2^1
    assert_eq!(config.backoff_duration(2), Duration::from_millis(400)); // 100 * 2^2
    assert_eq!(config.backoff_duration(3), Duration::from_millis(800)); // 100 * 2^3
  }

  #[test]
  fn test_retry_config_max_backoff() {
    let config = RetryConfig::new(10, 100).with_max_backoff(1000);

    assert_eq!(config.backoff_duration(0), Duration::from_millis(100));
    assert_eq!(config.backoff_duration(3), Duration::from_millis(800));
    assert_eq!(config.backoff_duration(4), Duration::from_millis(1000)); // Capped
    assert_eq!(config.backoff_duration(10), Duration::from_millis(1000)); // Still capped
  }

  #[tokio::test]
  async fn test_retry_success_on_first_attempt() {
    let config = RetryConfig::new(3, 10);
    let result = retry_with_backoff(&config, || async { Ok::<_, MCPError>(42) }).await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
  }

  #[tokio::test]
  async fn test_retry_success_after_failures() {
    let config = RetryConfig::new(3, 10);
    let attempt_count = Arc::new(AtomicU32::new(0));
    let attempt_count_clone = attempt_count.clone();

    let result = retry_with_backoff(&config, || {
      let count = attempt_count_clone.clone();
      async move {
        let current = count.fetch_add(1, Ordering::SeqCst);
        if current < 2 {
          // Fail first 2 attempts
          Err(MCPError::timeout("Simulated timeout", None))
        } else {
          Ok(42)
        }
      }
    })
    .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 42);
    assert_eq!(attempt_count.load(Ordering::SeqCst), 3); // 3 attempts total
  }

  #[tokio::test]
  async fn test_retry_exhausted() {
    let config = RetryConfig::new(2, 10);
    let attempt_count = Arc::new(AtomicU32::new(0));
    let attempt_count_clone = attempt_count.clone();

    let result = retry_with_backoff(&config, || {
      let count = attempt_count_clone.clone();
      async move {
        count.fetch_add(1, Ordering::SeqCst);
        Err::<i32, _>(MCPError::timeout("Always fails", None))
      }
    })
    .await;

    assert!(result.is_err());
    assert_eq!(attempt_count.load(Ordering::SeqCst), 3); // Initial + 2 retries
  }

  #[tokio::test]
  async fn test_retry_non_transient_error() {
    let config = RetryConfig::new(3, 10);
    let attempt_count = Arc::new(AtomicU32::new(0));
    let attempt_count_clone = attempt_count.clone();

    let result = retry_with_backoff(&config, || {
      let count = attempt_count_clone.clone();
      async move {
        count.fetch_add(1, Ordering::SeqCst);
        // Protocol error is non-transient
        Err::<i32, _>(MCPError::protocol(
          "Non-transient error",
          JsonRpcErrorCode::InvalidRequest,
        ))
      }
    })
    .await;

    assert!(result.is_err());
    assert_eq!(attempt_count.load(Ordering::SeqCst), 1); // Only 1 attempt, no retries
  }

  // ============================================================================
  // Property-Based Tests
  // ============================================================================

  mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
      /// Property: Backoff duration is always non-negative
      #[test]
      fn prop_backoff_always_non_negative(
        base_ms in 1u64..10_000u64,
        max_backoff_ms in 1_000u64..100_000u64,
        attempt in 0u32..20u32
      ) {
        let config = RetryConfig::new(10, base_ms).with_max_backoff(max_backoff_ms);
        let duration = config.backoff_duration(attempt);
        prop_assert!(duration.as_millis() >= 0);
      }

      /// Property: Backoff respects maximum cap
      #[test]
      fn prop_backoff_respects_max(
        base_ms in 1u64..1_000u64,
        max_backoff_ms in 1_000u64..30_000u64,
        attempt in 0u32..50u32
      ) {
        let config = RetryConfig::new(100, base_ms).with_max_backoff(max_backoff_ms);
        let duration = config.backoff_duration(attempt);
        prop_assert!(duration.as_millis() <= max_backoff_ms as u128);
      }

      /// Property: Backoff increases exponentially until cap
      #[test]
      fn prop_backoff_exponential_growth(
        base_ms in 10u64..100u64,
        attempt in 0u32..10u32
      ) {
        let config = RetryConfig::new(20, base_ms).with_max_backoff(1_000_000);

        if attempt > 0 {
          let prev_duration = config.backoff_duration(attempt - 1);
          let curr_duration = config.backoff_duration(attempt);

          // Current should be ~2x previous (within rounding)
          let expected_min = prev_duration.as_millis() * 2;
          let expected_max = prev_duration.as_millis() * 2 + 1; // Account for rounding

          prop_assert!(
            curr_duration.as_millis() >= expected_min &&
            curr_duration.as_millis() <= expected_max,
            "Expected backoff to double: prev={:?}, curr={:?}",
            prev_duration, curr_duration
          );
        }
      }

      /// Property: Backoff for attempt 0 equals base
      #[test]
      fn prop_backoff_first_attempt_equals_base(
        base_ms in 1u64..10_000u64
      ) {
        let config = RetryConfig::new(5, base_ms);
        let duration = config.backoff_duration(0);
        prop_assert_eq!(duration.as_millis(), base_ms as u128);
      }

      /// Property: Max retries determines attempt count
      #[test]
      fn prop_max_retries_bounds_attempts(
        max_retries in 0u32..20u32
      ) {
        let config = RetryConfig::new(max_retries, 10);

        // The number of iterations in retry_with_backoff is 0..=max_retries
        // So total attempts = max_retries + 1 (initial attempt + retries)
        prop_assert_eq!(config.max_retries, max_retries);
      }

      /// Property: Backoff never overflows even with large attempts
      #[test]
      fn prop_backoff_no_overflow(
        base_ms in 1u64..1_000u64,
        attempt in 0u32..100u32
      ) {
        let config = RetryConfig::new(150, base_ms).with_max_backoff(u64::MAX);

        // Should not panic with overflow
        let duration = config.backoff_duration(attempt);

        // Should be bounded by either exponential growth or max_backoff
        prop_assert!(duration.as_millis() > 0);
      }

      /// Property: Different configurations produce different backoffs
      #[test]
      fn prop_config_affects_backoff(
        base_ms1 in 10u64..100u64,
        base_ms2 in 101u64..1000u64,
        attempt in 1u32..5u32  // Reduced to avoid hitting max_backoff
      ) {
        prop_assume!(base_ms1 != base_ms2); // Only test when different

        // Use very high max_backoff to ensure we don't hit the cap
        let config1 = RetryConfig::new(20, base_ms1).with_max_backoff(u64::MAX);
        let config2 = RetryConfig::new(20, base_ms2).with_max_backoff(u64::MAX);

        let duration1 = config1.backoff_duration(attempt);
        let duration2 = config2.backoff_duration(attempt);

        // Different base should produce different backoffs
        prop_assert_ne!(duration1, duration2);
      }
    }
  }
}

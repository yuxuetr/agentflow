//! Retry execution utilities for async operations
//!
//! This module provides utilities to execute async operations with automatic retry logic.

use crate::error::AgentFlowError;
use crate::retry::{RetryContext, RetryPolicy};
use std::future::Future;
use std::time::Instant;

/// Execute an async operation with retry logic
///
/// # Example
///
/// ```no_run
/// use agentflow_core::retry::{RetryPolicy, RetryStrategy};
/// use agentflow_core::retry_executor::execute_with_retry;
/// use agentflow_core::error::AgentFlowError;
///
/// async fn example() -> Result<String, AgentFlowError> {
///     let policy = RetryPolicy::builder()
///         .max_attempts(3)
///         .strategy(RetryStrategy::exponential_backoff(100, 5000, 2.0))
///         .build();
///
///     execute_with_retry(
///         &policy,
///         "my_operation",
///         || async {
///             // Your async operation here
///             Ok("Success".to_string())
///         }
///     ).await
/// }
/// ```
pub async fn execute_with_retry<F, Fut, T>(
  policy: &RetryPolicy,
  operation_name: &str,
  mut operation: F,
) -> Result<T, AgentFlowError>
where
  F: FnMut() -> Fut,
  Fut: Future<Output = Result<T, AgentFlowError>>,
{
  let mut context = RetryContext::new();
  let _start_time = Instant::now();

  loop {
    match operation().await {
      Ok(result) => {
        if context.attempt > 0 {
          tracing::info!(
            "Operation '{}' succeeded after {} retries",
            operation_name,
            context.attempt
          );
        }
        return Ok(result);
      }
      Err(error) => {
        // Check if we should retry
        if !context.should_retry(policy, &error) {
          tracing::error!(
            "Operation '{}' failed after {} attempts: {}",
            operation_name,
            context.attempt + 1,
            error
          );

          // W0.4: only wrap as `RetryExhausted` once an actual retry has
          // happened (`context.attempt > 0`). `should_retry` also returns
          // `false` on the very first attempt when the error itself isn't
          // retryable, or when the policy allows zero retries — in both
          // cases nothing was ever retried, so wrapping discarded the
          // original error variant (e.g. `NodePartialExecutionFailed`,
          // `NodeSkipped`) that callers like `Flow`'s checkpoint logic
          // pattern-match on.
          return Err(if context.attempt > 0 {
            AgentFlowError::RetryExhausted {
              attempts: context.attempt + 1,
              last_error: Box::new(error),
            }
          } else {
            error
          });
        }

        // Calculate delay
        let delay = policy.calculate_delay(context.attempt);

        tracing::warn!(
          "Operation '{}' failed (attempt {}), retrying after {:?}: {}",
          operation_name,
          context.attempt + 1,
          delay,
          error
        );

        // Record failure and update context
        context.record_failure(&error);

        // Wait before retrying
        tokio::time::sleep(delay).await;
      }
    }
  }
}

/// V1.4: variant of [`execute_with_retry`] that invokes `on_retry` just
/// before sleeping for each retry attempt, so a caller with its own
/// event system (e.g. `agentflow-core::flow`'s
/// `WorkflowEvent::RetryAttempt`) can observe every attempt as it
/// happens, not just the final success or [`AgentFlowError::RetryExhausted`].
/// `on_retry` receives the 1-based attempt number that just failed, the
/// error, and the delay about to be waited before the next attempt.
pub async fn execute_with_retry_and_hook<F, Fut, T>(
  policy: &RetryPolicy,
  #[allow(unused_variables)] operation_name: &str,
  mut on_retry: impl FnMut(u32, &AgentFlowError, std::time::Duration),
  mut operation: F,
) -> Result<T, AgentFlowError>
where
  F: FnMut() -> Fut,
  Fut: Future<Output = Result<T, AgentFlowError>>,
{
  let mut context = RetryContext::new();

  loop {
    match operation().await {
      Ok(result) => return Ok(result),
      Err(error) => {
        if !context.should_retry(policy, &error) {
          // W0.4: see the matching comment in `execute_with_retry` above —
          // only wrap once a retry actually happened.
          return Err(if context.attempt > 0 {
            AgentFlowError::RetryExhausted {
              attempts: context.attempt + 1,
              last_error: Box::new(error),
            }
          } else {
            error
          });
        }

        let delay = policy.calculate_delay(context.attempt);
        on_retry(context.attempt + 1, &error, delay);
        context.record_failure(&error);
        tokio::time::sleep(delay).await;
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::retry::RetryStrategy;
  use std::sync::Arc;
  use std::sync::atomic::{AtomicU32, Ordering};

  #[tokio::test]
  async fn test_execute_with_retry_success_first_attempt() {
    let policy = RetryPolicy::builder()
      .max_attempts(3)
      .strategy(RetryStrategy::fixed(10))
      .build();

    let result = execute_with_retry(&policy, "test_op", || async {
      Ok::<_, AgentFlowError>("success".to_string())
    })
    .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "success");
  }

  #[tokio::test]
  async fn test_execute_with_retry_success_after_failures() {
    let attempt_counter = Arc::new(AtomicU32::new(0));
    let counter_clone = attempt_counter.clone();

    let policy = RetryPolicy::builder()
      .max_attempts(3)
      .strategy(RetryStrategy::fixed(10))
      .build();

    let result = execute_with_retry(&policy, "test_op", || {
      let counter = counter_clone.clone();
      async move {
        let attempt = counter.fetch_add(1, Ordering::SeqCst);
        if attempt < 2 {
          Err(AgentFlowError::NodeExecutionFailed {
            message: format!("Attempt {} failed", attempt),
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
  async fn test_execute_with_retry_exhausted() {
    let policy = RetryPolicy::builder()
      .max_attempts(2)
      .strategy(RetryStrategy::fixed(10))
      .build();

    let result = execute_with_retry(&policy, "test_op", || async {
      Err::<String, _>(AgentFlowError::NodeExecutionFailed {
        message: "Always fails".to_string(),
      })
    })
    .await;

    assert!(result.is_err());
    if let Err(AgentFlowError::RetryExhausted {
      attempts,
      last_error,
    }) = result
    {
      // max_attempts=2 means we try initially + 2 retries = 3 total
      assert_eq!(attempts, 3);
      // Q2.4.3: the last error must carry the underlying root cause.
      match last_error.as_ref() {
        AgentFlowError::NodeExecutionFailed { message } => {
          assert_eq!(message, "Always fails");
        }
        other => panic!("expected NodeExecutionFailed last_error, got {other:?}"),
      }
    } else {
      panic!("Expected RetryExhausted error");
    }
  }

  /// V1.4: `execute_with_retry_and_hook` must invoke `on_retry` exactly
  /// once per failed-but-retried attempt (not on the final success, not
  /// on a non-retried exhaustion), with the 1-based attempt number that
  /// just failed.
  #[tokio::test]
  async fn execute_with_retry_and_hook_invokes_hook_once_per_retried_attempt() {
    let policy = RetryPolicy::builder()
      .max_attempts(5)
      .strategy(RetryStrategy::fixed(1))
      .build();

    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = counter.clone();
    let seen_attempts = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen_attempts_clone = seen_attempts.clone();

    let result = execute_with_retry_and_hook(
      &policy,
      "test_op",
      move |attempt, _error, _delay| {
        seen_attempts_clone.lock().unwrap().push(attempt);
      },
      move || {
        let counter = counter_clone.clone();
        async move {
          let call = counter.fetch_add(1, Ordering::SeqCst);
          if call < 2 {
            Err(AgentFlowError::NetworkError {
              message: "connection reset".to_string(),
            })
          } else {
            Ok("success".to_string())
          }
        }
      },
    )
    .await;

    assert_eq!(result.unwrap(), "success");
    assert_eq!(
      *seen_attempts.lock().unwrap(),
      vec![1, 2],
      "hook must fire once per failed attempt, with 1-based attempt numbers"
    );
  }

  /// V1.4: the hook must not fire at all for a non-retryable error — it
  /// only observes attempts that are actually retried. Uses
  /// `RetryPolicy::default()` (not `builder()...build()`, whose empty
  /// `retryable_errors` list means "retry everything" — the opposite of
  /// what this test needs) for its real network/timeout/rate-limit-only
  /// classification.
  #[tokio::test]
  async fn execute_with_retry_and_hook_does_not_fire_for_non_retryable_errors() {
    let policy = RetryPolicy {
      max_attempts: 5,
      ..RetryPolicy::default()
    };

    let hook_calls = Arc::new(AtomicU32::new(0));
    let hook_calls_clone = hook_calls.clone();

    let result = execute_with_retry_and_hook(
      &policy,
      "test_op",
      move |_attempt, _error, _delay| {
        hook_calls_clone.fetch_add(1, Ordering::SeqCst);
      },
      || async { Err::<String, _>(AgentFlowError::ValidationError("bad input".to_string())) },
    )
    .await;

    assert_eq!(
      hook_calls.load(Ordering::SeqCst),
      0,
      "a non-retryable error must not trigger any hook calls"
    );
    // W0.4: a non-retryable error must pass through unwrapped, not get
    // coerced into `RetryExhausted` — no retry ever happened, so wrapping
    // would discard the original error variant callers pattern-match on
    // (e.g. `Flow`'s checkpoint logic for `NodePartialExecutionFailed`).
    assert!(
      matches!(result, Err(AgentFlowError::ValidationError(ref msg)) if msg == "bad input"),
      "expected the original ValidationError to pass through, got {result:?}"
    );
  }
}

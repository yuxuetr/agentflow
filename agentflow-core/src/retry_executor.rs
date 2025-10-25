//! Retry execution utilities for async operations
//!
//! This module provides utilities to execute async operations with automatic retry logic.

use crate::error::AgentFlowError;
use crate::error_context::ErrorContext;
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
    let start_time = Instant::now();

    loop {
        match operation().await {
            Ok(result) => {
                #[cfg(feature = "observability")]
                {
                    if context.attempt > 0 {
                        tracing::info!(
                            "Operation '{}' succeeded after {} retries",
                            operation_name,
                            context.attempt
                        );
                    }
                }
                return Ok(result);
            }
            Err(error) => {
                // Check if we should retry
                if !context.should_retry(policy, &error) {
                    #[cfg(feature = "observability")]
                    tracing::error!(
                        "Operation '{}' failed after {} attempts: {}",
                        operation_name,
                        context.attempt + 1,
                        error
                    );

                    return Err(AgentFlowError::RetryExhausted {
                        attempts: context.attempt + 1,
                    });
                }

                // Calculate delay
                let delay = policy.calculate_delay(context.attempt);

                #[cfg(feature = "observability")]
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

/// Execute an async operation with retry and detailed error context
///
/// This variant captures more detailed error context for debugging.
pub async fn execute_with_retry_and_context<F, Fut, T>(
    policy: &RetryPolicy,
    run_id: &str,
    node_name: &str,
    node_type: Option<&str>,
    mut operation: F,
) -> Result<T, (AgentFlowError, ErrorContext)>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, AgentFlowError>>,
{
    let mut retry_ctx = RetryContext::new();
    let operation_start = Instant::now();

    loop {
        let attempt_start = Instant::now();

        match operation().await {
            Ok(result) => {
                #[cfg(feature = "observability")]
                {
                    if retry_ctx.attempt > 0 {
                        tracing::info!(
                            "Node '{}' succeeded after {} retries",
                            node_name,
                            retry_ctx.attempt
                        );
                    }
                }
                return Ok(result);
            }
            Err(error) => {
                let _attempt_duration = attempt_start.elapsed();

                // Check if we should retry
                if !retry_ctx.should_retry(policy, &error) {
                    // Build comprehensive error context
                    let mut error_context = ErrorContext::builder(run_id, node_name)
                        .duration(operation_start.elapsed())
                        .retry_attempt(retry_ctx.attempt)
                        .error(&error)
                        .build();

                    if let Some(nt) = node_type {
                        error_context.node_type = Some(nt.to_string());
                    }

                    #[cfg(feature = "observability")]
                    tracing::error!(
                        "Node '{}' failed after {} attempts: {}",
                        node_name,
                        retry_ctx.attempt + 1,
                        error_context.summary()
                    );

                    let final_error = if retry_ctx.attempt > 0 {
                        AgentFlowError::RetryExhausted {
                            attempts: retry_ctx.attempt + 1,
                        }
                    } else {
                        error
                    };

                    return Err((final_error, error_context));
                }

                // Calculate delay
                let delay = policy.calculate_delay(retry_ctx.attempt);

                #[cfg(feature = "observability")]
                tracing::warn!(
                    "Node '{}' failed (attempt {}), retrying after {:?}: {}",
                    node_name,
                    retry_ctx.attempt + 1,
                    delay,
                    error
                );

                // Record failure
                retry_ctx.record_failure(&error);

                // Wait before retrying
                tokio::time::sleep(delay).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::retry::RetryStrategy;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

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
        if let Err(AgentFlowError::RetryExhausted { attempts }) = result {
            // max_attempts=2 means we try initially + 2 retries = 3 total
            assert_eq!(attempts, 3);
        } else {
            panic!("Expected RetryExhausted error");
        }
    }

    #[tokio::test]
    async fn test_execute_with_retry_and_context() {
        let policy = RetryPolicy::builder()
            .max_attempts(2)
            .strategy(RetryStrategy::fixed(10))
            .build();

        let result = execute_with_retry_and_context(
            &policy,
            "run-123",
            "test_node",
            Some("http"),
            || async {
                Err::<String, _>(AgentFlowError::NodeExecutionFailed {
                    message: "Test failure".to_string(),
                })
            },
        )
        .await;

        assert!(result.is_err());
        if let Err((error, context)) = result {
            assert!(matches!(error, AgentFlowError::RetryExhausted { .. }));
            assert_eq!(context.node_name, "test_node");
            assert_eq!(context.node_type, Some("http".to_string()));
            // After 2 failed attempts, retry_attempt should be 2 (0-indexed, but we record after failure)
            assert_eq!(context.retry_attempt, Some(2));
            assert!(!context.error_chain.is_empty());
        } else {
            panic!("Expected error with context");
        }
    }
}

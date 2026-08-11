//! Timeout control for async operations
//!
//! This module provides utilities for adding timeout controls to async operations
//! throughout AgentFlow, ensuring that operations don't hang indefinitely.
//!
//! # Examples
//!
//! ```rust
//! use agentflow_async_util::timeout::with_timeout;
//! use std::time::Duration;
//!
//! async fn my_operation() -> agentflow_async_util::error::Result<String> {
//!     // Simulate work
//!     tokio::time::sleep(Duration::from_millis(100)).await;
//!     Ok("done".to_string())
//! }
//!
//! # async fn example() -> agentflow_async_util::error::Result<()> {
//! let result = with_timeout(my_operation(), Duration::from_secs(30)).await?;
//! assert_eq!(result, "done");
//! # Ok(())
//! # }
//! ```

use crate::error::{AgentFlowError, Result};
use std::future::Future;
use std::time::Duration;
use tokio::time::timeout;

/// Execute a future with a timeout
///
/// If the operation doesn't complete within the specified duration,
/// returns a TimeoutExceeded error.
///
/// # Examples
///
/// ```rust
/// use agentflow_async_util::timeout::with_timeout;
/// use std::time::Duration;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let result = with_timeout(
///     async {
///         tokio::time::sleep(Duration::from_millis(10)).await;
///         Ok("done")
///     },
///     Duration::from_secs(1)
/// ).await?;
/// assert_eq!(result, "done");
/// # Ok(())
/// # }
/// ```
pub async fn with_timeout<F, T>(future: F, duration: Duration) -> Result<T>
where
  F: Future<Output = Result<T>>,
{
  match timeout(duration, future).await {
    Ok(result) => result,
    Err(_) => Err(AgentFlowError::TimeoutExceeded {
      duration_ms: duration.as_millis() as u64,
    }),
  }
}

/// Execute a future with a timeout, converting inner errors
///
/// Similar to `with_timeout`, but automatically converts inner errors
/// to AgentFlowError using From trait.
pub async fn with_timeout_convert<F, T, E>(future: F, duration: Duration) -> Result<T>
where
  F: Future<Output = std::result::Result<T, E>>,
  AgentFlowError: From<E>,
{
  match timeout(duration, future).await {
    Ok(Ok(value)) => Ok(value),
    Ok(Err(e)) => Err(AgentFlowError::from(e)),
    Err(_) => Err(AgentFlowError::TimeoutExceeded {
      duration_ms: duration.as_millis() as u64,
    }),
  }
}

/// Execute a future with timeout and error context.
///
/// Q2.4.5: pre-fix the `operation`, `node_id`, and `workflow_id`
/// parameters were prefixed with `_` and entirely discarded — the
/// only thing the function did with them was satisfy the type
/// checker. Now we emit them via `tracing::warn!` on timeout so the
/// operator-facing log trail explains *which* operation timed out,
/// in *which* node, in *which* workflow. The returned
/// `TimeoutExceeded` error keeps its existing shape so callers that
/// pattern-match the variant still compile.
pub async fn with_timeout_context<F, T>(
  future: F,
  duration: Duration,
  operation: &str,
  node_id: Option<&str>,
  workflow_id: Option<&str>,
) -> Result<T>
where
  F: Future<Output = Result<T>>,
{
  match timeout(duration, future).await {
    Ok(result) => result,
    Err(_) => {
      tracing::warn!(
        target = "agentflow_async_util::timeout",
        operation,
        node_id,
        workflow_id,
        duration_ms = duration.as_millis() as u64,
        "operation timed out"
      );
      Err(AgentFlowError::TimeoutExceeded {
        duration_ms: duration.as_millis() as u64,
      })
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_with_timeout_success() {
    async fn fast_operation() -> Result<String> {
      tokio::time::sleep(Duration::from_millis(10)).await;
      Ok("success".to_string())
    }

    let result = with_timeout(fast_operation(), Duration::from_secs(1)).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "success");
  }

  #[tokio::test]
  async fn test_with_timeout_exceeded() {
    async fn slow_operation() -> Result<String> {
      tokio::time::sleep(Duration::from_secs(2)).await;
      Ok("success".to_string())
    }

    let result = with_timeout(slow_operation(), Duration::from_millis(100)).await;
    assert!(result.is_err());
    match result.unwrap_err() {
      AgentFlowError::TimeoutExceeded { duration_ms } => {
        assert_eq!(duration_ms, 100);
      }
      _ => panic!("Expected TimeoutExceeded error"),
    }
  }

  #[tokio::test]
  async fn test_with_timeout_convert() {
    async fn operation() -> std::result::Result<String, std::io::Error> {
      tokio::time::sleep(Duration::from_millis(10)).await;
      Ok("success".to_string())
    }

    let result = with_timeout_convert(operation(), Duration::from_secs(1)).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "success");
  }

  #[tokio::test]
  async fn test_with_timeout_convert_timeout() {
    async fn slow_operation() -> std::result::Result<String, std::io::Error> {
      tokio::time::sleep(Duration::from_secs(2)).await;
      Ok("success".to_string())
    }

    let result = with_timeout_convert(slow_operation(), Duration::from_millis(100)).await;
    assert!(result.is_err());
    assert!(matches!(
      result.unwrap_err(),
      AgentFlowError::TimeoutExceeded { .. }
    ));
  }

  #[tokio::test]
  async fn test_with_timeout_context() {
    async fn fast_operation() -> Result<String> {
      tokio::time::sleep(Duration::from_millis(10)).await;
      Ok("success".to_string())
    }

    let result = with_timeout_context(
      fast_operation(),
      Duration::from_secs(1),
      "test_operation",
      Some("node1"),
      Some("workflow1"),
    )
    .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "success");
  }
}

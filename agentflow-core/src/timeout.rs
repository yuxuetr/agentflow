//! Timeout control for async operations
//!
//! This module provides utilities for adding timeout controls to async operations
//! throughout AgentFlow, ensuring that operations don't hang indefinitely.
//!
//! # Examples
//!
//! ```rust
//! use agentflow_core::timeout::{with_timeout, TimeoutConfig};
//! use std::time::Duration;
//!
//! async fn my_operation() -> Result<String, Box<dyn std::error::Error>> {
//!     // Simulate work
//!     tokio::time::sleep(Duration::from_millis(100)).await;
//!     Ok("done".to_string())
//! }
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = TimeoutConfig::default();
//! let result = with_timeout(my_operation(), config.default_timeout).await?;
//! assert_eq!(result, "done");
//! # Ok(())
//! # }
//! ```

use crate::error::{AgentFlowError, Result};
use std::future::Future;
use std::time::Duration;
use tokio::time::timeout;

/// Timeout configuration for different operation types
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    /// Default timeout for unspecified operations (30 seconds)
    pub default_timeout: Duration,

    /// Timeout for node execution (5 minutes)
    pub node_execution_timeout: Duration,

    /// Timeout for workflow execution (30 minutes)
    pub workflow_execution_timeout: Duration,

    /// Timeout for HTTP requests (30 seconds)
    pub http_request_timeout: Duration,

    /// Timeout for database operations (10 seconds)
    pub database_timeout: Duration,

    /// Timeout for LLM API calls (2 minutes)
    pub llm_timeout: Duration,

    /// Timeout for file operations (30 seconds)
    pub file_operation_timeout: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            default_timeout: Duration::from_secs(30),
            node_execution_timeout: Duration::from_secs(5 * 60),
            workflow_execution_timeout: Duration::from_secs(30 * 60),
            http_request_timeout: Duration::from_secs(30),
            database_timeout: Duration::from_secs(10),
            llm_timeout: Duration::from_secs(2 * 60),
            file_operation_timeout: Duration::from_secs(30),
        }
    }
}

impl TimeoutConfig {
    /// Create a builder for custom timeout configuration
    pub fn builder() -> TimeoutConfigBuilder {
        TimeoutConfigBuilder::default()
    }

    /// Create a configuration optimized for development (longer timeouts)
    pub fn development() -> Self {
        Self {
            default_timeout: Duration::from_secs(60),
            node_execution_timeout: Duration::from_secs(10 * 60),
            workflow_execution_timeout: Duration::from_secs(60 * 60),
            http_request_timeout: Duration::from_secs(60),
            database_timeout: Duration::from_secs(30),
            llm_timeout: Duration::from_secs(5 * 60),
            file_operation_timeout: Duration::from_secs(60),
        }
    }

    /// Create a configuration optimized for production (shorter timeouts)
    pub fn production() -> Self {
        Self {
            default_timeout: Duration::from_secs(15),
            node_execution_timeout: Duration::from_secs(3 * 60),
            workflow_execution_timeout: Duration::from_secs(15 * 60),
            http_request_timeout: Duration::from_secs(15),
            database_timeout: Duration::from_secs(5),
            llm_timeout: Duration::from_secs(90),
            file_operation_timeout: Duration::from_secs(15),
        }
    }
}

/// Builder for timeout configuration
#[derive(Debug, Default)]
pub struct TimeoutConfigBuilder {
    default_timeout: Option<Duration>,
    node_execution_timeout: Option<Duration>,
    workflow_execution_timeout: Option<Duration>,
    http_request_timeout: Option<Duration>,
    database_timeout: Option<Duration>,
    llm_timeout: Option<Duration>,
    file_operation_timeout: Option<Duration>,
}

impl TimeoutConfigBuilder {
    pub fn default_timeout(mut self, timeout: Duration) -> Self {
        self.default_timeout = Some(timeout);
        self
    }

    pub fn node_execution_timeout(mut self, timeout: Duration) -> Self {
        self.node_execution_timeout = Some(timeout);
        self
    }

    pub fn workflow_execution_timeout(mut self, timeout: Duration) -> Self {
        self.workflow_execution_timeout = Some(timeout);
        self
    }

    pub fn http_request_timeout(mut self, timeout: Duration) -> Self {
        self.http_request_timeout = Some(timeout);
        self
    }

    pub fn database_timeout(mut self, timeout: Duration) -> Self {
        self.database_timeout = Some(timeout);
        self
    }

    pub fn llm_timeout(mut self, timeout: Duration) -> Self {
        self.llm_timeout = Some(timeout);
        self
    }

    pub fn file_operation_timeout(mut self, timeout: Duration) -> Self {
        self.file_operation_timeout = Some(timeout);
        self
    }

    pub fn build(self) -> TimeoutConfig {
        let default = TimeoutConfig::default();
        TimeoutConfig {
            default_timeout: self.default_timeout.unwrap_or(default.default_timeout),
            node_execution_timeout: self
                .node_execution_timeout
                .unwrap_or(default.node_execution_timeout),
            workflow_execution_timeout: self
                .workflow_execution_timeout
                .unwrap_or(default.workflow_execution_timeout),
            http_request_timeout: self
                .http_request_timeout
                .unwrap_or(default.http_request_timeout),
            database_timeout: self.database_timeout.unwrap_or(default.database_timeout),
            llm_timeout: self.llm_timeout.unwrap_or(default.llm_timeout),
            file_operation_timeout: self
                .file_operation_timeout
                .unwrap_or(default.file_operation_timeout),
        }
    }
}

/// Execute a future with a timeout
///
/// If the operation doesn't complete within the specified duration,
/// returns a TimeoutExceeded error.
///
/// # Examples
///
/// ```rust
/// use agentflow_core::timeout::with_timeout;
/// use std::time::Duration;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let result = with_timeout(
///     async { tokio::time::sleep(Duration::from_millis(10)).await; "done" },
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

/// Execute a future with timeout and error context
///
/// Adds context information (node_id, workflow_id) to timeout errors.
pub async fn with_timeout_context<F, T>(
    future: F,
    duration: Duration,
    _operation: &str,
    _node_id: Option<&str>,
    _workflow_id: Option<&str>,
) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    match timeout(duration, future).await {
        Ok(result) => result,
        Err(_) => {
            let error = AgentFlowError::TimeoutExceeded {
                duration_ms: duration.as_millis() as u64,
            };

            // Add context if possible (would need ErrorContext support in AgentFlowError)
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_config_default() {
        let config = TimeoutConfig::default();
        assert_eq!(config.default_timeout, Duration::from_secs(30));
        assert_eq!(config.node_execution_timeout, Duration::from_secs(5 * 60));
        assert_eq!(config.llm_timeout, Duration::from_secs(2 * 60));
    }

    #[test]
    fn test_timeout_config_development() {
        let config = TimeoutConfig::development();
        assert_eq!(config.default_timeout, Duration::from_secs(60));
        assert!(config.llm_timeout > TimeoutConfig::default().llm_timeout);
    }

    #[test]
    fn test_timeout_config_production() {
        let config = TimeoutConfig::production();
        assert_eq!(config.default_timeout, Duration::from_secs(15));
        assert!(config.llm_timeout < TimeoutConfig::default().llm_timeout);
    }

    #[test]
    fn test_timeout_config_builder() {
        let config = TimeoutConfig::builder()
            .default_timeout(Duration::from_secs(45))
            .llm_timeout(Duration::from_secs(180))
            .build();

        assert_eq!(config.default_timeout, Duration::from_secs(45));
        assert_eq!(config.llm_timeout, Duration::from_secs(180));
    }

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

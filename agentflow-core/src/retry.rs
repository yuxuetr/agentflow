//! Retry mechanism for transient failures
//!
//! This module provides configurable retry policies for workflow node execution,
//! supporting various retry strategies including exponential backoff, fixed delays,
//! and linear backoff.
//!
//! # Example
//!
//! ```rust
//! use agentflow_core::retry::{RetryPolicy, RetryStrategy};
//! use std::time::Duration;
//!
//! let policy = RetryPolicy::builder()
//!     .max_attempts(3)
//!     .strategy(RetryStrategy::exponential_backoff(100, 5000, 2.0))
//!     .build();
//! ```

use crate::error::AgentFlowError;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

/// Retry policy configuration
///
/// Defines how and when failed operations should be retried.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts (0 means no retries)
    pub max_attempts: u32,

    /// Retry strategy to use
    pub strategy: RetryStrategy,

    /// Which errors should trigger retries
    #[serde(default)]
    pub retryable_errors: Vec<ErrorPattern>,

    /// Maximum total retry duration (None = no limit)
    #[serde(default)]
    #[serde(with = "humantime_serde")]
    pub max_duration: Option<Duration>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            strategy: RetryStrategy::ExponentialBackoff {
                initial_delay_ms: 100,
                max_delay_ms: 10000,
                multiplier: 2.0,
                jitter: true,
            },
            retryable_errors: vec![
                ErrorPattern::NetworkError,
                ErrorPattern::TimeoutError,
                ErrorPattern::RateLimitError,
            ],
            max_duration: Some(Duration::from_secs(300)), // 5 minutes
        }
    }
}

impl RetryPolicy {
    /// Create a new retry policy builder
    pub fn builder() -> RetryPolicyBuilder {
        RetryPolicyBuilder::default()
    }

    /// Check if an error is retryable according to this policy
    pub fn is_retryable(&self, error: &AgentFlowError) -> bool {
        if self.retryable_errors.is_empty() {
            // If no patterns specified, retry all errors
            return true;
        }

        self.retryable_errors
            .iter()
            .any(|pattern| pattern.matches(error))
    }

    /// Calculate delay before next retry attempt
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        self.strategy.calculate_delay(attempt)
    }

    /// Check if we should retry based on elapsed time
    pub fn should_retry_time(&self, elapsed: Duration) -> bool {
        match self.max_duration {
            Some(max) => elapsed < max,
            None => true,
        }
    }
}

/// Builder for retry policies
#[derive(Default)]
pub struct RetryPolicyBuilder {
    max_attempts: Option<u32>,
    strategy: Option<RetryStrategy>,
    retryable_errors: Vec<ErrorPattern>,
    max_duration: Option<Duration>,
}

impl RetryPolicyBuilder {
    pub fn max_attempts(mut self, attempts: u32) -> Self {
        self.max_attempts = Some(attempts);
        self
    }

    pub fn strategy(mut self, strategy: RetryStrategy) -> Self {
        self.strategy = Some(strategy);
        self
    }

    pub fn retryable_error(mut self, pattern: ErrorPattern) -> Self {
        self.retryable_errors.push(pattern);
        self
    }

    pub fn max_duration(mut self, duration: Duration) -> Self {
        self.max_duration = Some(duration);
        self
    }

    pub fn build(self) -> RetryPolicy {
        RetryPolicy {
            max_attempts: self.max_attempts.unwrap_or(3),
            strategy: self.strategy.unwrap_or_else(|| {
                RetryStrategy::ExponentialBackoff {
                    initial_delay_ms: 100,
                    max_delay_ms: 10000,
                    multiplier: 2.0,
                    jitter: true,
                }
            }),
            retryable_errors: self.retryable_errors,
            max_duration: self.max_duration,
        }
    }
}

/// Retry strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RetryStrategy {
    /// Fixed delay between retries
    Fixed {
        delay_ms: u64,
    },

    /// Exponential backoff with optional jitter
    ExponentialBackoff {
        initial_delay_ms: u64,
        max_delay_ms: u64,
        multiplier: f64,
        #[serde(default)]
        jitter: bool,
    },

    /// Linear backoff
    Linear {
        initial_delay_ms: u64,
        increment_ms: u64,
    },
}

impl RetryStrategy {
    /// Create a fixed delay strategy
    pub fn fixed(delay_ms: u64) -> Self {
        Self::Fixed { delay_ms }
    }

    /// Create an exponential backoff strategy
    pub fn exponential_backoff(
        initial_delay_ms: u64,
        max_delay_ms: u64,
        multiplier: f64,
    ) -> Self {
        Self::ExponentialBackoff {
            initial_delay_ms,
            max_delay_ms,
            multiplier,
            jitter: true,
        }
    }

    /// Create a linear backoff strategy
    pub fn linear(initial_delay_ms: u64, increment_ms: u64) -> Self {
        Self::Linear {
            initial_delay_ms,
            increment_ms,
        }
    }

    /// Calculate delay for a given attempt number
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        let delay_ms = match self {
            Self::Fixed { delay_ms } => *delay_ms,

            Self::ExponentialBackoff {
                initial_delay_ms,
                max_delay_ms,
                multiplier,
                jitter,
            } => {
                let delay = (*initial_delay_ms as f64) * multiplier.powi(attempt as i32);
                let mut delay = delay.min(*max_delay_ms as f64) as u64;

                if *jitter {
                    // Add ±25% jitter
                    let jitter_range = delay / 4;
                    let jitter_offset = (rand::random::<u64>() % (jitter_range * 2))
                        .saturating_sub(jitter_range);
                    delay = delay.saturating_add(jitter_offset);
                }

                delay
            }

            Self::Linear {
                initial_delay_ms,
                increment_ms,
            } => initial_delay_ms + (increment_ms * attempt as u64),
        };

        Duration::from_millis(delay_ms)
    }
}

/// Pattern matching for retryable errors
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ErrorPattern {
    /// Match by error variant name
    ErrorType { name: String },

    /// Match by error message substring
    MessageContains { text: String },

    /// Network-related errors
    NetworkError,

    /// Timeout errors
    TimeoutError,

    /// Rate limit errors
    RateLimitError,

    /// Service unavailable errors
    ServiceUnavailable,
}

impl ErrorPattern {
    /// Check if this pattern matches the given error
    pub fn matches(&self, error: &AgentFlowError) -> bool {
        match self {
            Self::ErrorType { name } => {
                let error_type = match error {
                    AgentFlowError::ConfigurationError { .. } => "ConfigurationError",
                    AgentFlowError::NodeExecutionFailed { .. } => "NodeExecutionFailed",
                    AgentFlowError::NodeInputError { .. } => "NodeInputError",
                    AgentFlowError::AsyncExecutionError { .. } => "AsyncExecutionError",
                    AgentFlowError::FlowExecutionFailed { .. } => "FlowExecutionFailed",
                    AgentFlowError::FlowDefinitionError { .. } => "FlowDefinitionError",
                    AgentFlowError::SerializationError(_) => "SerializationError",
                    AgentFlowError::TimeoutExceeded { .. } => "TimeoutExceeded",
                    AgentFlowError::CircuitBreakerOpen { .. } => "CircuitBreakerOpen",
                    AgentFlowError::RateLimitExceeded { .. } => "RateLimitExceeded",
                    AgentFlowError::DependencyNotMet { .. } => "DependencyNotMet",
                    AgentFlowError::SharedStateError { .. } => "SharedStateError",
                    AgentFlowError::PersistenceError { .. } => "PersistenceError",
                    AgentFlowError::BatchProcessingFailed { .. } => "BatchProcessingFailed",
                    AgentFlowError::MonitoringError { .. } => "MonitoringError",
                    _ => "Unknown",
                };
                error_type.contains(name)
            }

            Self::MessageContains { text } => {
                let message = error.to_string();
                message.contains(text)
            }

            Self::NetworkError => matches!(
                error,
                AgentFlowError::AsyncExecutionError { message }
                    if message.to_lowercase().contains("network")
                        || message.to_lowercase().contains("connection")
            ),

            Self::TimeoutError => matches!(
                error,
                AgentFlowError::AsyncExecutionError { message }
                    if message.to_lowercase().contains("timeout")
                        || message.to_lowercase().contains("timed out")
            ),

            Self::RateLimitError => matches!(
                error,
                AgentFlowError::AsyncExecutionError { message }
                    if message.to_lowercase().contains("rate limit")
                        || message.to_lowercase().contains("too many requests")
                        || message.contains("429")
            ),

            Self::ServiceUnavailable => matches!(
                error,
                AgentFlowError::AsyncExecutionError { message }
                    if message.contains("503")
                        || message.to_lowercase().contains("unavailable")
            ),
        }
    }
}

/// Context for tracking retry state
#[derive(Debug, Clone)]
pub struct RetryContext {
    /// Current attempt number (0-indexed)
    pub attempt: u32,

    /// Time when retries started
    pub start_time: SystemTime,

    /// Last error that triggered retry
    pub last_error: Option<String>,

    /// Total time elapsed in retries
    pub elapsed: Duration,
}

impl RetryContext {
    /// Create a new retry context
    pub fn new() -> Self {
        Self {
            attempt: 0,
            start_time: SystemTime::now(),
            last_error: None,
            elapsed: Duration::ZERO,
        }
    }

    /// Record a failed attempt
    pub fn record_failure(&mut self, error: &AgentFlowError) {
        self.attempt += 1;
        self.last_error = Some(error.to_string());
        self.elapsed = self.start_time.elapsed().unwrap_or(Duration::ZERO);
    }

    /// Check if we should retry based on policy
    pub fn should_retry(&self, policy: &RetryPolicy, error: &AgentFlowError) -> bool {
        // Check attempt limit
        if self.attempt >= policy.max_attempts {
            return false;
        }

        // Check time limit
        if !policy.should_retry_time(self.elapsed) {
            return false;
        }

        // Check if error is retryable
        policy.is_retryable(error)
    }
}

impl Default for RetryContext {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_delay() {
        let strategy = RetryStrategy::fixed(1000);
        assert_eq!(strategy.calculate_delay(0), Duration::from_millis(1000));
        assert_eq!(strategy.calculate_delay(1), Duration::from_millis(1000));
        assert_eq!(strategy.calculate_delay(5), Duration::from_millis(1000));
    }

    #[test]
    fn test_exponential_backoff() {
        let strategy = RetryStrategy::ExponentialBackoff {
            initial_delay_ms: 100,
            max_delay_ms: 5000,
            multiplier: 2.0,
            jitter: false,
        };

        assert_eq!(strategy.calculate_delay(0), Duration::from_millis(100));
        assert_eq!(strategy.calculate_delay(1), Duration::from_millis(200));
        assert_eq!(strategy.calculate_delay(2), Duration::from_millis(400));
        assert_eq!(strategy.calculate_delay(10), Duration::from_millis(5000)); // Capped
    }

    #[test]
    fn test_linear_backoff() {
        let strategy = RetryStrategy::linear(100, 50);
        assert_eq!(strategy.calculate_delay(0), Duration::from_millis(100));
        assert_eq!(strategy.calculate_delay(1), Duration::from_millis(150));
        assert_eq!(strategy.calculate_delay(2), Duration::from_millis(200));
    }

    #[test]
    fn test_error_pattern_matching() {
        let network_error = AgentFlowError::AsyncExecutionError {
            message: "Network connection failed".to_string(),
        };

        assert!(ErrorPattern::NetworkError.matches(&network_error));
        assert!(ErrorPattern::MessageContains {
            text: "connection".to_string()
        }
        .matches(&network_error));
    }

    #[test]
    fn test_retry_context() {
        let policy = RetryPolicy::builder().max_attempts(3).build();
        let mut context = RetryContext::new();

        let error = AgentFlowError::NodeExecutionFailed {
            message: "Test error".to_string(),
        };

        assert!(context.should_retry(&policy, &error));
        context.record_failure(&error);

        assert!(context.should_retry(&policy, &error));
        context.record_failure(&error);

        assert!(context.should_retry(&policy, &error));
        context.record_failure(&error);

        assert!(!context.should_retry(&policy, &error)); // Max attempts reached
    }
}

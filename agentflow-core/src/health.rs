//! Health check system for AgentFlow
//!
//! Provides health and readiness checks for monitoring and deployment orchestration.
//! Compatible with Kubernetes liveness and readiness probes.
//!
//! # Examples
//!
//! ```rust
//! use agentflow_core::health::{HealthChecker, HealthStatus};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let mut checker = HealthChecker::new();
//!
//! // Add custom health checks
//! checker.add_check("database", Box::new(|_| async {
//!     // Check database connection
//!     Ok(HealthStatus::Healthy)
//! }));
//!
//! // Perform health check
//! let status = checker.check_health().await;
//! assert!(status.is_healthy);
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Health status of a component
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Component is healthy and operating normally
    Healthy,
    /// Component is degraded but still functional
    Degraded,
    /// Component is unhealthy and not functioning
    Unhealthy,
}

impl HealthStatus {
    /// Check if status is healthy
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthStatus::Healthy)
    }

    /// Check if status is at least functional (healthy or degraded)
    pub fn is_functional(&self) -> bool {
        matches!(self, HealthStatus::Healthy | HealthStatus::Degraded)
    }
}

/// Result of a health check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    /// Component name
    pub name: String,
    /// Health status
    pub status: HealthStatus,
    /// Optional message providing details
    pub message: Option<String>,
    /// Optional metrics
    pub metrics: HashMap<String, String>,
}

impl HealthCheckResult {
    /// Create a healthy result
    pub fn healthy(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: HealthStatus::Healthy,
            message: None,
            metrics: HashMap::new(),
        }
    }

    /// Create a degraded result with message
    pub fn degraded(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: HealthStatus::Degraded,
            message: Some(message.into()),
            metrics: HashMap::new(),
        }
    }

    /// Create an unhealthy result with message
    pub fn unhealthy(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: HealthStatus::Unhealthy,
            message: Some(message.into()),
            metrics: HashMap::new(),
        }
    }

    /// Add a metric
    pub fn with_metric(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metrics.insert(key.into(), value.into());
        self
    }
}

/// Overall health report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    /// Overall health status
    pub status: HealthStatus,
    /// Whether system is healthy
    pub is_healthy: bool,
    /// Whether system is ready (all components functional)
    pub is_ready: bool,
    /// Individual component checks
    pub checks: Vec<HealthCheckResult>,
    /// Timestamp of the report
    pub timestamp: String,
}

impl HealthReport {
    /// Create a new health report from check results
    pub fn new(checks: Vec<HealthCheckResult>) -> Self {
        let is_healthy = checks.iter().all(|c| c.status.is_healthy());
        let is_ready = checks.iter().all(|c| c.status.is_functional());

        // Overall status is worst of all checks
        let status = if is_healthy {
            HealthStatus::Healthy
        } else if is_ready {
            HealthStatus::Degraded
        } else {
            HealthStatus::Unhealthy
        };

        Self {
            status,
            is_healthy,
            is_ready,
            checks,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// Type alias for async health check functions
type HealthCheckFn = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Result<HealthStatus, String>> + Send>> + Send + Sync>;

/// Health checker manages and executes health checks
pub struct HealthChecker {
    checks: Arc<RwLock<HashMap<String, HealthCheckFn>>>,
    metadata: Arc<RwLock<HashMap<String, String>>>,
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthChecker {
    /// Create a new health checker
    pub fn new() -> Self {
        Self {
            checks: Arc::new(RwLock::new(HashMap::new())),
            metadata: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add metadata about the system
    pub async fn set_metadata(&self, key: impl Into<String>, value: impl Into<String>) {
        self.metadata.write().await.insert(key.into(), value.into());
    }

    /// Add a health check
    pub async fn add_check<F>(&self, name: impl Into<String>, check: F)
    where
        F: Fn() -> Pin<Box<dyn Future<Output = Result<HealthStatus, String>> + Send>> + Send + Sync + 'static,
    {
        self.checks
            .write()
            .await
            .insert(name.into(), Arc::new(check));
    }

    /// Remove a health check
    pub async fn remove_check(&self, name: &str) -> bool {
        self.checks.write().await.remove(name).is_some()
    }

    /// Check overall health
    pub async fn check_health(&self) -> HealthReport {
        let checks = self.checks.read().await;
        let mut results = Vec::new();

        for (name, check_fn) in checks.iter() {
            let result = match check_fn().await {
                Ok(HealthStatus::Healthy) => HealthCheckResult::healthy(name.clone()),
                Ok(HealthStatus::Degraded) => {
                    HealthCheckResult::degraded(name.clone(), "Component is degraded")
                }
                Ok(HealthStatus::Unhealthy) => {
                    HealthCheckResult::unhealthy(name.clone(), "Component is unhealthy")
                }
                Err(msg) => HealthCheckResult::unhealthy(name.clone(), msg),
            };
            results.push(result);
        }

        HealthReport::new(results)
    }

    /// Check if system is ready (for Kubernetes readiness probe)
    pub async fn is_ready(&self) -> bool {
        let report = self.check_health().await;
        report.is_ready
    }

    /// Check if system is alive (for Kubernetes liveness probe)
    ///
    /// This is a simpler check than full health - just verifies the process is responsive
    pub async fn is_alive(&self) -> bool {
        true // If we can execute this, we're alive
    }
}

/// Built-in health checks

/// Check if metrics are enabled
pub async fn check_metrics_enabled() -> Result<HealthStatus, String> {
    #[cfg(feature = "metrics")]
    {
        use crate::metrics::METRICS_ENABLED;
        use std::sync::atomic::Ordering;

        if METRICS_ENABLED.load(Ordering::Relaxed) {
            Ok(HealthStatus::Healthy)
        } else {
            Ok(HealthStatus::Degraded)
        }
    }
    #[cfg(not(feature = "metrics"))]
    {
        Ok(HealthStatus::Degraded)
    }
}

/// Check memory usage
pub async fn check_memory_usage() -> Result<HealthStatus, String> {
    // This is a placeholder - real implementation would check actual memory
    Ok(HealthStatus::Healthy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status() {
        assert!(HealthStatus::Healthy.is_healthy());
        assert!(HealthStatus::Healthy.is_functional());
        assert!(!HealthStatus::Unhealthy.is_healthy());
        assert!(!HealthStatus::Unhealthy.is_functional());
        assert!(HealthStatus::Degraded.is_functional());
        assert!(!HealthStatus::Degraded.is_healthy());
    }

    #[test]
    fn test_health_check_result() {
        let result = HealthCheckResult::healthy("test");
        assert_eq!(result.name, "test");
        assert_eq!(result.status, HealthStatus::Healthy);
        assert!(result.message.is_none());

        let result = HealthCheckResult::unhealthy("test", "error message");
        assert_eq!(result.status, HealthStatus::Unhealthy);
        assert_eq!(result.message.unwrap(), "error message");
    }

    #[test]
    fn test_health_check_result_with_metric() {
        let result = HealthCheckResult::healthy("test")
            .with_metric("connections", "10")
            .with_metric("latency_ms", "50");

        assert_eq!(result.metrics.len(), 2);
        assert_eq!(result.metrics.get("connections").unwrap(), "10");
    }

    #[test]
    fn test_health_report() {
        let checks = vec![
            HealthCheckResult::healthy("component1"),
            HealthCheckResult::healthy("component2"),
        ];
        let report = HealthReport::new(checks);

        assert_eq!(report.status, HealthStatus::Healthy);
        assert!(report.is_healthy);
        assert!(report.is_ready);
    }

    #[test]
    fn test_health_report_degraded() {
        let checks = vec![
            HealthCheckResult::healthy("component1"),
            HealthCheckResult::degraded("component2", "minor issue"),
        ];
        let report = HealthReport::new(checks);

        assert_eq!(report.status, HealthStatus::Degraded);
        assert!(!report.is_healthy);
        assert!(report.is_ready);
    }

    #[test]
    fn test_health_report_unhealthy() {
        let checks = vec![
            HealthCheckResult::healthy("component1"),
            HealthCheckResult::unhealthy("component2", "critical error"),
        ];
        let report = HealthReport::new(checks);

        assert_eq!(report.status, HealthStatus::Unhealthy);
        assert!(!report.is_healthy);
        assert!(!report.is_ready);
    }

    #[tokio::test]
    async fn test_health_checker_basic() {
        let checker = HealthChecker::new();

        checker
            .add_check("test", || {
                Box::pin(async { Ok(HealthStatus::Healthy) })
            })
            .await;

        let report = checker.check_health().await;
        assert!(report.is_healthy);
        assert_eq!(report.checks.len(), 1);
    }

    #[tokio::test]
    async fn test_health_checker_multiple() {
        let checker = HealthChecker::new();

        checker
            .add_check("component1", || {
                Box::pin(async { Ok(HealthStatus::Healthy) })
            })
            .await;

        checker
            .add_check("component2", || {
                Box::pin(async { Ok(HealthStatus::Degraded) })
            })
            .await;

        let report = checker.check_health().await;
        assert!(!report.is_healthy);
        assert!(report.is_ready);
        assert_eq!(report.checks.len(), 2);
    }

    #[tokio::test]
    async fn test_health_checker_remove() {
        let checker = HealthChecker::new();

        checker
            .add_check("test", || {
                Box::pin(async { Ok(HealthStatus::Healthy) })
            })
            .await;

        assert!(checker.remove_check("test").await);
        assert!(!checker.remove_check("nonexistent").await);

        let report = checker.check_health().await;
        assert_eq!(report.checks.len(), 0);
    }

    #[tokio::test]
    async fn test_health_checker_ready() {
        let checker = HealthChecker::new();

        checker
            .add_check("test", || {
                Box::pin(async { Ok(HealthStatus::Healthy) })
            })
            .await;

        assert!(checker.is_ready().await);
        assert!(checker.is_alive().await);
    }

    #[tokio::test]
    async fn test_health_checker_metadata() {
        let checker = HealthChecker::new();
        checker.set_metadata("version", "0.2.0").await;
        checker.set_metadata("environment", "test").await;

        let metadata = checker.metadata.read().await;
        assert_eq!(metadata.get("version").unwrap(), "0.2.0");
        assert_eq!(metadata.get("environment").unwrap(), "test");
    }
}

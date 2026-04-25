//! Unified resource management for workflow execution.
//!
//! This module provides the `ResourceManager` which coordinates all resource management concerns:
//! - Memory limits and monitoring
//! - Concurrency control
//! - Resource statistics and alerts
//!
//! # Examples
//!
//! ```rust
//! use agentflow_core::resource_manager::{ResourceManager, ResourceManagerConfig};
//!
//! #[tokio::main]
//! async fn main() {
//!     // Create manager with default configuration
//!     let manager = ResourceManager::new(ResourceManagerConfig::default());
//!
//!     // Acquire a concurrency permit
//!     let permit = manager.acquire_global_permit().await.unwrap();
//!
//!     // Track memory allocation
//!     manager.record_allocation("key1", 1024);
//!
//!     // Check if cleanup is needed
//!     if manager.should_cleanup() {
//!         let result = manager.cleanup(0.5).await.unwrap();
//!         println!("Freed {} bytes", result.0);
//!     }
//!
//!     // Get comprehensive statistics
//!     let stats = manager.get_stats().await;
//!     println!("Resource usage: {:?}", stats);
//! }
//! ```

use crate::concurrency::{ConcurrencyConfig, ConcurrencyLimiter, ConcurrencyStats, ScopedPermit};
use crate::error::Result;
use crate::resource_limits::ResourceLimits;
use crate::state_monitor::{ResourceAlert, ResourceStats, StateMonitor};
use serde::{Deserialize, Serialize};

/// Configuration for the unified resource manager.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceManagerConfig {
  /// Memory limits configuration
  pub memory_limits: ResourceLimits,

  /// Concurrency limits configuration
  pub concurrency_limits: ConcurrencyConfig,

  /// Enable detailed resource tracking
  pub enable_detailed_tracking: bool,

  /// Workflow-level memory limit override (bytes)
  pub workflow_memory_limit: Option<usize>,

  /// Node-level memory limit override (bytes)
  pub node_memory_limit: Option<usize>,
}

impl Default for ResourceManagerConfig {
  fn default() -> Self {
    Self {
      memory_limits: ResourceLimits::default(),
      concurrency_limits: ConcurrencyConfig::default(),
      enable_detailed_tracking: true,
      workflow_memory_limit: Some(2 * 1024 * 1024 * 1024), // 2 GB default
      node_memory_limit: Some(100 * 1024 * 1024),          // 100 MB default
    }
  }
}

impl ResourceManagerConfig {
  /// Create a new builder for ResourceManagerConfig.
  pub fn builder() -> ResourceManagerConfigBuilder {
    ResourceManagerConfigBuilder::default()
  }
}

/// Builder for ResourceManagerConfig.
#[derive(Default)]
pub struct ResourceManagerConfigBuilder {
  memory_limits: Option<ResourceLimits>,
  concurrency_limits: Option<ConcurrencyConfig>,
  enable_detailed_tracking: Option<bool>,
  workflow_memory_limit: Option<usize>,
  node_memory_limit: Option<usize>,
}

impl ResourceManagerConfigBuilder {
  pub fn memory_limits(mut self, limits: ResourceLimits) -> Self {
    self.memory_limits = Some(limits);
    self
  }

  pub fn concurrency_limits(mut self, limits: ConcurrencyConfig) -> Self {
    self.concurrency_limits = Some(limits);
    self
  }

  pub fn enable_detailed_tracking(mut self, enabled: bool) -> Self {
    self.enable_detailed_tracking = Some(enabled);
    self
  }

  pub fn workflow_memory_limit(mut self, limit: usize) -> Self {
    self.workflow_memory_limit = Some(limit);
    self
  }

  pub fn node_memory_limit(mut self, limit: usize) -> Self {
    self.node_memory_limit = Some(limit);
    self
  }

  pub fn build(self) -> ResourceManagerConfig {
    let defaults = ResourceManagerConfig::default();
    ResourceManagerConfig {
      memory_limits: self.memory_limits.unwrap_or(defaults.memory_limits),
      concurrency_limits: self
        .concurrency_limits
        .unwrap_or(defaults.concurrency_limits),
      enable_detailed_tracking: self
        .enable_detailed_tracking
        .unwrap_or(defaults.enable_detailed_tracking),
      workflow_memory_limit: self
        .workflow_memory_limit
        .or(defaults.workflow_memory_limit),
      node_memory_limit: self.node_memory_limit.or(defaults.node_memory_limit),
    }
  }
}

/// Unified resource manager for workflow execution.
///
/// Coordinates memory management, concurrency control, and resource monitoring.
#[derive(Clone)]
pub struct ResourceManager {
  config: ResourceManagerConfig,
  concurrency_limiter: ConcurrencyLimiter,
  state_monitor: StateMonitor,
}

impl ResourceManager {
  /// Create a new ResourceManager with the given configuration.
  pub fn new(config: ResourceManagerConfig) -> Self {
    let state_monitor = if config.enable_detailed_tracking {
      StateMonitor::new(config.memory_limits.clone())
    } else {
      StateMonitor::new_fast(config.memory_limits.clone())
    };

    Self {
      concurrency_limiter: ConcurrencyLimiter::new(config.concurrency_limits.clone()),
      state_monitor,
      config,
    }
  }

  /// Get the configuration.
  pub fn config(&self) -> &ResourceManagerConfig {
    &self.config
  }

  // ===== Concurrency Management =====

  /// Acquire a global concurrency permit.
  pub async fn acquire_global_permit(&self) -> Result<ScopedPermit> {
    self.concurrency_limiter.acquire_global().await
  }

  /// Acquire a workflow-level concurrency permit.
  pub async fn acquire_workflow_permit(&self, workflow_id: &str) -> Result<ScopedPermit> {
    self.concurrency_limiter.acquire_workflow(workflow_id).await
  }

  /// Acquire a node-type-specific concurrency permit.
  pub async fn acquire_node_type_permit(&self, node_type: &str) -> Result<ScopedPermit> {
    self.concurrency_limiter.acquire_node_type(node_type).await
  }

  /// Get the number of available global concurrency permits.
  pub fn available_global_permits(&self) -> usize {
    self.concurrency_limiter.available_global()
  }

  /// Cleanup workflow-specific concurrency resources.
  pub async fn cleanup_workflow(&self, workflow_id: &str) {
    self.concurrency_limiter.cleanup_workflow(workflow_id).await
  }

  // ===== Memory Management =====

  /// Record a memory allocation for a key.
  ///
  /// Returns `true` if allocation was successful, `false` if limits would be exceeded.
  pub fn record_allocation(&self, key: &str, size: usize) -> bool {
    self.state_monitor.record_allocation(key, size)
  }

  /// Record a memory deallocation for a key.
  pub fn record_deallocation(&self, key: &str) {
    self.state_monitor.record_deallocation(key)
  }

  /// Record an access to a key (for LRU tracking).
  pub fn record_access(&self, key: &str) {
    self.state_monitor.record_access(key)
  }

  /// Get the current total memory usage in bytes.
  pub fn current_memory_usage(&self) -> usize {
    self.state_monitor.current_size()
  }

  /// Get the current number of stored values.
  pub fn value_count(&self) -> usize {
    self.state_monitor.value_count()
  }

  /// Get memory usage as a percentage (0.0 - 1.0).
  pub fn memory_usage_percentage(&self) -> f64 {
    self.state_monitor.usage_percentage()
  }

  /// Check if cleanup should be triggered.
  pub fn should_cleanup(&self) -> bool {
    self.state_monitor.should_cleanup()
  }

  /// Perform automatic cleanup to reduce memory usage to target percentage.
  ///
  /// Returns (bytes_freed, entries_removed).
  pub async fn cleanup(&self, target_percentage: f64) -> Result<(usize, usize)> {
    self
      .state_monitor
      .cleanup(target_percentage)
      .map_err(|e| crate::error::AgentFlowError::MonitoringError { message: e })
  }

  /// Get all resource alerts.
  pub fn get_alerts(&self) -> Vec<ResourceAlert> {
    self.state_monitor.get_alerts()
  }

  /// Clear all resource alerts.
  pub fn clear_alerts(&self) {
    self.state_monitor.clear_alerts()
  }

  /// Reset all monitoring state.
  pub fn reset(&self) {
    self.state_monitor.reset();
  }

  // ===== Statistics and Monitoring =====

  /// Get comprehensive resource usage statistics.
  pub async fn get_stats(&self) -> CombinedResourceStats {
    CombinedResourceStats {
      memory: self.state_monitor.get_stats(),
      concurrency: self.concurrency_limiter.get_stats().await,
      alerts: self.state_monitor.peek_alerts(),
      workflow_memory_limit: self.config.workflow_memory_limit,
      node_memory_limit: self.config.node_memory_limit,
    }
  }

  /// Get memory statistics only.
  pub fn get_memory_stats(&self) -> ResourceStats {
    self.state_monitor.get_stats()
  }

  /// Get concurrency statistics only.
  pub async fn get_concurrency_stats(&self) -> ConcurrencyStats {
    self.concurrency_limiter.get_stats().await
  }

  /// Reset all statistics.
  pub async fn reset_stats(&self) {
    self.state_monitor.reset();
    self.concurrency_limiter.reset_stats().await;
  }
}

/// Combined resource usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombinedResourceStats {
  /// Memory usage statistics
  pub memory: ResourceStats,

  /// Concurrency usage statistics
  pub concurrency: ConcurrencyStats,

  /// Active resource alerts
  pub alerts: Vec<ResourceAlert>,

  /// Workflow-level memory limit (bytes)
  pub workflow_memory_limit: Option<usize>,

  /// Node-level memory limit (bytes)
  pub node_memory_limit: Option<usize>,
}

impl std::fmt::Display for CombinedResourceStats {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    writeln!(f, "=== Resource Usage Statistics ===")?;
    writeln!(f, "Memory: {}", self.memory)?;
    writeln!(f, "Concurrency: {}", self.concurrency)?;
    if !self.alerts.is_empty() {
      writeln!(f, "Alerts: {} active", self.alerts.len())?;
      for alert in &self.alerts {
        writeln!(f, "  - {}", alert)?;
      }
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_resource_manager_creation() {
    let config = ResourceManagerConfig::default();
    let manager = ResourceManager::new(config);

    assert_eq!(manager.current_memory_usage(), 0);
    assert_eq!(manager.value_count(), 0);
    assert!(manager.available_global_permits() > 0);
  }

  #[tokio::test]
  async fn test_memory_allocation() {
    let manager = ResourceManager::new(ResourceManagerConfig::default());

    assert!(manager.record_allocation("key1", 1024));
    assert_eq!(manager.current_memory_usage(), 1024);
    assert_eq!(manager.value_count(), 1);

    assert!(manager.record_allocation("key2", 2048));
    assert_eq!(manager.current_memory_usage(), 3072);
    assert_eq!(manager.value_count(), 2);

    manager.record_deallocation("key1");
    assert_eq!(manager.current_memory_usage(), 2048);
    assert_eq!(manager.value_count(), 1);
  }

  #[tokio::test]
  async fn test_concurrency_control() {
    let config = ResourceManagerConfig::builder()
      .concurrency_limits(ConcurrencyConfig::builder().global_limit(2).build())
      .build();
    let manager = ResourceManager::new(config);

    let permit1 = manager.acquire_global_permit().await.unwrap();
    let permit2 = manager.acquire_global_permit().await.unwrap();

    assert_eq!(manager.available_global_permits(), 0);

    drop(permit1);
    drop(permit2);
  }

  #[tokio::test]
  async fn test_cleanup() {
    let config = ResourceManagerConfig::builder()
      .memory_limits(
        ResourceLimits::builder()
          .max_state_size(10000)
          .cleanup_threshold(0.8)
          .build(),
      )
      .build();
    let manager = ResourceManager::new(config);

    manager.record_allocation("key1", 3000);
    manager.record_allocation("key2", 3000);
    manager.record_allocation("key3", 3000);

    assert_eq!(manager.current_memory_usage(), 9000);

    let result = manager.cleanup(0.5).await.unwrap();
    assert!(result.0 >= 4000); // Should free at least 4000 bytes
    assert!(manager.current_memory_usage() <= 5000);
  }

  #[tokio::test]
  async fn test_combined_stats() {
    let manager = ResourceManager::new(ResourceManagerConfig::default());

    manager.record_allocation("key1", 1024);
    let _permit = manager.acquire_global_permit().await.unwrap();

    let stats = manager.get_stats().await;
    assert_eq!(stats.memory.current_size, 1024);
    assert_eq!(stats.concurrency.total_acquire_attempts, 1);
  }

  #[tokio::test]
  async fn test_workflow_cleanup() {
    let manager = ResourceManager::new(ResourceManagerConfig::default());

    let _permit = manager.acquire_workflow_permit("wf1").await.unwrap();

    manager.cleanup_workflow("wf1").await;

    // Workflow should be cleaned up
    let stats = manager.get_concurrency_stats().await;
    assert!(!stats.current_workflow_active.contains_key("wf1"));
  }

  #[tokio::test]
  async fn test_alerts() {
    let config = ResourceManagerConfig::builder()
      .memory_limits(
        ResourceLimits::builder()
          .max_value_size(1000)
          .auto_cleanup(false)
          .build(),
      )
      .build();
    let manager = ResourceManager::new(config);

    // Try to allocate too large value
    assert!(!manager.record_allocation("too_large", 2000));

    let alerts = manager.get_alerts();
    assert!(!alerts.is_empty());

    manager.clear_alerts();
    assert!(manager.get_alerts().is_empty());
  }

  #[test]
  fn test_config_builder() {
    let config = ResourceManagerConfig::builder()
      .workflow_memory_limit(1024 * 1024 * 1024) // 1 GB
      .node_memory_limit(50 * 1024 * 1024) // 50 MB
      .enable_detailed_tracking(false)
      .build();

    assert_eq!(config.workflow_memory_limit, Some(1024 * 1024 * 1024));
    assert_eq!(config.node_memory_limit, Some(50 * 1024 * 1024));
    assert!(!config.enable_detailed_tracking);
  }

  #[tokio::test]
  async fn test_reset_stats() {
    let manager = ResourceManager::new(ResourceManagerConfig::default());

    manager.record_allocation("key1", 1024);
    let _permit = manager.acquire_global_permit().await.unwrap();

    assert_eq!(manager.current_memory_usage(), 1024);

    manager.reset_stats().await;

    assert_eq!(manager.current_memory_usage(), 0);
    assert_eq!(manager.value_count(), 0);
  }

  #[tokio::test]
  async fn test_memory_usage_percentage() {
    let config = ResourceManagerConfig::builder()
      .memory_limits(ResourceLimits::builder().max_state_size(10000).build())
      .build();
    let manager = ResourceManager::new(config);

    manager.record_allocation("key1", 5000);
    assert_eq!(manager.memory_usage_percentage(), 0.5);

    manager.record_allocation("key2", 2500);
    assert_eq!(manager.memory_usage_percentage(), 0.75);
  }
}

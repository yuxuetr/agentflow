//! Concurrency control and rate limiting for workflow execution.
//!
//! This module provides configurable concurrency limits at multiple levels:
//! - Global: Limits total concurrent operations across all workflows
//! - Workflow: Limits concurrent operations within a single workflow
//! - Node Type: Limits concurrent operations for specific node types (e.g., LLM calls)
//!
//! # Examples
//!
//! ```rust
//! use agentflow_core::concurrency::{ConcurrencyLimiter, ConcurrencyConfig};
//!
//! #[tokio::main]
//! async fn main() {
//!     // Create limiter with default config (CPU cores * 2)
//!     let limiter = ConcurrencyLimiter::new(ConcurrencyConfig::default());
//!
//!     // Acquire a permit for global operations
//!     let permit = limiter.acquire_global().await.unwrap();
//!
//!     // ... perform operation ...
//!
//!     // Permit is automatically released when dropped
//!     drop(permit);
//! }
//! ```

use crate::error::{AgentFlowError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::timeout;

/// Configuration for concurrency limits at different levels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrencyConfig {
  /// Global concurrency limit (default: CPU cores * 2)
  pub global_limit: usize,

  /// Default workflow-level concurrency limit
  pub workflow_limit: usize,

  /// Node type-specific limits (e.g., "llm" -> 5, "http" -> 50)
  pub node_type_limits: HashMap<String, usize>,

  /// Timeout for acquiring permits (in milliseconds)
  pub acquire_timeout_ms: u64,

  /// Enable detailed statistics tracking
  pub enable_stats: bool,
}

impl Default for ConcurrencyConfig {
  fn default() -> Self {
    let cpu_count = num_cpus::get();
    Self {
      global_limit: cpu_count * 2,
      workflow_limit: cpu_count,
      node_type_limits: HashMap::new(),
      acquire_timeout_ms: 30000, // 30 seconds
      enable_stats: true,
    }
  }
}

impl ConcurrencyConfig {
  /// Create a new builder for ConcurrencyConfig.
  pub fn builder() -> ConcurrencyConfigBuilder {
    ConcurrencyConfigBuilder::default()
  }

  /// Set a node type limit.
  pub fn with_node_type_limit(mut self, node_type: impl Into<String>, limit: usize) -> Self {
    self.node_type_limits.insert(node_type.into(), limit);
    self
  }

  /// Get the limit for a specific node type.
  pub fn get_node_type_limit(&self, node_type: &str) -> Option<usize> {
    self.node_type_limits.get(node_type).copied()
  }
}

/// Builder for ConcurrencyConfig.
#[derive(Default)]
pub struct ConcurrencyConfigBuilder {
  global_limit: Option<usize>,
  workflow_limit: Option<usize>,
  node_type_limits: HashMap<String, usize>,
  acquire_timeout_ms: Option<u64>,
  enable_stats: Option<bool>,
}

impl ConcurrencyConfigBuilder {
  pub fn global_limit(mut self, limit: usize) -> Self {
    self.global_limit = Some(limit);
    self
  }

  pub fn workflow_limit(mut self, limit: usize) -> Self {
    self.workflow_limit = Some(limit);
    self
  }

  pub fn node_type_limit(mut self, node_type: impl Into<String>, limit: usize) -> Self {
    self.node_type_limits.insert(node_type.into(), limit);
    self
  }

  pub fn acquire_timeout_ms(mut self, timeout_ms: u64) -> Self {
    self.acquire_timeout_ms = Some(timeout_ms);
    self
  }

  pub fn enable_stats(mut self, enabled: bool) -> Self {
    self.enable_stats = Some(enabled);
    self
  }

  pub fn build(self) -> ConcurrencyConfig {
    let defaults = ConcurrencyConfig::default();
    ConcurrencyConfig {
      global_limit: self.global_limit.unwrap_or(defaults.global_limit),
      workflow_limit: self.workflow_limit.unwrap_or(defaults.workflow_limit),
      node_type_limits: if self.node_type_limits.is_empty() {
        defaults.node_type_limits
      } else {
        self.node_type_limits
      },
      acquire_timeout_ms: self
        .acquire_timeout_ms
        .unwrap_or(defaults.acquire_timeout_ms),
      enable_stats: self.enable_stats.unwrap_or(defaults.enable_stats),
    }
  }
}

/// Concurrency limiter for workflow execution.
///
/// Provides multi-level concurrency control using Tokio semaphores.
/// Thread-safe and designed for concurrent workflow execution.
#[derive(Clone)]
pub struct ConcurrencyLimiter {
  config: Arc<ConcurrencyConfig>,

  /// Global semaphore for all operations
  global_semaphore: Arc<Semaphore>,

  /// Per-workflow semaphores
  workflow_semaphores: Arc<tokio::sync::RwLock<HashMap<String, Arc<Semaphore>>>>,

  /// Per-node-type semaphores
  node_type_semaphores: Arc<tokio::sync::RwLock<HashMap<String, Arc<Semaphore>>>>,

  /// Statistics tracking
  stats: Arc<tokio::sync::RwLock<ConcurrencyStats>>,
}

impl ConcurrencyLimiter {
  /// Create a new ConcurrencyLimiter with the given configuration.
  pub fn new(config: ConcurrencyConfig) -> Self {
    Self {
      global_semaphore: Arc::new(Semaphore::new(config.global_limit)),
      workflow_semaphores: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
      node_type_semaphores: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
      stats: Arc::new(tokio::sync::RwLock::new(ConcurrencyStats::default())),
      config: Arc::new(config),
    }
  }

  /// Get the configuration.
  pub fn config(&self) -> &ConcurrencyConfig {
    &self.config
  }

  /// Acquire a global permit.
  ///
  /// Blocks until a permit is available or timeout is reached.
  pub async fn acquire_global(&self) -> Result<ScopedPermit> {
    let timeout_duration = Duration::from_millis(self.config.acquire_timeout_ms);

    // Update stats
    if self.config.enable_stats {
      let mut stats = self.stats.write().await;
      stats.total_acquire_attempts += 1;
    }

    let semaphore = self.global_semaphore.clone();
    match timeout(timeout_duration, semaphore.acquire_owned()).await {
      Ok(Ok(permit)) => {
        if self.config.enable_stats {
          let mut stats = self.stats.write().await;
          stats.current_global_active += 1;
          stats.peak_global_active = stats.peak_global_active.max(stats.current_global_active);
        }

        Ok(ScopedPermit {
          _permit: permit,
          stats: if self.config.enable_stats {
            Some(self.stats.clone())
          } else {
            None
          },
          scope: PermitScope::Global,
        })
      }
      Ok(Err(_)) => Err(AgentFlowError::ConcurrencyLimitExceeded {
        limit: self.config.global_limit,
      }),
      Err(_) => Err(AgentFlowError::TimeoutExceeded {
        duration_ms: self.config.acquire_timeout_ms,
      }),
    }
  }

  /// Acquire a workflow-level permit.
  pub async fn acquire_workflow(&self, workflow_id: &str) -> Result<ScopedPermit> {
    // Get or create workflow semaphore
    let semaphore = {
      let mut workflows = self.workflow_semaphores.write().await;
      workflows
        .entry(workflow_id.to_string())
        .or_insert_with(|| Arc::new(Semaphore::new(self.config.workflow_limit)))
        .clone()
    };

    let timeout_duration = Duration::from_millis(self.config.acquire_timeout_ms);

    match timeout(timeout_duration, semaphore.acquire_owned()).await {
      Ok(Ok(permit)) => {
        if self.config.enable_stats {
          let mut stats = self.stats.write().await;
          *stats
            .current_workflow_active
            .entry(workflow_id.to_string())
            .or_insert(0) += 1;
        }

        Ok(ScopedPermit {
          _permit: permit,
          stats: if self.config.enable_stats {
            Some(self.stats.clone())
          } else {
            None
          },
          scope: PermitScope::Workflow(workflow_id.to_string()),
        })
      }
      Ok(Err(_)) => Err(AgentFlowError::ConcurrencyLimitExceeded {
        limit: self.config.workflow_limit,
      }),
      Err(_) => Err(AgentFlowError::TimeoutExceeded {
        duration_ms: self.config.acquire_timeout_ms,
      }),
    }
  }

  /// Acquire a node-type-specific permit.
  pub async fn acquire_node_type(&self, node_type: &str) -> Result<ScopedPermit> {
    // Get limit for this node type
    let limit = self.config.get_node_type_limit(node_type).ok_or_else(|| {
      AgentFlowError::ConfigurationError {
        message: format!(
          "No concurrency limit configured for node type: {}",
          node_type
        ),
      }
    })?;

    // Get or create node type semaphore
    let semaphore = {
      let mut node_types = self.node_type_semaphores.write().await;
      node_types
        .entry(node_type.to_string())
        .or_insert_with(|| Arc::new(Semaphore::new(limit)))
        .clone()
    };

    let timeout_duration = Duration::from_millis(self.config.acquire_timeout_ms);

    match timeout(timeout_duration, semaphore.acquire_owned()).await {
      Ok(Ok(permit)) => {
        if self.config.enable_stats {
          let mut stats = self.stats.write().await;
          *stats
            .current_node_type_active
            .entry(node_type.to_string())
            .or_insert(0) += 1;
        }

        Ok(ScopedPermit {
          _permit: permit,
          stats: if self.config.enable_stats {
            Some(self.stats.clone())
          } else {
            None
          },
          scope: PermitScope::NodeType(node_type.to_string()),
        })
      }
      Ok(Err(_)) => Err(AgentFlowError::ConcurrencyLimitExceeded { limit }),
      Err(_) => Err(AgentFlowError::TimeoutExceeded {
        duration_ms: self.config.acquire_timeout_ms,
      }),
    }
  }

  /// Get current concurrency statistics.
  pub async fn get_stats(&self) -> ConcurrencyStats {
    self.stats.read().await.clone()
  }

  /// Reset statistics.
  pub async fn reset_stats(&self) {
    *self.stats.write().await = ConcurrencyStats::default();
  }

  /// Get the number of available permits for global operations.
  pub fn available_global(&self) -> usize {
    self.global_semaphore.available_permits()
  }

  /// Cleanup workflow semaphores for completed workflows.
  pub async fn cleanup_workflow(&self, workflow_id: &str) {
    self.workflow_semaphores.write().await.remove(workflow_id);

    if self.config.enable_stats {
      self
        .stats
        .write()
        .await
        .current_workflow_active
        .remove(workflow_id);
    }
  }
}

/// RAII guard for acquired concurrency permits.
///
/// The permit is automatically released when this guard is dropped.
pub struct ScopedPermit {
  _permit: OwnedSemaphorePermit,
  stats: Option<Arc<tokio::sync::RwLock<ConcurrencyStats>>>,
  scope: PermitScope,
}

impl Drop for ScopedPermit {
  fn drop(&mut self) {
    let stats = self.stats.clone();
    let scope = self.scope.clone();

    if let Some(stats) = stats {
      // Update stats when permit is released
      // We spawn a task to avoid blocking the drop
      tokio::spawn(async move {
        let mut stats = stats.write().await;
        match scope {
          PermitScope::Global => {
            stats.current_global_active = stats.current_global_active.saturating_sub(1);
          }
          PermitScope::Workflow(workflow_id) => {
            if let Some(count) = stats.current_workflow_active.get_mut(&workflow_id) {
              *count = count.saturating_sub(1);
            }
          }
          PermitScope::NodeType(node_type) => {
            if let Some(count) = stats.current_node_type_active.get_mut(&node_type) {
              *count = count.saturating_sub(1);
            }
          }
        }
      });
    }
  }
}

#[derive(Debug, Clone)]
enum PermitScope {
  Global,
  Workflow(String),
  NodeType(String),
}

/// Statistics for concurrency usage.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConcurrencyStats {
  /// Total number of permit acquire attempts
  pub total_acquire_attempts: u64,

  /// Current number of active global permits
  pub current_global_active: usize,

  /// Peak number of global permits used
  pub peak_global_active: usize,

  /// Current active permits per workflow
  pub current_workflow_active: HashMap<String, usize>,

  /// Current active permits per node type
  pub current_node_type_active: HashMap<String, usize>,
}

impl std::fmt::Display for ConcurrencyStats {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      "Global: {}/{} peak, Workflows: {}, Node Types: {}, Total Attempts: {}",
      self.current_global_active,
      self.peak_global_active,
      self.current_workflow_active.len(),
      self.current_node_type_active.len(),
      self.total_acquire_attempts
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::atomic::{AtomicUsize, Ordering};

  #[tokio::test]
  async fn test_global_concurrency_limit() {
    let config = ConcurrencyConfig::builder()
      .global_limit(2)
      .enable_stats(true)
      .build();
    let limiter = ConcurrencyLimiter::new(config);

    // Acquire 2 permits
    let permit1 = limiter.acquire_global().await.unwrap();
    let permit2 = limiter.acquire_global().await.unwrap();

    assert_eq!(limiter.available_global(), 0);

    // Try to acquire a third - should timeout quickly
    let config_short_timeout = ConcurrencyConfig::builder()
      .global_limit(2)
      .acquire_timeout_ms(100)
      .build();
    let limiter_short = ConcurrencyLimiter::new(config_short_timeout);
    let _p1 = limiter_short.acquire_global().await.unwrap();
    let _p2 = limiter_short.acquire_global().await.unwrap();

    let result = limiter_short.acquire_global().await;
    assert!(result.is_err());

    // Release permits
    drop(permit1);
    drop(permit2);
  }

  #[tokio::test]
  async fn test_workflow_concurrency_limit() {
    let config = ConcurrencyConfig::builder()
      .workflow_limit(2)
      .acquire_timeout_ms(100)
      .build();
    let limiter = ConcurrencyLimiter::new(config);

    let permit1 = limiter.acquire_workflow("wf1").await.unwrap();
    let permit2 = limiter.acquire_workflow("wf1").await.unwrap();

    // Third permit should timeout
    let result = limiter.acquire_workflow("wf1").await;
    assert!(result.is_err());

    // Different workflow should succeed
    let permit3 = limiter.acquire_workflow("wf2").await.unwrap();

    drop(permit1);
    drop(permit2);
    drop(permit3);
  }

  #[tokio::test]
  async fn test_node_type_concurrency_limit() {
    let config = ConcurrencyConfig::builder()
      .node_type_limit("llm", 3)
      .acquire_timeout_ms(100)
      .build();
    let limiter = ConcurrencyLimiter::new(config);

    let permit1 = limiter.acquire_node_type("llm").await.unwrap();
    let permit2 = limiter.acquire_node_type("llm").await.unwrap();
    let permit3 = limiter.acquire_node_type("llm").await.unwrap();

    // Fourth permit should timeout
    let result = limiter.acquire_node_type("llm").await;
    assert!(result.is_err());

    drop(permit1);
    drop(permit2);
    drop(permit3);
  }

  #[tokio::test]
  async fn test_concurrent_operations() {
    let config = ConcurrencyConfig::builder()
      .global_limit(10)
      .enable_stats(true)
      .build();
    let limiter = Arc::new(ConcurrencyLimiter::new(config));
    let counter = Arc::new(AtomicUsize::new(0));

    let mut handles = vec![];

    // Spawn 20 tasks that each acquire a permit and increment counter
    for _ in 0..20 {
      let limiter = limiter.clone();
      let counter = counter.clone();

      let handle = tokio::spawn(async move {
        let _permit = limiter.acquire_global().await.unwrap();
        counter.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(10)).await;
      });

      handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
      handle.await.unwrap();
    }

    // All tasks should have completed
    assert_eq!(counter.load(Ordering::SeqCst), 20);

    // Check stats
    let stats = limiter.get_stats().await;
    assert_eq!(stats.total_acquire_attempts, 20);
    assert!(stats.peak_global_active <= 10);
  }

  #[tokio::test]
  async fn test_cleanup_workflow() {
    let config = ConcurrencyConfig::default();
    let limiter = ConcurrencyLimiter::new(config);

    let _permit = limiter.acquire_workflow("wf1").await.unwrap();

    assert!(limiter.workflow_semaphores.read().await.contains_key("wf1"));

    limiter.cleanup_workflow("wf1").await;

    assert!(!limiter.workflow_semaphores.read().await.contains_key("wf1"));
  }

  #[tokio::test]
  async fn test_stats_tracking() {
    let config = ConcurrencyConfig::builder()
      .global_limit(5)
      .enable_stats(true)
      .build();
    let limiter = ConcurrencyLimiter::new(config);

    let permit1 = limiter.acquire_global().await.unwrap();
    let permit2 = limiter.acquire_global().await.unwrap();

    let stats = limiter.get_stats().await;
    assert_eq!(stats.current_global_active, 2);
    assert_eq!(stats.peak_global_active, 2);
    assert_eq!(stats.total_acquire_attempts, 2);

    drop(permit1);
    drop(permit2);

    // Give some time for the async drop cleanup to complete
    tokio::time::sleep(Duration::from_millis(50)).await;
  }

  #[test]
  fn test_config_builder() {
    let config = ConcurrencyConfig::builder()
      .global_limit(100)
      .workflow_limit(50)
      .node_type_limit("llm", 10)
      .node_type_limit("http", 50)
      .acquire_timeout_ms(5000)
      .enable_stats(false)
      .build();

    assert_eq!(config.global_limit, 100);
    assert_eq!(config.workflow_limit, 50);
    assert_eq!(config.get_node_type_limit("llm"), Some(10));
    assert_eq!(config.get_node_type_limit("http"), Some(50));
    assert_eq!(config.acquire_timeout_ms, 5000);
    assert!(!config.enable_stats);
  }

  #[test]
  fn test_default_config() {
    let config = ConcurrencyConfig::default();
    let cpu_count = num_cpus::get();

    assert_eq!(config.global_limit, cpu_count * 2);
    assert_eq!(config.workflow_limit, cpu_count);
    assert!(config.enable_stats);
  }
}

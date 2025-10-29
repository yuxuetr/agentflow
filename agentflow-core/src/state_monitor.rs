//! Real-time resource usage monitoring and alerting.
//!
//! This module provides the `StateMonitor` for tracking workflow execution resource usage,
//! detecting when limits are approached, and triggering cleanup operations.
//!
//! # Examples
//!
//! ```rust
//! use agentflow_core::state_monitor::{StateMonitor, ResourceAlert};
//! use agentflow_core::resource_limits::ResourceLimits;
//!
//! let limits = ResourceLimits::default();
//! let monitor = StateMonitor::new(limits);
//!
//! // Track memory allocation
//! monitor.record_allocation("key1", 1024);
//! monitor.record_allocation("key2", 2048);
//!
//! // Check current usage
//! println!("Current size: {} bytes", monitor.current_size());
//! println!("Value count: {}", monitor.value_count());
//!
//! // Check for alerts
//! let alerts = monitor.get_alerts();
//! for alert in alerts {
//!     println!("Alert: {}", alert);
//! }
//! ```

use crate::resource_limits::ResourceLimits;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Real-time resource usage monitor for workflow execution.
///
/// Tracks memory allocations, value counts, and triggers alerts when limits are approached.
/// Thread-safe and designed for concurrent workflow execution.
#[derive(Clone)]
pub struct StateMonitor {
  /// Resource limits configuration
  limits: ResourceLimits,

  /// Current total state size in bytes (atomic for thread-safety)
  current_size: Arc<AtomicUsize>,

  /// Current number of values stored (atomic for thread-safety)
  value_count: Arc<AtomicUsize>,

  /// Map of key -> size for tracking individual allocations
  allocations: Arc<Mutex<HashMap<String, usize>>>,

  /// Map of key -> last access timestamp for LRU tracking
  access_times: Arc<Mutex<HashMap<String, u64>>>,

  /// Access counter for LRU tracking (monotonically increasing)
  access_counter: Arc<AtomicUsize>,

  /// Alert history
  alerts: Arc<Mutex<Vec<ResourceAlert>>>,

  /// Enable detailed tracking (impacts performance slightly)
  detailed_tracking: bool,
}

/// Resource usage alert types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ResourceAlert {
  /// Approaching a resource limit
  ApproachingLimit {
    /// Resource type (e.g., "state_size", "cache_entries")
    resource: String,
    /// Current usage as percentage of limit (0.0 - 1.0)
    percentage: f64,
    /// Current value
    current: usize,
    /// Limit value
    limit: usize,
  },

  /// Resource limit exceeded
  LimitExceeded {
    /// Resource type
    resource: String,
    /// Current value
    current: usize,
    /// Limit value
    limit: usize,
  },

  /// Automatic cleanup triggered
  CleanupTriggered {
    /// Number of bytes freed
    freed: usize,
    /// Number of entries removed
    entries_removed: usize,
  },

  /// Cleanup failed
  CleanupFailed {
    /// Error message
    message: String,
  },
}

impl fmt::Display for ResourceAlert {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      ResourceAlert::ApproachingLimit {
        resource,
        percentage,
        current,
        limit,
      } => write!(
        f,
        "Approaching limit for {}: {:.1}% ({}/{})",
        resource,
        percentage * 100.0,
        format_bytes(*current),
        format_bytes(*limit)
      ),
      ResourceAlert::LimitExceeded {
        resource,
        current,
        limit,
      } => write!(
        f,
        "Limit exceeded for {}: {} > {}",
        resource,
        format_bytes(*current),
        format_bytes(*limit)
      ),
      ResourceAlert::CleanupTriggered {
        freed,
        entries_removed,
      } => write!(
        f,
        "Cleanup triggered: freed {}, removed {} entries",
        format_bytes(*freed),
        entries_removed
      ),
      ResourceAlert::CleanupFailed { message } => {
        write!(f, "Cleanup failed: {}", message)
      }
    }
  }
}

impl StateMonitor {
  /// Create a new StateMonitor with the given resource limits.
  pub fn new(limits: ResourceLimits) -> Self {
    Self {
      limits,
      current_size: Arc::new(AtomicUsize::new(0)),
      value_count: Arc::new(AtomicUsize::new(0)),
      allocations: Arc::new(Mutex::new(HashMap::new())),
      access_times: Arc::new(Mutex::new(HashMap::new())),
      access_counter: Arc::new(AtomicUsize::new(0)),
      alerts: Arc::new(Mutex::new(Vec::new())),
      detailed_tracking: true,
    }
  }

  /// Create a monitor with detailed tracking disabled (better performance).
  pub fn new_fast(limits: ResourceLimits) -> Self {
    let mut monitor = Self::new(limits);
    monitor.detailed_tracking = false;
    monitor
  }

  /// Get the resource limits configuration.
  pub fn limits(&self) -> &ResourceLimits {
    &self.limits
  }

  /// Get current total state size in bytes.
  pub fn current_size(&self) -> usize {
    self.current_size.load(Ordering::Relaxed)
  }

  /// Get current number of values stored.
  pub fn value_count(&self) -> usize {
    self.value_count.load(Ordering::Relaxed)
  }

  /// Get current memory usage as percentage of limit (0.0 - 1.0).
  pub fn usage_percentage(&self) -> f64 {
    let current = self.current_size() as f64;
    let limit = self.limits.max_state_size as f64;
    if limit == 0.0 {
      0.0
    } else {
      (current / limit).min(1.0)
    }
  }

  /// Record a memory allocation for a key.
  ///
  /// Returns `true` if allocation was successful, `false` if limits would be exceeded.
  pub fn record_allocation(&self, key: &str, size: usize) -> bool {
    // Check value size limit
    if self.limits.exceeds_value_limit(size) {
      self.add_alert(ResourceAlert::LimitExceeded {
        resource: "value_size".to_string(),
        current: size,
        limit: self.limits.max_value_size,
      });
      return false;
    }

    // Check if we need to update existing allocation
    let size_delta = if self.detailed_tracking {
      let mut allocations = self.allocations.lock().unwrap();
      let old_size = allocations.get(key).copied().unwrap_or(0);
      let delta = size as i64 - old_size as i64;

      // Update allocation tracking
      if size > 0 {
        allocations.insert(key.to_string(), size);
      } else {
        allocations.remove(key);
      }

      delta
    } else {
      size as i64
    };

    // Update total size
    if size_delta > 0 {
      let new_size = self
        .current_size
        .fetch_add(size_delta as usize, Ordering::Relaxed)
        + size_delta as usize;

      // Check state size limit
      if self.limits.exceeds_state_limit(new_size) {
        self.add_alert(ResourceAlert::LimitExceeded {
          resource: "state_size".to_string(),
          current: new_size,
          limit: self.limits.max_state_size,
        });

        // Rollback if auto_cleanup is disabled
        if !self.limits.auto_cleanup {
          self
            .current_size
            .fetch_sub(size_delta as usize, Ordering::Relaxed);
          if self.detailed_tracking {
            self.allocations.lock().unwrap().remove(key);
          }
          return false;
        }
      }

      // Check if approaching cleanup threshold
      if self.limits.should_cleanup(new_size) {
        let percentage = new_size as f64 / self.limits.max_state_size as f64;
        self.add_alert(ResourceAlert::ApproachingLimit {
          resource: "state_size".to_string(),
          percentage,
          current: new_size,
          limit: self.limits.max_state_size,
        });
      }
    } else if size_delta < 0 {
      self
        .current_size
        .fetch_sub((-size_delta) as usize, Ordering::Relaxed);
    }

    // Update value count
    if self.detailed_tracking {
      let mut allocations = self.allocations.lock().unwrap();
      let new_count = allocations.len();

      self.value_count.store(new_count, Ordering::Relaxed);

      // Check cache entries limit
      if self.limits.exceeds_cache_limit(new_count) {
        self.add_alert(ResourceAlert::LimitExceeded {
          resource: "cache_entries".to_string(),
          current: new_count,
          limit: self.limits.max_cache_entries,
        });

        if !self.limits.auto_cleanup {
          allocations.remove(key);
          self.value_count.store(allocations.len(), Ordering::Relaxed);
          return false;
        }
      }
    } else if size > 0 {
      self.value_count.fetch_add(1, Ordering::Relaxed);
    }

    // Update access time for LRU tracking
    if self.detailed_tracking {
      let access_time = self.access_counter.fetch_add(1, Ordering::Relaxed);
      self
        .access_times
        .lock()
        .unwrap()
        .insert(key.to_string(), access_time as u64);
    }

    true
  }

  /// Record a deallocation for a key.
  pub fn record_deallocation(&self, key: &str) {
    if !self.detailed_tracking {
      self.value_count.fetch_sub(1, Ordering::Relaxed);
      return;
    }

    let size = {
      let mut allocations = self.allocations.lock().unwrap();
      allocations.remove(key).unwrap_or(0)
    };

    if size > 0 {
      self.current_size.fetch_sub(size, Ordering::Relaxed);
      self.value_count.store(
        self.allocations.lock().unwrap().len(),
        Ordering::Relaxed,
      );
    }

    self.access_times.lock().unwrap().remove(key);
  }

  /// Record an access to a key (for LRU tracking).
  pub fn record_access(&self, key: &str) {
    if !self.detailed_tracking {
      return;
    }

    let access_time = self.access_counter.fetch_add(1, Ordering::Relaxed);
    self
      .access_times
      .lock()
      .unwrap()
      .insert(key.to_string(), access_time as u64);
  }

  /// Get the least recently used keys.
  ///
  /// Returns up to `count` keys sorted by access time (oldest first).
  pub fn get_lru_keys(&self, count: usize) -> Vec<String> {
    if !self.detailed_tracking {
      return Vec::new();
    }

    let access_times = self.access_times.lock().unwrap();
    let mut entries: Vec<_> = access_times.iter().collect();

    // Sort by access time (ascending = oldest first)
    entries.sort_by_key(|(_, &time)| time);

    entries
      .into_iter()
      .take(count)
      .map(|(key, _)| key.clone())
      .collect()
  }

  /// Get all allocated keys and their sizes.
  pub fn get_allocations(&self) -> HashMap<String, usize> {
    if !self.detailed_tracking {
      return HashMap::new();
    }

    self.allocations.lock().unwrap().clone()
  }

  /// Get resource usage statistics.
  pub fn get_stats(&self) -> ResourceStats {
    ResourceStats {
      current_size: self.current_size(),
      max_state_size: self.limits.max_state_size,
      usage_percentage: self.usage_percentage(),
      value_count: self.value_count(),
      max_cache_entries: self.limits.max_cache_entries,
      cleanup_threshold_bytes: self.limits.cleanup_threshold_bytes(),
      should_cleanup: self.limits.should_cleanup(self.current_size()),
    }
  }

  /// Check if cleanup should be triggered based on current usage.
  pub fn should_cleanup(&self) -> bool {
    self.limits.should_cleanup(self.current_size())
      || self.limits.exceeds_cache_limit(self.value_count())
  }

  /// Perform automatic cleanup by removing least recently used values.
  ///
  /// Returns the number of bytes freed and entries removed, or an error if cleanup fails.
  pub fn cleanup(&self, target_percentage: f64) -> Result<(usize, usize), String> {
    if !self.detailed_tracking {
      return Err("Detailed tracking disabled, cannot perform cleanup".to_string());
    }

    let target_size = (self.limits.max_state_size as f64 * target_percentage) as usize;
    let current = self.current_size();

    if current <= target_size {
      return Ok((0, 0)); // No cleanup needed
    }

    let to_free = current - target_size;
    let mut freed = 0;
    let mut removed = 0;

    // Get LRU keys to remove
    let allocations = self.allocations.lock().unwrap().clone();
    let lru_keys = self.get_lru_keys(allocations.len());

    for key in lru_keys {
      if freed >= to_free {
        break;
      }

      if let Some(size) = allocations.get(&key) {
        self.record_deallocation(&key);
        freed += size;
        removed += 1;
      }
    }

    if freed > 0 {
      self.add_alert(ResourceAlert::CleanupTriggered {
        freed,
        entries_removed: removed,
      });
    }

    Ok((freed, removed))
  }

  /// Add an alert to the alert history.
  fn add_alert(&self, alert: ResourceAlert) {
    self.alerts.lock().unwrap().push(alert);
  }

  /// Get all alerts and clear the alert history.
  pub fn get_alerts(&self) -> Vec<ResourceAlert> {
    let mut alerts = self.alerts.lock().unwrap();
    std::mem::take(&mut *alerts)
  }

  /// Get all alerts without clearing.
  pub fn peek_alerts(&self) -> Vec<ResourceAlert> {
    self.alerts.lock().unwrap().clone()
  }

  /// Clear all alerts.
  pub fn clear_alerts(&self) {
    self.alerts.lock().unwrap().clear();
  }

  /// Reset all monitoring state.
  pub fn reset(&self) {
    self.current_size.store(0, Ordering::Relaxed);
    self.value_count.store(0, Ordering::Relaxed);
    self.access_counter.store(0, Ordering::Relaxed);
    if self.detailed_tracking {
      self.allocations.lock().unwrap().clear();
      self.access_times.lock().unwrap().clear();
    }
    self.alerts.lock().unwrap().clear();
  }
}

/// Resource usage statistics snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceStats {
  /// Current total state size in bytes
  pub current_size: usize,
  /// Maximum allowed state size
  pub max_state_size: usize,
  /// Current usage as percentage (0.0 - 1.0)
  pub usage_percentage: f64,
  /// Current number of values
  pub value_count: usize,
  /// Maximum allowed cache entries
  pub max_cache_entries: usize,
  /// Cleanup threshold in bytes
  pub cleanup_threshold_bytes: usize,
  /// Whether cleanup should be triggered
  pub should_cleanup: bool,
}

impl fmt::Display for ResourceStats {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "Memory: {}/{} ({:.1}%), Entries: {}/{}, Cleanup: {}",
      format_bytes(self.current_size),
      format_bytes(self.max_state_size),
      self.usage_percentage * 100.0,
      self.value_count,
      self.max_cache_entries,
      if self.should_cleanup { "YES" } else { "NO" }
    )
  }
}

/// Helper function to format bytes in human-readable format.
fn format_bytes(bytes: usize) -> String {
  const KB: usize = 1024;
  const MB: usize = KB * 1024;
  const GB: usize = MB * 1024;

  if bytes >= GB {
    format!("{:.2} GB", bytes as f64 / GB as f64)
  } else if bytes >= MB {
    format!("{:.2} MB", bytes as f64 / MB as f64)
  } else if bytes >= KB {
    format!("{:.2} KB", bytes as f64 / KB as f64)
  } else {
    format!("{} B", bytes)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_new_monitor() {
    let limits = ResourceLimits::default();
    let monitor = StateMonitor::new(limits);

    assert_eq!(monitor.current_size(), 0);
    assert_eq!(monitor.value_count(), 0);
    assert_eq!(monitor.usage_percentage(), 0.0);
  }

  #[test]
  fn test_record_allocation() {
    let limits = ResourceLimits::default();
    let monitor = StateMonitor::new(limits);

    assert!(monitor.record_allocation("key1", 1024));
    assert_eq!(monitor.current_size(), 1024);
    assert_eq!(monitor.value_count(), 1);

    assert!(monitor.record_allocation("key2", 2048));
    assert_eq!(monitor.current_size(), 3072);
    assert_eq!(monitor.value_count(), 2);
  }

  #[test]
  fn test_record_deallocation() {
    let limits = ResourceLimits::default();
    let monitor = StateMonitor::new(limits);

    monitor.record_allocation("key1", 1024);
    monitor.record_allocation("key2", 2048);

    monitor.record_deallocation("key1");
    assert_eq!(monitor.current_size(), 2048);
    assert_eq!(monitor.value_count(), 1);
  }

  #[test]
  fn test_value_size_limit() {
    let limits = ResourceLimits::builder()
      .max_value_size(1024)
      .build();
    let monitor = StateMonitor::new(limits);

    assert!(!monitor.record_allocation("too_large", 2048));
    assert_eq!(monitor.current_size(), 0);

    let alerts = monitor.get_alerts();
    assert_eq!(alerts.len(), 1);
    matches!(alerts[0], ResourceAlert::LimitExceeded { .. });
  }

  #[test]
  fn test_state_size_limit_with_auto_cleanup() {
    let limits = ResourceLimits::builder()
      .max_state_size(5000)
      .auto_cleanup(true)
      .build();
    let monitor = StateMonitor::new(limits);

    // This exceeds the limit but should succeed with auto_cleanup enabled
    assert!(monitor.record_allocation("large", 6000));

    let alerts = monitor.get_alerts();
    assert!(!alerts.is_empty());
  }

  #[test]
  fn test_state_size_limit_without_auto_cleanup() {
    let limits = ResourceLimits::builder()
      .max_state_size(5000)
      .auto_cleanup(false)
      .build();
    let monitor = StateMonitor::new(limits);

    // This exceeds the limit and should fail without auto_cleanup
    assert!(!monitor.record_allocation("large", 6000));
    assert_eq!(monitor.current_size(), 0);
  }

  #[test]
  fn test_cleanup_threshold_alert() {
    let limits = ResourceLimits::builder()
      .max_state_size(10000)
      .cleanup_threshold(0.8)
      .auto_cleanup(true)
      .build();
    let monitor = StateMonitor::new(limits);

    // Below threshold - no alert
    monitor.record_allocation("small", 7000);
    assert!(monitor.get_alerts().is_empty());

    // At threshold - should trigger alert
    monitor.record_allocation("medium", 1500);
    let alerts = monitor.get_alerts();
    assert_eq!(alerts.len(), 1);
    matches!(alerts[0], ResourceAlert::ApproachingLimit { .. });
  }

  #[test]
  fn test_lru_tracking() {
    let limits = ResourceLimits::default();
    let monitor = StateMonitor::new(limits);

    monitor.record_allocation("key1", 100);
    monitor.record_allocation("key2", 200);
    monitor.record_allocation("key3", 300);

    // Access key1 again to make it more recently used
    monitor.record_access("key1");

    let lru_keys = monitor.get_lru_keys(2);
    assert_eq!(lru_keys.len(), 2);
    assert_eq!(lru_keys[0], "key2"); // Oldest
    assert_eq!(lru_keys[1], "key3");
  }

  #[test]
  fn test_cleanup() {
    let limits = ResourceLimits::builder()
      .max_state_size(10000)
      .cleanup_threshold(0.8)
      .build();
    let monitor = StateMonitor::new(limits);

    monitor.record_allocation("key1", 3000);
    monitor.record_allocation("key2", 3000);
    monitor.record_allocation("key3", 3000);

    assert_eq!(monitor.current_size(), 9000);

    // Cleanup to 50% (5000 bytes)
    let result = monitor.cleanup(0.5);
    assert!(result.is_ok());

    let (freed, removed) = result.unwrap();
    assert!(freed >= 4000);
    assert!(removed >= 1);

    assert!(monitor.current_size() <= 5000);
  }

  #[test]
  fn test_get_stats() {
    let limits = ResourceLimits::builder()
      .max_state_size(10000)
      .cleanup_threshold(0.8)
      .build();
    let monitor = StateMonitor::new(limits);

    monitor.record_allocation("key1", 5000);

    let stats = monitor.get_stats();
    assert_eq!(stats.current_size, 5000);
    assert_eq!(stats.max_state_size, 10000);
    assert_eq!(stats.usage_percentage, 0.5);
    assert_eq!(stats.value_count, 1);
  }

  #[test]
  fn test_reset() {
    let limits = ResourceLimits::default();
    let monitor = StateMonitor::new(limits);

    monitor.record_allocation("key1", 1000);
    monitor.record_allocation("key2", 2000);

    monitor.reset();

    assert_eq!(monitor.current_size(), 0);
    assert_eq!(monitor.value_count(), 0);
    assert!(monitor.get_allocations().is_empty());
  }
}

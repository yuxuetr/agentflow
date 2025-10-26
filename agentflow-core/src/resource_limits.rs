//! Resource limits and memory management for workflow execution.
//!
//! This module provides configurable resource limits to prevent unbounded memory growth
//! during workflow execution. It includes limits for state size, individual values,
//! cache entries, and cleanup thresholds.
//!
//! # Examples
//!
//! ```rust
//! use agentflow_core::resource_limits::ResourceLimits;
//!
//! // Use default limits (100MB state, 10MB per value)
//! let limits = ResourceLimits::default();
//!
//! // Create custom limits
//! let custom_limits = ResourceLimits::builder()
//!     .max_state_size(50 * 1024 * 1024)  // 50 MB
//!     .max_value_size(5 * 1024 * 1024)   // 5 MB
//!     .max_cache_entries(500)
//!     .cleanup_threshold(0.75)
//!     .build();
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;

/// Resource limits for workflow execution.
///
/// Defines configurable limits to prevent unbounded memory growth and resource exhaustion.
/// These limits are enforced during workflow execution to ensure stability and predictability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceLimits {
  /// Maximum total workflow state size in bytes.
  ///
  /// When the total size of all values in the execution context exceeds this limit,
  /// automatic cleanup will be triggered if `cleanup_threshold` is reached.
  ///
  /// Default: 100 MB (100 * 1024 * 1024 bytes)
  pub max_state_size: usize,

  /// Maximum size for individual values in bytes.
  ///
  /// Any single value (string, JSON object, etc.) exceeding this size will be rejected
  /// or truncated, depending on the operation.
  ///
  /// Default: 10 MB (10 * 1024 * 1024 bytes)
  pub max_value_size: usize,

  /// Maximum number of cached items in the execution context.
  ///
  /// This limits the number of key-value pairs that can be stored in the workflow state.
  /// Useful for preventing memory leaks in long-running workflows with dynamic keys.
  ///
  /// Default: 1000 entries
  pub max_cache_entries: usize,

  /// Memory cleanup threshold as a percentage (0.0 - 1.0).
  ///
  /// When memory usage reaches this percentage of `max_state_size`, automatic cleanup
  /// will be triggered to free least-recently-used values.
  ///
  /// Default: 0.8 (80%)
  pub cleanup_threshold: f64,

  /// Enable automatic cleanup when thresholds are reached.
  ///
  /// If `true`, the system will automatically remove least-recently-used values when
  /// memory limits are approached. If `false`, operations will fail when limits are exceeded.
  ///
  /// Default: true
  pub auto_cleanup: bool,

  /// Enable streaming mode for large values.
  ///
  /// When enabled, values exceeding `max_value_size` will be stored as file references
  /// instead of being loaded into memory. This allows processing of arbitrarily large data.
  ///
  /// Default: false
  pub enable_streaming: bool,

  /// Chunk size for streaming operations in bytes.
  ///
  /// When streaming is enabled, large values will be processed in chunks of this size.
  ///
  /// Default: 1 MB (1024 * 1024 bytes)
  pub stream_chunk_size: usize,
}

impl Default for ResourceLimits {
  fn default() -> Self {
    Self {
      max_state_size: 100 * 1024 * 1024,    // 100 MB
      max_value_size: 10 * 1024 * 1024,     // 10 MB
      max_cache_entries: 1000,
      cleanup_threshold: 0.8,                // 80%
      auto_cleanup: true,
      enable_streaming: false,
      stream_chunk_size: 1024 * 1024,       // 1 MB
    }
  }
}

impl ResourceLimits {
  /// Create a new builder for ResourceLimits.
  pub fn builder() -> ResourceLimitsBuilder {
    ResourceLimitsBuilder::default()
  }

  /// Check if the given size exceeds the maximum state size.
  pub fn exceeds_state_limit(&self, size: usize) -> bool {
    size > self.max_state_size
  }

  /// Check if the given size exceeds the maximum value size.
  pub fn exceeds_value_limit(&self, size: usize) -> bool {
    size > self.max_value_size
  }

  /// Check if the given count exceeds the maximum cache entries.
  pub fn exceeds_cache_limit(&self, count: usize) -> bool {
    count > self.max_cache_entries
  }

  /// Check if cleanup should be triggered based on current usage.
  ///
  /// Returns `true` if `current_size` >= `cleanup_threshold` * `max_state_size`.
  pub fn should_cleanup(&self, current_size: usize) -> bool {
    if !self.auto_cleanup {
      return false;
    }
    let threshold_bytes = (self.max_state_size as f64 * self.cleanup_threshold) as usize;
    current_size >= threshold_bytes
  }

  /// Get the cleanup threshold in bytes.
  pub fn cleanup_threshold_bytes(&self) -> usize {
    (self.max_state_size as f64 * self.cleanup_threshold) as usize
  }

  /// Validate that all configuration values are reasonable.
  pub fn validate(&self) -> Result<(), String> {
    if self.max_state_size == 0 {
      return Err("max_state_size must be greater than 0".to_string());
    }
    if self.max_value_size == 0 {
      return Err("max_value_size must be greater than 0".to_string());
    }
    if self.max_value_size > self.max_state_size {
      return Err("max_value_size cannot exceed max_state_size".to_string());
    }
    if self.max_cache_entries == 0 {
      return Err("max_cache_entries must be greater than 0".to_string());
    }
    if !(0.0..=1.0).contains(&self.cleanup_threshold) {
      return Err("cleanup_threshold must be between 0.0 and 1.0".to_string());
    }
    if self.stream_chunk_size == 0 {
      return Err("stream_chunk_size must be greater than 0".to_string());
    }
    Ok(())
  }
}

impl fmt::Display for ResourceLimits {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "ResourceLimits {{ state: {}, value: {}, cache: {}, cleanup: {}%, auto_cleanup: {}, streaming: {} }}",
      format_bytes(self.max_state_size),
      format_bytes(self.max_value_size),
      self.max_cache_entries,
      (self.cleanup_threshold * 100.0) as u32,
      self.auto_cleanup,
      self.enable_streaming
    )
  }
}

/// Builder for ResourceLimits with fluent API.
#[derive(Debug, Clone)]
pub struct ResourceLimitsBuilder {
  max_state_size: Option<usize>,
  max_value_size: Option<usize>,
  max_cache_entries: Option<usize>,
  cleanup_threshold: Option<f64>,
  auto_cleanup: Option<bool>,
  enable_streaming: Option<bool>,
  stream_chunk_size: Option<usize>,
}

impl Default for ResourceLimitsBuilder {
  fn default() -> Self {
    Self {
      max_state_size: None,
      max_value_size: None,
      max_cache_entries: None,
      cleanup_threshold: None,
      auto_cleanup: None,
      enable_streaming: None,
      stream_chunk_size: None,
    }
  }
}

impl ResourceLimitsBuilder {
  /// Set the maximum state size in bytes.
  pub fn max_state_size(mut self, size: usize) -> Self {
    self.max_state_size = Some(size);
    self
  }

  /// Set the maximum value size in bytes.
  pub fn max_value_size(mut self, size: usize) -> Self {
    self.max_value_size = Some(size);
    self
  }

  /// Set the maximum cache entries.
  pub fn max_cache_entries(mut self, count: usize) -> Self {
    self.max_cache_entries = Some(count);
    self
  }

  /// Set the cleanup threshold (0.0 - 1.0).
  pub fn cleanup_threshold(mut self, threshold: f64) -> Self {
    self.cleanup_threshold = Some(threshold);
    self
  }

  /// Enable or disable automatic cleanup.
  pub fn auto_cleanup(mut self, enabled: bool) -> Self {
    self.auto_cleanup = Some(enabled);
    self
  }

  /// Enable or disable streaming mode.
  pub fn enable_streaming(mut self, enabled: bool) -> Self {
    self.enable_streaming = Some(enabled);
    self
  }

  /// Set the streaming chunk size in bytes.
  pub fn stream_chunk_size(mut self, size: usize) -> Self {
    self.stream_chunk_size = Some(size);
    self
  }

  /// Build the ResourceLimits, using defaults for unset values.
  pub fn build(self) -> ResourceLimits {
    let defaults = ResourceLimits::default();
    ResourceLimits {
      max_state_size: self.max_state_size.unwrap_or(defaults.max_state_size),
      max_value_size: self.max_value_size.unwrap_or(defaults.max_value_size),
      max_cache_entries: self.max_cache_entries.unwrap_or(defaults.max_cache_entries),
      cleanup_threshold: self.cleanup_threshold.unwrap_or(defaults.cleanup_threshold),
      auto_cleanup: self.auto_cleanup.unwrap_or(defaults.auto_cleanup),
      enable_streaming: self.enable_streaming.unwrap_or(defaults.enable_streaming),
      stream_chunk_size: self.stream_chunk_size.unwrap_or(defaults.stream_chunk_size),
    }
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
  fn test_default_limits() {
    let limits = ResourceLimits::default();
    assert_eq!(limits.max_state_size, 100 * 1024 * 1024);
    assert_eq!(limits.max_value_size, 10 * 1024 * 1024);
    assert_eq!(limits.max_cache_entries, 1000);
    assert_eq!(limits.cleanup_threshold, 0.8);
    assert!(limits.auto_cleanup);
    assert!(!limits.enable_streaming);
  }

  #[test]
  fn test_builder() {
    let limits = ResourceLimits::builder()
      .max_state_size(50 * 1024 * 1024)
      .max_value_size(5 * 1024 * 1024)
      .max_cache_entries(500)
      .cleanup_threshold(0.75)
      .auto_cleanup(false)
      .enable_streaming(true)
      .stream_chunk_size(512 * 1024)
      .build();

    assert_eq!(limits.max_state_size, 50 * 1024 * 1024);
    assert_eq!(limits.max_value_size, 5 * 1024 * 1024);
    assert_eq!(limits.max_cache_entries, 500);
    assert_eq!(limits.cleanup_threshold, 0.75);
    assert!(!limits.auto_cleanup);
    assert!(limits.enable_streaming);
    assert_eq!(limits.stream_chunk_size, 512 * 1024);
  }

  #[test]
  fn test_exceeds_limits() {
    let limits = ResourceLimits::default();

    assert!(!limits.exceeds_state_limit(50 * 1024 * 1024));
    assert!(limits.exceeds_state_limit(150 * 1024 * 1024));

    assert!(!limits.exceeds_value_limit(5 * 1024 * 1024));
    assert!(limits.exceeds_value_limit(15 * 1024 * 1024));

    assert!(!limits.exceeds_cache_limit(500));
    assert!(limits.exceeds_cache_limit(1500));
  }

  #[test]
  fn test_should_cleanup() {
    let limits = ResourceLimits::builder()
      .max_state_size(100 * 1024 * 1024)
      .cleanup_threshold(0.8)
      .auto_cleanup(true)
      .build();

    // Below threshold
    assert!(!limits.should_cleanup(70 * 1024 * 1024));

    // At threshold
    assert!(limits.should_cleanup(80 * 1024 * 1024));

    // Above threshold
    assert!(limits.should_cleanup(90 * 1024 * 1024));
  }

  #[test]
  fn test_should_cleanup_disabled() {
    let limits = ResourceLimits::builder()
      .max_state_size(100 * 1024 * 1024)
      .cleanup_threshold(0.8)
      .auto_cleanup(false)
      .build();

    // Should not cleanup even when threshold is exceeded
    assert!(!limits.should_cleanup(90 * 1024 * 1024));
  }

  #[test]
  fn test_cleanup_threshold_bytes() {
    let limits = ResourceLimits::builder()
      .max_state_size(100 * 1024 * 1024)
      .cleanup_threshold(0.8)
      .build();

    assert_eq!(limits.cleanup_threshold_bytes(), 80 * 1024 * 1024);
  }

  #[test]
  fn test_validate_success() {
    let limits = ResourceLimits::default();
    assert!(limits.validate().is_ok());
  }

  #[test]
  fn test_validate_zero_state_size() {
    let limits = ResourceLimits::builder()
      .max_state_size(0)
      .build();
    assert!(limits.validate().is_err());
  }

  #[test]
  fn test_validate_zero_value_size() {
    let limits = ResourceLimits::builder()
      .max_value_size(0)
      .build();
    assert!(limits.validate().is_err());
  }

  #[test]
  fn test_validate_value_exceeds_state() {
    let limits = ResourceLimits::builder()
      .max_state_size(10 * 1024 * 1024)
      .max_value_size(20 * 1024 * 1024)
      .build();
    assert!(limits.validate().is_err());
  }

  #[test]
  fn test_validate_invalid_threshold() {
    let limits = ResourceLimits::builder()
      .cleanup_threshold(1.5)
      .build();
    assert!(limits.validate().is_err());

    let limits = ResourceLimits::builder()
      .cleanup_threshold(-0.1)
      .build();
    assert!(limits.validate().is_err());
  }

  #[test]
  fn test_format_bytes() {
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(1024), "1.00 KB");
    assert_eq!(format_bytes(1536), "1.50 KB");
    assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
    assert_eq!(format_bytes(100 * 1024 * 1024), "100.00 MB");
    assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
  }

  #[test]
  fn test_display() {
    let limits = ResourceLimits::default();
    let display = format!("{}", limits);
    assert!(display.contains("100.00 MB"));
    assert!(display.contains("10.00 MB"));
    assert!(display.contains("1000"));
    assert!(display.contains("80%"));
  }
}

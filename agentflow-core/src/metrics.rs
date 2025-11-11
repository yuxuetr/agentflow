//! Prometheus metrics collection for AgentFlow
//!
//! This module provides production-ready Prometheus metrics for monitoring:
//! - Workflow execution (started, completed, failed, duration)
//! - Node execution (executed, failed, duration by type)
//! - Resource usage (memory, CPU, active workflows/nodes)
//! - Error tracking (total errors, retries by type)
//!
//! # Examples
//!
//! ```rust,no_run
//! use agentflow_core::metrics::{init_metrics, record_workflow_started, METRICS_ENABLED};
//!
//! // Initialize metrics at application startup
//! if let Err(e) = init_metrics() {
//!     eprintln!("Failed to initialize metrics: {}", e);
//! }
//!
//! // Record metrics in your code
//! if *METRICS_ENABLED {
//!     record_workflow_started();
//! }
//! ```

#[cfg(feature = "metrics")]
use lazy_static::lazy_static;
#[cfg(feature = "metrics")]
use prometheus::{
  register_counter_vec, register_gauge_vec, register_histogram_vec, CounterVec, Encoder, GaugeVec,
  HistogramVec, TextEncoder,
};

use std::sync::atomic::{AtomicBool, Ordering};

/// Global flag indicating whether metrics are enabled
pub static METRICS_ENABLED: AtomicBool = AtomicBool::new(false);

#[cfg(feature = "metrics")]
lazy_static! {
  /// Workflow execution counters
  pub static ref WORKFLOW_STARTED: CounterVec = register_counter_vec!(
    "agentflow_workflow_started_total",
    "Total number of workflows started",
    &[]
  )
  .expect("Failed to register workflow_started metric");

  pub static ref WORKFLOW_COMPLETED: CounterVec = register_counter_vec!(
    "agentflow_workflow_completed_total",
    "Total number of workflows completed successfully",
    &[]
  )
  .expect("Failed to register workflow_completed metric");

  pub static ref WORKFLOW_FAILED: CounterVec = register_counter_vec!(
    "agentflow_workflow_failed_total",
    "Total number of workflows that failed",
    &["error_type"]
  )
  .expect("Failed to register workflow_failed metric");

  pub static ref WORKFLOW_DURATION: HistogramVec = register_histogram_vec!(
    "agentflow_workflow_duration_seconds",
    "Workflow execution duration in seconds",
    &[],
    vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0]
  )
  .expect("Failed to register workflow_duration metric");

  /// Node execution counters
  pub static ref NODE_EXECUTED: CounterVec = register_counter_vec!(
    "agentflow_node_executed_total",
    "Total number of nodes executed",
    &["node_type"]
  )
  .expect("Failed to register node_executed metric");

  pub static ref NODE_FAILED: CounterVec = register_counter_vec!(
    "agentflow_node_failed_total",
    "Total number of node executions that failed",
    &["node_type", "error_type"]
  )
  .expect("Failed to register node_failed metric");

  pub static ref NODE_DURATION: HistogramVec = register_histogram_vec!(
    "agentflow_node_duration_seconds",
    "Node execution duration in seconds",
    &["node_type"],
    vec![0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0]
  )
  .expect("Failed to register node_duration metric");

  /// Resource usage gauges
  pub static ref MEMORY_USED: GaugeVec = register_gauge_vec!(
    "agentflow_memory_used_bytes",
    "Memory used by AgentFlow in bytes",
    &[]
  )
  .expect("Failed to register memory_used metric");

  pub static ref CPU_USAGE: GaugeVec = register_gauge_vec!(
    "agentflow_cpu_usage_percent",
    "CPU usage percentage",
    &[]
  )
  .expect("Failed to register cpu_usage metric");

  pub static ref ACTIVE_WORKFLOWS: GaugeVec = register_gauge_vec!(
    "agentflow_active_workflows",
    "Number of currently active workflows",
    &[]
  )
  .expect("Failed to register active_workflows metric");

  pub static ref ACTIVE_NODES: GaugeVec = register_gauge_vec!(
    "agentflow_active_nodes",
    "Number of currently executing nodes",
    &[]
  )
  .expect("Failed to register active_nodes metric");

  /// Error tracking counters
  pub static ref ERROR_TOTAL: CounterVec = register_counter_vec!(
    "agentflow_error_total",
    "Total number of errors",
    &["error_type"]
  )
  .expect("Failed to register error_total metric");

  pub static ref RETRY_TOTAL: CounterVec = register_counter_vec!(
    "agentflow_retry_total",
    "Total number of retries",
    &["node_type"]
  )
  .expect("Failed to register retry_total metric");
}

/// Initialize the metrics system
///
/// This should be called once at application startup.
/// Sets the METRICS_ENABLED flag to true.
#[cfg(feature = "metrics")]
pub fn init_metrics() -> Result<(), Box<dyn std::error::Error>> {
  // Initialize all lazy_static metrics
  lazy_static::initialize(&WORKFLOW_STARTED);
  lazy_static::initialize(&WORKFLOW_COMPLETED);
  lazy_static::initialize(&WORKFLOW_FAILED);
  lazy_static::initialize(&WORKFLOW_DURATION);
  lazy_static::initialize(&NODE_EXECUTED);
  lazy_static::initialize(&NODE_FAILED);
  lazy_static::initialize(&NODE_DURATION);
  lazy_static::initialize(&MEMORY_USED);
  lazy_static::initialize(&CPU_USAGE);
  lazy_static::initialize(&ACTIVE_WORKFLOWS);
  lazy_static::initialize(&ACTIVE_NODES);
  lazy_static::initialize(&ERROR_TOTAL);
  lazy_static::initialize(&RETRY_TOTAL);

  METRICS_ENABLED.store(true, Ordering::Relaxed);
  Ok(())
}

#[cfg(not(feature = "metrics"))]
pub fn init_metrics() -> Result<(), Box<dyn std::error::Error>> {
  eprintln!("Warning: Metrics are disabled. Enable the 'metrics' feature to use Prometheus metrics.");
  Ok(())
}

/// Export metrics in Prometheus text format
///
/// Returns a string containing all metrics in the Prometheus exposition format.
#[cfg(feature = "metrics")]
pub fn export_metrics() -> Result<String, Box<dyn std::error::Error>> {
  let encoder = TextEncoder::new();
  let metric_families = prometheus::gather();
  let mut buffer = vec![];
  encoder.encode(&metric_families, &mut buffer)?;
  Ok(String::from_utf8(buffer)?)
}

#[cfg(not(feature = "metrics"))]
pub fn export_metrics() -> Result<String, Box<dyn std::error::Error>> {
  Ok("# Metrics disabled\n".to_string())
}

// Convenience functions for recording metrics

/// Record workflow started
#[cfg(feature = "metrics")]
pub fn record_workflow_started() {
  WORKFLOW_STARTED.with_label_values(&[]).inc();
  ACTIVE_WORKFLOWS.with_label_values(&[]).inc();
}

#[cfg(not(feature = "metrics"))]
pub fn record_workflow_started() {}

/// Record workflow completed
#[cfg(feature = "metrics")]
pub fn record_workflow_completed(duration_secs: f64) {
  WORKFLOW_COMPLETED.with_label_values(&[]).inc();
  WORKFLOW_DURATION.with_label_values(&[]).observe(duration_secs);
  ACTIVE_WORKFLOWS.with_label_values(&[]).dec();
}

#[cfg(not(feature = "metrics"))]
pub fn record_workflow_completed(_duration_secs: f64) {}

/// Record workflow failed
#[cfg(feature = "metrics")]
pub fn record_workflow_failed(error_type: &str) {
  WORKFLOW_FAILED.with_label_values(&[error_type]).inc();
  ACTIVE_WORKFLOWS.with_label_values(&[]).dec();
  ERROR_TOTAL.with_label_values(&[error_type]).inc();
}

#[cfg(not(feature = "metrics"))]
pub fn record_workflow_failed(_error_type: &str) {}

/// Record node executed
#[cfg(feature = "metrics")]
pub fn record_node_executed(node_type: &str, duration_secs: f64) {
  NODE_EXECUTED.with_label_values(&[node_type]).inc();
  NODE_DURATION.with_label_values(&[node_type]).observe(duration_secs);
}

#[cfg(not(feature = "metrics"))]
pub fn record_node_executed(_node_type: &str, _duration_secs: f64) {}

/// Record node failed
#[cfg(feature = "metrics")]
pub fn record_node_failed(node_type: &str, error_type: &str) {
  NODE_FAILED
    .with_label_values(&[node_type, error_type])
    .inc();
  ERROR_TOTAL.with_label_values(&[error_type]).inc();
}

#[cfg(not(feature = "metrics"))]
pub fn record_node_failed(_node_type: &str, _error_type: &str) {}

/// Record retry attempt
#[cfg(feature = "metrics")]
pub fn record_retry(node_type: &str) {
  RETRY_TOTAL.with_label_values(&[node_type]).inc();
}

#[cfg(not(feature = "metrics"))]
pub fn record_retry(_node_type: &str) {}

/// Update memory usage
#[cfg(feature = "metrics")]
pub fn update_memory_usage(bytes: f64) {
  MEMORY_USED.with_label_values(&[]).set(bytes);
}

#[cfg(not(feature = "metrics"))]
pub fn update_memory_usage(_bytes: f64) {}

/// Update CPU usage
#[cfg(feature = "metrics")]
pub fn update_cpu_usage(percent: f64) {
  CPU_USAGE.with_label_values(&[]).set(percent);
}

#[cfg(not(feature = "metrics"))]
pub fn update_cpu_usage(_percent: f64) {}

/// Update active nodes count
#[cfg(feature = "metrics")]
pub fn update_active_nodes(count: i64) {
  if count >= 0 {
    ACTIVE_NODES.with_label_values(&[]).inc();
  } else {
    ACTIVE_NODES.with_label_values(&[]).dec();
  }
}

#[cfg(not(feature = "metrics"))]
pub fn update_active_nodes(_count: i64) {}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_init_metrics() {
    let result = init_metrics();
    assert!(result.is_ok());
  }

  #[test]
  #[cfg(feature = "metrics")]
  fn test_export_metrics() {
    init_metrics().expect("Failed to init metrics");
    let result = export_metrics();
    assert!(result.is_ok());
    let metrics = result.unwrap();
    assert!(!metrics.is_empty());
  }

  #[test]
  #[cfg(not(feature = "metrics"))]
  fn test_export_metrics_disabled() {
    let result = export_metrics();
    assert!(result.is_ok());
  }

  #[test]
  #[cfg(feature = "metrics")]
  fn test_record_workflow_metrics() {
    record_workflow_started();
    record_workflow_completed(1.5);
    record_workflow_failed("timeout");

    let metrics = export_metrics().unwrap();
    assert!(metrics.contains("agentflow_workflow_started_total"));
    assert!(metrics.contains("agentflow_workflow_completed_total"));
    assert!(metrics.contains("agentflow_workflow_failed_total"));
  }

  #[test]
  #[cfg(feature = "metrics")]
  fn test_record_node_metrics() {
    record_node_executed("llm", 0.5);
    record_node_failed("http", "network_error");
    record_retry("llm");

    let metrics = export_metrics().unwrap();
    assert!(metrics.contains("agentflow_node_executed_total"));
    assert!(metrics.contains("agentflow_node_failed_total"));
    assert!(metrics.contains("agentflow_retry_total"));
  }

  #[test]
  #[cfg(feature = "metrics")]
  fn test_resource_metrics() {
    update_memory_usage(100_000_000.0); // 100 MB
    update_cpu_usage(45.5);
    update_active_nodes(1);
    update_active_nodes(-1);

    let metrics = export_metrics().unwrap();
    assert!(metrics.contains("agentflow_memory_used_bytes"));
    assert!(metrics.contains("agentflow_cpu_usage_percent"));
    assert!(metrics.contains("agentflow_active_nodes"));
  }
}

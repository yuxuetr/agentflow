// Observability and monitoring - tests first, implementation follows

// use async_trait::async_trait;
// use serde_json::Value;
#[warn(unused_imports)]
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[cfg(feature = "observability")]
use tracing::{error, info, span, warn, Level};

// Core observability types
#[derive(Debug, Clone)]
pub struct ExecutionEvent {
  pub node_id: String,
  pub event_type: String,
  pub timestamp: Instant,
  pub duration_ms: Option<u64>,
  pub metadata: HashMap<String, String>,
}

#[derive(Debug)]
pub struct MetricsCollector {
  metrics: Arc<Mutex<HashMap<String, f64>>>,
  events: Arc<Mutex<Vec<ExecutionEvent>>>,
}

impl MetricsCollector {
  pub fn new() -> Self {
    Self {
      metrics: Arc::new(Mutex::new(HashMap::new())),
      events: Arc::new(Mutex::new(Vec::new())),
    }
  }

  pub fn increment_counter(&self, name: &str, value: f64) {
    let mut metrics = self.metrics.lock().unwrap();
    *metrics.entry(name.to_string()).or_insert(0.0) += value;
  }

  pub fn record_event(&self, event: ExecutionEvent) {
    self.events.lock().unwrap().push(event);
  }

  pub fn get_metric(&self, name: &str) -> Option<f64> {
    self.metrics.lock().unwrap().get(name).copied()
  }

  pub fn get_events(&self) -> Vec<ExecutionEvent> {
    self.events.lock().unwrap().clone()
  }
}

#[derive(Debug)]
pub struct AlertRule {
  pub name: String,
  pub condition: String,
  pub threshold: f64,
  pub action: String,
}

#[derive(Debug)]
pub struct AlertManager {
  rules: Vec<AlertRule>,
  triggered_alerts: Arc<Mutex<Vec<String>>>,
}

impl AlertManager {
  pub fn new() -> Self {
    Self {
      rules: Vec::new(),
      triggered_alerts: Arc::new(Mutex::new(Vec::new())),
    }
  }

  pub fn add_alert_rule(&mut self, rule: AlertRule) {
    self.rules.push(rule);
  }

  pub fn check_alerts(&self, metrics: &MetricsCollector) {
    for rule in &self.rules {
      if let Some(value) = metrics.get_metric(&rule.condition) {
        if value > rule.threshold {
          self
            .triggered_alerts
            .lock()
            .unwrap()
            .push(rule.name.clone());
        }
      }
    }
  }

  pub fn get_triggered_alerts(&self) -> Vec<String> {
    self.triggered_alerts.lock().unwrap().clone()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{AgentFlowError, AsyncFlow, AsyncNode, Result, SharedState};
  use async_trait::async_trait;
  use serde_json::Value;
  use std::sync::{Arc, Mutex};
  use std::time::{Duration, Instant};
  #[cfg(feature = "observability")]
  use tracing::{error, info, span, warn, Level};
  #[cfg(feature = "observability")]
  use tracing_test::traced_test;

  // Mock observability components
  struct MonitoredNode {
    id: String,
    delay_ms: u64,
    should_fail: bool,
    metrics_collector: Arc<MetricsCollector>,
  }

  #[async_trait]
  impl AsyncNode for MonitoredNode {
    async fn prep_async(&self, _shared: &SharedState) -> Result<Value> {
      Ok(Value::String(format!("prep_{}", self.id)))
    }

    async fn exec_async(&self, _prep_result: Value) -> Result<Value> {
      if self.delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
      }

      if self.should_fail {
        return Err(AgentFlowError::AsyncExecutionError {
          message: format!("Node {} failed", self.id),
        });
      }

      Ok(Value::String(format!("success_{}", self.id)))
    }

    async fn post_async(
      &self,
      shared: &SharedState,
      _prep_result: Value,
      exec_result: Value,
    ) -> Result<Option<String>> {
      shared.insert("result".to_string(), exec_result);
      Ok(None)
    }

    fn get_node_id(&self) -> Option<String> {
      Some(self.id.clone())
    }
  }

  struct TracedNode {
    id: String,
    delay_ms: u64,
    trace_id: String,
  }

  #[async_trait]
  impl AsyncNode for TracedNode {
    async fn prep_async(&self, _shared: &SharedState) -> Result<Value> {
      #[cfg(feature = "observability")]
      info!("entering {}", self.id);
      Ok(Value::String(format!("prep_{}", self.id)))
    }

    async fn exec_async(&self, _prep_result: Value) -> Result<Value> {
      if self.delay_ms > 0 {
        tokio::time::sleep(Duration::from_millis(self.delay_ms)).await;
      }
      Ok(Value::String(format!("success_{}", self.trace_id)))
    }

    async fn post_async(
      &self,
      _shared: &SharedState,
      _prep_result: Value,
      _exec_result: Value,
    ) -> Result<Option<String>> {
      #[cfg(feature = "observability")]
      info!("exiting {}", self.id);
      Ok(None)
    }

    fn get_node_id(&self) -> Option<String> {
      Some(self.id.clone())
    }
  }

  #[tokio::test]
  #[cfg(feature = "observability")]
  async fn test_distributed_tracing() {
    // Test distributed tracing across async flow
    let node1 = TracedNode {
      id: "node1".to_string(),
      delay_ms: 10,
      trace_id: "trace-123".to_string(),
    };

    let mut flow = AsyncFlow::new(Box::new(node1));
    flow.enable_tracing("test-flow".to_string());

    let shared = SharedState::new();
    let result = flow.run_async(&shared).await;

    assert!(result.is_ok());

    // Verify tracing spans were created - basic success test
    // Note: specific log content verification depends on tracing setup
  }

  #[tokio::test]
  async fn test_metrics_collection() {
    // Test metrics collection for performance monitoring
    let metrics_collector = Arc::new(MetricsCollector::new());

    let node = MonitoredNode {
      id: "monitored".to_string(),
      delay_ms: 10,
      should_fail: false,
      metrics_collector: metrics_collector.clone(),
    };

    let shared = SharedState::new();

    // Run node multiple times with observability
    for _ in 0..3 {
      let _ = node
        .run_async_with_observability(&shared, Some(metrics_collector.clone()))
        .await;
    }

    // Should have collected execution metrics
    let execution_count = metrics_collector
      .get_metric("monitored.execution_count")
      .unwrap_or(0.0);
    let success_count = metrics_collector
      .get_metric("monitored.success_count")
      .unwrap_or(0.0);

    assert_eq!(execution_count, 3.0);
    assert_eq!(success_count, 3.0);

    // Should have recorded events
    let events = metrics_collector.get_events();
    assert!(events.len() >= 6); // At least start and end events for 3 executions
  }

  #[tokio::test]
  async fn test_real_time_monitoring() {
    // Test real-time monitoring with flow-level metrics
    let metrics_collector = Arc::new(MetricsCollector::new());

    let node = MonitoredNode {
      id: "realtime".to_string(),
      delay_ms: 20,
      should_fail: false,
      metrics_collector: metrics_collector.clone(),
    };

    let mut flow = AsyncFlow::new(Box::new(node));
    flow.set_metrics_collector(metrics_collector.clone());
    flow.set_flow_name("realtime_flow".to_string());

    let shared = SharedState::new();
    let result = flow.run_async(&shared).await;

    assert!(result.is_ok());

    // Verify flow-level metrics were collected
    let flow_execution_count = metrics_collector
      .get_metric("realtime_flow.execution_count")
      .unwrap_or(0.0);
    let flow_success_count = metrics_collector
      .get_metric("realtime_flow.success_count")
      .unwrap_or(0.0);

    assert_eq!(flow_execution_count, 1.0);
    assert_eq!(flow_success_count, 1.0);
  }

  #[tokio::test]
  async fn test_flow_visualization() {
    // Test basic flow visualization through metrics collection
    let metrics_collector = Arc::new(MetricsCollector::new());

    let node = MonitoredNode {
      id: "viz_node".to_string(),
      delay_ms: 10,
      should_fail: false,
      metrics_collector: metrics_collector.clone(),
    };

    let mut flow = AsyncFlow::new(Box::new(node));
    flow.set_metrics_collector(metrics_collector.clone());
    flow.set_flow_name("viz_flow".to_string());

    let shared = SharedState::new();
    let result = flow.run_async(&shared).await;

    assert!(result.is_ok());

    // Verify execution events were captured for visualization
    let events = metrics_collector.get_events();
    assert!(events.len() >= 4); // Flow start/end + node start/end events

    // Check event types for visualization
    let event_types: Vec<String> = events.iter().map(|e| e.event_type.clone()).collect();
    assert!(event_types.contains(&"flow_start".to_string()));
    assert!(event_types.contains(&"flow_success".to_string()));
    assert!(event_types.contains(&"node_start".to_string()));
    assert!(event_types.contains(&"node_success".to_string()));
  }

  #[tokio::test]
  async fn test_performance_profiling() {
    // Test performance profiling through duration metrics
    let metrics_collector = Arc::new(MetricsCollector::new());

    let fast_node = MonitoredNode {
      id: "fast".to_string(),
      delay_ms: 5,
      should_fail: false,
      metrics_collector: metrics_collector.clone(),
    };

    let slow_node = MonitoredNode {
      id: "slow".to_string(),
      delay_ms: 50,
      should_fail: false,
      metrics_collector: metrics_collector.clone(),
    };

    let shared = SharedState::new();

    // Run both nodes and compare performance
    let _ = fast_node
      .run_async_with_observability(&shared, Some(metrics_collector.clone()))
      .await;
    let _ = slow_node
      .run_async_with_observability(&shared, Some(metrics_collector.clone()))
      .await;

    // Check duration metrics for performance analysis
    let fast_duration = metrics_collector
      .get_metric("fast.duration_ms")
      .unwrap_or(0.0);
    let slow_duration = metrics_collector
      .get_metric("slow.duration_ms")
      .unwrap_or(0.0);

    // Slow node should take longer than fast node
    assert!(slow_duration > fast_duration);
    assert!(fast_duration < 20.0); // Should be relatively fast
    assert!(slow_duration >= 50.0); // Should be slower
  }

  #[tokio::test]
  async fn test_alert_system() {
    // Test basic alerting system
    let mut alert_manager = AlertManager::new();
    let metrics_collector = Arc::new(MetricsCollector::new());

    // Configure alerts
    alert_manager.add_alert_rule(AlertRule {
      name: "high_error_count".to_string(),
      condition: "error_count".to_string(),
      threshold: 2.0,
      action: "notify".to_string(),
    });

    // Simulate high error rate
    metrics_collector.increment_counter("error_count", 5.0);

    // Check alerts
    alert_manager.check_alerts(&metrics_collector);
    let triggered_alerts = alert_manager.get_triggered_alerts();

    assert!(!triggered_alerts.is_empty());
    assert!(triggered_alerts.contains(&"high_error_count".to_string()));
  }

  #[tokio::test]
  async fn test_log_aggregation() {
    // Test log aggregation through event collection
    let metrics_collector = Arc::new(MetricsCollector::new());

    let node = TracedNode {
      id: "logged".to_string(),
      delay_ms: 10,
      trace_id: "log-trace-456".to_string(),
    };

    let shared = SharedState::new();
    let result = node
      .run_async_with_observability(&shared, Some(metrics_collector.clone()))
      .await;

    assert!(result.is_ok());

    // Verify execution events were logged
    let events = metrics_collector.get_events();
    assert!(!events.is_empty());

    // Check that events have proper structure for log aggregation
    for event in events {
      assert!(!event.node_id.is_empty());
      assert!(!event.event_type.is_empty());
      assert!(event.duration_ms.is_some() || event.event_type.contains("start"));
    }
  }

  #[tokio::test]
  async fn test_parallel_flow_observability() {
    // Test observability in parallel execution
    let metrics_collector = Arc::new(MetricsCollector::new());

    let node1 = MonitoredNode {
      id: "parallel1".to_string(),
      delay_ms: 20,
      should_fail: false,
      metrics_collector: metrics_collector.clone(),
    };

    let node2 = MonitoredNode {
      id: "parallel2".to_string(),
      delay_ms: 30,
      should_fail: false,
      metrics_collector: metrics_collector.clone(),
    };

    let nodes: Vec<Box<dyn AsyncNode>> = vec![Box::new(node1), Box::new(node2)];
    let mut flow = AsyncFlow::new_parallel(nodes);
    flow.set_metrics_collector(metrics_collector.clone());
    flow.set_flow_name("parallel_flow".to_string());

    let shared = SharedState::new();
    let result = flow.run_async(&shared).await;

    assert!(result.is_ok());

    // Verify parallel execution was observed
    let flow_execution_count = metrics_collector
      .get_metric("parallel_flow.execution_count")
      .unwrap_or(0.0);
    let node1_execution_count = metrics_collector
      .get_metric("parallel1.execution_count")
      .unwrap_or(0.0);
    let node2_execution_count = metrics_collector
      .get_metric("parallel2.execution_count")
      .unwrap_or(0.0);

    assert_eq!(flow_execution_count, 1.0);
    assert_eq!(node1_execution_count, 1.0);
    assert_eq!(node2_execution_count, 1.0);
  }
}

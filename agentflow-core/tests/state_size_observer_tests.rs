//! End-to-end tests for [`StateSizeObserver`] wiring (P10.14.2-FU6).
//!
//! Confirms the observer fires after every node completes in both the
//! serial and concurrent execution paths, and that the sample value
//! monotonically grows as more node outputs land in the state pool.

use agentflow_core::FlowExt;
use agentflow_core::{
  async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
  events::{EventListener, WorkflowEvent},
  flow::{Flow, GraphNode, NodeType},
  resource_limits::ResourceLimits,
  scheduler::FlowExecutionConfig,
  scheduler::FlowExecutionMode,
  state_size::StateSizeObserver,
  value::FlowValue,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct RecordingObserver {
  samples: Mutex<Vec<u64>>,
}

impl RecordingObserver {
  fn snapshot(&self) -> Vec<u64> {
    self.samples.lock().unwrap().clone()
  }
}

impl StateSizeObserver for RecordingObserver {
  fn observe(&self, bytes: u64) {
    self.samples.lock().unwrap().push(bytes);
  }
}

#[derive(Clone)]
struct PayloadNode {
  output: String,
}

#[async_trait]
impl AsyncNode for PayloadNode {
  async fn execute(&self, _inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let mut outputs = HashMap::new();
    outputs.insert(
      "result".to_string(),
      FlowValue::Json(serde_json::json!(self.output.clone())),
    );
    Ok(outputs)
  }
}

fn use_writable_home() {
  let home = std::env::temp_dir().join(format!(
    "agentflow-state-size-test-{}",
    uuid::Uuid::new_v4()
  ));
  std::fs::create_dir_all(&home).unwrap();
  // SAFETY: per-test HOME set before any AgentFlow state is constructed;
  // tests run on independent tokio runtimes.
  unsafe {
    std::env::set_var("HOME", home);
  }
}

#[tokio::test]
async fn state_size_observer_fires_per_node_in_serial_mode() {
  use_writable_home();
  let nodes = vec![
    GraphNode {
      id: "a".to_string(),
      node_type: NodeType::Standard(Arc::new(PayloadNode {
        output: "first".to_string(),
      })),
      dependencies: vec![],
      input_mapping: None,
      run_if: None,
      initial_inputs: HashMap::new(),
    },
    GraphNode {
      id: "b".to_string(),
      node_type: NodeType::Standard(Arc::new(PayloadNode {
        output: "second_node_payload".to_string(),
      })),
      dependencies: vec!["a".to_string()],
      input_mapping: None,
      run_if: None,
      initial_inputs: HashMap::new(),
    },
  ];

  let observer = Arc::new(RecordingObserver::default());
  let flow = Flow::new(nodes).with_state_size_observer(observer.clone());
  let result = flow.run().await;
  assert!(result.is_ok(), "flow run should succeed");

  let samples = observer.snapshot();
  assert_eq!(
    samples.len(),
    2,
    "expected one sample per node in the serial path, got {samples:?}"
  );
  assert!(samples[0] > 0, "first sample should be non-zero");
  assert!(
    samples[1] > samples[0],
    "state pool size should grow monotonically as more nodes complete; got {samples:?}"
  );
}

#[tokio::test]
async fn state_size_observer_fires_per_node_in_concurrent_mode() {
  use_writable_home();
  // Two independent (no-dep) nodes so the concurrent scheduler dispatches
  // both at once and the observer sees one sample per completion.
  let nodes = vec![
    GraphNode {
      id: "a".to_string(),
      node_type: NodeType::Standard(Arc::new(PayloadNode {
        output: "alpha".to_string(),
      })),
      dependencies: vec![],
      input_mapping: None,
      run_if: None,
      initial_inputs: HashMap::new(),
    },
    GraphNode {
      id: "b".to_string(),
      node_type: NodeType::Standard(Arc::new(PayloadNode {
        output: "bravo_payload".to_string(),
      })),
      dependencies: vec![],
      input_mapping: None,
      run_if: None,
      initial_inputs: HashMap::new(),
    },
  ];

  let observer = Arc::new(RecordingObserver::default());
  let flow = Flow::new(nodes).with_state_size_observer(observer.clone());
  let result = flow
    .execute_from_inputs_with_config(
      HashMap::new(),
      FlowExecutionConfig {
        mode: FlowExecutionMode::Concurrent,
        ..Default::default()
      },
    )
    .await;
  assert!(result.is_ok(), "concurrent flow run should succeed");

  let samples = observer.snapshot();
  assert_eq!(
    samples.len(),
    2,
    "expected one sample per node in the concurrent path, got {samples:?}"
  );
  // Order is non-deterministic in concurrent mode but the second sample
  // must always be larger than (or equal to, in the edge case of equal
  // serialized lengths) the first because state pool only grows.
  assert!(samples[1] >= samples[0]);
}

#[tokio::test]
async fn flow_without_observer_runs_unchanged() {
  use_writable_home();
  // Smoke test: opting out of the observer is the default; nothing
  // about Flow execution should depend on whether one is attached.
  let nodes = vec![GraphNode {
    id: "a".to_string(),
    node_type: NodeType::Standard(Arc::new(PayloadNode {
      output: "ok".to_string(),
    })),
    dependencies: vec![],
    input_mapping: None,
    run_if: None,
    initial_inputs: HashMap::new(),
  }];
  let flow = Flow::new(nodes);
  let state = flow
    .run()
    .await
    .expect("default flow runs without observer");
  assert!(state.contains_key("a"));
}

#[derive(Default)]
struct ResourceWarningListener {
  warnings: Mutex<Vec<(String, f64, f64)>>,
}

impl ResourceWarningListener {
  fn snapshot(&self) -> Vec<(String, f64, f64)> {
    self.warnings.lock().unwrap().clone()
  }
}

impl EventListener for ResourceWarningListener {
  fn on_event(&self, event: &WorkflowEvent) {
    if let WorkflowEvent::ResourceWarning {
      resource_type,
      usage,
      limit,
      ..
    } = event
    {
      self
        .warnings
        .lock()
        .unwrap()
        .push((resource_type.clone(), *usage, *limit));
    }
  }
}

/// W5.3: `FlowExecutionConfig::resource_limits` is advisory-only — it must
/// emit `WorkflowEvent::ResourceWarning` once the estimated state pool
/// size crosses the configured `max_state_size`, without evicting or
/// rejecting anything (the run still succeeds).
#[tokio::test]
async fn resource_limits_emits_warning_event_when_state_pool_exceeds_configured_max() {
  use_writable_home();
  let nodes = vec![GraphNode {
    id: "a".to_string(),
    node_type: NodeType::Standard(Arc::new(PayloadNode {
      output: "a payload long enough to blow past a tiny byte limit".to_string(),
    })),
    dependencies: vec![],
    input_mapping: None,
    run_if: None,
    initial_inputs: HashMap::new(),
  }];

  let listener = Arc::new(ResourceWarningListener::default());
  let flow = Flow::new(nodes).with_event_listener(listener.clone());
  let result = flow
    .execute_from_inputs_with_config(
      HashMap::new(),
      FlowExecutionConfig {
        resource_limits: Some(ResourceLimits::builder().max_state_size(1).build()),
        ..Default::default()
      },
    )
    .await;

  assert!(
    result.is_ok(),
    "advisory resource_limits must never fail the run: {result:?}"
  );

  let warnings = listener.snapshot();
  assert_eq!(
    warnings.len(),
    1,
    "expected exactly one ResourceWarning for the single node, got: {warnings:?}"
  );
  let (resource_type, usage, limit) = &warnings[0];
  assert_eq!(resource_type, "state_pool_bytes");
  assert!(*usage > 1.0, "usage ratio should exceed 1.0 (over limit)");
  assert_eq!(*limit, 1.0);
}

/// W5.3 regression: `resource_limits: None` (the default) must never emit
/// `ResourceWarning`, even for a state pool that would exceed any
/// realistic limit — this is the pre-W5.3 behavior and must stay
/// unchanged for every existing caller that doesn't opt in.
#[tokio::test]
async fn resource_limits_none_never_emits_warning_event() {
  use_writable_home();
  let nodes = vec![GraphNode {
    id: "a".to_string(),
    node_type: NodeType::Standard(Arc::new(PayloadNode {
      output: "some payload".to_string(),
    })),
    dependencies: vec![],
    input_mapping: None,
    run_if: None,
    initial_inputs: HashMap::new(),
  }];

  let listener = Arc::new(ResourceWarningListener::default());
  let flow = Flow::new(nodes).with_event_listener(listener.clone());
  let result = flow.run().await;
  assert!(result.is_ok());

  assert!(
    listener.snapshot().is_empty(),
    "default FlowExecutionConfig (resource_limits: None) must never emit ResourceWarning"
  );
}

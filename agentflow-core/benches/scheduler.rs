//! Criterion micro-benchmarks for the DAG scheduler.
//!
//! Covers three shapes (10/100/1000 nodes) across serial vs concurrent
//! execution. Each node is a no-op pass-through so the wall-clock time
//! is dominated by topological-sort / dispatch overhead, not user code.
//!
//! Run:
//!
//! ```sh
//! cargo bench -p agentflow-core --bench scheduler
//! ```
//!
//! Baselines are checked in at `benches/baselines/<host>.json`. Treat
//! them as signals, not gates — host differences (CPU, OS scheduling,
//! noise) are expected to shift absolute numbers.

use std::{
  collections::HashMap,
  sync::Arc,
  time::Duration,
};

use agentflow_core::{
  async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
  flow::{Flow, GraphNode, NodeType},
  scheduler::{FlowExecutionConfig, FlowExecutionMode},
  value::FlowValue,
};
use async_trait::async_trait;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use serde_json::json;
use tokio::runtime::Runtime;

/// No-op node that returns a single output keyed on its id.
///
/// The fixed payload keeps the benchmark deterministic: every bench
/// shape produces the same serialized state pool size per node.
struct PassthroughNode {
  id: String,
}

#[async_trait]
impl AsyncNode for PassthroughNode {
  async fn execute(&self, _inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let mut out = HashMap::new();
    out.insert(
      "value".to_string(),
      FlowValue::Json(json!({ "id": self.id })),
    );
    Ok(out)
  }
}

fn make_node(id: &str, deps: Vec<String>) -> GraphNode {
  GraphNode {
    id: id.to_string(),
    node_type: NodeType::Standard(Arc::new(PassthroughNode { id: id.to_string() })),
    dependencies: deps,
    input_mapping: None,
    run_if: None,
    initial_inputs: HashMap::new(),
  }
}

/// Linear chain: node_0 -> node_1 -> ... -> node_(n-1). Concurrency
/// is bounded by the dependency graph here — useful for measuring
/// pure dispatch overhead without any real parallelism.
fn linear_flow(n: usize) -> Flow {
  let nodes = (0..n)
    .map(|i| {
      let id = format!("node_{i}");
      let deps = if i == 0 {
        Vec::new()
      } else {
        vec![format!("node_{}", i - 1)]
      };
      make_node(&id, deps)
    })
    .collect();
  Flow::new(nodes)
}

/// Fan-out / fan-in shape: a single root, `n - 2` parallel mid-tier
/// nodes, and a sink that depends on every mid-tier node. Exercises
/// the concurrent scheduler's dependency-ready dispatch path.
fn fanout_flow(n: usize) -> Flow {
  assert!(n >= 3, "fan-out shape needs at least 3 nodes");
  let mid_count = n - 2;
  let mut nodes = Vec::with_capacity(n);
  nodes.push(make_node("root", Vec::new()));
  let mid_ids: Vec<String> = (0..mid_count).map(|i| format!("mid_{i}")).collect();
  for id in &mid_ids {
    nodes.push(make_node(id, vec!["root".to_string()]));
  }
  nodes.push(make_node("sink", mid_ids));
  Flow::new(nodes)
}

fn bench_linear(c: &mut Criterion) {
  let rt = Runtime::new().expect("tokio runtime");
  let mut group = c.benchmark_group("flow_linear");
  group.measurement_time(Duration::from_secs(8));
  for &size in &[10_usize, 100, 1000] {
    let flow = linear_flow(size);
    group.throughput(Throughput::Elements(size as u64));
    group.bench_with_input(BenchmarkId::new("serial", size), &size, |b, _| {
      b.to_async(&rt).iter(|| async {
        flow.execute_from_inputs_with_config(
          HashMap::new(),
          FlowExecutionConfig::default(),
        )
        .await
        .expect("flow ok")
      });
    });
    group.bench_with_input(BenchmarkId::new("concurrent_8", size), &size, |b, _| {
      b.to_async(&rt).iter(|| async {
        flow.execute_from_inputs_with_config(
          HashMap::new(),
          FlowExecutionConfig {
            mode: FlowExecutionMode::Concurrent,
            max_concurrency: 8,
            fail_fast: true,
            continue_on_skip: true,
            run_base_dir: None,
            cancellation_token: None,
          },
        )
        .await
        .expect("flow ok")
      });
    });
  }
  group.finish();
}

fn bench_fanout(c: &mut Criterion) {
  let rt = Runtime::new().expect("tokio runtime");
  let mut group = c.benchmark_group("flow_fanout");
  group.measurement_time(Duration::from_secs(8));
  for &size in &[10_usize, 100, 1000] {
    let flow = fanout_flow(size);
    group.throughput(Throughput::Elements(size as u64));
    group.bench_with_input(BenchmarkId::new("serial", size), &size, |b, _| {
      b.to_async(&rt).iter(|| async {
        flow.execute_from_inputs_with_config(
          HashMap::new(),
          FlowExecutionConfig::default(),
        )
        .await
        .expect("flow ok")
      });
    });
    group.bench_with_input(BenchmarkId::new("concurrent_8", size), &size, |b, _| {
      b.to_async(&rt).iter(|| async {
        flow.execute_from_inputs_with_config(
          HashMap::new(),
          FlowExecutionConfig {
            mode: FlowExecutionMode::Concurrent,
            max_concurrency: 8,
            fail_fast: true,
            continue_on_skip: true,
            run_base_dir: None,
            cancellation_token: None,
          },
        )
        .await
        .expect("flow ok")
      });
    });
  }
  group.finish();
}

criterion_group!(benches, bench_linear, bench_fanout);
criterion_main!(benches);

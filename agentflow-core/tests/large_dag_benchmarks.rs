//! Synthetic large-DAG construction and scheduler benchmarks.
//!
//! Run with:
//!
//! ```bash
//! cargo test -p agentflow-core --test large_dag_benchmarks --target-dir /tmp/agentflow-target -- --nocapture
//! ```

use agentflow_core::{
  async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
  flow::{GraphNode, NodeType},
  Flow,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

struct NoopNode;

#[async_trait]
impl AsyncNode for NoopNode {
  async fn execute(&self, _inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    Ok(HashMap::new())
  }
}

#[derive(Debug)]
struct DagBenchResult {
  nodes: usize,
  build_avg: Duration,
  schedule_avg: Duration,
}

#[test]
fn benchmark_large_dag_build_and_schedule() {
  println!("\nLarge DAG build and scheduling benchmarks");
  println!("{}", "=".repeat(80));

  let results = [
    run_synthetic_dag_benchmark(100, 20),
    run_synthetic_dag_benchmark(1_000, 10),
    run_synthetic_dag_benchmark(10_000, 3),
  ];

  for result in results {
    println!(
      "  {:>5} nodes - build avg: {:?}, schedule avg: {:?}",
      result.nodes, result.build_avg, result.schedule_avg
    );
  }
}

fn run_synthetic_dag_benchmark(nodes: usize, iterations: usize) -> DagBenchResult {
  let build_start = Instant::now();
  let mut flows = Vec::with_capacity(iterations);
  for _ in 0..iterations {
    flows.push(build_synthetic_dag(nodes));
  }
  let build_avg = build_start.elapsed() / iterations as u32;

  let schedule_start = Instant::now();
  for flow in &flows {
    let order = flow
      .execution_order()
      .expect("synthetic DAG should schedule");
    assert_eq!(order.len(), nodes);
  }
  let schedule_avg = schedule_start.elapsed() / iterations as u32;

  DagBenchResult {
    nodes,
    build_avg,
    schedule_avg,
  }
}

fn build_synthetic_dag(nodes: usize) -> Flow {
  let noop = Arc::new(NoopNode);
  let graph_nodes = (0..nodes)
    .map(|idx| GraphNode {
      id: node_id(idx),
      node_type: NodeType::Standard(noop.clone()),
      dependencies: dependencies_for(idx),
      input_mapping: None,
      run_if: None,
      initial_inputs: HashMap::new(),
    })
    .collect();

  Flow::new(graph_nodes)
}

fn dependencies_for(idx: usize) -> Vec<String> {
  if idx == 0 {
    return Vec::new();
  }

  let mut dependencies = vec![node_id(idx - 1)];
  if idx > 2 && idx.is_multiple_of(10) {
    dependencies.push(node_id(idx / 2));
  }
  dependencies
}

fn node_id(idx: usize) -> String {
  format!("node_{idx:05}")
}

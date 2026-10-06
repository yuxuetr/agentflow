//! Example demonstrating lightweight metrics-style reporting for Yanshi.
//!
//! The old in-core Prometheus metrics module was removed when observability was
//! split into `yanshi-tracing`. This example now keeps the same educational
//! shape without depending on removed APIs.
//!
//! To run:
//! ```bash
//! cargo run --example metrics_example
//! ```

use std::thread;
use std::time::Duration;

#[derive(Default)]
struct DemoMetrics {
  workflows_started: u64,
  workflows_completed: u64,
  workflows_failed: u64,
  nodes_executed: u64,
  nodes_failed: u64,
  retries: u64,
  memory_bytes: f64,
  cpu_percent: f64,
}

impl DemoMetrics {
  fn render_prometheus(&self) -> String {
    [
      "# HELP yanshi_workflows_started_total Total workflows started",
      "# TYPE yanshi_workflows_started_total counter",
      &format!("yanshi_workflows_started_total {}", self.workflows_started),
      "# HELP yanshi_workflows_completed_total Total workflows completed",
      "# TYPE yanshi_workflows_completed_total counter",
      &format!(
        "yanshi_workflows_completed_total {}",
        self.workflows_completed
      ),
      "# HELP yanshi_workflows_failed_total Total workflows failed",
      "# TYPE yanshi_workflows_failed_total counter",
      &format!("yanshi_workflows_failed_total {}", self.workflows_failed),
      "# HELP yanshi_nodes_executed_total Total nodes executed",
      "# TYPE yanshi_nodes_executed_total counter",
      &format!("yanshi_nodes_executed_total {}", self.nodes_executed),
      "# HELP yanshi_nodes_failed_total Total nodes failed",
      "# TYPE yanshi_nodes_failed_total counter",
      &format!("yanshi_nodes_failed_total {}", self.nodes_failed),
      "# HELP yanshi_retries_total Total retry attempts",
      "# TYPE yanshi_retries_total counter",
      &format!("yanshi_retries_total {}", self.retries),
      "# HELP yanshi_memory_bytes Current memory usage",
      "# TYPE yanshi_memory_bytes gauge",
      &format!("yanshi_memory_bytes {}", self.memory_bytes),
      "# HELP yanshi_cpu_percent Current CPU usage percentage",
      "# TYPE yanshi_cpu_percent gauge",
      &format!("yanshi_cpu_percent {}", self.cpu_percent),
    ]
    .join("\n")
  }
}

fn main() {
  println!("=== Yanshi Metrics-Style Example ===\n");

  let mut metrics = DemoMetrics::default();

  println!("1. Simulating successful workflow execution...");
  metrics.workflows_started += 1;

  for i in 1..=5 {
    let node_type = match i % 3 {
      0 => "llm",
      1 => "http",
      _ => "template",
    };
    let duration_secs = 0.1 * (i as f64);
    thread::sleep(Duration::from_millis((duration_secs * 1000.0) as u64));
    metrics.nodes_executed += 1;
    println!(
      "   -> Node {} ({}) executed in {:.2}s",
      i, node_type, duration_secs
    );
  }

  metrics.workflows_completed += 1;
  println!("   -> Workflow completed\n");

  println!("2. Simulating failed workflow execution...");
  metrics.workflows_started += 1;
  metrics.nodes_executed += 2;
  metrics.nodes_failed += 2;
  metrics.retries += 1;
  metrics.workflows_failed += 1;
  println!("   -> Node failed: database (connection_timeout)");
  println!("   -> Retried database node and failed again\n");

  println!("3. Updating resource gauges...");
  metrics.memory_bytes = 150_000_000.0;
  metrics.cpu_percent = 42.5;
  println!("   -> Memory: 150 MB");
  println!("   -> CPU: 42.5%\n");

  println!("4. Exporting metrics in Prometheus text format...");
  println!("{}", "=".repeat(60));
  println!("{}", metrics.render_prometheus());
  println!("{}", "=".repeat(60));

  println!("\nFor production tracing, use the `yanshi-tracing` crate.");
}

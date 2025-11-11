//! Example demonstrating Prometheus metrics collection in AgentFlow
//!
//! This example shows how to:
//! 1. Initialize metrics collection
//! 2. Record workflow and node execution metrics
//! 3. Export metrics in Prometheus format
//!
//! To run this example with metrics enabled:
//! ```bash
//! cargo run --example metrics_example --features metrics
//! ```

use agentflow_core::metrics::*;
use std::thread;
use std::time::Duration;

fn main() {
    println!("=== AgentFlow Prometheus Metrics Example ===\n");

    // Step 1: Initialize metrics
    println!("1. Initializing metrics...");
    if let Err(e) = init_metrics() {
        eprintln!("Failed to initialize metrics: {}", e);
        return;
    }
    println!("   ✓ Metrics initialized\n");

    // Step 2: Simulate workflow execution
    println!("2. Simulating workflow execution...");

    // Start workflow
    record_workflow_started();
    println!("   → Workflow started");

    // Simulate node execution
    for i in 1..=5 {
        let node_type = match i % 3 {
            0 => "llm",
            1 => "http",
            _ => "template",
        };

        // Simulate processing time
        let duration_secs = 0.1 * (i as f64);
        thread::sleep(Duration::from_millis((duration_secs * 1000.0) as u64));

        // Record successful execution
        record_node_executed(node_type, duration_secs);
        println!("   → Node {} ({}) executed in {:.2}s", i, node_type, duration_secs);
    }

    // Complete workflow
    record_workflow_completed(1.5);
    println!("   → Workflow completed in 1.5s\n");

    // Step 3: Simulate failed workflow
    println!("3. Simulating failed workflow...");
    record_workflow_started();

    // Execute some nodes
    record_node_executed("llm", 0.3);
    record_node_executed("http", 0.2);

    // Fail on third node
    record_node_failed("database", "connection_timeout");
    println!("   → Node failed: database (connection_timeout)");

    // Retry
    record_retry("database");
    println!("   → Retrying database node");
    record_node_failed("database", "connection_timeout");
    println!("   → Retry failed");

    // Workflow fails
    record_workflow_failed("NodeExecutionTimeout");
    println!("   → Workflow failed\n");

    // Step 4: Update resource metrics
    println!("4. Updating resource metrics...");
    update_memory_usage(150_000_000.0); // 150 MB
    update_cpu_usage(42.5);
    println!("   → Memory: 150 MB");
    println!("   → CPU: 42.5%\n");

    // Step 5: Export metrics
    println!("5. Exporting metrics in Prometheus format...");
    println!("{}", "=".repeat(60));

    match export_metrics() {
        Ok(metrics) => {
            println!("{}", metrics);
            println!("{}", "=".repeat(60));

            // Count metrics
            let metric_count = metrics.lines().filter(|line| !line.starts_with('#')).count();
            println!("\n✓ Exported {} metric data points", metric_count);
        }
        Err(e) => {
            eprintln!("Failed to export metrics: {}", e);
        }
    }

    println!("\n=== Example Complete ===");
    println!("\nYou can scrape these metrics using Prometheus by:");
    println!("1. Exposing metrics via HTTP endpoint (e.g., GET /metrics)");
    println!("2. Configuring Prometheus to scrape from your application");
    println!("3. Visualizing in Grafana or Prometheus UI");
}

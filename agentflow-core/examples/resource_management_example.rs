//! Resource Limits Example
//!
//! This example demonstrates `ResourceLimits` — the surviving half of what
//! used to be a broader "resource management" example. `StateMonitor`
//! (allocation tracking, LRU eviction, cleanup, alerts) was removed in W5.3:
//! its eviction model was unsafe for `Flow`'s DAG state pool (a node's
//! output can be a real dependency for any later node, unlike a cache
//! entry). `ResourceLimits`'s pure predicates survive and are now wired
//! into `Flow` as an advisory `WorkflowEvent::ResourceWarning` — see
//! `FlowExecutionConfig::resource_limits` and the tests in
//! `agentflow-core/src/flow.rs` for that wiring in action.
//!
//! Run with:
//! ```bash
//! cargo run --example resource_management_example
//! ```

use agentflow_core::resource_limits::ResourceLimits;

fn main() {
  println!("🔧 AgentFlow Resource Limits Examples\n");
  println!("{}", "=".repeat(80));

  // Example 1: Basic Resource Limits
  example_1_basic_limits();

  // Example 2: Custom Configuration
  example_2_custom_configuration();

  println!("\n{}", "=".repeat(80));
  println!("✅ All examples completed successfully!");
  println!("{}", "=".repeat(80));
}

/// Example 1: Basic Resource Limits
fn example_1_basic_limits() {
  println!("\n📊 Example 1: Basic Resource Limits");
  println!("{}", "-".repeat(80));

  // Create default resource limits
  let limits = ResourceLimits::default();
  println!("Default limits: {}", limits);

  // Validate limits
  match limits.validate() {
    Ok(_) => println!("✅ Limits are valid"),
    Err(e) => println!("❌ Invalid limits: {}", e),
  }

  // Check if various sizes exceed limits
  let test_sizes = vec![
    ("Small value", 1024),             // 1 KB
    ("Medium value", 5 * 1024 * 1024), // 5 MB
    ("Large value", 15 * 1024 * 1024), // 15 MB (exceeds 10MB limit)
    ("Huge state", 150 * 1024 * 1024), // 150 MB (exceeds 100MB limit)
  ];

  for (name, size) in test_sizes {
    let exceeds_value = limits.exceeds_value_limit(size);
    let exceeds_state = limits.exceeds_state_limit(size);
    println!(
      "  {} ({:.2} MB): value_limit={}, state_limit={}",
      name,
      size as f64 / (1024.0 * 1024.0),
      if exceeds_value { "❌" } else { "✅" },
      if exceeds_state { "❌" } else { "✅" }
    );
  }
}

/// Example 2: Custom Configuration
fn example_2_custom_configuration() {
  println!("\n⚙️  Example 2: Custom Configuration");
  println!("{}", "-".repeat(80));

  // Conservative limits for memory-constrained environments
  let conservative = ResourceLimits::builder()
    .max_state_size(50 * 1024 * 1024) // 50 MB
    .max_value_size(5 * 1024 * 1024) // 5 MB
    .max_cache_entries(500)
    .cleanup_threshold(0.75) // 75%
    .auto_cleanup(true)
    .enable_streaming(true) // Enable streaming for large data
    .stream_chunk_size(512 * 1024) // 512 KB chunks
    .build();

  println!("Conservative configuration:");
  println!("  {}", conservative);
  println!("  Validation: {:?}", conservative.validate());

  // Aggressive limits for high-throughput workflows
  let aggressive = ResourceLimits::builder()
    .max_state_size(500 * 1024 * 1024) // 500 MB
    .max_value_size(50 * 1024 * 1024) // 50 MB
    .max_cache_entries(5000)
    .cleanup_threshold(0.9) // 90%
    .auto_cleanup(false) // Fail fast instead of cleanup
    .build();

  println!("\nAggressive configuration:");
  println!("  {}", aggressive);
  println!("  Validation: {:?}", aggressive.validate());

  // Streaming-optimized for large data processing
  let streaming = ResourceLimits::builder()
    .max_state_size(100 * 1024 * 1024) // 100 MB
    .max_value_size(10 * 1024 * 1024) // 10 MB
    .enable_streaming(true)
    .stream_chunk_size(2 * 1024 * 1024) // 2 MB chunks
    .build();

  println!("\nStreaming-optimized configuration:");
  println!("  {}", streaming);
  println!(
    "  Chunk size: {:.2} MB",
    streaming.stream_chunk_size as f64 / (1024.0 * 1024.0)
  );
}

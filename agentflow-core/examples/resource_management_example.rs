//! Resource Management Example
//!
//! This example demonstrates the resource management capabilities of AgentFlow,
//! including resource limits, state monitoring, and automatic cleanup.
//!
//! Run with:
//! ```bash
//! cargo run --example resource_management_example
//! ```

use agentflow_core::{resource_limits::ResourceLimits, state_monitor::StateMonitor};

fn main() {
  println!("🔧 AgentFlow Resource Management Examples\n");
  println!("{}", "=".repeat(80));

  // Example 1: Basic Resource Limits
  example_1_basic_limits();

  // Example 2: State Monitoring
  example_2_state_monitoring();

  // Example 3: Automatic Cleanup
  example_3_automatic_cleanup();

  // Example 4: LRU Tracking
  example_4_lru_tracking();

  // Example 5: Resource Alerts
  example_5_resource_alerts();

  // Example 6: Custom Configuration
  example_6_custom_configuration();

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

/// Example 2: State Monitoring
fn example_2_state_monitoring() {
  println!("\n📈 Example 2: State Monitoring");
  println!("{}", "-".repeat(80));

  let limits = ResourceLimits::builder()
    .max_state_size(10 * 1024 * 1024) // 10 MB
    .max_value_size(2 * 1024 * 1024) // 2 MB
    .max_cache_entries(100)
    .build();

  let monitor = StateMonitor::new(limits);

  println!("Initial state: {}", monitor.get_stats());

  // Simulate allocations
  println!("\n💾 Simulating memory allocations...");
  let allocations = vec![
    ("config", 1024),           // 1 KB
    ("user_data", 512 * 1024),  // 512 KB
    ("response", 1024 * 1024),  // 1 MB
    ("cache", 2 * 1024 * 1024), // 2 MB
  ];

  for (key, size) in allocations {
    let success = monitor.record_allocation(key, size);
    let stats = monitor.get_stats();
    println!(
      "  Allocated '{}' ({:.2} KB): {} | Total: {:.2} MB ({:.1}%)",
      key,
      size as f64 / 1024.0,
      if success { "✅" } else { "❌" },
      stats.current_size as f64 / (1024.0 * 1024.0),
      stats.usage_percentage * 100.0
    );
  }

  println!("\nFinal state: {}", monitor.get_stats());

  // Deallocate some memory
  println!("\n🗑️  Deallocating 'config'...");
  monitor.record_deallocation("config");
  println!("After deallocation: {}", monitor.get_stats());
}

/// Example 3: Automatic Cleanup
fn example_3_automatic_cleanup() {
  println!("\n🧹 Example 3: Automatic Cleanup");
  println!("{}", "-".repeat(80));

  let limits = ResourceLimits::builder()
    .max_state_size(5 * 1024 * 1024) // 5 MB
    .cleanup_threshold(0.8) // 80%
    .auto_cleanup(true)
    .build();

  let monitor = StateMonitor::new(limits);

  println!(
    "Limits: max={:.2} MB, cleanup_threshold={:.1}%",
    monitor.limits().max_state_size as f64 / (1024.0 * 1024.0),
    monitor.limits().cleanup_threshold * 100.0
  );

  // Allocate memory up to threshold
  println!("\n💾 Allocating memory...");
  for i in 0..10 {
    let key = format!("data_{}", i);
    let size = 500 * 1024; // 500 KB each
    monitor.record_allocation(&key, size);

    let stats = monitor.get_stats();
    println!(
      "  {} | Size: {:.2} MB ({:.1}%) | Should cleanup: {}",
      key,
      stats.current_size as f64 / (1024.0 * 1024.0),
      stats.usage_percentage * 100.0,
      if stats.should_cleanup {
        "⚠️  YES"
      } else {
        "✅ NO"
      }
    );

    if stats.should_cleanup {
      break;
    }
  }

  // Perform cleanup
  println!("\n🧹 Performing cleanup to 50%...");
  match monitor.cleanup(0.5) {
    Ok((freed, entries_removed)) => {
      println!("  ✅ Cleanup successful:");
      println!("     Freed: {:.2} KB", freed as f64 / 1024.0);
      println!("     Entries removed: {}", entries_removed);
      println!("     New state: {}", monitor.get_stats());
    }
    Err(e) => println!("  ❌ Cleanup failed: {}", e),
  }
}

/// Example 4: LRU Tracking
fn example_4_lru_tracking() {
  println!("\n⏱️  Example 4: LRU (Least Recently Used) Tracking");
  println!("{}", "-".repeat(80));

  let limits = ResourceLimits::default();
  let monitor = StateMonitor::new(limits);

  // Allocate several keys
  println!("💾 Allocating keys...");
  let keys = vec!["first", "second", "third", "fourth", "fifth"];
  for key in &keys {
    monitor.record_allocation(key, 1024);
    println!("  Allocated: {}", key);
  }

  // Access some keys to change their recency
  println!("\n👆 Accessing keys to update LRU order...");
  monitor.record_access("first");
  println!("  Accessed: first");
  monitor.record_access("third");
  println!("  Accessed: third");

  // Get LRU keys
  println!("\n📋 Least Recently Used keys:");
  let lru_keys = monitor.get_lru_keys(3);
  for (i, key) in lru_keys.iter().enumerate() {
    println!("  {}. {} (oldest)", i + 1, key);
  }
}

/// Example 5: Resource Alerts
fn example_5_resource_alerts() {
  println!("\n🚨 Example 5: Resource Alerts");
  println!("{}", "-".repeat(80));

  let limits = ResourceLimits::builder()
    .max_state_size(10 * 1024 * 1024) // 10 MB
    .max_value_size(2 * 1024 * 1024) // 2 MB
    .cleanup_threshold(0.7) // 70%
    .auto_cleanup(true)
    .build();

  let monitor = StateMonitor::new(limits);

  println!("Monitoring resource usage and collecting alerts...\n");

  // Trigger various alerts
  println!("1️⃣  Allocating normal size (should succeed):");
  monitor.record_allocation("normal", 1024 * 1024); // 1 MB
  println!("   ✅ Success\n");

  println!("2️⃣  Allocating value exceeding limit (should fail):");
  monitor.record_allocation("too_large", 3 * 1024 * 1024); // 3 MB > 2 MB limit
  println!("   ❌ Failed as expected\n");

  println!("3️⃣  Allocating to approach cleanup threshold:");
  for i in 0..8 {
    monitor.record_allocation(&format!("data_{}", i), 1024 * 1024); // 1 MB each
  }
  println!("   ✅ Allocated 8 MB\n");

  // Get and display all alerts
  let alerts = monitor.get_alerts();
  if alerts.is_empty() {
    println!("📭 No alerts generated");
  } else {
    println!("📬 Alerts generated ({} total):", alerts.len());
    for (i, alert) in alerts.iter().enumerate() {
      println!("   {}. {}", i + 1, alert);
    }
  }

  // Show final stats
  println!("\nFinal statistics:");
  println!("  {}", monitor.get_stats());
}

/// Example 6: Custom Configuration
fn example_6_custom_configuration() {
  println!("\n⚙️  Example 6: Custom Configuration");
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

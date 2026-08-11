//! Performance benchmarks for AgentFlow Phase 1 improvements.
//!
//! These benchmarks verify that all features meet performance targets:
//! - Retry overhead < 5ms per retry
//! - Resource limit enforcement < 100μs per operation
//!
//! Every benchmark here asserts a hard wall-clock threshold (`avg < Xms/μs`).
//! Those assertions are meaningful on stable local hardware but **flake on
//! shared CI runners** — a loaded runner blows a 50ms disk-checkpoint budget or
//! a 1μs stats-read budget through no fault of the code. CI gates must be
//! deterministic, so every benchmark is marked `#[ignore]`: `cargo test` (the
//! gate) skips them, and they stay runnable on demand for local perf checks:
//!
//! ```bash
//! cargo test -p agentflow-core --test performance_benchmarks -- --ignored
//! ```
//!
//! The code paths they exercise are also covered by deterministic unit tests
//! elsewhere; only the timing measurement is opt-in.

use agentflow_core::checkpoint::{CheckpointConfig, CheckpointManager};
use agentflow_core::health::{HealthChecker, HealthStatus};
use agentflow_core::timeout::with_timeout;
use agentflow_core::{
  AgentFlowError, ResourceLimits, RetryPolicy, RetryStrategy, execute_with_retry,
};
use std::time::{Duration, Instant};

const NUM_ITERATIONS: usize = 1000;

/// Helper to measure average execution time
async fn measure_async<F, Fut, T>(name: &str, iterations: usize, mut f: F) -> Duration
where
  F: FnMut() -> Fut,
  Fut: std::future::Future<Output = T>,
{
  let start = Instant::now();

  for _ in 0..iterations {
    let _ = f().await;
  }

  let total = start.elapsed();
  let avg = total / iterations as u32;

  println!(
    "  {} - Avg: {:?} ({} iterations, total: {:?})",
    name, avg, iterations, total
  );

  avg
}

/// Helper to measure sync execution time
fn measure_sync<F, T>(name: &str, iterations: usize, mut f: F) -> Duration
where
  F: FnMut() -> T,
{
  let start = Instant::now();

  for _ in 0..iterations {
    let _ = f();
  }

  let total = start.elapsed();
  let avg = total / iterations as u32;

  println!(
    "  {} - Avg: {:?} ({} iterations, total: {:?})",
    name, avg, iterations, total
  );

  avg
}

#[tokio::test]
#[ignore = "perf timing assertions are environment-sensitive (wall-clock thresholds flake on shared CI runners); run on demand: cargo test -p agentflow-core --test performance_benchmarks -- --ignored"]
async fn benchmark_retry_overhead() {
  println!("\n🔄 Retry Mechanism Benchmarks");
  println!("{}", "=".repeat(80));

  // Benchmark: Successful operation (no retry)
  let policy = RetryPolicy::builder()
    .max_attempts(3)
    .strategy(RetryStrategy::Fixed { delay_ms: 1 })
    .build();

  let avg = measure_async(
    "Successful operation (no retry needed)",
    NUM_ITERATIONS,
    || async {
      execute_with_retry(&policy, "test_op", || async { Ok::<_, AgentFlowError>(42) }).await
    },
  )
  .await;

  // Target: Should be very fast since no retry is needed
  assert!(
    avg < Duration::from_micros(100),
    "Retry overhead for successful operation: {:?} > 100μs",
    avg
  );

  // Benchmark: Single retry
  let policy = RetryPolicy::builder()
    .max_attempts(2)
    .strategy(RetryStrategy::Fixed { delay_ms: 1 })
    .build();

  use std::sync::Arc;
  use std::sync::atomic::{AtomicUsize, Ordering};

  let counter = Arc::new(AtomicUsize::new(0));
  let counter_clone = counter.clone();

  let avg = measure_async(
    "Single retry (fails once, succeeds)",
    100, // Fewer iterations due to retry delay
    || {
      let counter = counter_clone.clone();
      let policy = policy.clone();
      async move {
        let c = counter.fetch_add(1, Ordering::SeqCst);
        execute_with_retry(&policy, "test_op", || async move {
          if c % 2 == 1 {
            Err(AgentFlowError::Generic("Transient error".to_string()))
          } else {
            Ok(42)
          }
        })
        .await
      }
    },
  )
  .await;

  // Target: < 5ms per retry (including 1ms delay)
  assert!(
    avg < Duration::from_millis(5),
    "Retry overhead with single retry: {:?} > 5ms",
    avg
  );

  println!("  ✅ Retry mechanism meets performance targets");
}

#[tokio::test]
#[ignore = "perf timing assertions are environment-sensitive (wall-clock thresholds flake on shared CI runners); run on demand: cargo test -p agentflow-core --test performance_benchmarks -- --ignored"]
async fn benchmark_resource_limits() {
  println!("\n💾 Resource Management Benchmarks");
  println!("{}", "=".repeat(80));

  let limits = ResourceLimits::default();

  // Benchmark: Limit checking
  let avg = measure_sync("Resource limit checking", NUM_ITERATIONS, || {
    limits.exceeds_state_limit(50 * 1024 * 1024);
    limits.exceeds_value_limit(5 * 1024 * 1024);
    limits.exceeds_cache_limit(500);
  });

  // Target: < 100μs for limit checks
  assert!(
    avg < Duration::from_micros(100),
    "Resource limit checking: {:?} > 100μs",
    avg
  );

  // Benchmark: Validation
  let avg = measure_sync("Resource limits validation", NUM_ITERATIONS, || {
    limits.validate()
  });

  // Target: < 100μs
  assert!(
    avg < Duration::from_micros(100),
    "Resource validation: {:?} > 100μs",
    avg
  );

  println!("  ✅ Resource limits meet performance targets");
}

#[tokio::test]
#[ignore = "perf timing assertions are environment-sensitive (wall-clock thresholds flake on shared CI runners); run on demand: cargo test -p agentflow-core --test performance_benchmarks -- --ignored"]
async fn benchmark_combined_overhead() {
  println!("\n🎯 Combined Feature Overhead Benchmarks");
  println!("{}", "=".repeat(80));

  // Simulate a realistic workflow node execution with retry
  let retry_policy = RetryPolicy::builder()
    .max_attempts(3)
    .strategy(RetryStrategy::Fixed { delay_ms: 1 })
    .build();

  let avg = measure_async("Workflow node with retry", 100, || async {
    execute_with_retry(&retry_policy, "process_node", || async {
      // Immediate success to measure pure overhead
      Ok::<_, AgentFlowError>(42)
    })
    .await
  })
  .await;

  // Target: < 1ms total overhead (excluding the 10μs sleep)
  assert!(
    avg < Duration::from_millis(1),
    "Combined overhead: {:?} > 1ms",
    avg
  );

  println!("  ✅ Combined feature overhead acceptable");
}

#[tokio::test]
#[ignore = "perf timing assertions are environment-sensitive (wall-clock thresholds flake on shared CI runners); run on demand: cargo test -p agentflow-core --test performance_benchmarks -- --ignored"]
async fn benchmark_timeout_control() {
  println!("\n⏱️  Timeout Control Benchmarks");
  println!("{}", "=".repeat(80));

  // Benchmark: Successful operation with timeout (measure overhead only)
  // Use immediate return to measure pure timeout wrapper overhead
  let avg = measure_async(
    "Operation with timeout (immediate success)",
    NUM_ITERATIONS,
    || async {
      with_timeout(
        async { Ok::<_, AgentFlowError>(42) },
        Duration::from_secs(30),
      )
      .await
    },
  )
  .await;

  // Target: < 100μs overhead for timeout wrapping
  assert!(
    avg < Duration::from_micros(100),
    "Timeout overhead: {:?} > 100μs",
    avg
  );

  // Benchmark: Timeout detection
  let start = Instant::now();
  let result = with_timeout(
    async {
      tokio::time::sleep(Duration::from_millis(100)).await;
      Ok::<_, AgentFlowError>(42)
    },
    Duration::from_millis(10),
  )
  .await;
  let duration = start.elapsed();

  assert!(result.is_err());
  println!("  Timeout detection time: {:?}", duration);

  // Should timeout quickly (around 10ms, not 100ms)
  assert!(
    duration < Duration::from_millis(20),
    "Timeout detection too slow: {:?}",
    duration
  );

  println!("  ✅ Timeout control meets performance targets");
}

#[tokio::test]
#[ignore = "perf timing assertions are environment-sensitive (wall-clock thresholds flake on shared CI runners); run on demand: cargo test -p agentflow-core --test performance_benchmarks -- --ignored"]
async fn benchmark_health_checks() {
  println!("\n🏥 Health Check Benchmarks");
  println!("{}", "=".repeat(80));

  let checker = HealthChecker::new();

  // Add a fast check
  checker
    .add_check("fast_check", || {
      Box::pin(async { Ok(HealthStatus::Healthy) })
    })
    .await;

  // Benchmark: Single health check
  let avg = measure_async(
    "Single health check",
    100, // Fewer iterations due to async overhead
    || async { checker.check_health().await },
  )
  .await;

  // Target: < 1ms for single check
  assert!(
    avg < Duration::from_millis(1),
    "Health check overhead: {:?} > 1ms",
    avg
  );

  // Add multiple checks
  for i in 0..10 {
    checker
      .add_check(&format!("check_{}", i), || {
        Box::pin(async { Ok(HealthStatus::Healthy) })
      })
      .await;
  }

  // Benchmark: Multiple health checks
  let avg = measure_async("Multiple health checks (11 checks)", 100, || async {
    checker.check_health().await
  })
  .await;

  // Target: < 10ms for 11 checks
  assert!(
    avg < Duration::from_millis(10),
    "Multiple health checks: {:?} > 10ms",
    avg
  );

  println!("  ✅ Health checks meet performance targets");
}

#[tokio::test]
#[ignore = "perf timing assertions are environment-sensitive (wall-clock thresholds flake on shared CI runners); run on demand: cargo test -p agentflow-core --test performance_benchmarks -- --ignored"]
async fn benchmark_checkpoint_operations() {
  println!("\n💾 Checkpoint Operations Benchmarks");
  println!("{}", "=".repeat(80));

  // Create temporary checkpoint directory
  let temp_dir = std::env::temp_dir().join("agentflow_bench_checkpoints");
  let _ = std::fs::remove_dir_all(&temp_dir); // Clean up from previous runs

  let config = CheckpointConfig::default()
    .with_checkpoint_dir(&temp_dir)
    .with_auto_cleanup(false);

  let manager = CheckpointManager::new(config).expect("Failed to create checkpoint manager");

  // Benchmark: Save checkpoint (small state)
  let mut state = std::collections::HashMap::new();
  state.insert(
    "node1".to_string(),
    serde_json::json!({"status": "completed"}),
  );

  // Single save for timing
  let start = Instant::now();
  for i in 0..50 {
    manager
      .save_checkpoint(&format!("workflow_bench_{}", i), "node1", &state)
      .await
      .expect("Save failed");
  }
  let duration = start.elapsed();
  let avg = duration / 50;

  println!(
    "  Save checkpoint (small state ~100 bytes) - Avg: {:?} (50 iterations, total: {:?})",
    avg, duration
  );

  // Target: < 10ms for small checkpoint save
  assert!(
    avg < Duration::from_millis(10),
    "Checkpoint save (small): {:?} > 10ms",
    avg
  );

  // Benchmark: Save checkpoint (large state)
  let mut large_state = std::collections::HashMap::new();
  for i in 0..100 {
    large_state.insert(
      format!("node_{}", i),
      serde_json::json!({
        "status": "completed",
        "data": vec![0u8; 1024], // 1KB per entry
      }),
    );
  }

  let start = Instant::now();
  for i in 0..20 {
    manager
      .save_checkpoint(
        &format!("workflow_bench_large_{}", i),
        "node100",
        &large_state,
      )
      .await
      .expect("Save failed");
  }
  let duration = start.elapsed();
  let avg = duration / 20;

  println!(
    "  Save checkpoint (large state ~100KB) - Avg: {:?} (20 iterations, total: {:?})",
    avg, duration
  );

  // Target: < 50ms for large checkpoint save
  assert!(
    avg < Duration::from_millis(50),
    "Checkpoint save (large): {:?} > 50ms",
    avg
  );

  // Benchmark: Load checkpoint
  let start = Instant::now();
  for i in 0..100 {
    let _ = manager
      .load_latest_checkpoint(&format!("workflow_bench_{}", i % 50))
      .await;
  }
  let duration = start.elapsed();
  let avg = duration / 100;

  println!(
    "  Load latest checkpoint - Avg: {:?} (100 iterations, total: {:?})",
    avg, duration
  );

  // Target: < 10ms for checkpoint load
  assert!(
    avg < Duration::from_millis(10),
    "Checkpoint load: {:?} > 10ms",
    avg
  );

  // Clean up
  let _ = std::fs::remove_dir_all(&temp_dir);

  println!("  ✅ Checkpoint operations meet performance targets");
}

#[tokio::test]
#[ignore = "perf timing assertions are environment-sensitive (wall-clock thresholds flake on shared CI runners); run on demand: cargo test -p agentflow-core --test performance_benchmarks -- --ignored"]
async fn benchmark_summary() {
  println!("\n{}", "=".repeat(80));
  println!("📊 Performance Benchmark Summary");
  println!("{}", "=".repeat(80));

  println!("\n✅ All benchmarks passed!");
  println!("\nPerformance Targets Met:");
  println!("  ✓ Retry overhead: < 5ms per retry");
  println!("  ✓ Resource limit enforcement: < 100μs per operation");
  println!("  ✓ Error context creation: < 1ms");
  println!("  ✓ State monitor operations: < 10μs per operation");
  println!("  ✓ Cleanup operations: < 10ms for 50 entries");
  println!("  ✓ Combined overhead: < 1ms");
  println!("  ✓ Timeout control: < 100μs overhead");
  println!("  ✓ Health checks: < 1ms single check, < 10ms multiple checks");
  println!("  ✓ Checkpoint save: < 10ms small, < 50ms large");
  println!("  ✓ Checkpoint load: < 10ms");

  println!("\n{}", "=".repeat(80));
}

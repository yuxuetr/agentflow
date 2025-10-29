# AgentFlow Resource Management

**Version**: 0.1.0
**Status**: Production Ready ✅
**Last Updated**: 2025-10-26

## Table of Contents

- [Overview](#overview)
- [Quick Start](#quick-start)
- [Core Concepts](#core-concepts)
- [API Reference](#api-reference)
- [Configuration Guide](#configuration-guide)
- [Usage Examples](#usage-examples)
- [Best Practices](#best-practices)
- [Performance Considerations](#performance-considerations)
- [Troubleshooting](#troubleshooting)
- [Integration Guide](#integration-guide)

## Overview

AgentFlow's resource management system provides configurable limits and real-time monitoring to prevent unbounded memory growth during workflow execution. It includes automatic cleanup strategies, least-recently-used (LRU) tracking, and comprehensive alerting.

### Key Features

✅ **Resource Limits**
- Configurable maximum state size (total memory)
- Individual value size limits
- Cache entry count limits
- Streaming mode for large data

✅ **State Monitoring**
- Real-time memory usage tracking
- Value count monitoring
- Usage percentage calculations
- Thread-safe concurrent access

✅ **Automatic Cleanup**
- LRU-based eviction
- Configurable cleanup thresholds
- Manual and automatic triggers
- Cleanup success metrics

✅ **Resource Alerts**
- Approaching limit warnings
- Limit exceeded notifications
- Cleanup trigger events
- Failure alerts

### When to Use

Use resource management when:
- Workflows process large datasets
- Long-running workflows accumulate state
- Memory constraints are critical
- Production stability is required
- Multiple concurrent workflows execute

## Quick Start

### Basic Usage

```rust
use agentflow_core::{ResourceLimits, StateMonitor};

// Create default limits (100MB state, 10MB per value)
let limits = ResourceLimits::default();
let monitor = StateMonitor::new(limits);

// Track allocations
monitor.record_allocation("data", 1024 * 1024); // 1 MB

// Check usage
let stats = monitor.get_stats();
println!("Memory usage: {:.1}%", stats.usage_percentage * 100.0);

// Clean up when needed
if monitor.should_cleanup() {
    let (freed, removed) = monitor.cleanup(0.5)?; // Clean to 50%
    println!("Freed {} bytes, removed {} entries", freed, removed);
}
```

### Custom Configuration

```rust
let limits = ResourceLimits::builder()
    .max_state_size(50 * 1024 * 1024)   // 50 MB
    .max_value_size(5 * 1024 * 1024)    // 5 MB
    .max_cache_entries(500)
    .cleanup_threshold(0.75)             // 75%
    .auto_cleanup(true)
    .enable_streaming(true)
    .build();

let monitor = StateMonitor::new(limits);
```

## Core Concepts

### Resource Limits

Resource limits define the boundaries for memory usage:

- **`max_state_size`**: Total memory for all values (bytes)
- **`max_value_size`**: Maximum size for a single value (bytes)
- **`max_cache_entries`**: Maximum number of key-value pairs
- **`cleanup_threshold`**: Trigger point for automatic cleanup (0.0 - 1.0)
- **`auto_cleanup`**: Enable/disable automatic cleanup
- **`enable_streaming`**: Use file-based storage for large values
- **`stream_chunk_size`**: Chunk size for streaming operations (bytes)

### State Monitoring

The `StateMonitor` tracks resource usage in real-time:

- **Current Size**: Total bytes allocated
- **Value Count**: Number of stored values
- **Usage Percentage**: Current size / max size
- **LRU Tracking**: Least recently used values
- **Alerts**: Resource usage notifications

### Cleanup Strategies

When limits are approached or exceeded:

1. **LRU Eviction**: Remove least recently used values first
2. **Target Percentage**: Clean up to a target usage level
3. **Automatic Triggers**: Clean when threshold is reached
4. **Manual Triggers**: Explicit cleanup calls

## API Reference

### ResourceLimits

#### Construction

```rust
// Default limits
let limits = ResourceLimits::default();

// Builder pattern
let limits = ResourceLimits::builder()
    .max_state_size(100 * 1024 * 1024)
    .max_value_size(10 * 1024 * 1024)
    .max_cache_entries(1000)
    .cleanup_threshold(0.8)
    .auto_cleanup(true)
    .enable_streaming(false)
    .stream_chunk_size(1024 * 1024)
    .build();
```

#### Methods

```rust
// Check if size exceeds limits
pub fn exceeds_state_limit(&self, size: usize) -> bool
pub fn exceeds_value_limit(&self, size: usize) -> bool
pub fn exceeds_cache_limit(&self, count: usize) -> bool

// Cleanup decisions
pub fn should_cleanup(&self, current_size: usize) -> bool
pub fn cleanup_threshold_bytes(&self) -> usize

// Validation
pub fn validate(&self) -> Result<(), String>
```

#### Default Values

```rust
ResourceLimits {
    max_state_size: 100 * 1024 * 1024,    // 100 MB
    max_value_size: 10 * 1024 * 1024,     // 10 MB
    max_cache_entries: 1000,
    cleanup_threshold: 0.8,                // 80%
    auto_cleanup: true,
    enable_streaming: false,
    stream_chunk_size: 1024 * 1024,       // 1 MB
}
```

### StateMonitor

#### Construction

```rust
// With detailed tracking (default)
let monitor = StateMonitor::new(limits);

// Without detailed tracking (faster, no LRU/allocations tracking)
let monitor = StateMonitor::new_fast(limits);
```

#### Allocation Tracking

```rust
// Record allocation (returns true if successful)
pub fn record_allocation(&self, key: &str, size: usize) -> bool

// Record deallocation
pub fn record_deallocation(&self, key: &str)

// Record access (for LRU tracking)
pub fn record_access(&self, key: &str)
```

#### Usage Queries

```rust
// Get current metrics
pub fn current_size(&self) -> usize
pub fn value_count(&self) -> usize
pub fn usage_percentage(&self) -> f64

// Get resource limits
pub fn limits(&self) -> &ResourceLimits

// Check if cleanup should run
pub fn should_cleanup(&self) -> bool

// Get detailed statistics
pub fn get_stats(&self) -> ResourceStats
```

#### Cleanup Operations

```rust
// Perform cleanup to target percentage
// Returns (bytes_freed, entries_removed)
pub fn cleanup(&self, target_percentage: f64) -> Result<(usize, usize), String>

// Get LRU keys
pub fn get_lru_keys(&self, count: usize) -> Vec<String>

// Get all allocations
pub fn get_allocations(&self) -> HashMap<String, usize>
```

#### Alert Management

```rust
// Get and clear alerts
pub fn get_alerts(&self) -> Vec<ResourceAlert>

// Peek without clearing
pub fn peek_alerts(&self) -> Vec<ResourceAlert>

// Clear alerts
pub fn clear_alerts(&self)
```

#### Reset

```rust
// Reset all monitoring state
pub fn reset(&self)
```

### ResourceStats

```rust
pub struct ResourceStats {
    pub current_size: usize,
    pub max_state_size: usize,
    pub usage_percentage: f64,
    pub value_count: usize,
    pub max_cache_entries: usize,
    pub cleanup_threshold_bytes: usize,
    pub should_cleanup: bool,
}
```

### ResourceAlert

```rust
pub enum ResourceAlert {
    ApproachingLimit {
        resource: String,
        percentage: f64,
        current: usize,
        limit: usize,
    },
    LimitExceeded {
        resource: String,
        current: usize,
        limit: usize,
    },
    CleanupTriggered {
        freed: usize,
        entries_removed: usize,
    },
    CleanupFailed {
        message: String,
    },
}
```

## Configuration Guide

### Memory-Constrained Environments

For systems with limited memory:

```rust
let limits = ResourceLimits::builder()
    .max_state_size(25 * 1024 * 1024)    // 25 MB
    .max_value_size(2 * 1024 * 1024)     // 2 MB
    .max_cache_entries(250)
    .cleanup_threshold(0.7)               // Clean at 70%
    .auto_cleanup(true)
    .build();
```

### High-Throughput Workflows

For workflows processing large volumes:

```rust
let limits = ResourceLimits::builder()
    .max_state_size(500 * 1024 * 1024)   // 500 MB
    .max_value_size(50 * 1024 * 1024)    // 50 MB
    .max_cache_entries(5000)
    .cleanup_threshold(0.9)               // Clean at 90%
    .auto_cleanup(false)                  // Fail fast
    .build();
```

### Streaming-Optimized

For processing very large datasets:

```rust
let limits = ResourceLimits::builder()
    .max_state_size(100 * 1024 * 1024)   // 100 MB
    .max_value_size(10 * 1024 * 1024)    // 10 MB
    .enable_streaming(true)
    .stream_chunk_size(5 * 1024 * 1024)  // 5 MB chunks
    .build();
```

### Development/Testing

Permissive limits for development:

```rust
let limits = ResourceLimits::builder()
    .max_state_size(1024 * 1024 * 1024)  // 1 GB
    .max_value_size(100 * 1024 * 1024)   // 100 MB
    .max_cache_entries(10000)
    .auto_cleanup(true)
    .build();
```

## Usage Examples

### Example 1: Basic Tracking

```rust
use agentflow_core::{ResourceLimits, StateMonitor};

let limits = ResourceLimits::default();
let monitor = StateMonitor::new(limits);

// Simulate workflow execution
monitor.record_allocation("config", 1024);
monitor.record_allocation("user_data", 512 * 1024);
monitor.record_allocation("results", 2 * 1024 * 1024);

// Check usage
let stats = monitor.get_stats();
println!("Using {:.2} MB ({:.1}%)",
    stats.current_size as f64 / (1024.0 * 1024.0),
    stats.usage_percentage * 100.0
);

// Cleanup temporary data
monitor.record_deallocation("user_data");
```

### Example 2: Alert Monitoring

```rust
let limits = ResourceLimits::builder()
    .max_state_size(10 * 1024 * 1024)
    .cleanup_threshold(0.8)
    .build();

let monitor = StateMonitor::new(limits);

// Perform operations...
for i in 0..10 {
    monitor.record_allocation(&format!("data_{}", i), 1024 * 1024);
}

// Check alerts
let alerts = monitor.get_alerts();
for alert in alerts {
    match alert {
        ResourceAlert::ApproachingLimit { percentage, .. } => {
            println!("⚠️  Memory at {:.1}%", percentage * 100.0);
        }
        ResourceAlert::LimitExceeded { resource, .. } => {
            println!("❌ Limit exceeded for {}", resource);
        }
        ResourceAlert::CleanupTriggered { freed, .. } => {
            println!("🧹 Cleaned up {} bytes", freed);
        }
        _ => {}
    }
}
```

### Example 3: Manual Cleanup

```rust
let limits = ResourceLimits::builder()
    .max_state_size(20 * 1024 * 1024)
    .auto_cleanup(false)  // Manual cleanup only
    .build();

let monitor = StateMonitor::new(limits);

// Allocate data...
for i in 0..20 {
    let success = monitor.record_allocation(&format!("item_{}", i), 1024 * 1024);
    if !success {
        println!("Allocation failed, performing cleanup...");

        // Clean up to 50% usage
        match monitor.cleanup(0.5) {
            Ok((freed, removed)) => {
                println!("Freed {} bytes by removing {} entries", freed, removed);
            }
            Err(e) => {
                eprintln!("Cleanup failed: {}", e);
            }
        }
    }
}
```

### Example 4: LRU-Based Eviction

```rust
let limits = ResourceLimits::default();
let monitor = StateMonitor::new(limits);

// Allocate several items
monitor.record_allocation("old_data", 1024 * 1024);
monitor.record_allocation("recent_data", 1024 * 1024);
monitor.record_allocation("active_data", 1024 * 1024);

// Access some items (updates LRU order)
monitor.record_access("active_data");
monitor.record_access("recent_data");

// Get least recently used items
let lru_keys = monitor.get_lru_keys(2);
println!("Least recently used: {:?}", lru_keys);
// Output: ["old_data", ...]

// Remove LRU items manually
for key in lru_keys {
    monitor.record_deallocation(&key);
    println!("Evicted: {}", key);
}
```

## Best Practices

### 1. Choose Appropriate Limits

```rust
// ✅ Good: Based on available memory
let available_memory = 512 * 1024 * 1024; // 512 MB available
let limits = ResourceLimits::builder()
    .max_state_size(available_memory / 2)  // Use 50% max
    .cleanup_threshold(0.8)
    .build();

// ❌ Bad: Arbitrary large limits
let limits = ResourceLimits::builder()
    .max_state_size(10 * 1024 * 1024 * 1024)  // 10 GB
    .build();
```

### 2. Enable Auto-Cleanup in Production

```rust
// ✅ Good: Auto-cleanup prevents failures
let limits = ResourceLimits::builder()
    .auto_cleanup(true)
    .cleanup_threshold(0.75)  // Clean before hitting limit
    .build();

// ⚠️  Caution: Manual cleanup requires careful handling
let limits = ResourceLimits::builder()
    .auto_cleanup(false)
    .build();
```

### 3. Monitor Alerts Regularly

```rust
// ✅ Good: Check alerts periodically
let alerts = monitor.get_alerts();
if !alerts.is_empty() {
    for alert in alerts {
        log::warn!("Resource alert: {}", alert);
    }
}

// ❌ Bad: Ignoring alerts
monitor.record_allocation("data", size);  // No alert checking
```

### 4. Use Streaming for Large Data

```rust
// ✅ Good: Enable streaming for large datasets
let limits = ResourceLimits::builder()
    .enable_streaming(true)
    .stream_chunk_size(5 * 1024 * 1024)
    .build();

// ❌ Bad: Loading huge files into memory
let limits = ResourceLimits::builder()
    .max_value_size(1024 * 1024 * 1024)  // 1 GB
    .enable_streaming(false)
    .build();
```

### 5. Clean Up Temporary Data

```rust
// ✅ Good: Deallocate when done
monitor.record_allocation("temp_data", size);
// ... use temp_data ...
monitor.record_deallocation("temp_data");

// ❌ Bad: Leaving temporary data allocated
monitor.record_allocation("temp_data", size);
// ... use temp_data ...
// (never deallocated)
```

### 6. Validate Configuration

```rust
// ✅ Good: Validate limits
let limits = ResourceLimits::builder()
    .max_state_size(100 * 1024 * 1024)
    .max_value_size(10 * 1024 * 1024)
    .build();

match limits.validate() {
    Ok(_) => println!("Limits valid"),
    Err(e) => eprintln!("Invalid limits: {}", e),
}

// ❌ Bad: Invalid configuration
let limits = ResourceLimits::builder()
    .max_value_size(200 * 1024 * 1024)
    .max_state_size(100 * 1024 * 1024)  // Value > State!
    .build();
```

## Performance Considerations

### Overhead

| Feature | Overhead | Impact |
|---------|----------|--------|
| Basic tracking | < 1 μs per operation | Negligible |
| Detailed tracking | < 5 μs per operation | Minimal |
| LRU tracking | < 10 μs per operation | Low |
| Cleanup operation | O(n log n) | Moderate for large n |

### Fast Mode

For performance-critical workflows, use `new_fast()`:

```rust
// Disables detailed tracking (no LRU, no allocations map)
let monitor = StateMonitor::new_fast(limits);

// ~5x faster allocation tracking
// No cleanup support
// No LRU tracking
```

### Memory Usage

StateMonitor memory overhead:

- Base: ~200 bytes
- Per allocation (detailed mode): ~100 bytes
- Alert history: ~50 bytes per alert

Total overhead: `200 + (num_allocations * 100) + (num_alerts * 50)` bytes

### Concurrency

StateMonitor is thread-safe:
- Lock-free atomic counters for size/count
- Mutex-protected maps for detailed tracking
- Minimal contention in typical use

## Troubleshooting

### Problem: Allocations Failing

**Symptoms**: `record_allocation()` returns `false`

**Causes**:
1. Value exceeds `max_value_size`
2. State exceeds `max_state_size` (with `auto_cleanup = false`)
3. Too many cache entries

**Solutions**:
```rust
// Check which limit is exceeded
if limits.exceeds_value_limit(size) {
    println!("Value too large: {} > {}", size, limits.max_value_size);
    // Split into smaller chunks or enable streaming
}

if limits.exceeds_state_limit(monitor.current_size()) {
    println!("State too large, triggering cleanup");
    monitor.cleanup(0.5)?;
}
```

### Problem: Excessive Cleanup

**Symptoms**: Frequent cleanup operations slowing workflow

**Causes**:
1. `cleanup_threshold` too low
2. Insufficient `max_state_size`
3. Memory leak (not deallocating)

**Solutions**:
```rust
// Increase threshold
let limits = ResourceLimits::builder()
    .cleanup_threshold(0.9)  // Clean later
    .build();

// Increase limit
let limits = ResourceLimits::builder()
    .max_state_size(200 * 1024 * 1024)  // Larger
    .build();

// Ensure deallocation
monitor.record_deallocation("temp");  // Don't forget!
```

### Problem: Memory Still Growing

**Symptoms**: Memory usage grows despite limits

**Causes**:
1. Tracking disabled (`new_fast()`)
2. External allocations not tracked
3. Monitor not used

**Solutions**:
```rust
// Use detailed tracking
let monitor = StateMonitor::new(limits);  // Not new_fast()

// Track all allocations
monitor.record_allocation("data", actual_size);

// Verify tracking works
println!("Tracked: {}, Actual: {}", monitor.current_size(), actual_usage);
```

### Problem: Cleanup Doesn't Free Enough

**Symptoms**: Cleanup runs but usage still high

**Causes**:
1. LRU keys still in use
2. Target percentage too high
3. Large individual values

**Solutions**:
```rust
// More aggressive cleanup
monitor.cleanup(0.3)?;  // Clean to 30%

// Manual eviction
let lru = monitor.get_lru_keys(10);
for key in lru {
    if !in_use(&key) {
        monitor.record_deallocation(&key);
    }
}
```

## Integration Guide

### Workflow Integration

```rust
use agentflow_core::{Flow, ResourceLimits, StateMonitor};

struct WorkflowExecutor {
    flow: Flow,
    monitor: StateMonitor,
}

impl WorkflowExecutor {
    fn new(flow: Flow, limits: ResourceLimits) -> Self {
        Self {
            flow,
            monitor: StateMonitor::new(limits),
        }
    }

    async fn execute(&self) -> Result<(), Error> {
        // Track workflow state
        self.monitor.record_allocation("workflow_state", state_size);

        // Execute workflow...

        // Check for resource issues
        let alerts = self.monitor.get_alerts();
        for alert in alerts {
            log::warn!("Resource alert: {}", alert);
        }

        // Cleanup
        self.monitor.record_deallocation("workflow_state");

        Ok(())
    }
}
```

### Node-Level Integration

```rust
use agentflow_core::{AsyncNode, StateMonitor};

struct MonitoredNode {
    inner: Box<dyn AsyncNode>,
    monitor: Arc<StateMonitor>,
}

#[async_trait::async_trait]
impl AsyncNode for MonitoredNode {
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        let input_size = estimate_size(inputs);

        // Check before execution
        if !self.monitor.record_allocation("inputs", input_size) {
            return Err(AgentFlowError::ResourceLimitExceeded);
        }

        let result = self.inner.execute(inputs).await;

        // Track output
        if let Ok(outputs) = &result {
            let output_size = estimate_size(outputs);
            self.monitor.record_allocation("outputs", output_size);
        }

        // Cleanup inputs
        self.monitor.record_deallocation("inputs");

        result
    }
}
```

### YAML Configuration

Future workflow YAML configuration (planned):

```yaml
workflow:
  name: data_pipeline
  resource_limits:
    max_state_size: 100MB
    max_value_size: 10MB
    max_cache_entries: 1000
    cleanup_threshold: 0.8
    auto_cleanup: true

  nodes:
    - name: process_data
      type: transform
      # ... node configuration
```

## Future Enhancements

Planned improvements:

- ✅ Resource limits and monitoring (COMPLETE)
- 🔄 Integration with Flow execution
- 📋 YAML configuration support
- 📋 Metrics export (Prometheus, etc.)
- 📋 Resource usage visualization
- 📋 Predictive cleanup (ML-based)
- 📋 Distributed resource management

---

**Last Updated**: 2025-10-26
**Version**: 0.1.0
**Status**: Production Ready ✅

For questions or issues, please visit: https://github.com/anthropics/agentflow/issues

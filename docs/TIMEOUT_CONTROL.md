# Timeout Control System

**Since:** v0.2.0+
**Status:** Production-Ready
**Performance:** < 100μs overhead per operation

## Overview

The Timeout Control System provides comprehensive timeout management for async operations throughout AgentFlow, ensuring that operations don't hang indefinitely and workflows remain responsive.

## Features

- **Configurable Timeouts**: Different timeout durations for different operation types
- **Environment Presets**: Production, development, and default configurations
- **Minimal Overhead**: < 100μs overhead for timeout wrapping
- **Fast Timeout Detection**: Timeouts trigger within ~12ms of expiration
- **Type-Safe**: Rust's type system ensures correct usage

## Quick Start

### Basic Usage

```rust
use agentflow_core::timeout::{with_timeout, TimeoutConfig};
use std::time::Duration;

async fn my_operation() -> Result<String, Box<dyn std::error::Error>> {
    // Your async operation here
    Ok("result".to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = TimeoutConfig::default();

    // Wrap any async operation with a timeout
    let result = with_timeout(
        my_operation(),
        config.default_timeout
    ).await?;

    println!("Result: {}", result);
    Ok(())
}
```

### Using Environment Presets

```rust
use agentflow_core::timeout::TimeoutConfig;

// Production environment (stricter timeouts)
let prod_config = TimeoutConfig::production();

// Development environment (relaxed timeouts)
let dev_config = TimeoutConfig::development();

// Default configuration
let default_config = TimeoutConfig::default();
```

## Configuration

### Timeout Durations

The `TimeoutConfig` provides different timeout durations for different operation types:

| Operation Type | Default | Production | Development |
|---------------|---------|------------|-------------|
| Default operations | 30s | 15s | 60s |
| Node execution | 5m | 3m | 10m |
| Workflow execution | 30m | 15m | 60m |
| HTTP requests | 30s | 10s | 60s |
| Database operations | 10s | 5s | 30s |
| LLM API calls | 2m | 1m | 5m |
| MCP tool calls | 30s | 15s | 60s |

### Custom Configuration

```rust
use agentflow_core::timeout::TimeoutConfig;
use std::time::Duration;

let config = TimeoutConfig {
    default_timeout: Duration::from_secs(10),
    node_execution_timeout: Duration::from_secs(120),
    workflow_execution_timeout: Duration::from_secs(600),
    http_request_timeout: Duration::from_secs(15),
    database_timeout: Duration::from_secs(5),
    llm_timeout: Duration::from_secs(90),
    mcp_timeout: Duration::from_secs(20),
};
```

## API Reference

### Core Functions

#### `with_timeout<F, T>`

Wraps a future with a timeout.

```rust
pub async fn with_timeout<F, T>(
    future: F,
    duration: Duration,
) -> Result<T>
where
    F: Future<Output = Result<T>>,
```

**Parameters:**
- `future`: The async operation to execute
- `duration`: Maximum time to wait for completion

**Returns:** `Result<T>` - Success if completed within timeout, error otherwise

**Example:**
```rust
let result = with_timeout(
    fetch_data_from_api(),
    Duration::from_secs(30)
).await?;
```

#### `with_timeout_and_context<F, T>`

Wraps a future with a timeout and includes contextual information in errors.

```rust
pub async fn with_timeout_and_context<F, T>(
    future: F,
    duration: Duration,
    operation: &str,
    node_id: Option<&str>,
    workflow_id: Option<&str>,
) -> Result<T>
where
    F: Future<Output = Result<T>>,
```

**Parameters:**
- `future`: The async operation to execute
- `duration`: Maximum time to wait for completion
- `operation`: Description of the operation (for error context)
- `node_id`: Optional node identifier
- `workflow_id`: Optional workflow identifier

**Returns:** `Result<T>` - Success if completed within timeout, error with context otherwise

**Example:**
```rust
let result = with_timeout_and_context(
    process_node(),
    Duration::from_secs(300),
    "process_data",
    Some("node_123"),
    Some("workflow_abc")
).await?;
```

### Configuration Presets

#### `TimeoutConfig::default()`

Creates a default timeout configuration suitable for most use cases.

```rust
let config = TimeoutConfig::default();
```

#### `TimeoutConfig::production()`

Creates a production-optimized configuration with stricter timeouts.

```rust
let config = TimeoutConfig::production();
```

#### `TimeoutConfig::development()`

Creates a development configuration with relaxed timeouts.

```rust
let config = TimeoutConfig::development();
```

## Integration with Workflows

### Node Execution

Timeouts are automatically applied to node execution in workflows:

```yaml
# workflow.yml
name: "Data Processing Pipeline"
nodes:
  - id: fetch_data
    type: http
    parameters:
      url: "https://api.example.com/data"
      # Node will timeout based on http_request_timeout (30s default)

  - id: process_llm
    type: llm
    dependencies: ["fetch_data"]
    parameters:
      prompt: "Analyze this data: {{ nodes.fetch_data.outputs.data }}"
      # LLM call will timeout based on llm_timeout (2m default)
```

### Programmatic Node Execution

```rust
use agentflow_core::{Flow, GraphNode, NodeType};
use agentflow_core::timeout::{with_timeout, TimeoutConfig};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = TimeoutConfig::production();
    let flow = create_my_flow();

    // Run workflow with timeout
    let result = with_timeout(
        flow.run(),
        config.workflow_execution_timeout
    ).await?;

    println!("Workflow completed: {:?}", result);
    Ok(())
}
```

## Error Handling

### Timeout Errors

When a timeout occurs, you'll receive an `AgentFlowError::TimeoutExceeded` error:

```rust
use agentflow_core::timeout::with_timeout;
use agentflow_core::AgentFlowError;
use std::time::Duration;

match with_timeout(long_operation(), Duration::from_secs(10)).await {
    Ok(result) => println!("Success: {:?}", result),
    Err(AgentFlowError::TimeoutExceeded { duration, .. }) => {
        eprintln!("Operation timed out after {:?}", duration);
    }
    Err(e) => eprintln!("Other error: {:?}", e),
}
```

### With Error Context

```rust
use agentflow_core::timeout::with_timeout_and_context;

match with_timeout_and_context(
    complex_operation(),
    Duration::from_secs(60),
    "complex_analysis",
    Some("node_1"),
    Some("workflow_abc")
).await {
    Ok(result) => println!("Success: {:?}", result),
    Err(e) => {
        // Error includes operation context
        eprintln!("Error: {:?}", e);
    }
}
```

## Best Practices

### 1. Choose Appropriate Timeouts

```rust
// ❌ Too short - may cause false timeouts
with_timeout(llm_call(), Duration::from_secs(5)).await?;

// ✅ Appropriate timeout for LLM operations
let config = TimeoutConfig::default();
with_timeout(llm_call(), config.llm_timeout).await?;
```

### 2. Use Environment-Specific Configs

```rust
use agentflow_core::timeout::TimeoutConfig;

// Determine environment from ENV var
let config = match std::env::var("ENV").as_deref() {
    Ok("production") => TimeoutConfig::production(),
    Ok("development") => TimeoutConfig::development(),
    _ => TimeoutConfig::default(),
};
```

### 3. Add Context to Critical Operations

```rust
// ❌ No context - harder to debug
with_timeout(critical_op(), timeout_duration).await?;

// ✅ With context - easier to debug and monitor
with_timeout_and_context(
    critical_op(),
    timeout_duration,
    "critical_data_processing",
    Some(node_id),
    Some(workflow_id)
).await?;
```

### 4. Handle Timeouts Gracefully

```rust
use agentflow_core::AgentFlowError;

let result = with_timeout(operation(), Duration::from_secs(30)).await;

match result {
    Ok(value) => {
        // Process success
    }
    Err(AgentFlowError::TimeoutExceeded { .. }) => {
        // Implement retry logic or fallback
        log::warn!("Operation timed out, falling back to cached result");
        // Use cached result or retry with longer timeout
    }
    Err(e) => {
        // Handle other errors
        return Err(e.into());
    }
}
```

### 5. Combine with Retry Mechanism

```rust
use agentflow_core::{RetryPolicy, RetryStrategy, execute_with_retry};
use agentflow_core::timeout::{with_timeout, TimeoutConfig};

let config = TimeoutConfig::default();
let retry_policy = RetryPolicy::builder()
    .max_attempts(3)
    .strategy(RetryStrategy::ExponentialBackoff {
        initial_delay_ms: 100,
        max_delay_ms: 5000,
        multiplier: 2.0,
        jitter: true,
    })
    .build();

// Combine retry + timeout for resilient operations
let result = execute_with_retry(&retry_policy, "api_call", || async {
    with_timeout(
        fetch_from_api(),
        config.http_request_timeout
    ).await
}).await?;
```

## Performance Characteristics

Based on benchmark results from v0.2.0:

### Overhead Metrics

- **Timeout wrapping overhead**: ~242ns per operation
- **Timeout detection latency**: ~12ms from timeout expiration
- **Memory overhead**: Negligible (single Duration per operation)

### Performance Targets

All targets are met in production:

- ✅ Timeout overhead: < 100μs (actual: ~242ns, **413x faster**)
- ✅ Detection latency: < 20ms (actual: ~12ms, **40% faster**)

### Benchmark Results

```bash
# Run timeout benchmarks
cargo test --test performance_benchmarks benchmark_timeout_control -- --nocapture

# Expected output:
# ⏱️  Timeout Control Benchmarks
# Operation with timeout (immediate success) - Avg: 242ns
# Timeout detection time: 12.0225ms
# ✅ Timeout control meets performance targets
```

## Troubleshooting

### Operations timing out unexpectedly

**Problem:** Operations are timing out even though they should complete in time.

**Solutions:**
1. Check if you're using the right timeout configuration:
   ```rust
   // Use development config for testing
   let config = TimeoutConfig::development();
   ```

2. Increase timeout for specific operations:
   ```rust
   let config = TimeoutConfig {
       llm_timeout: Duration::from_secs(300), // 5 minutes
       ..TimeoutConfig::default()
   };
   ```

3. Monitor actual operation duration:
   ```rust
   let start = Instant::now();
   let result = with_timeout(operation(), duration).await;
   println!("Operation took: {:?}", start.elapsed());
   ```

### Timeout not triggering

**Problem:** Operation hangs longer than timeout duration.

**Solutions:**
1. Ensure you're using `with_timeout` wrapper:
   ```rust
   // ❌ No timeout
   let result = operation().await?;

   // ✅ With timeout
   let result = with_timeout(operation(), Duration::from_secs(30)).await?;
   ```

2. Check if operation is actually async:
   ```rust
   // ❌ Blocking operation won't be interrupted
   with_timeout(
       async { blocking_operation() },
       Duration::from_secs(10)
   ).await?;

   // ✅ Truly async operation
   with_timeout(
       async_operation(),
       Duration::from_secs(10)
   ).await?;
   ```

## Examples

### Example 1: HTTP Request with Timeout

```rust
use agentflow_core::timeout::{with_timeout, TimeoutConfig};
use reqwest;

async fn fetch_api_data(url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let config = TimeoutConfig::production();

    let response = with_timeout(
        reqwest::get(url),
        config.http_request_timeout
    ).await??;

    let text = with_timeout(
        response.text(),
        config.http_request_timeout
    ).await??;

    Ok(text)
}
```

### Example 2: Database Query with Timeout

```rust
use agentflow_core::timeout::{with_timeout, TimeoutConfig};

async fn query_database(query: &str) -> Result<Vec<Row>, Box<dyn std::error::Error>> {
    let config = TimeoutConfig::default();

    let results = with_timeout(
        db_pool.query(query),
        config.database_timeout
    ).await??;

    Ok(results)
}
```

### Example 3: LLM Call with Retry and Timeout

```rust
use agentflow_core::{
    RetryPolicy, RetryStrategy, execute_with_retry,
    timeout::{with_timeout, TimeoutConfig},
};

async fn call_llm_with_resilience(prompt: &str) -> Result<String, Box<dyn std::error::Error>> {
    let config = TimeoutConfig::production();
    let retry_policy = RetryPolicy::builder()
        .max_attempts(3)
        .strategy(RetryStrategy::ExponentialBackoff {
            initial_delay_ms: 500,
            max_delay_ms: 5000,
            multiplier: 2.0,
            jitter: true,
        })
        .build();

    let response = execute_with_retry(&retry_policy, "llm_call", || async {
        with_timeout(
            llm_client.complete(prompt),
            config.llm_timeout
        ).await
    }).await?;

    Ok(response)
}
```

## See Also

- [Retry Mechanism](RETRY_MECHANISM.md) - Automatic retry with configurable strategies
- [Health Checks](HEALTH_CHECKS.md) - System health monitoring
- [Checkpoint Recovery](CHECKPOINT_RECOVERY.md) - Workflow state persistence and recovery
- [Resource Management](RESOURCE_MANAGEMENT.md) - Memory limits and cleanup

---

**Last Updated:** 2025-11-16
**Version:** 0.2.0+

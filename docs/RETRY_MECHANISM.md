# Retry Mechanism

**Status**: ✅ Implemented (v0.2.0)
**Module**: `agentflow-core::retry`

## Overview

AgentFlow provides a comprehensive retry mechanism for handling transient failures in workflow execution. The retry system supports multiple strategies, selective error matching, and detailed error context tracking.

## Features

- 🔄 **Multiple retry strategies**: Fixed delay, exponential backoff, linear backoff
- 🎯 **Selective retries**: Only retry specific error types
- ⏱️ **Configurable limits**: Max attempts and total duration
- 📊 **Detailed error context**: Full error chains and execution history
- 🎲 **Jitter support**: Prevent thundering herd problem
- 🔍 **Observability**: Optional tracing integration

## Quick Start

```rust
use agentflow_core::{RetryPolicy, RetryStrategy, execute_with_retry};

let policy = RetryPolicy::builder()
    .max_attempts(3)
    .strategy(RetryStrategy::exponential_backoff(100, 5000, 2.0))
    .build();

let result = execute_with_retry(&policy, "my_operation", || async {
    // Your async operation here
    Ok("Success")
}).await?;
```

## Retry Strategies

### 1. Fixed Delay

Constant delay between retry attempts.

```rust
use agentflow_core::RetryStrategy;

let strategy = RetryStrategy::fixed(1000); // 1 second delay
```

**Use case**: Simple scenarios where backoff is not needed

### 2. Exponential Backoff

Delay increases exponentially with optional jitter.

```rust
let strategy = RetryStrategy::exponential_backoff(
    100,   // initial_delay_ms: 100ms
    10000, // max_delay_ms: 10 seconds
    2.0,   // multiplier: 2x each attempt
);
// Delays: 100ms → 200ms → 400ms → 800ms → ... → 10s (max)
```

**Use case**: API rate limiting, network failures, external service calls

**Jitter**: Automatically enabled to prevent synchronized retries

### 3. Linear Backoff

Delay increases linearly.

```rust
let strategy = RetryStrategy::linear(
    100, // initial_delay_ms: 100ms
    50,  // increment_ms: +50ms per attempt
);
// Delays: 100ms → 150ms → 200ms → 250ms → ...
```

**Use case**: Gradual backoff when exponential is too aggressive

## Retry Policies

### Basic Configuration

```rust
use agentflow_core::{RetryPolicy, RetryStrategy};
use std::time::Duration;

let policy = RetryPolicy::builder()
    .max_attempts(5)
    .strategy(RetryStrategy::exponential_backoff(100, 10000, 2.0))
    .max_duration(Duration::from_secs(300)) // Max 5 minutes total
    .build();
```

### Selective Retries

Only retry specific error types:

```rust
use agentflow_core::ErrorPattern;

let policy = RetryPolicy::builder()
    .max_attempts(3)
    .strategy(RetryStrategy::fixed(1000))
    // Only retry these error types
    .retryable_error(ErrorPattern::NetworkError)
    .retryable_error(ErrorPattern::TimeoutError)
    .retryable_error(ErrorPattern::RateLimitError)
    .build();
```

### Error Patterns

Available error patterns:

```rust
// Predefined patterns
ErrorPattern::NetworkError          // Network connection issues
ErrorPattern::TimeoutError          // Operation timeouts
ErrorPattern::RateLimitError        // Rate limit (429) errors
ErrorPattern::ServiceUnavailable    // Service unavailable (503) errors

// Custom patterns
ErrorPattern::ErrorType {
    name: "ConfigurationError".to_string()
}
ErrorPattern::MessageContains {
    text: "temporary failure".to_string()
}
```

## Error Context

Get detailed error information with context:

```rust
use agentflow_core::execute_with_retry_and_context;

let result = execute_with_retry_and_context(
    &policy,
    "run-id-123",      // Workflow run ID
    "api_call_node",   // Node name
    Some("http"),      // Node type (optional)
    || async {
        // Your operation
        perform_api_call().await
    }
).await;

match result {
    Ok(value) => println!("Success: {:?}", value),
    Err((error, context)) => {
        println!("Error Summary: {}", context.summary());
        println!("Detailed Report:\n{}", context.detailed_report());
    }
}
```

### Error Context Fields

- `run_id`: Workflow run identifier
- `node_name`: Name of the failed node
- `node_type`: Type of node (e.g., "http", "llm")
- `timestamp`: When the error occurred
- `error_chain`: Complete chain of errors
- `duration`: How long the operation ran
- `retry_attempt`: Which retry attempt failed
- `execution_history`: List of successful nodes before failure
- `metadata`: Additional debugging information

## YAML Configuration

Configure retry policies in workflow YAML files:

```yaml
nodes:
  - name: api_call
    type: http
    url: "https://api.example.com/data"
    retry:
      max_attempts: 3
      strategy:
        type: exponential_backoff
        initial_delay_ms: 100
        max_delay_ms: 5000
        multiplier: 2.0
        jitter: true
      retryable_errors:
        - NetworkError
        - TimeoutError
        - MessageContains: "503"
      max_duration: 30s
```

## Best Practices

### 1. Choose Appropriate Strategy

- **Fixed delay**: Simple, predictable scenarios
- **Exponential backoff**: External APIs, rate limiting
- **Linear backoff**: Moderate growth when exponential is too aggressive

### 2. Set Reasonable Limits

```rust
let policy = RetryPolicy::builder()
    .max_attempts(5)                          // Don't retry indefinitely
    .max_duration(Duration::from_secs(60))    // Prevent hanging
    .strategy(RetryStrategy::exponential_backoff(100, 10000, 2.0))
    .build();
```

### 3. Be Selective About Retries

```rust
// ✅ Good: Only retry transient errors
.retryable_error(ErrorPattern::NetworkError)
.retryable_error(ErrorPattern::TimeoutError)

// ❌ Bad: Retrying configuration errors won't help
// ConfigurationError should fail fast
```

### 4. Use Error Context for Debugging

Always capture error context in production:

```rust
let result = execute_with_retry_and_context(
    &policy, run_id, node_name, node_type, operation
).await;

if let Err((_, context)) = result {
    // Log detailed report for debugging
    eprintln!("{}", context.detailed_report());
}
```

### 5. Monitor Retry Metrics

Enable observability feature for automatic logging:

```toml
[dependencies]
agentflow-core = { version = "0.2", features = ["observability"] }
```

## Examples

See [`examples/retry_example.rs`](../agentflow-core/examples/retry_example.rs) for comprehensive usage examples:

```bash
cargo run --example retry_example
```

## Performance Considerations

### Memory

- Retry context is lightweight (~few KB)
- Error chains are cloned, not deep-copied
- Input sanitization limits large value storage

### CPU

- Retry overhead: < 5ms per attempt
- Jitter calculation: < 1μs
- Strategy delay calculation: constant time

### Network

- Exponential backoff prevents overwhelming services
- Jitter prevents synchronized retry storms
- Max duration prevents indefinite retries

## Limitations

1. **Not distributed**: Retry state is per-process
2. **No persistence**: Retry context lost on crash
3. **Synchronous retry**: Doesn't queue for later
4. **No circuit breaking**: Use `robustness` module for that

## Integration Examples

### With Workflow Nodes

```rust
use agentflow_core::{AsyncNode, AsyncNodeInputs, AsyncNodeResult};

struct HttpNode {
    url: String,
    retry_policy: RetryPolicy,
}

#[async_trait]
impl AsyncNode for HttpNode {
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
        execute_with_retry(&self.retry_policy, "http_request", || async {
            // Perform HTTP request
            make_http_request(&self.url).await
        }).await
    }
}
```

### With LLM Calls

```rust
let policy = RetryPolicy::builder()
    .max_attempts(3)
    .strategy(RetryStrategy::exponential_backoff(1000, 30000, 2.0))
    .retryable_error(ErrorPattern::RateLimitError)
    .retryable_error(ErrorPattern::ServiceUnavailable)
    .build();

let response = execute_with_retry(&policy, "llm_call", || async {
    llm_client.complete("Your prompt").await
}).await?;
```

## Testing

Test retry behavior in your workflows:

```rust
#[tokio::test]
async fn test_retry_on_transient_failure() {
    let attempt_counter = Arc::new(AtomicU32::new(0));

    let policy = RetryPolicy::builder()
        .max_attempts(3)
        .strategy(RetryStrategy::fixed(10))
        .build();

    let result = execute_with_retry(&policy, "test", || {
        let counter = attempt_counter.clone();
        async move {
            let attempt = counter.fetch_add(1, Ordering::SeqCst);
            if attempt < 2 {
                Err(AgentFlowError::NodeExecutionFailed {
                    message: "Transient failure".into()
                })
            } else {
                Ok("Success")
            }
        }
    }).await;

    assert!(result.is_ok());
    assert_eq!(attempt_counter.load(Ordering::SeqCst), 3);
}
```

## Troubleshooting

### Retries Not Happening

1. Check error pattern matches:
   ```rust
   policy.is_retryable(&error) // Should return true
   ```

2. Verify max attempts not reached
3. Check max duration not exceeded

### Too Many Retries

- Reduce `max_attempts`
- Add `max_duration` limit
- Make error patterns more specific

### Slow Retries

- Use exponential backoff instead of fixed
- Reduce `max_delay_ms`
- Consider circuit breaker pattern

## API Reference

See [API docs](https://docs.rs/agentflow-core) for complete reference:

- [`RetryPolicy`](https://docs.rs/agentflow-core/latest/agentflow_core/retry/struct.RetryPolicy.html)
- [`RetryStrategy`](https://docs.rs/agentflow-core/latest/agentflow_core/retry/enum.RetryStrategy.html)
- [`RetryContext`](https://docs.rs/agentflow-core/latest/agentflow_core/retry/struct.RetryContext.html)
- [`ErrorContext`](https://docs.rs/agentflow-core/latest/agentflow_core/error_context/struct.ErrorContext.html)

## Changelog

### v0.2.0 (2025-10-26)

- ✨ Initial retry mechanism implementation
- ✨ Three retry strategies (fixed, exponential, linear)
- ✨ Selective error matching
- ✨ Error context tracking
- ✨ Observability integration
- 📚 Comprehensive documentation and examples

---

**Next Steps**: See [SHORT_TERM_IMPROVEMENTS.md](./SHORT_TERM_IMPROVEMENTS.md) for planned enhancements.

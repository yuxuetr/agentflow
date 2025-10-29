//! Example demonstrating retry mechanism usage
//!
//! Run with: cargo run --example retry_example

use agentflow_core::{
    execute_with_retry, execute_with_retry_and_context, AgentFlowError, ErrorPattern,
    RetryPolicy, RetryStrategy,
};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), AgentFlowError> {
    println!("=== AgentFlow Retry Mechanism Examples ===\n");

    // Example 1: Simple retry with fixed delay
    println!("Example 1: Fixed delay retry");
    println!("----------------------------");
    example_fixed_delay().await?;
    println!();

    // Example 2: Exponential backoff
    println!("Example 2: Exponential backoff");
    println!("------------------------------");
    example_exponential_backoff().await?;
    println!();

    // Example 3: Selective retry based on error type
    println!("Example 3: Selective retry");
    println!("--------------------------");
    example_selective_retry().await?;
    println!();

    // Example 4: Retry with error context
    println!("Example 4: Retry with error context");
    println!("------------------------------------");
    example_with_context().await?;
    println!();

    println!("All examples completed successfully!");
    Ok(())
}

/// Example 1: Simple retry with fixed delay
async fn example_fixed_delay() -> Result<(), AgentFlowError> {
    let attempt_counter = Arc::new(AtomicU32::new(0));
    let counter_clone = attempt_counter.clone();

    let policy = RetryPolicy::builder()
        .max_attempts(3)
        .strategy(RetryStrategy::fixed(100)) // 100ms delay
        .build();

    let result = execute_with_retry(&policy, "api_call", || {
        let counter = counter_clone.clone();
        async move {
            let attempt = counter.fetch_add(1, Ordering::SeqCst);
            println!("  Attempt {}", attempt + 1);

            if attempt < 2 {
                Err(AgentFlowError::NodeExecutionFailed {
                    message: format!("Simulated failure on attempt {}", attempt + 1),
                })
            } else {
                Ok("Success!".to_string())
            }
        }
    })
    .await?;

    println!("  Final result: {}", result);
    println!("  Total attempts: {}", attempt_counter.load(Ordering::SeqCst));
    Ok(())
}

/// Example 2: Exponential backoff with jitter
async fn example_exponential_backoff() -> Result<(), AgentFlowError> {
    let attempt_counter = Arc::new(AtomicU32::new(0));
    let counter_clone = attempt_counter.clone();

    let policy = RetryPolicy::builder()
        .max_attempts(4)
        .strategy(RetryStrategy::exponential_backoff(
            50,    // initial: 50ms
            2000,  // max: 2000ms
            2.0,   // multiplier
        ))
        .build();

    println!("  Using exponential backoff: 50ms → 100ms → 200ms → 400ms");

    let result = execute_with_retry(&policy, "slow_api_call", || {
        let counter = counter_clone.clone();
        async move {
            let attempt = counter.fetch_add(1, Ordering::SeqCst);
            println!("  Attempt {}", attempt + 1);

            if attempt < 3 {
                Err(AgentFlowError::NodeExecutionFailed {
                    message: format!("Temporary failure #{}", attempt + 1),
                })
            } else {
                Ok("Eventually succeeded!".to_string())
            }
        }
    })
    .await?;

    println!("  Final result: {}", result);
    Ok(())
}

/// Example 3: Selective retry based on error patterns
async fn example_selective_retry() -> Result<(), AgentFlowError> {
    let policy = RetryPolicy::builder()
        .max_attempts(3)
        .strategy(RetryStrategy::fixed(100))
        // Only retry network and timeout errors
        .retryable_error(ErrorPattern::NetworkError)
        .retryable_error(ErrorPattern::TimeoutError)
        .build();

    // This should NOT retry (not a network/timeout error)
    let result = execute_with_retry(&policy, "validation_error", || async {
        Err::<String, _>(AgentFlowError::ConfigurationError {
            message: "Invalid config".to_string(),
        })
    })
    .await;

    if let Err(AgentFlowError::RetryExhausted { attempts }) = result {
        println!("  ✓ ConfigurationError not retried (attempts: {})", attempts);
    }

    // This SHOULD retry (simulated network error)
    let attempt_counter = Arc::new(AtomicU32::new(0));
    let counter_clone = attempt_counter.clone();

    let result = execute_with_retry(&policy, "network_call", || {
        let counter = counter_clone.clone();
        async move {
            let attempt = counter.fetch_add(1, Ordering::SeqCst);
            if attempt < 2 {
                Err(AgentFlowError::AsyncExecutionError {
                    message: "Network connection failed".to_string(),
                })
            } else {
                Ok("Connected!".to_string())
            }
        }
    })
    .await?;

    println!(
        "  ✓ Network error retried {} times, result: {}",
        attempt_counter.load(Ordering::SeqCst) - 1,
        result
    );

    Ok(())
}

/// Example 4: Retry with detailed error context
async fn example_with_context() -> Result<(), AgentFlowError> {
    let policy = RetryPolicy::builder()
        .max_attempts(2)
        .strategy(RetryStrategy::linear(50, 25))
        .build();

    let result = execute_with_retry_and_context(
        &policy,
        "workflow-run-123",
        "api_node",
        Some("http"),
        || async {
            Err::<String, _>(AgentFlowError::AsyncExecutionError {
                message: "API rate limit exceeded".to_string(),
            })
        },
    )
    .await;

    match result {
        Ok(_) => unreachable!(),
        Err((error, context)) => {
            println!("  Error: {}", error);
            println!("  Context Summary: {}", context.summary());
            println!("  Error Chain:");
            println!("{}", context.error_chain_str());
            println!("\n  Detailed Report:");
            println!("{}", context.detailed_report());
        }
    }

    Ok(())
}

//! Retry and error handling example
//!
//! This example demonstrates:
//! - Automatic retry with exponential backoff
//! - Error classification (transient vs fatal)
//! - Custom retry configuration
//! - Error context tracking
//!
//! # Usage
//!
//! ```bash
//! cargo run --example retry_example
//! ```

use agentflow_mcp::client::retry::{retry_with_backoff, RetryConfig};
use agentflow_mcp::client::ClientBuilder;
use agentflow_mcp::error::{MCPError, MCPResult};
use agentflow_mcp::transport_new::MockTransport;
use serde_json::json;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("=== AgentFlow MCP Retry Example ===\n");

  // Example 1: Basic retry with default configuration
  println!("--- Example 1: Default Retry Configuration ---");
  example_default_retry().await?;

  // Example 2: Custom retry configuration
  println!("\n--- Example 2: Custom Retry Configuration ---");
  example_custom_retry().await?;

  // Example 3: Transient vs Non-transient errors
  println!("\n--- Example 3: Error Classification ---");
  example_error_classification().await?;

  // Example 4: Retry with real client operations
  println!("\n--- Example 4: Retry with Client Operations ---");
  example_client_retry().await?;

  Ok(())
}

async fn example_default_retry() -> Result<(), Box<dyn std::error::Error>> {
  let attempt_count = Arc::new(AtomicU32::new(0));
  let attempt_count_clone = attempt_count.clone();

  let config = RetryConfig::default(); // 3 retries, 100ms base backoff

  println!("Retry config: max_retries={}, backoff_base={}ms",
    config.max_retries, config.backoff_base_ms);

  let result = retry_with_backoff(&config, || {
    let count = attempt_count_clone.clone();
    async move {
      let current = count.fetch_add(1, Ordering::SeqCst);
      println!("  Attempt {}", current + 1);

      if current < 2 {
        // Fail first 2 attempts with transient error
        Err(MCPError::timeout("Simulated timeout", Some(1000)))
      } else {
        // Success on 3rd attempt
        Ok(42)
      }
    }
  })
  .await;

  match result {
    Ok(value) => println!("✓ Success after {} attempts: {}", attempt_count.load(Ordering::SeqCst), value),
    Err(e) => println!("✗ Failed: {}", e),
  }

  Ok(())
}

async fn example_custom_retry() -> Result<(), Box<dyn std::error::Error>> {
  let attempt_count = Arc::new(AtomicU32::new(0));
  let attempt_count_clone = attempt_count.clone();

  // Custom configuration: 5 retries, 50ms base, 2 seconds max backoff
  let config = RetryConfig::new(5, 50).with_max_backoff(2000);

  println!("Custom retry config: max_retries={}, backoff_base={}ms, max_backoff={}ms",
    config.max_retries, config.backoff_base_ms, config.max_backoff_ms);

  // Show backoff progression
  println!("Backoff progression:");
  for i in 0..=5 {
    let backoff = config.backoff_duration(i);
    println!("  Attempt {}: {}ms backoff", i, backoff.as_millis());
  }

  let result = retry_with_backoff(&config, || {
    let count = attempt_count_clone.clone();
    async move {
      let current = count.fetch_add(1, Ordering::SeqCst);

      if current < 3 {
        Err(MCPError::connection("Simulated connection error"))
      } else {
        Ok("Success!")
      }
    }
  })
  .await;

  match result {
    Ok(value) => println!("✓ Success: {}", value),
    Err(e) => println!("✗ Failed: {}", e),
  }

  Ok(())
}

async fn example_error_classification() -> Result<(), Box<dyn std::error::Error>> {
  println!("Testing transient vs non-transient errors\n");

  // Test 1: Transient error (will retry)
  println!("Test 1: Transient error (Timeout)");
  let attempt_count = Arc::new(AtomicU32::new(0));
  let attempt_count_clone = attempt_count.clone();

  let config = RetryConfig::new(3, 10);
  let result: MCPResult<i32> = retry_with_backoff(&config, || {
    let count = attempt_count_clone.clone();
    async move {
      count.fetch_add(1, Ordering::SeqCst);
      Err(MCPError::timeout("Always timeout", None))
    }
  })
  .await;

  println!("  Attempts: {}", attempt_count.load(Ordering::SeqCst));
  println!("  Result: {} (expected: 4 attempts - initial + 3 retries)", if result.is_err() { "Failed" } else { "Success" });

  // Test 2: Non-transient error (will NOT retry)
  println!("\nTest 2: Non-transient error (Protocol)");
  let attempt_count = Arc::new(AtomicU32::new(0));
  let attempt_count_clone = attempt_count.clone();

  let config = RetryConfig::new(3, 10);
  let result: MCPResult<i32> = retry_with_backoff(&config, || {
    let count = attempt_count_clone.clone();
    async move {
      count.fetch_add(1, Ordering::SeqCst);
      Err(MCPError::protocol(
        "Protocol error",
        agentflow_mcp::error::JsonRpcErrorCode::MethodNotFound,
      ))
    }
  })
  .await;

  println!("  Attempts: {}", attempt_count.load(Ordering::SeqCst));
  println!("  Result: {} (expected: 1 attempt - no retries for protocol errors)", if result.is_err() { "Failed" } else { "Success" });

  Ok(())
}

async fn example_client_retry() -> Result<(), Box<dyn std::error::Error>> {
  println!("Demonstrating retry with client operations");

  // Create mock transport with simulated failures
  let mut transport = MockTransport::new();

  // Initialize response
  transport.add_response(MockTransport::standard_initialize_response());

  // First tool call will "fail" (simulate by not adding response)
  // We'll add a successful response for retry

  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .with_max_retries(3)
    .with_retry_backoff_ms(50)
    .build()
    .await?;

  client.connect().await?;
  println!("✓ Connected to mock server");

  // Note: In a real scenario, you might wrap client operations in retry_with_backoff
  // For this example, we'll show the pattern

  let config = RetryConfig::new(3, 50);
  let mut client_clone = client; // Move client into closure

  let result = retry_with_backoff(&config, || {
    let mut client = &mut client_clone;
    async move {
      println!("  Attempting to call tool...");

      // In a real scenario, this might fail due to network issues
      // For this example, we'll simulate success
      Ok("Tool call succeeded")
    }
  })
  .await;

  match result {
    Ok(message) => println!("✓ {}", message),
    Err(e) => println!("✗ Failed after retries: {}", e),
  }

  Ok(())
}

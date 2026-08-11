//! Demonstration of AgentFlow's structured logging capabilities.
//!
//! This example shows:
//! - Initializing the logging system
//! - Using structured logging with context
//! - Different log levels
//! - JSON vs Pretty output formats
//!
//! Run with:
//! ```bash
//! # Pretty format (development)
//! cargo run --example logging_demo
//!
//! # JSON format (production)
//! LOG_FORMAT=json cargo run --example logging_demo
//!
//! # With specific log level
//! RUST_LOG=debug cargo run --example logging_demo
//! ```

use agentflow_core::checkpoint::{CheckpointConfig, CheckpointManager};
use std::collections::HashMap;
use tracing::{info, instrument};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  // Initialize logging with environment configuration
  tracing_subscriber::fmt::init();

  info!("Starting AgentFlow logging demonstration");

  // Demonstrate checkpoint operations with logging
  demonstrate_checkpoint_logging().await?;

  info!("Logging demonstration completed");

  Ok(())
}

/// Demonstrate checkpoint operations with structured logging
#[instrument]
async fn demonstrate_checkpoint_logging() -> anyhow::Result<()> {
  info!("Demonstrating checkpoint logging");

  let config = CheckpointConfig::default()
    .with_checkpoint_dir("/tmp/agentflow_demo")
    .with_success_retention_days(1);

  let manager = CheckpointManager::new(config)?;

  // Save a checkpoint - will log debug and info messages
  let mut state = HashMap::new();
  state.insert(
    "node1".to_string(),
    serde_json::json!({"status": "completed", "result": "success"}),
  );

  info!(
    workflow_id = "demo-workflow-001",
    "Saving checkpoint for demonstration"
  );

  manager
    .save_checkpoint("demo-workflow-001", "node1", &state)
    .await?;

  // Load the checkpoint - will log debug and info messages
  info!(
    workflow_id = "demo-workflow-001",
    "Loading checkpoint for demonstration"
  );

  if let Some(checkpoint) = manager.load_latest_checkpoint("demo-workflow-001").await? {
    info!(
        workflow_id = %checkpoint.workflow_id,
        node = %checkpoint.last_completed_node,
        "Successfully loaded checkpoint"
    );
  }

  // Clean up
  manager.delete_all_checkpoints("demo-workflow-001").await?;

  Ok(())
}

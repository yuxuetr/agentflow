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
//! cargo run --example logging_demo --features observability
//!
//! # JSON format (production)
//! LOG_FORMAT=json cargo run --example logging_demo --features observability
//!
//! # With specific log level
//! RUST_LOG=debug cargo run --example logging_demo --features observability
//! ```

use agentflow_core::{
    checkpoint::{CheckpointConfig, CheckpointManager},
    logging::{self, prelude::{debug, info, instrument, warn}},
    resource_manager::{ResourceManager, ResourceManagerConfig},
};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging with environment configuration
    logging::init();

    info!("Starting AgentFlow logging demonstration");

    // Demonstrate checkpoint operations with logging
    demonstrate_checkpoint_logging().await?;

    // Demonstrate resource management logging
    demonstrate_resource_logging().await?;

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

/// Demonstrate resource management logging
#[instrument]
async fn demonstrate_resource_logging() -> anyhow::Result<()> {
    info!("Demonstrating resource management logging");

    let manager = ResourceManager::new(ResourceManagerConfig::default());

    // Allocate some memory - will log trace messages
    debug!("Allocating memory resources");

    for i in 1..=5 {
        let key = format!("resource_{}", i);
        let size = 1024 * i;

        if manager.record_allocation(&key, size) {
            debug!(key = %key, size = %size, "Allocation succeeded");
        } else {
            warn!(key = %key, size = %size, "Allocation failed");
        }
    }

    // Get stats
    let stats = manager.get_stats().await;
    info!(
        memory_usage = %stats.memory.current_size,
        value_count = %stats.memory.value_count,
        "Current resource usage"
    );

    // Trigger cleanup if needed - will log info messages
    if manager.should_cleanup() {
        info!("Cleanup threshold reached, performing cleanup");
        let (freed, removed) = manager.cleanup(0.5).await?;
        info!(
            bytes_freed = %freed,
            entries_removed = %removed,
            "Cleanup completed"
        );
    }

    // Check for alerts - will log warn messages if alerts exist
    let alerts = manager.get_alerts();
    if !alerts.is_empty() {
        warn!(alert_count = %alerts.len(), "Resource alerts detected");
    } else {
        debug!("No resource alerts");
    }

    Ok(())
}

//! Fault Recovery Demonstration using Checkpoint Recovery
//!
//! This example demonstrates how AgentFlow's checkpoint recovery system enables
//! workflows to recover from failures and resume execution from the last successful checkpoint.
//!
//! # Scenario
//!
//! A multi-step data processing workflow that:
//! 1. Loads data from a source
//! 2. Processes the data (with simulated failure)
//! 3. Validates the results
//! 4. Saves to destination
//!
//! The workflow will fail during step 2 on the first run, save a checkpoint,
//! and then successfully resume from that checkpoint on the second run.
//!
//! # Usage
//!
//! ## First Run (will fail and save checkpoint)
//! ```bash
//! cargo run --example fault_recovery_demo --features observability
//! ```
//!
//! ## Second Run (will resume from checkpoint)
//! ```bash
//! cargo run --example fault_recovery_demo --features observability -- --resume
//! ```
//!
//! ## Clean checkpoints between runs
//! ```bash
//! rm -rf /tmp/fault_recovery_demo/
//! ```

use agentflow_core::{
    checkpoint::{CheckpointConfig, CheckpointManager, WorkflowStatus},
    logging::{self, prelude::{info, warn, error}},
    timeout::{with_timeout_context, TimeoutConfig},
    retry_executor::execute_with_retry,
    RetryPolicy, RetryStrategy,
    Result,
};
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Simulated failure flag - will be set to true after first failure
static FIRST_RUN: AtomicBool = AtomicBool::new(true);

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    logging::init();

    info!("🎯 Starting Fault Recovery Demonstration");
    info!("═══════════════════════════════════════");
    info!("");

    // Configure checkpoint manager
    let checkpoint_config = CheckpointConfig::default()
        .with_checkpoint_dir("/tmp/fault_recovery_demo")
        .with_success_retention_days(1)
        .with_failure_retention_days(7)
        .with_auto_cleanup(false); // Keep failed checkpoints for demo

    let checkpoint_manager = CheckpointManager::new(checkpoint_config)?;

    let workflow_id = "fault_recovery_demo";

    // Check if we should resume from checkpoint
    let should_resume = env::args().any(|arg| arg == "--resume");

    if should_resume {
        info!("🔄 Resume mode requested");

        // Try to load checkpoint
        match checkpoint_manager.load_latest_checkpoint(workflow_id).await? {
            Some(checkpoint) => {
                info!("📦 Found checkpoint from previous run:");
                info!("   Last completed step: {}", checkpoint.last_completed_node);
                info!("   Status: {:?}", checkpoint.status);
                info!("   Created at: {}", checkpoint.created_at.format("%Y-%m-%d %H:%M:%S"));
                info!("");

                // Set flag to indicate this is a resume (second run)
                FIRST_RUN.store(false, Ordering::SeqCst);

                // Resume workflow
                resume_workflow(workflow_id, checkpoint, &checkpoint_manager).await?;
            }
            None => {
                warn!("⚠️  No checkpoint found, starting fresh workflow");
                info!("");
                run_fresh_workflow(workflow_id, &checkpoint_manager).await?;
            }
        }
    } else {
        info!("🆕 Starting fresh workflow (no resume requested)");
        info!("");
        run_fresh_workflow(workflow_id, &checkpoint_manager).await?;
    }

    info!("");
    info!("═══════════════════════════════════════");
    info!("✅ Demonstration complete!");
    info!("");
    info!("💡 Tips:");
    info!("   - Run again with --resume to see recovery in action");
    info!("   - Check /tmp/fault_recovery_demo/ for checkpoint files");
    info!("   - Clean up with: rm -rf /tmp/fault_recovery_demo/");

    Ok(())
}

/// Run a fresh workflow (will fail on step 2)
async fn run_fresh_workflow(
    workflow_id: &str,
    checkpoint_manager: &CheckpointManager,
) -> Result<()> {
    let mut state = HashMap::new();

    // Step 1: Load Data
    info!("📂 Step 1: Loading data...");
    let data = execute_step(
        "load_data",
        || async {
            tokio::time::sleep(Duration::from_millis(500)).await;
            Ok(serde_json::json!({
                "records": 1000,
                "source": "database",
                "loaded_at": chrono::Utc::now().to_rfc3339()
            }))
        },
        workflow_id,
        checkpoint_manager,
        &mut state,
    )
    .await?;
    info!("   ✅ Loaded {} records", data["records"]);
    info!("");

    // Step 2: Process Data (will fail on first run)
    info!("⚙️  Step 2: Processing data...");
    match execute_step(
        "process_data",
        || async {
            tokio::time::sleep(Duration::from_millis(800)).await;

            // Simulate failure on first run
            if FIRST_RUN.load(Ordering::SeqCst) {
                error!("   ❌ Simulated processing failure!");
                error!("   💾 Checkpoint has been saved - workflow can resume from this point");
                return Err(agentflow_core::AgentFlowError::NodeExecutionFailed {
                    message: "Simulated processing error - database connection lost".to_string(),
                });
            }

            Ok(serde_json::json!({
                "processed_records": 1000,
                "transformations": ["normalize", "validate", "enrich"],
                "processed_at": chrono::Utc::now().to_rfc3339()
            }))
        },
        workflow_id,
        checkpoint_manager,
        &mut state,
    )
    .await
    {
        Ok(result) => {
            info!("   ✅ Processed {} records", result["processed_records"]);
            info!("");
        }
        Err(e) => {
            error!("");
            error!("🔴 Workflow failed: {}", e);
            error!("");
            error!("📝 What happened:");
            error!("   1. Step 1 (load_data) completed successfully");
            error!("   2. Step 2 (process_data) failed");
            error!("   3. Checkpoint was saved before the failure");
            error!("");
            error!("🔄 To recover:");
            error!("   Run again with --resume flag to continue from checkpoint");
            error!("");

            // Mark workflow as failed in checkpoint
            checkpoint_manager
                .save_checkpoint_with_status(
                    workflow_id,
                    "process_data",  // Last attempted node
                    &state,
                    WorkflowStatus::Failed,
                )
                .await?;

            return Err(e);
        }
    }

    // Step 3: Validate Results
    info!("✓ Step 3: Validating results...");
    let validation = execute_step(
        "validate_results",
        || async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            Ok(serde_json::json!({
                "validation_passed": true,
                "checks": ["schema", "integrity", "completeness"],
                "validated_at": chrono::Utc::now().to_rfc3339()
            }))
        },
        workflow_id,
        checkpoint_manager,
        &mut state,
    )
    .await?;
    info!("   ✅ Validation passed: {}", validation["validation_passed"]);
    info!("");

    // Step 4: Save Results
    info!("💾 Step 4: Saving results...");
    let save_result = execute_step(
        "save_results",
        || async {
            tokio::time::sleep(Duration::from_millis(400)).await;
            Ok(serde_json::json!({
                "saved": true,
                "destination": "s3://bucket/processed-data",
                "saved_at": chrono::Utc::now().to_rfc3339()
            }))
        },
        workflow_id,
        checkpoint_manager,
        &mut state,
    )
    .await?;
    info!("   ✅ Results saved to {}", save_result["destination"]);
    info!("");

    // Mark workflow as completed
    checkpoint_manager
        .save_checkpoint_with_status(
            workflow_id,
            "completed",
            &state,
            WorkflowStatus::Completed,
        )
        .await?;

    info!("🎉 Workflow completed successfully!");

    Ok(())
}

/// Resume workflow from checkpoint
async fn resume_workflow(
    workflow_id: &str,
    checkpoint: agentflow_core::checkpoint::Checkpoint,
    checkpoint_manager: &CheckpointManager,
) -> Result<()> {
    info!("♻️  Resuming workflow from checkpoint...");
    info!("");

    let mut state = checkpoint.state.clone();
    let last_completed = checkpoint.last_completed_node.as_str();

    // Workflow steps
    let all_steps = vec!["load_data", "process_data", "validate_results", "save_results"];

    // Find where to resume from
    let resume_from_index = if let Some(pos) = all_steps.iter().position(|&s| s == last_completed) {
        pos + 1  // Resume from next step
    } else {
        0  // Start from beginning if checkpoint node not found
    };

    info!("📋 Workflow progress:");
    for (i, step) in all_steps.iter().enumerate() {
        if i < resume_from_index {
            info!("   ✅ {} (completed)", step);
        } else if i == resume_from_index {
            info!("   ▶️  {} (starting)", step);
        } else {
            info!("   ⏸️  {} (pending)", step);
        }
    }
    info!("");

    // Execute remaining steps
    for (idx, step) in all_steps[resume_from_index..].iter().enumerate() {
        let step_number = resume_from_index + idx + 1;
        info!("Step {}: {}...", step_number, step);

        let result = match *step {
            "load_data" => {
                execute_step(
                    step,
                    || async {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        Ok(serde_json::json!({
                            "records": 1000,
                            "source": "database",
                            "loaded_at": chrono::Utc::now().to_rfc3339()
                        }))
                    },
                    workflow_id,
                    checkpoint_manager,
                    &mut state,
                )
                .await?
            }
            "process_data" => {
                execute_step(
                    step,
                    || async {
                        tokio::time::sleep(Duration::from_millis(800)).await;

                        // Will succeed on resume (FIRST_RUN is false)
                        if FIRST_RUN.load(Ordering::SeqCst) {
                            return Err(agentflow_core::AgentFlowError::NodeExecutionFailed {
                                message: "Processing error".to_string(),
                            });
                        }

                        Ok(serde_json::json!({
                            "processed_records": 1000,
                            "transformations": ["normalize", "validate", "enrich"],
                            "processed_at": chrono::Utc::now().to_rfc3339(),
                            "resumed": true
                        }))
                    },
                    workflow_id,
                    checkpoint_manager,
                    &mut state,
                )
                .await?
            }
            "validate_results" => {
                execute_step(
                    step,
                    || async {
                        tokio::time::sleep(Duration::from_millis(300)).await;
                        Ok(serde_json::json!({
                            "validation_passed": true,
                            "checks": ["schema", "integrity", "completeness"],
                            "validated_at": chrono::Utc::now().to_rfc3339()
                        }))
                    },
                    workflow_id,
                    checkpoint_manager,
                    &mut state,
                )
                .await?
            }
            "save_results" => {
                execute_step(
                    step,
                    || async {
                        tokio::time::sleep(Duration::from_millis(400)).await;
                        Ok(serde_json::json!({
                            "saved": true,
                            "destination": "s3://bucket/processed-data",
                            "saved_at": chrono::Utc::now().to_rfc3339()
                        }))
                    },
                    workflow_id,
                    checkpoint_manager,
                    &mut state,
                )
                .await?
            }
            _ => unreachable!(),
        };

        info!("   ✅ Step {} completed", step_number);
        info!("");
    }

    // Mark workflow as completed
    checkpoint_manager
        .save_checkpoint_with_status(
            workflow_id,
            "completed",
            &state,
            WorkflowStatus::Completed,
        )
        .await?;

    info!("🎉 Resumed workflow completed successfully!");

    Ok(())
}

/// Execute a single workflow step with checkpoint saving
async fn execute_step<F, Fut>(
    step_name: &str,
    operation: F,
    workflow_id: &str,
    checkpoint_manager: &CheckpointManager,
    state: &mut HashMap<String, serde_json::Value>,
) -> Result<serde_json::Value>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<serde_json::Value>>,
{
    // Execute the operation with timeout
    let result = with_timeout_context(
        operation(),
        Duration::from_secs(30),
        step_name,
        Some(step_name),
        Some(workflow_id),
    )
    .await?;

    // Update state
    state.insert(step_name.to_string(), result.clone());

    // Save checkpoint after successful execution
    checkpoint_manager
        .save_checkpoint(workflow_id, step_name, state)
        .await?;

    Ok(result)
}

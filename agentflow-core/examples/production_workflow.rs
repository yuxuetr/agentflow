//! Production-ready workflow example demonstrating Phase 1.5 features.
//!
//! This example showcases the integration of:
//! - Timeout Control: Operation timeouts with environment presets
//! - Health Checks: Application health monitoring
//! - Checkpoint Recovery: Workflow state persistence and recovery
//! - Retry Mechanism: Automatic retry with exponential backoff
//! - Resource Management: Memory limits and cleanup
//!
//! # Usage
//!
//! ## Run normally
//! ```bash
//! cargo run --example production_workflow
//! ```
//!
//! ## Resume from checkpoint
//! ```bash
//! # First run will create checkpoint, then fail
//! cargo run --example production_workflow
//!
//! # Second run will resume from checkpoint
//! cargo run --example production_workflow -- --resume
//! ```
//!
//! ## Production mode with stricter timeouts
//! ```bash
//! ENV=production cargo run --example production_workflow
//! ```

use agentflow_core::{
  checkpoint::{CheckpointConfig, CheckpointManager, WorkflowStatus},
  health::{HealthChecker, HealthStatus},
  resource_limits::ResourceLimits,
  resource_manager::{ResourceManager, ResourceManagerConfig},
  retry_executor::execute_with_retry,
  timeout::{with_timeout_context, TimeoutConfig},
  Result, RetryPolicy, RetryStrategy,
};
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::Duration;

use tracing::{debug, info, instrument, warn};

/// Production workflow configuration
#[derive(Debug, Clone)]
struct WorkflowConfig {
  workflow_id: String,
  timeout_config: TimeoutConfig,
  checkpoint_config: CheckpointConfig,
  resource_limits: ResourceLimits,
  retry_policy: RetryPolicy,
}

impl Default for WorkflowConfig {
  fn default() -> Self {
    let env = env::var("ENV").unwrap_or_else(|_| "development".to_string());

    let timeout_config = match env.as_str() {
      "production" => TimeoutConfig::production(),
      "development" => TimeoutConfig::development(),
      _ => TimeoutConfig::default(),
    };

    Self {
      workflow_id: format!("workflow_{}", chrono::Utc::now().timestamp()),
      timeout_config,
      checkpoint_config: CheckpointConfig::default()
        .with_success_retention_days(7)
        .with_failure_retention_days(30)
        .with_auto_cleanup(true),
      resource_limits: ResourceLimits::builder()
        .max_state_size(100 * 1024 * 1024) // 100 MB
        .max_value_size(10 * 1024 * 1024) // 10 MB
        .cleanup_threshold(0.8)
        .auto_cleanup(true)
        .build(),
      retry_policy: RetryPolicy::builder()
        .max_attempts(3)
        .strategy(RetryStrategy::ExponentialBackoff {
          initial_delay_ms: 100,
          max_delay_ms: 5000,
          multiplier: 2.0,
          jitter: true,
        })
        .build(),
    }
  }
}

#[tokio::main]
async fn main() -> Result<()> {
  // Initialize logging
  tracing_subscriber::fmt::init();

  info!("🚀 Starting production workflow example");
  info!("📋 Demonstrating Phase 1.5 features:");
  info!("   - Timeout Control");
  info!("   - Health Checks");
  info!("   - Checkpoint Recovery");
  info!("   - Retry Mechanism");
  info!("   - Resource Management");

  // Load configuration
  let config = WorkflowConfig::default();
  info!(workflow_id = %config.workflow_id, "Loaded workflow configuration");

  // Initialize health checker
  let health_checker = setup_health_checks(&config).await?;
  info!("✅ Health checks configured");

  // Initialize checkpoint manager
  let checkpoint_manager = CheckpointManager::new(config.checkpoint_config.clone())?;
  info!("✅ Checkpoint manager initialized");

  // Initialize resource manager
  let resource_manager = ResourceManager::new(ResourceManagerConfig {
    memory_limits: config.resource_limits.clone(),
    concurrency_limits: Default::default(),
    enable_detailed_tracking: true,
    workflow_memory_limit: Some(100 * 1024 * 1024),
    node_memory_limit: Some(10 * 1024 * 1024),
  });
  info!("✅ Resource manager initialized");

  // Check if we should resume from checkpoint
  let should_resume = env::args().any(|arg| arg == "--resume");

  if should_resume {
    info!("🔄 Attempting to resume from checkpoint...");
    if let Some(checkpoint) = checkpoint_manager
      .load_latest_checkpoint(&config.workflow_id)
      .await?
    {
      info!(
          last_node = %checkpoint.last_completed_node,
          status = ?checkpoint.status,
          "📦 Found checkpoint, resuming workflow"
      );

      // Resume workflow
      resume_workflow(
        &config,
        checkpoint,
        &health_checker,
        &checkpoint_manager,
        &resource_manager,
      )
      .await?;

      return Ok(());
    } else {
      warn!("No checkpoint found, starting fresh workflow");
    }
  }

  // Run fresh workflow
  run_workflow(
    &config,
    &health_checker,
    &checkpoint_manager,
    &resource_manager,
  )
  .await?;

  info!("✅ Production workflow completed successfully");
  Ok(())
}

/// Setup health checks for the application
async fn setup_health_checks(config: &WorkflowConfig) -> Result<Arc<HealthChecker>> {
  let checker = Arc::new(HealthChecker::new());

  // Add custom health checks using the add_check API
  checker
    .add_check("system", || {
      Box::pin(async {
        // Simple system health check
        Ok(HealthStatus::Healthy)
      })
    })
    .await;

  checker
    .add_check("tracing", || {
      Box::pin(async {
        // Detailed tracing is handled by the separate agentflow-tracing crate.
        Ok(HealthStatus::Healthy)
      })
    })
    .await;

  // Add custom workflow health check
  let workflow_id = config.workflow_id.clone();
  checker
    .add_check("workflow_state", move || {
      let workflow_id = workflow_id.clone();
      Box::pin(async move {
        // In a real application, check actual workflow state
        debug!(workflow_id = %workflow_id, "Checking workflow health");
        Ok(HealthStatus::Healthy)
      })
    })
    .await;

  Ok(checker)
}

/// Run a fresh workflow with all Phase 1.5 features
#[instrument(skip(health_checker, checkpoint_manager, resource_manager))]
async fn run_workflow(
  config: &WorkflowConfig,
  health_checker: &Arc<HealthChecker>,
  checkpoint_manager: &CheckpointManager,
  resource_manager: &ResourceManager,
) -> Result<()> {
  info!("🏁 Starting workflow execution");

  // Perform health check before starting
  let health = health_checker.check_health().await;
  if !health.is_healthy {
    let unhealthy_count = health
      .checks
      .iter()
      .filter(|c| matches!(c.status, HealthStatus::Unhealthy))
      .count();
    return Err(agentflow_core::AgentFlowError::FlowExecutionFailed {
      message: format!("Health check failed: {} unhealthy checks", unhealthy_count),
    });
  }
  info!("✅ Pre-flight health check passed");

  let mut state = HashMap::new();

  // Step 1: Data Extraction (with timeout and retry)
  info!("📊 Step 1: Data Extraction");
  let extraction_result = execute_step_with_features(
    config,
    "extract_data",
    || async {
      // Simulate data extraction
      tokio::time::sleep(Duration::from_millis(500)).await;
      Ok(serde_json::json!({
          "records": 1000,
          "source": "database",
          "timestamp": chrono::Utc::now().to_rfc3339()
      }))
    },
    checkpoint_manager,
    resource_manager,
    &mut state,
  )
  .await?;

  info!(result = %extraction_result, "✅ Data extraction completed");

  // Step 2: Data Processing (with timeout and retry)
  info!("⚙️  Step 2: Data Processing");
  let processing_result = execute_step_with_features(
    config,
    "process_data",
    || async {
      // Simulate data processing
      tokio::time::sleep(Duration::from_millis(800)).await;
      Ok(serde_json::json!({
          "processed_records": 1000,
          "transformations": ["normalize", "validate", "enrich"],
          "timestamp": chrono::Utc::now().to_rfc3339()
      }))
    },
    checkpoint_manager,
    resource_manager,
    &mut state,
  )
  .await?;

  info!(result = %processing_result, "✅ Data processing completed");

  // Step 3: Result Storage (with timeout and retry)
  info!("💾 Step 3: Result Storage");
  let storage_result = execute_step_with_features(
    config,
    "store_results",
    || async {
      // Simulate result storage
      tokio::time::sleep(Duration::from_millis(300)).await;
      Ok(serde_json::json!({
          "stored": true,
          "location": "s3://bucket/results",
          "timestamp": chrono::Utc::now().to_rfc3339()
      }))
    },
    checkpoint_manager,
    resource_manager,
    &mut state,
  )
  .await?;

  info!(result = %storage_result, "✅ Result storage completed");

  // Mark workflow as completed
  checkpoint_manager
    .save_checkpoint_with_status(
      &config.workflow_id,
      "completed",
      &state,
      WorkflowStatus::Completed,
    )
    .await?;
  info!("✅ Workflow marked as completed");

  // Final health check
  let final_health = health_checker.check_health().await;
  info!(
    is_healthy = final_health.is_healthy,
    checks = final_health.checks.len(),
    "📊 Final health check completed"
  );

  // Resource usage report
  let resource_stats = resource_manager.get_stats().await;
  info!(
    memory_usage = resource_stats.memory.current_size,
    value_count = resource_stats.memory.value_count,
    "📈 Resource usage statistics"
  );

  Ok(())
}

/// Resume workflow from checkpoint
#[instrument(skip(_health_checker, checkpoint, checkpoint_manager, resource_manager))]
async fn resume_workflow(
  config: &WorkflowConfig,
  checkpoint: agentflow_core::checkpoint::Checkpoint,
  _health_checker: &Arc<HealthChecker>,
  checkpoint_manager: &CheckpointManager,
  resource_manager: &ResourceManager,
) -> Result<()> {
  info!("♻️  Resuming workflow from checkpoint");

  let mut state = checkpoint.state.clone();
  let last_completed = checkpoint.last_completed_node.as_str();

  // Determine which steps to skip
  let steps = ["extract_data", "process_data", "store_results"];
  let resume_from_index = steps.iter().position(|&s| s == last_completed).unwrap_or(0) + 1;

  info!(
      last_completed = %last_completed,
      resume_from_index,
      "Resuming from step index {}", resume_from_index
  );

  // Execute remaining steps
  for step in &steps[resume_from_index..] {
    info!("▶️  Executing step: {}", step);

    let result = execute_step_with_features(
      config,
      step,
      || async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        Ok(serde_json::json!({
            "step": step,
            "resumed": true,
            "timestamp": chrono::Utc::now().to_rfc3339()
        }))
      },
      checkpoint_manager,
      resource_manager,
      &mut state,
    )
    .await?;

    info!(step = %step, result = %result, "✅ Step completed");
  }

  // Mark as completed
  checkpoint_manager
    .save_checkpoint_with_status(
      &config.workflow_id,
      "completed",
      &state,
      WorkflowStatus::Completed,
    )
    .await?;
  info!("✅ Resumed workflow completed successfully");

  Ok(())
}

/// Execute a workflow step with all Phase 1.5 features
async fn execute_step_with_features<F, Fut>(
  config: &WorkflowConfig,
  step_name: &str,
  operation: F,
  checkpoint_manager: &CheckpointManager,
  resource_manager: &ResourceManager,
  state: &mut HashMap<String, serde_json::Value>,
) -> Result<serde_json::Value>
where
  F: Fn() -> Fut,
  Fut: std::future::Future<Output = Result<serde_json::Value>>,
{
  // Record resource allocation
  resource_manager.record_allocation(step_name, 1024 * 1024); // 1MB

  // Execute with retry, timeout, and error context
  let result = execute_with_retry(&config.retry_policy, step_name, || async {
    with_timeout_context(
      operation(),
      config.timeout_config.node_execution_timeout,
      step_name,
      Some(step_name),
      Some(&config.workflow_id),
    )
    .await
  })
  .await?;

  // Update state
  state.insert(step_name.to_string(), result.clone());

  // Save checkpoint
  checkpoint_manager
    .save_checkpoint(&config.workflow_id, step_name, state)
    .await?;

  debug!(step = %step_name, "Checkpoint saved");

  // Check resource usage and cleanup if needed
  if resource_manager.should_cleanup() {
    warn!("Resource usage high, performing cleanup");
    resource_manager.cleanup(0.5).await?;
  }

  Ok(result)
}

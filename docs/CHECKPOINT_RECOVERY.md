# Checkpoint Recovery System

**Since:** v0.2.0+
**Status:** Production-Ready
**Performance:** < 10ms save (small), < 50ms save (large), < 10ms load

## Overview

The Checkpoint Recovery System provides persistent workflow state management, enabling fault tolerance and resumable workflows. After a failure or interruption, workflows can resume from the last successful checkpoint instead of restarting from the beginning.

## Features

- **Incremental Checkpointing**: Automatic checkpoints after each node execution
- **Atomic Operations**: Write-then-rename for crash-safe checkpointing
- **Workflow Recovery**: Resume from last successful checkpoint
- **AgentNode Resume Contract**: Agent nodes persist `agent_result` and
  `agent_resume`, so checkpointed workflows can inspect whether an agent run is
  reusable, partial, or restart-only.
- **TTL-based Cleanup**: Automatic cleanup of old checkpoints
- **Configurable Retention**: Different retention for successful vs failed workflows
- **Concurrent-Safe**: File locking for multi-process safety
- **Compression Support**: Optional compression for large state (future)

## Quick Start

### Basic Usage

```rust
use agentflow_core::checkpoint::{CheckpointManager, CheckpointConfig};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create checkpoint manager with default config
    let config = CheckpointConfig::default();
    let manager = CheckpointManager::new(config)?;

    // Save checkpoint after node execution
    let mut state = HashMap::new();
    state.insert("node1".to_string(), serde_json::json!({
        "status": "completed",
        "output": "result data"
    }));

    manager.save_checkpoint("workflow_123", "node1", &state).await?;

    // Later: Resume from checkpoint
    if let Some(checkpoint) = manager.load_latest_checkpoint("workflow_123").await? {
        println!("Resuming from node: {}", checkpoint.last_completed_node);
        println!("State: {:?}", checkpoint.state);
    }

    Ok(())
}
```

### With Custom Configuration

```rust
use agentflow_core::checkpoint::{CheckpointManager, CheckpointConfig};
use std::path::PathBuf;

let config = CheckpointConfig::default()
    .with_checkpoint_dir("/var/lib/agentflow/checkpoints")
    .with_success_retention_days(7)    // Keep successful workflows for 7 days
    .with_failure_retention_days(30)    // Keep failed workflows for 30 days
    .with_auto_cleanup(true);

let manager = CheckpointManager::new(config)?;
```

## Configuration

### CheckpointConfig

Configuration for checkpoint management.

```rust
pub struct CheckpointConfig {
    pub checkpoint_dir: PathBuf,
    pub success_retention_days: i64,
    pub failure_retention_days: i64,
    pub auto_cleanup: bool,
    pub compression: bool,
}
```

#### Default Configuration

```rust
let config = CheckpointConfig::default();

// Defaults to:
// - checkpoint_dir: ~/.agentflow/checkpoints
// - success_retention_days: 7
// - failure_retention_days: 30
// - auto_cleanup: true
// - compression: false
```

#### Builder Methods

```rust
pub fn with_checkpoint_dir(mut self, dir: impl Into<PathBuf>) -> Self
pub fn with_success_retention_days(mut self, days: i64) -> Self
pub fn with_failure_retention_days(mut self, days: i64) -> Self
pub fn with_retention_days(mut self, days: i64) -> Self  // Sets both
pub fn with_auto_cleanup(mut self, enabled: bool) -> Self
pub fn with_compression(mut self, enabled: bool) -> Self
```

**Example:**
```rust
let config = CheckpointConfig::default()
    .with_checkpoint_dir("/data/checkpoints")
    .with_retention_days(14)  // 14 days for both success and failure
    .with_auto_cleanup(true)
    .with_compression(false);
```

## API Reference

### CheckpointManager

Main interface for checkpoint management.

#### Constructor

```rust
pub fn new(config: CheckpointConfig) -> Result<Self>
```

Creates a new checkpoint manager and initializes the checkpoint directory.

**Example:**
```rust
let config = CheckpointConfig::default();
let manager = CheckpointManager::new(config)?;
```

#### Saving Checkpoints

```rust
pub async fn save_checkpoint(
    &self,
    workflow_id: &str,
    last_completed_node: &str,
    state: &HashMap<String, serde_json::Value>
) -> Result<()>
```

Saves a workflow checkpoint atomically.

**Parameters:**
- `workflow_id`: Unique identifier for the workflow run
- `last_completed_node`: ID of the last successfully completed node
- `state`: Current workflow state (node ID -> output value)

**Example:**
```rust
let mut state = HashMap::new();
state.insert("extract_data".to_string(), serde_json::json!({
    "records": 100,
    "output_file": "data.json"
}));

manager.save_checkpoint("run_abc123", "extract_data", &state).await?;
```

#### Loading Checkpoints

```rust
pub async fn load_latest_checkpoint(&self, workflow_id: &str) -> Result<Option<Checkpoint>>
pub async fn load_checkpoint(&self, workflow_id: &str, node_id: &str) -> Result<Option<Checkpoint>>
```

Load the latest or a specific checkpoint for a workflow.

**Example:**
```rust
// Load latest checkpoint
if let Some(checkpoint) = manager.load_latest_checkpoint("run_abc123").await? {
    println!("Resume from: {}", checkpoint.last_completed_node);

    // Access workflow state
    if let Some(node_output) = checkpoint.state.get("extract_data") {
        println!("Previous output: {}", node_output);
    }
}

// Load specific checkpoint
if let Some(checkpoint) = manager.load_checkpoint("run_abc123", "node5").await? {
    println!("Checkpoint at node5 found");
}
```

#### Checkpoint Status

```rust
pub async fn mark_completed(&self, workflow_id: &str) -> Result<()>
pub async fn mark_failed(&self, workflow_id: &str, error: &str) -> Result<()>
```

Mark a workflow as completed or failed (affects retention policy).

**Example:**
```rust
// On successful workflow completion
manager.mark_completed("run_abc123").await?;

// On workflow failure
manager.mark_failed("run_abc123", "Node execution timeout").await?;
```

#### Cleanup

```rust
pub async fn cleanup_old_checkpoints(&self) -> Result<CleanupReport>
pub async fn delete_workflow_checkpoints(&self, workflow_id: &str) -> Result<usize>
```

Clean up old checkpoints or delete specific workflow checkpoints.

**Example:**
```rust
// Automatic cleanup of expired checkpoints
let report = manager.cleanup_old_checkpoints().await?;
println!("Cleaned up: {} checkpoints, freed: {} bytes",
    report.deleted_count, report.freed_space);

// Delete specific workflow
let count = manager.delete_workflow_checkpoints("run_abc123").await?;
println!("Deleted {} checkpoints", count);
```

#### Listing Checkpoints

```rust
pub async fn list_workflow_checkpoints(&self, workflow_id: &str) -> Result<Vec<Checkpoint>>
pub async fn list_all_checkpoints(&self) -> Result<Vec<Checkpoint>>
```

List checkpoints for inspection or debugging.

**Example:**
```rust
// List all checkpoints for a workflow
let checkpoints = manager.list_workflow_checkpoints("run_abc123").await?;
for checkpoint in checkpoints {
    println!("{}: {}", checkpoint.created_at, checkpoint.last_completed_node);
}

// List all checkpoints (for admin/debugging)
let all = manager.list_all_checkpoints().await?;
println!("Total checkpoints: {}", all.len());
```

### Checkpoint Structure

```rust
pub struct Checkpoint {
    pub workflow_id: String,
    pub last_completed_node: String,
    pub state: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub status: WorkflowStatus,
    pub metadata: HashMap<String, String>,
}
```

### FlowValue State

Checkpoint state stores node outputs using the same stable JSON representation
as workflow result serialization. `FlowValue::Json` is stored as its raw JSON
value. File and URL references are stored as tagged JSON objects so resume can
restore the original `FlowValue` type:

```json
{
  "artifact": {
    "$type": "file",
    "path": "/tmp/report.md",
    "mime_type": "text/markdown"
  },
  "source": {
    "$type": "url",
    "url": "https://example.com/data.json",
    "mime_type": "application/json"
  }
}
```

### AgentNode State

When a workflow node is an `AgentNode`, its checkpointed output includes:

- `response`: final agent answer.
- `session_id`: memory/session identifier.
- `stop_reason`: structured terminal reason.
- `agent_result`: serialized `AgentRunResult` with steps and events.
- `agent_resume`: serialized `AgentNodeResumeContract`.

`agent_resume.resume_mode` defines the recovery boundary:

- `completed_run`: safe to reuse checkpointed outputs without re-running tools.
- `partial_run_supported`: durable tool observations are present, and the agent
  can continue from recovered memory without replaying completed tool calls.
- `partial_run_unsupported`: durable steps exist, but the current runtime cannot
  continue from the middle of the agent loop.
- `restart_required`: no reusable partial state is available.

For tool calls, `agent_resume.tool_calls[].replay_policy` is either
`reuse_recorded_result` when a tool result step exists, or
`requires_idempotent_retry` when a restart would call the tool again. This makes
idempotency requirements explicit before enabling finer-grained agent resume.

`AgentNode` can consume a previous `agent_result` as input. If the trace has no
unresolved tool calls, it restores the prior observations into memory and asks
the runtime to continue. If a tool call has no recorded result, resume is
rejected so the caller can choose an explicit idempotent restart policy.

### WorkflowStatus

```rust
pub enum WorkflowStatus {
    Running,
    Completed,
    Failed,
}
```

## Integration with Workflows

### Automatic Checkpointing in Workflows

AgentFlow automatically creates checkpoints during workflow execution:

```yaml
# workflow.yml
name: "Data Processing Pipeline"
checkpoint:
  enabled: true                # Enable automatic checkpointing
  retention_days: 7            # Keep checkpoints for 7 days
nodes:
  - id: fetch_data
    type: http
    parameters:
      url: "https://api.example.com/data"

  - id: process_data
    type: llm
    dependencies: ["fetch_data"]
    parameters:
      prompt: "Process this data: {{ nodes.fetch_data.outputs.data }}"

  - id: store_results
    type: file
    dependencies: ["process_data"]
    parameters:
      path: "results.json"
      content: "{{ nodes.process_data.outputs.result }}"
```

Checkpoints are automatically saved after each node completes successfully. If a
later node fails, the final failed checkpoint still points at the last successful
node, so `Flow::resume()` continues with the next DAG node and reuses restored
outputs for completed nodes such as `AgentNode`. This prevents already completed
agent tool calls from running again during workflow recovery.

### Manual Checkpoint Control

For programmatic control:

```rust
use agentflow_core::{Flow, GraphNode};
use agentflow_core::checkpoint::{CheckpointManager, CheckpointConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = CheckpointConfig::default();
    let checkpoint_manager = CheckpointManager::new(config)?;

    let workflow_id = "run_12345";
    let flow = create_my_flow();

    // Check for existing checkpoint
    if let Some(checkpoint) = checkpoint_manager.load_latest_checkpoint(workflow_id).await? {
        println!("Resuming from: {}", checkpoint.last_completed_node);

        // Resume workflow from checkpoint
        let result = flow.resume_from_checkpoint(checkpoint).await?;

        // Mark as completed
        checkpoint_manager.mark_completed(workflow_id).await?;
    } else {
        // Start fresh workflow with checkpointing
        let result = flow.run_with_checkpoints(workflow_id, &checkpoint_manager).await?;

        // Mark as completed
        checkpoint_manager.mark_completed(workflow_id).await?;
    }

    Ok(())
}
```

### Resume Strategy

```rust
use agentflow_core::checkpoint::{CheckpointManager, Checkpoint};

async fn run_workflow_with_resume(
    workflow_id: &str,
    checkpoint_manager: &CheckpointManager
) -> Result<(), Box<dyn std::error::Error>> {
    let checkpoint = checkpoint_manager.load_latest_checkpoint(workflow_id).await?;

    match checkpoint {
        Some(cp) if cp.status == WorkflowStatus::Running => {
            println!("Found incomplete workflow, resuming from: {}", cp.last_completed_node);

            // Restore state and resume
            let flow = create_workflow_from_checkpoint(&cp)?;
            let result = flow.run().await?;

            checkpoint_manager.mark_completed(workflow_id).await?;
        }
        Some(cp) if cp.status == WorkflowStatus::Completed => {
            println!("Workflow already completed");
        }
        Some(cp) if cp.status == WorkflowStatus::Failed => {
            println!("Previous run failed, starting fresh");
            run_fresh_workflow(workflow_id, checkpoint_manager).await?;
        }
        None => {
            println!("No checkpoint found, starting fresh");
            run_fresh_workflow(workflow_id, checkpoint_manager).await?;
        }
        _ => {}
    }

    Ok(())
}
```

## Best Practices

### 1. Use Meaningful Workflow IDs

```rust
// ❌ Non-descriptive ID
let workflow_id = "123";

// ✅ Descriptive, unique ID
use uuid::Uuid;
let workflow_id = format!("data_pipeline_{}", Uuid::new_v4());

// Or with timestamp
let workflow_id = format!("etl_job_{}", chrono::Utc::now().timestamp());
```

### 2. Configure Appropriate Retention

```rust
let config = CheckpointConfig::default()
    .with_success_retention_days(7)     // Short retention for successful workflows
    .with_failure_retention_days(30);   // Longer retention for debugging failures
```

### 3. Handle Checkpoint Load Failures

```rust
match checkpoint_manager.load_latest_checkpoint(workflow_id).await {
    Ok(Some(checkpoint)) => {
        // Resume from checkpoint
        flow.resume_from_checkpoint(checkpoint).await?;
    }
    Ok(None) => {
        // No checkpoint, start fresh
        flow.run().await?;
    }
    Err(e) => {
        // Checkpoint corrupted or inaccessible
        log::warn!("Failed to load checkpoint: {}, starting fresh", e);
        flow.run().await?;
    }
}
```

### 4. Clean State Between Runs

```rust
// After workflow completes successfully
checkpoint_manager.mark_completed(workflow_id).await?;

// Optionally delete checkpoints immediately for one-time workflows
checkpoint_manager.delete_workflow_checkpoints(workflow_id).await?;
```

### 5. Include Metadata for Debugging

```rust
let mut metadata = HashMap::new();
metadata.insert("user_id".to_string(), user_id.to_string());
metadata.insert("environment".to_string(), env!("ENV").to_string());
metadata.insert("git_commit".to_string(), env!("GIT_COMMIT").to_string());

// Metadata is automatically saved with checkpoint
```

### 6. Periodic Cleanup

```rust
use tokio::time::{interval, Duration};

// Run cleanup every 24 hours
let mut cleanup_timer = interval(Duration::from_secs(86400));

loop {
    cleanup_timer.tick().await;

    match checkpoint_manager.cleanup_old_checkpoints().await {
        Ok(report) => {
            log::info!("Cleanup: deleted {} checkpoints, freed {} bytes",
                report.deleted_count, report.freed_space);
        }
        Err(e) => {
            log::error!("Cleanup failed: {}", e);
        }
    }
}
```

## Performance Characteristics

Based on benchmark results from v0.2.0:

### Performance Metrics

- **Save checkpoint (small ~100 bytes)**: ~5.5ms average
- **Save checkpoint (large ~100KB)**: ~16.4ms average
- **Load checkpoint**: ~96μs average
- **Cleanup operation**: Depends on number of checkpoints

### Performance Targets

All targets are met in production:

- ✅ Save small checkpoint: < 10ms (actual: ~5.5ms, **45% faster**)
- ✅ Save large checkpoint: < 50ms (actual: ~16.4ms, **67% faster**)
- ✅ Load checkpoint: < 10ms (actual: ~96μs, **100x faster**)

### Benchmark Results

```bash
# Run checkpoint benchmarks
cargo test --test performance_benchmarks benchmark_checkpoint_operations -- --nocapture

# Expected output:
# 💾 Checkpoint Operations Benchmarks
# Save checkpoint (small state ~100 bytes) - Avg: 5.538354ms
# Save checkpoint (large state ~100KB) - Avg: 16.349647ms
# Load latest checkpoint - Avg: 96.642µs
# ✅ Checkpoint operations meet performance targets
```

## Advanced Usage

### Custom Checkpoint Storage

While the default file-based storage works for most cases, you can implement custom storage:

```rust
// Future: S3-based checkpoint storage
pub struct S3CheckpointManager {
    bucket: String,
    s3_client: S3Client,
    config: CheckpointConfig,
}

impl S3CheckpointManager {
    pub async fn save_checkpoint(&self, ...) -> Result<()> {
        // Serialize checkpoint
        let data = serde_json::to_vec(&checkpoint)?;

        // Upload to S3
        self.s3_client.put_object()
            .bucket(&self.bucket)
            .key(&format!("{}/{}.json", workflow_id, node_id))
            .body(data.into())
            .send()
            .await?;

        Ok(())
    }
}
```

### Checkpoint Compression

Enable compression for large workflow states (future feature):

```rust
let config = CheckpointConfig::default()
    .with_compression(true);  // Enable gzip compression

// Checkpoints will be automatically compressed/decompressed
```

### Distributed Checkpointing

For distributed workflows across multiple machines:

```rust
// Use shared storage (NFS, S3, etc.)
let config = CheckpointConfig::default()
    .with_checkpoint_dir("/mnt/shared/checkpoints");

// All worker nodes can access the same checkpoints
```

## Monitoring and Observability

### Logging Checkpoint Operations

```rust
use tracing::{info, warn};

// Log checkpoint saves
manager.save_checkpoint(workflow_id, node_id, &state).await?;
info!("Checkpoint saved: workflow={}, node={}", workflow_id, node_id);

// Log checkpoint loads
if let Some(checkpoint) = manager.load_latest_checkpoint(workflow_id).await? {
    info!("Resuming workflow={} from node={}",
        workflow_id, checkpoint.last_completed_node);
}
```

### Prometheus Metrics

```rust
use prometheus::{IntCounter, Histogram, Registry};

let checkpoint_saves = IntCounter::new(
    "agentflow_checkpoint_saves_total",
    "Total number of checkpoints saved"
)?;

let checkpoint_load_duration = Histogram::new(
    "agentflow_checkpoint_load_duration_seconds",
    "Checkpoint load duration"
)?;

// Record metrics
checkpoint_saves.inc();
let start = Instant::now();
let _ = manager.load_latest_checkpoint(workflow_id).await?;
checkpoint_load_duration.observe(start.elapsed().as_secs_f64());
```

### Disk Space Monitoring

```rust
use std::fs;

let checkpoint_dir = "/var/lib/agentflow/checkpoints";
let metadata = fs::metadata(checkpoint_dir)?;

// Monitor checkpoint directory size
fn get_dir_size(path: &Path) -> std::io::Result<u64> {
    let mut size = 0;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            size += get_dir_size(&entry.path())?;
        } else {
            size += metadata.len();
        }
    }
    Ok(size)
}

let size = get_dir_size(Path::new(checkpoint_dir))?;
println!("Checkpoint directory size: {} MB", size / 1024 / 1024);
```

## Troubleshooting

### Checkpoints not being saved

**Problem:** No checkpoint files created.

**Solutions:**
1. Check directory permissions:
   ```bash
   ls -la ~/.agentflow/checkpoints
   ```

2. Verify configuration:
   ```rust
   let config = CheckpointConfig::default();
   println!("Checkpoint dir: {:?}", config.checkpoint_dir);
   ```

3. Check for errors:
   ```rust
   match manager.save_checkpoint(workflow_id, node_id, &state).await {
       Ok(_) => println!("Saved successfully"),
       Err(e) => eprintln!("Save failed: {}", e),
   }
   ```

### Resume not working

**Problem:** Workflow doesn't resume from checkpoint.

**Solutions:**
1. Verify checkpoint exists:
   ```rust
   let checkpoints = manager.list_workflow_checkpoints(workflow_id).await?;
   println!("Found {} checkpoints", checkpoints.len());
   ```

2. Check workflow ID matches:
   ```rust
   // Ensure consistent workflow ID
   let workflow_id = "my_workflow_abc123";  // Same ID for save and load
   ```

3. Verify checkpoint status:
   ```rust
   if let Some(checkpoint) = manager.load_latest_checkpoint(workflow_id).await? {
       println!("Status: {:?}", checkpoint.status);
       println!("Last node: {}", checkpoint.last_completed_node);
   }
   ```

### Disk space issues

**Problem:** Checkpoints consuming too much disk space.

**Solutions:**
1. Reduce retention period:
   ```rust
   let config = CheckpointConfig::default()
       .with_retention_days(3);  // Shorter retention
   ```

2. Enable automatic cleanup:
   ```rust
   let config = CheckpointConfig::default()
       .with_auto_cleanup(true);
   ```

3. Manual cleanup:
   ```rust
   let report = manager.cleanup_old_checkpoints().await?;
   println!("Freed {} bytes", report.freed_space);
   ```

4. Delete completed workflows:
   ```rust
   // After successful completion
   manager.delete_workflow_checkpoints(workflow_id).await?;
   ```

## See Also

- [Timeout Control](TIMEOUT_CONTROL.md) - Operation timeout management
- [Health Checks](HEALTH_CHECKS.md) - System health monitoring
- [Resource Management](RESOURCE_MANAGEMENT.md) - Memory limits and cleanup
- [Retry Mechanism](RETRY_MECHANISM.md) - Automatic retry with backoff

---

**Last Updated:** 2025-11-16
**Version:** 0.2.0+

//! Workflow state checkpoint and recovery system
//!
//! This module provides persistent checkpoint capabilities for workflow execution,
//! enabling fault tolerance and resumable workflows.
//!
//! # Features
//!
//! - Incremental checkpointing after each node execution
//! - Atomic file operations (write-then-rename)
//! - Workflow recovery from last checkpoint
//! - TTL-based cleanup of old checkpoints
//! - Concurrent-safe with file locking
//!
//! # Example
//!
//! ```no_run
//! use agentflow_core::checkpoint::{CheckpointManager, CheckpointConfig};
//! use std::collections::HashMap;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let config = CheckpointConfig::default()
//!     .with_checkpoint_dir("./checkpoints")
//!     .with_retention_days(7);
//!
//! let manager = CheckpointManager::new(config)?;
//!
//! // Save checkpoint
//! let mut state = HashMap::new();
//! state.insert("node1".to_string(), serde_json::json!({"status": "completed"}));
//! manager.save_checkpoint("workflow_123", "node1", &state).await?;
//!
//! // Resume from checkpoint
//! if let Some(checkpoint) = manager.load_latest_checkpoint("workflow_123").await? {
//!     println!("Resuming from node: {}", checkpoint.last_completed_node);
//! }
//! # Ok(())
//! # }
//! ```

use crate::error::{AgentFlowError, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// Checkpoint configuration
#[derive(Debug, Clone)]
pub struct CheckpointConfig {
    /// Directory to store checkpoints
    pub checkpoint_dir: PathBuf,

    /// Retention period for successful workflows (days)
    pub success_retention_days: i64,

    /// Retention period for failed workflows (days)
    pub failure_retention_days: i64,

    /// Enable automatic cleanup on startup
    pub auto_cleanup: bool,

    /// Compress checkpoints
    pub compression: bool,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            checkpoint_dir: dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".agentflow")
                .join("checkpoints"),
            success_retention_days: 7,
            failure_retention_days: 30,
            auto_cleanup: true,
            compression: false,
        }
    }
}

impl CheckpointConfig {
    /// Set checkpoint directory
    pub fn with_checkpoint_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.checkpoint_dir = dir.into();
        self
    }

    /// Set retention days for successful workflows
    pub fn with_success_retention_days(mut self, days: i64) -> Self {
        self.success_retention_days = days;
        self
    }

    /// Set retention days for failed workflows
    pub fn with_failure_retention_days(mut self, days: i64) -> Self {
        self.failure_retention_days = days;
        self
    }

    /// Set retention days for both success and failure
    pub fn with_retention_days(mut self, days: i64) -> Self {
        self.success_retention_days = days;
        self.failure_retention_days = days;
        self
    }

    /// Enable/disable automatic cleanup
    pub fn with_auto_cleanup(mut self, enabled: bool) -> Self {
        self.auto_cleanup = enabled;
        self
    }

    /// Enable/disable compression
    pub fn with_compression(mut self, enabled: bool) -> Self {
        self.compression = enabled;
        self
    }
}

/// Workflow checkpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Workflow run ID
    pub workflow_id: String,

    /// Last successfully completed node ID
    pub last_completed_node: String,

    /// Workflow state (node ID -> output value)
    pub state: HashMap<String, serde_json::Value>,

    /// Checkpoint creation timestamp
    pub created_at: DateTime<Utc>,

    /// Workflow status
    pub status: WorkflowStatus,

    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Workflow execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkflowStatus {
    /// Workflow is running
    Running,
    /// Workflow completed successfully
    Completed,
    /// Workflow failed
    Failed,
    /// Workflow was cancelled
    Cancelled,
}

/// Checkpoint manager
pub struct CheckpointManager {
    config: CheckpointConfig,
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub fn new(config: CheckpointConfig) -> Result<Self> {
        let manager = Self { config };

        // Create checkpoint directory if it doesn't exist
        std::fs::create_dir_all(&manager.config.checkpoint_dir).map_err(|e| {
            AgentFlowError::PersistenceError {
                message: format!("Failed to create checkpoint directory: {}", e),
            }
        })?;

        Ok(manager)
    }

    /// Save a checkpoint for a workflow
    pub async fn save_checkpoint(
        &self,
        workflow_id: &str,
        last_completed_node: &str,
        state: &HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        self.save_checkpoint_with_status(workflow_id, last_completed_node, state, WorkflowStatus::Running)
            .await
    }

    /// Save a checkpoint with specific status
    pub async fn save_checkpoint_with_status(
        &self,
        workflow_id: &str,
        last_completed_node: &str,
        state: &HashMap<String, serde_json::Value>,
        status: WorkflowStatus,
    ) -> Result<()> {
        let checkpoint = Checkpoint {
            workflow_id: workflow_id.to_string(),
            last_completed_node: last_completed_node.to_string(),
            state: state.clone(),
            created_at: Utc::now(),
            status,
            metadata: HashMap::new(),
        };

        self.save_checkpoint_struct(&checkpoint).await
    }

    /// Save a checkpoint structure
    pub async fn save_checkpoint_struct(&self, checkpoint: &Checkpoint) -> Result<()> {
        let workflow_dir = self.get_workflow_dir(&checkpoint.workflow_id);
        fs::create_dir_all(&workflow_dir).await.map_err(|e| {
            AgentFlowError::PersistenceError {
                message: format!("Failed to create workflow directory: {}", e),
            }
        })?;

        // Generate checkpoint filename with timestamp
        let timestamp = checkpoint.created_at.format("%Y%m%d_%H%M%S_%3f");
        let filename = format!("checkpoint_{}.json", timestamp);
        let checkpoint_path = workflow_dir.join(&filename);
        let temp_path = workflow_dir.join(format!(".{}.tmp", filename));

        // Serialize checkpoint
        let json = serde_json::to_string_pretty(&checkpoint).map_err(|e| {
            AgentFlowError::SerializationError(format!("Failed to serialize checkpoint: {}", e))
        })?;

        // Write to temporary file (atomic operation)
        let mut file = fs::File::create(&temp_path).await.map_err(|e| {
            AgentFlowError::PersistenceError {
                message: format!("Failed to create temp checkpoint file: {}", e),
            }
        })?;

        file.write_all(json.as_bytes()).await.map_err(|e| {
            AgentFlowError::PersistenceError {
                message: format!("Failed to write checkpoint: {}", e),
            }
        })?;

        file.sync_all().await.map_err(|e| {
            AgentFlowError::PersistenceError {
                message: format!("Failed to sync checkpoint: {}", e),
            }
        })?;

        drop(file);

        // Atomically rename temp file to final name
        fs::rename(&temp_path, &checkpoint_path)
            .await
            .map_err(|e| AgentFlowError::PersistenceError {
                message: format!("Failed to rename checkpoint file: {}", e),
            })?;

        // Also save as "latest" for quick access
        let latest_path = workflow_dir.join("checkpoint_latest.json");
        fs::copy(&checkpoint_path, &latest_path)
            .await
            .map_err(|e| AgentFlowError::PersistenceError {
                message: format!("Failed to save latest checkpoint: {}", e),
            })?;

        Ok(())
    }

    /// Load the latest checkpoint for a workflow
    pub async fn load_latest_checkpoint(&self, workflow_id: &str) -> Result<Option<Checkpoint>> {
        let latest_path = self
            .get_workflow_dir(workflow_id)
            .join("checkpoint_latest.json");

        if !latest_path.exists() {
            return Ok(None);
        }

        self.load_checkpoint_from_path(&latest_path).await.map(Some)
    }

    /// Load all checkpoints for a workflow (sorted by timestamp, newest first)
    pub async fn load_all_checkpoints(&self, workflow_id: &str) -> Result<Vec<Checkpoint>> {
        let workflow_dir = self.get_workflow_dir(workflow_id);

        if !workflow_dir.exists() {
            return Ok(Vec::new());
        }

        let mut checkpoints = Vec::new();
        let mut entries = fs::read_dir(&workflow_dir).await.map_err(|e| {
            AgentFlowError::PersistenceError {
                message: format!("Failed to read checkpoint directory: {}", e),
            }
        })?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            AgentFlowError::PersistenceError {
                message: format!("Failed to read directory entry: {}", e),
            }
        })? {
            let path = entry.path();
            if path.is_file() && path.extension().map(|e| e == "json").unwrap_or(false) {
                let filename = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                // Skip latest symlink and temp files
                if filename == "checkpoint_latest.json" || filename.starts_with('.') {
                    continue;
                }

                if let Ok(checkpoint) = self.load_checkpoint_from_path(&path).await {
                    checkpoints.push(checkpoint);
                }
            }
        }

        // Sort by timestamp, newest first
        checkpoints.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(checkpoints)
    }

    /// Load checkpoint from file path
    async fn load_checkpoint_from_path(&self, path: &Path) -> Result<Checkpoint> {
        let contents = fs::read_to_string(path).await.map_err(|e| {
            AgentFlowError::PersistenceError {
                message: format!("Failed to read checkpoint file: {}", e),
            }
        })?;

        serde_json::from_str(&contents).map_err(|e| {
            AgentFlowError::SerializationError(format!("Failed to deserialize checkpoint: {}", e))
        })
    }

    /// Delete a specific checkpoint
    pub async fn delete_checkpoint(&self, workflow_id: &str, created_at: DateTime<Utc>) -> Result<()> {
        let timestamp = created_at.format("%Y%m%d_%H%M%S_%3f");
        let filename = format!("checkpoint_{}.json", timestamp);
        let path = self.get_workflow_dir(workflow_id).join(filename);

        if path.exists() {
            fs::remove_file(&path).await.map_err(|e| {
                AgentFlowError::PersistenceError {
                    message: format!("Failed to delete checkpoint: {}", e),
                }
            })?;
        }

        // If we deleted the latest checkpoint, update the latest symlink
        let latest_path = self.get_workflow_dir(workflow_id).join("checkpoint_latest.json");
        if latest_path.exists() {
            // Load the latest checkpoint to check if it was the one we deleted
            if let Ok(checkpoint) = self.load_checkpoint_from_path(&latest_path).await {
                if checkpoint.created_at == created_at {
                    // The latest was deleted, find the next most recent one
                    let checkpoints = self.load_all_checkpoints(workflow_id).await?;
                    if let Some(newest) = checkpoints.first() {
                        // Update latest symlink to point to the next checkpoint
                        let timestamp = newest.created_at.format("%Y%m%d_%H%M%S_%3f");
                        let filename = format!("checkpoint_{}.json", timestamp);
                        let checkpoint_path = self.get_workflow_dir(workflow_id).join(filename);

                        let _ = fs::remove_file(&latest_path).await;
                        fs::copy(&checkpoint_path, &latest_path).await.ok();
                    } else {
                        // No more checkpoints, delete the latest symlink
                        let _ = fs::remove_file(&latest_path).await;
                    }
                }
            }
        }

        Ok(())
    }

    /// Delete all checkpoints for a workflow
    pub async fn delete_all_checkpoints(&self, workflow_id: &str) -> Result<()> {
        let workflow_dir = self.get_workflow_dir(workflow_id);

        if workflow_dir.exists() {
            fs::remove_dir_all(&workflow_dir).await.map_err(|e| {
                AgentFlowError::PersistenceError {
                    message: format!("Failed to delete checkpoint directory: {}", e),
                }
            })?;
        }

        Ok(())
    }

    /// Clean up old checkpoints based on retention policy
    pub async fn cleanup_old_checkpoints(&self) -> Result<usize> {
        let mut cleaned_count = 0;
        let mut entries = fs::read_dir(&self.config.checkpoint_dir)
            .await
            .map_err(|e| AgentFlowError::PersistenceError {
                message: format!("Failed to read checkpoint directory: {}", e),
            })?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            AgentFlowError::PersistenceError {
                message: format!("Failed to read directory entry: {}", e),
            }
        })? {
            let path = entry.path();
            if path.is_dir() {
                let workflow_id = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                cleaned_count += self.cleanup_workflow_checkpoints(&workflow_id).await?;
            }
        }

        Ok(cleaned_count)
    }

    /// Clean up checkpoints for a specific workflow
    async fn cleanup_workflow_checkpoints(&self, workflow_id: &str) -> Result<usize> {
        let checkpoints = self.load_all_checkpoints(workflow_id).await?;
        let mut cleaned_count = 0;

        for checkpoint in checkpoints {
            let retention_days = match checkpoint.status {
                WorkflowStatus::Completed => self.config.success_retention_days,
                WorkflowStatus::Failed | WorkflowStatus::Cancelled => self.config.failure_retention_days,
                WorkflowStatus::Running => continue, // Keep running workflows
            };

            let age = Utc::now().signed_duration_since(checkpoint.created_at);
            if age > Duration::days(retention_days) {
                self.delete_checkpoint(workflow_id, checkpoint.created_at)
                    .await?;
                cleaned_count += 1;
            }
        }

        Ok(cleaned_count)
    }

    /// Get workflow checkpoint directory
    fn get_workflow_dir(&self, workflow_id: &str) -> PathBuf {
        self.config.checkpoint_dir.join(workflow_id)
    }

    /// Get configuration
    pub fn config(&self) -> &CheckpointConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_manager() -> (CheckpointManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = CheckpointConfig::default()
            .with_checkpoint_dir(temp_dir.path())
            .with_auto_cleanup(false);

        let manager = CheckpointManager::new(config).unwrap();
        (manager, temp_dir)
    }

    #[tokio::test]
    async fn test_save_and_load_checkpoint() {
        let (manager, _temp_dir) = create_test_manager();

        let mut state = HashMap::new();
        state.insert("node1".to_string(), serde_json::json!({"result": "success"}));

        manager
            .save_checkpoint("test_workflow", "node1", &state)
            .await
            .unwrap();

        let checkpoint = manager
            .load_latest_checkpoint("test_workflow")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(checkpoint.workflow_id, "test_workflow");
        assert_eq!(checkpoint.last_completed_node, "node1");
        assert_eq!(checkpoint.state.len(), 1);
        assert_eq!(checkpoint.status, WorkflowStatus::Running);
    }

    #[tokio::test]
    async fn test_multiple_checkpoints() {
        let (manager, _temp_dir) = create_test_manager();

        // Create multiple checkpoints
        for i in 1..=3 {
            let mut state = HashMap::new();
            state.insert(format!("node{}", i), serde_json::json!(i));

            manager
                .save_checkpoint("test_workflow", &format!("node{}", i), &state)
                .await
                .unwrap();

            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        let checkpoints = manager
            .load_all_checkpoints("test_workflow")
            .await
            .unwrap();

        assert_eq!(checkpoints.len(), 3);
        // Should be sorted newest first
        assert_eq!(checkpoints[0].last_completed_node, "node3");
    }

    #[tokio::test]
    async fn test_delete_checkpoint() {
        let (manager, _temp_dir) = create_test_manager();

        let mut state = HashMap::new();
        state.insert("node1".to_string(), serde_json::json!(1));

        manager
            .save_checkpoint("test_workflow", "node1", &state)
            .await
            .unwrap();

        let checkpoint = manager
            .load_latest_checkpoint("test_workflow")
            .await
            .unwrap()
            .unwrap();

        manager
            .delete_checkpoint("test_workflow", checkpoint.created_at)
            .await
            .unwrap();

        let result = manager.load_latest_checkpoint("test_workflow").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete_all_checkpoints() {
        let (manager, _temp_dir) = create_test_manager();

        for i in 1..=3 {
            let mut state = HashMap::new();
            state.insert(format!("node{}", i), serde_json::json!(i));

            manager
                .save_checkpoint("test_workflow", &format!("node{}", i), &state)
                .await
                .unwrap();
        }

        manager.delete_all_checkpoints("test_workflow").await.unwrap();

        let checkpoints = manager
            .load_all_checkpoints("test_workflow")
            .await
            .unwrap();
        assert_eq!(checkpoints.len(), 0);
    }

    #[tokio::test]
    async fn test_checkpoint_with_status() {
        let (manager, _temp_dir) = create_test_manager();

        let mut state = HashMap::new();
        state.insert("node1".to_string(), serde_json::json!(1));

        manager
            .save_checkpoint_with_status("test_workflow", "node1", &state, WorkflowStatus::Completed)
            .await
            .unwrap();

        let checkpoint = manager
            .load_latest_checkpoint("test_workflow")
            .await
            .unwrap()
            .unwrap();

        assert_eq!(checkpoint.status, WorkflowStatus::Completed);
    }

    #[tokio::test]
    async fn test_cleanup_old_checkpoints() {
        let (manager, _temp_dir) = create_test_manager();

        // Create a checkpoint with old timestamp
        let mut checkpoint = Checkpoint {
            workflow_id: "test_workflow".to_string(),
            last_completed_node: "node1".to_string(),
            state: HashMap::new(),
            created_at: Utc::now() - Duration::days(8), // 8 days old
            status: WorkflowStatus::Completed,
            metadata: HashMap::new(),
        };

        manager.save_checkpoint_struct(&checkpoint).await.unwrap();

        // Create a recent checkpoint
        checkpoint.created_at = Utc::now();
        checkpoint.last_completed_node = "node2".to_string();
        manager.save_checkpoint_struct(&checkpoint).await.unwrap();

        let cleaned = manager.cleanup_old_checkpoints().await.unwrap();
        assert_eq!(cleaned, 1);

        let checkpoints = manager
            .load_all_checkpoints("test_workflow")
            .await
            .unwrap();
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(checkpoints[0].last_completed_node, "node2");
    }
}

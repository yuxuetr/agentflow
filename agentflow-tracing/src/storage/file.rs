//! File-based trace storage
//!
//! Stores execution traces as JSON files in a directory.
//! Suitable for development and small-scale deployments.

use super::{TraceQuery, TraceStorage};
use crate::types::*;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::path::PathBuf;
use tokio::fs;

/// File-based trace storage
///
/// Stores each trace as a separate JSON file named `{workflow_id}.json`
pub struct FileTraceStorage {
  /// Base directory for trace files
  base_path: PathBuf,
}

impl FileTraceStorage {
  /// Create a new file storage
  ///
  /// Creates the directory if it doesn't exist
  pub fn new(base_path: PathBuf) -> Result<Self, anyhow::Error> {
    std::fs::create_dir_all(&base_path)?;
    Ok(Self { base_path })
  }

  /// Get the file path for a workflow ID
  fn trace_path(&self, workflow_id: &str) -> PathBuf {
    self.base_path.join(format!("{}.json", workflow_id))
  }

  /// Check if a trace matches query filters
  fn matches_query(trace: &ExecutionTrace, query: &TraceQuery) -> bool {
    // Filter by workflow IDs
    if let Some(ref ids) = query.workflow_ids {
      if !ids.contains(&trace.workflow_id) {
        return false;
      }
    }

    // Filter by status
    if let Some(ref status) = query.status {
      if std::mem::discriminant(&trace.status) != std::mem::discriminant(status) {
        return false;
      }
    }

    // Filter by user ID
    if let Some(ref user_id) = query.user_id {
      if trace.metadata.user_id.as_ref() != Some(user_id) {
        return false;
      }
    }

    // Filter by tags (any match)
    if let Some(ref tags) = query.tags {
      if !tags.iter().any(|t| trace.metadata.tags.contains(t)) {
        return false;
      }
    }

    // Filter by time range
    if let Some(ref range) = query.time_range {
      if !range.contains(trace.started_at) {
        return false;
      }
    }

    true
  }
}

#[async_trait]
impl TraceStorage for FileTraceStorage {
  async fn save_trace(&self, trace: &ExecutionTrace) -> Result<(), anyhow::Error> {
    let path = self.trace_path(&trace.workflow_id);
    let json = serde_json::to_string_pretty(trace)?;
    fs::write(path, json).await?;
    Ok(())
  }

  async fn get_trace(&self, workflow_id: &str) -> Result<Option<ExecutionTrace>, anyhow::Error> {
    let path = self.trace_path(workflow_id);

    if !path.exists() {
      return Ok(None);
    }

    let json = fs::read_to_string(path).await?;
    let trace = serde_json::from_str(&json)?;
    Ok(Some(trace))
  }

  async fn query_traces(&self, query: TraceQuery) -> Result<Vec<ExecutionTrace>, anyhow::Error> {
    let mut traces = Vec::new();
    let mut entries = fs::read_dir(&self.base_path).await?;

    while let Some(entry) = entries.next_entry().await? {
      let path = entry.path();

      // Only process .json files
      if path.extension().and_then(|s| s.to_str()) != Some("json") {
        continue;
      }

      // Read and parse trace
      if let Ok(json) = fs::read_to_string(&path).await {
        if let Ok(trace) = serde_json::from_str::<ExecutionTrace>(&json) {
          // Apply filters
          if Self::matches_query(&trace, &query) {
            traces.push(trace);
          }
        }
      }
    }

    // Sort by started_at (newest first)
    traces.sort_by(|a, b| b.started_at.cmp(&a.started_at));

    // Apply offset
    if let Some(offset) = query.offset {
      traces = traces.into_iter().skip(offset).collect();
    }

    // Apply limit
    if let Some(limit) = query.limit {
      traces.truncate(limit);
    }

    Ok(traces)
  }

  async fn delete_old_traces(&self, older_than: DateTime<Utc>) -> Result<usize, anyhow::Error> {
    let mut count = 0;
    let mut entries = fs::read_dir(&self.base_path).await?;

    while let Some(entry) = entries.next_entry().await? {
      let path = entry.path();

      if path.extension().and_then(|s| s.to_str()) != Some("json") {
        continue;
      }

      // Read trace to check timestamp
      if let Ok(json) = fs::read_to_string(&path).await {
        if let Ok(trace) = serde_json::from_str::<ExecutionTrace>(&json) {
          if trace.started_at < older_than {
            fs::remove_file(&path).await?;
            count += 1;
          }
        }
      }
    }

    Ok(count)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use tempfile::tempdir;

  #[tokio::test]
  async fn test_file_storage_save_and_get() {
    let dir = tempdir().unwrap();
    let storage = FileTraceStorage::new(dir.path().to_path_buf()).unwrap();

    let mut trace = ExecutionTrace::new("test-wf-1".to_string());
    trace.metadata.tags.push("test".to_string());

    // Save trace
    storage.save_trace(&trace).await.unwrap();

    // Retrieve trace
    let retrieved = storage.get_trace("test-wf-1").await.unwrap();
    assert!(retrieved.is_some());

    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.workflow_id, "test-wf-1");
    assert_eq!(retrieved.metadata.tags, vec!["test"]);
  }

  #[tokio::test]
  async fn test_file_storage_query() {
    let dir = tempdir().unwrap();
    let storage = FileTraceStorage::new(dir.path().to_path_buf()).unwrap();

    // Create multiple traces
    for i in 1..=5 {
      let mut trace = ExecutionTrace::new(format!("wf-{}", i));
      if i % 2 == 0 {
        trace.metadata.tags.push("even".to_string());
      }
      storage.save_trace(&trace).await.unwrap();
    }

    // Query all
    let all = storage.query_traces(TraceQuery::default()).await.unwrap();
    assert_eq!(all.len(), 5);

    // Query with tag filter
    let query = TraceQuery {
      tags: Some(vec!["even".to_string()]),
      ..Default::default()
    };
    let filtered = storage.query_traces(query).await.unwrap();
    assert_eq!(filtered.len(), 2);

    // Query with limit
    let query = TraceQuery {
      limit: Some(3),
      ..Default::default()
    };
    let limited = storage.query_traces(query).await.unwrap();
    assert_eq!(limited.len(), 3);
  }

  #[tokio::test]
  async fn test_file_storage_delete_old() {
    let dir = tempdir().unwrap();
    let storage = FileTraceStorage::new(dir.path().to_path_buf()).unwrap();

    // Create trace
    let trace = ExecutionTrace::new("old-wf".to_string());
    storage.save_trace(&trace).await.unwrap();

    // Delete traces older than now (should delete the trace)
    let count = storage
      .delete_old_traces(Utc::now() + chrono::Duration::seconds(1))
      .await
      .unwrap();
    assert_eq!(count, 1);

    // Verify deleted
    let retrieved = storage.get_trace("old-wf").await.unwrap();
    assert!(retrieved.is_none());
  }

  #[tokio::test]
  async fn test_file_storage_nonexistent_trace() {
    let dir = tempdir().unwrap();
    let storage = FileTraceStorage::new(dir.path().to_path_buf()).unwrap();

    let result = storage.get_trace("nonexistent").await.unwrap();
    assert!(result.is_none());
  }
}

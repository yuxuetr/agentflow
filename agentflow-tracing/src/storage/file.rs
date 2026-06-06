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
use tokio::io::AsyncWriteExt;

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
    if let Some(ref ids) = query.workflow_ids
      && !ids.contains(&trace.workflow_id)
    {
      return false;
    }

    // Filter by status
    if let Some(ref status) = query.status
      && std::mem::discriminant(&trace.status) != std::mem::discriminant(status)
    {
      return false;
    }

    // Filter by user ID
    if let Some(ref user_id) = query.user_id
      && trace.metadata.user_id.as_ref() != Some(user_id)
    {
      return false;
    }

    // Filter by tags (any match)
    if let Some(ref tags) = query.tags
      && !tags.iter().any(|t| trace.metadata.tags.contains(t))
    {
      return false;
    }

    // Filter by time range
    if let Some(ref range) = query.time_range
      && !range.contains(trace.started_at)
    {
      return false;
    }

    true
  }
}

#[async_trait]
impl TraceStorage for FileTraceStorage {
  async fn save_trace(&self, trace: &ExecutionTrace) -> Result<(), anyhow::Error> {
    // Q2.3.4: crash-safe + permission-tight trace persistence.
    //
    // Previously `fs::write(path, json).await` (a) made the file
    // world-readable under the default umask (typically 0o644) and
    // (b) had no fsync, so a crash between write and flush could
    // leave a zero-byte / partial-JSON file that future reads would
    // happily deserialize-error on. Switching to write-temp + fsync
    // + rename gives atomic, durable replacement; unix-only `mode`
    // sets the bits at create time so there's no readable window.
    let final_path = self.trace_path(&trace.workflow_id);
    let json = serde_json::to_string_pretty(trace)?;

    let tmp_name = match final_path.file_name() {
      Some(name) => {
        let mut s = name.to_os_string();
        s.push(".tmp");
        s
      }
      None => {
        return Err(anyhow::anyhow!(
          "trace path has no file name component: {}",
          final_path.display()
        ));
      }
    };
    let tmp_path = self.base_path.join(tmp_name);

    let mut options = fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&tmp_path).await?;
    file.write_all(json.as_bytes()).await?;
    file.flush().await?;
    file.sync_data().await?;
    drop(file);

    fs::rename(&tmp_path, &final_path).await?;
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
      if let Ok(json) = fs::read_to_string(&path).await
        && let Ok(trace) = serde_json::from_str::<ExecutionTrace>(&json)
      {
        // Apply filters
        if Self::matches_query(&trace, &query) {
          traces.push(trace);
        }
      }
    }

    // Sort by started_at (newest first)
    traces.sort_by_key(|trace| std::cmp::Reverse(trace.started_at));

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
      if let Ok(json) = fs::read_to_string(&path).await
        && let Ok(trace) = serde_json::from_str::<ExecutionTrace>(&json)
        && trace.started_at < older_than
      {
        fs::remove_file(&path).await?;
        count += 1;
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

  // Q2.3.4: saved trace files must have 0o600 permissions on unix so a
  // shared workstation's other users can't read redacted-but-still-
  // attacker-interesting payloads.
  #[cfg(unix)]
  #[tokio::test]
  async fn save_trace_uses_owner_only_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let storage = FileTraceStorage::new(dir.path().to_path_buf()).unwrap();
    let trace = ExecutionTrace::new("wf-perm".to_string());
    storage.save_trace(&trace).await.unwrap();

    let path = dir.path().join("wf-perm.json");
    let meta = std::fs::metadata(&path).expect("trace file must exist");
    let mode = meta.permissions().mode() & 0o777;
    assert_eq!(
      mode, 0o600,
      "trace files must be 0o600 (owner-only), got 0o{mode:o}"
    );
  }

  // Q2.3.4: write-temp + fsync + rename is atomic — even a crash mid-
  // write must not leave a partial-JSON `.json` file. We can't simulate
  // a crash easily, but we can verify the tmp file is gone after a
  // successful save and the final file deserializes.
  #[tokio::test]
  async fn save_trace_is_atomic_and_leaves_no_tmp_file() {
    let dir = tempdir().unwrap();
    let storage = FileTraceStorage::new(dir.path().to_path_buf()).unwrap();
    let trace = ExecutionTrace::new("wf-atomic".to_string());
    storage.save_trace(&trace).await.unwrap();

    let final_path = dir.path().join("wf-atomic.json");
    let tmp_path = dir.path().join("wf-atomic.json.tmp");
    assert!(final_path.exists(), "final json must exist after save");
    assert!(
      !tmp_path.exists(),
      "tmp file must not survive a successful rename"
    );

    let json = std::fs::read_to_string(&final_path).unwrap();
    let round_trip: ExecutionTrace = serde_json::from_str(&json).unwrap();
    assert_eq!(round_trip.workflow_id, "wf-atomic");
  }
}

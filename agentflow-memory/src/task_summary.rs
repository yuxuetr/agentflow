//! Concrete [`TaskSummaryStore`] implementations (L2.1).
//!
//! Two, mirroring the session-memory split: [`InMemoryTaskSummaryStore`]
//! (ephemeral, process-lifetime — matches [`crate::SessionMemory`]) and
//! [`SqliteTaskSummaryStore`] (persistent — matches [`crate::SqliteMemory`]).
//! Kept as standalone types rather than retrofitted onto `SessionMemory`/
//! `SqliteMemory` themselves: task-summary persistence is an optional,
//! separately-configured concern (a caller with compaction disabled has no
//! summaries to store), not every `MemoryStore` needs one.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;
use sqlx::sqlite::SqlitePool;

use crate::MemoryError;
use agentflow_store_spi::{TaskSummary, TaskSummaryStore};

/// Ephemeral, process-lifetime [`TaskSummaryStore`] — for tests and
/// short-lived sessions that don't need the summary to survive a restart.
#[derive(Default)]
pub struct InMemoryTaskSummaryStore {
  summaries: Mutex<HashMap<String, TaskSummary>>,
}

impl InMemoryTaskSummaryStore {
  pub fn new() -> Self {
    Self::default()
  }
}

#[async_trait]
impl TaskSummaryStore for InMemoryTaskSummaryStore {
  async fn get_task_summary(&self, session_id: &str) -> Result<Option<TaskSummary>, MemoryError> {
    Ok(
      self
        .summaries
        .lock()
        .map_err(|e| MemoryError::StorageError(format!("task summary mutex poisoned: {e}")))?
        .get(session_id)
        .cloned(),
    )
  }

  async fn set_task_summary(
    &self,
    session_id: &str,
    summary: TaskSummary,
  ) -> Result<(), MemoryError> {
    self
      .summaries
      .lock()
      .map_err(|e| MemoryError::StorageError(format!("task summary mutex poisoned: {e}")))?
      .insert(session_id.to_string(), summary);
    Ok(())
  }

  async fn clear_task_summary(&self, session_id: &str) -> Result<(), MemoryError> {
    self
      .summaries
      .lock()
      .map_err(|e| MemoryError::StorageError(format!("task summary mutex poisoned: {e}")))?
      .remove(session_id);
    Ok(())
  }
}

/// Persistent, SQLite-backed [`TaskSummaryStore`].
///
/// Schema (single table, one row per session — `set_task_summary` is a
/// plain UPSERT, not an append log; the summary itself is the compacted
/// artifact):
///
/// ```sql
/// CREATE TABLE task_summaries (
///   session_id TEXT PRIMARY KEY,
///   summary    TEXT NOT NULL,   -- JSON-encoded TaskSummary
///   updated_at TEXT NOT NULL    -- RFC 3339 UTC timestamp
/// );
/// ```
pub struct SqliteTaskSummaryStore {
  pool: SqlitePool,
}

impl SqliteTaskSummaryStore {
  /// Open (or create) a task-summary database at `path`.
  pub async fn open<P: AsRef<Path>>(path: P) -> Result<Self, MemoryError> {
    let pool = crate::sqlite_pool::build_pool(path).await?;
    let store = Self { pool };
    store.init_schema().await?;
    Ok(store)
  }

  /// In-memory database for tests.
  pub async fn in_memory() -> Result<Self, MemoryError> {
    let pool = crate::sqlite_pool::build_in_memory_pool().await?;
    let store = Self { pool };
    store.init_schema().await?;
    Ok(store)
  }

  async fn init_schema(&self) -> Result<(), MemoryError> {
    sqlx::query(
      "CREATE TABLE IF NOT EXISTS task_summaries (
        session_id TEXT PRIMARY KEY,
        summary    TEXT NOT NULL,
        updated_at TEXT NOT NULL
      );",
    )
    .execute(&self.pool)
    .await
    .map_err(|e| MemoryError::StorageError(e.to_string()))?;
    Ok(())
  }
}

#[async_trait]
impl TaskSummaryStore for SqliteTaskSummaryStore {
  async fn get_task_summary(&self, session_id: &str) -> Result<Option<TaskSummary>, MemoryError> {
    let row = sqlx::query("SELECT summary FROM task_summaries WHERE session_id = ?")
      .bind(session_id)
      .fetch_optional(&self.pool)
      .await
      .map_err(|e| MemoryError::StorageError(e.to_string()))?;

    let Some(row) = row else {
      return Ok(None);
    };
    let raw: String = row
      .try_get("summary")
      .map_err(|e| MemoryError::StorageError(e.to_string()))?;
    let summary: TaskSummary = serde_json::from_str(&raw)
      .map_err(|e| MemoryError::StorageError(format!("invalid stored JSON: {e}")))?;
    Ok(Some(summary))
  }

  async fn set_task_summary(
    &self,
    session_id: &str,
    summary: TaskSummary,
  ) -> Result<(), MemoryError> {
    let raw = serde_json::to_string(&summary)?;
    let now: DateTime<Utc> = Utc::now();
    sqlx::query(
      "INSERT INTO task_summaries (session_id, summary, updated_at)
       VALUES (?, ?, ?)
       ON CONFLICT(session_id) DO UPDATE SET summary = excluded.summary, updated_at = excluded.updated_at",
    )
    .bind(session_id)
    .bind(raw)
    .bind(now.to_rfc3339())
    .execute(&self.pool)
    .await
    .map_err(|e| MemoryError::StorageError(e.to_string()))?;
    Ok(())
  }

  async fn clear_task_summary(&self, session_id: &str) -> Result<(), MemoryError> {
    sqlx::query("DELETE FROM task_summaries WHERE session_id = ?")
      .bind(session_id)
      .execute(&self.pool)
      .await
      .map_err(|e| MemoryError::StorageError(e.to_string()))?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn sample_summary(goal: &str) -> TaskSummary {
    TaskSummary {
      goal: goal.to_string(),
      completed_steps: vec!["step one".to_string()],
      key_results: vec!["result one".to_string()],
      open_questions: vec![],
      next_steps: vec!["step two".to_string()],
      updated_at: Utc::now(),
    }
  }

  #[tokio::test]
  async fn in_memory_store_round_trips() {
    let store = InMemoryTaskSummaryStore::new();
    assert_eq!(store.get_task_summary("s1").await.unwrap(), None);

    store
      .set_task_summary("s1", sample_summary("goal"))
      .await
      .unwrap();
    let fetched = store.get_task_summary("s1").await.unwrap().unwrap();
    assert_eq!(fetched.goal, "goal");

    store.clear_task_summary("s1").await.unwrap();
    assert_eq!(store.get_task_summary("s1").await.unwrap(), None);
  }

  #[tokio::test]
  async fn in_memory_store_isolates_sessions() {
    let store = InMemoryTaskSummaryStore::new();
    store
      .set_task_summary("s1", sample_summary("goal-1"))
      .await
      .unwrap();
    store
      .set_task_summary("s2", sample_summary("goal-2"))
      .await
      .unwrap();

    assert_eq!(
      store.get_task_summary("s1").await.unwrap().unwrap().goal,
      "goal-1"
    );
    assert_eq!(
      store.get_task_summary("s2").await.unwrap().unwrap().goal,
      "goal-2"
    );
  }

  #[tokio::test]
  async fn sqlite_store_round_trips() {
    let store = SqliteTaskSummaryStore::in_memory().await.unwrap();
    assert_eq!(store.get_task_summary("s1").await.unwrap(), None);

    store
      .set_task_summary("s1", sample_summary("goal"))
      .await
      .unwrap();
    let fetched = store.get_task_summary("s1").await.unwrap().unwrap();
    assert_eq!(fetched.goal, "goal");
    assert_eq!(fetched.completed_steps, vec!["step one".to_string()]);

    store.clear_task_summary("s1").await.unwrap();
    assert_eq!(store.get_task_summary("s1").await.unwrap(), None);
  }

  #[tokio::test]
  async fn sqlite_store_upserts_on_repeated_writes() {
    let store = SqliteTaskSummaryStore::in_memory().await.unwrap();
    store
      .set_task_summary("s1", sample_summary("goal-v1"))
      .await
      .unwrap();
    store
      .set_task_summary("s1", sample_summary("goal-v2"))
      .await
      .unwrap();

    let fetched = store.get_task_summary("s1").await.unwrap().unwrap();
    assert_eq!(fetched.goal, "goal-v2");
  }

  #[tokio::test]
  async fn sqlite_store_persists_across_reopen() {
    let dir = tempfile::TempDir::new().unwrap();
    let db_path = dir.path().join("task_summaries.db");

    {
      let store = SqliteTaskSummaryStore::open(&db_path).await.unwrap();
      store
        .set_task_summary("s1", sample_summary("goal"))
        .await
        .unwrap();
    }

    let reopened = SqliteTaskSummaryStore::open(&db_path).await.unwrap();
    let fetched = reopened.get_task_summary("s1").await.unwrap().unwrap();
    assert_eq!(fetched.goal, "goal");
  }
}

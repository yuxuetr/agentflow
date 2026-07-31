//! Concrete [`ProjectMemoryStore`] implementations (L3.1).
//!
//! The contract (`ProjectMemoryStore` trait + `ProjectFact` +
//! `project_key_for_path`) lives in `agentflow-store-spi` (U2.5, mirroring
//! the `TaskSummaryStore` split in that crate) and is re-exported below
//! under its original `agentflow_memory::project::*` paths so existing call
//! sites keep compiling.
//!
//! Two implementations, mirroring the session-memory split:
//! [`InMemoryProjectMemoryStore`] (ephemeral, process-lifetime — matches
//! [`crate::SessionMemory`]) and [`SqliteProjectMemoryStore`] (persistent —
//! matches [`crate::SqliteMemory`]).
//!
//! Scope of this pass (see `TODOs.md` L3.1): a library capability —
//! `ProjectMemoryStore` + `ProjectFactGenerator` + the two concrete stores
//! below, wired into `agentflow-agents::ReActAgent` via
//! `with_project_memory`. Skill-manifest (`memory.project = true`) and CLI
//! wiring (resolving a real project root for `harness run`/`chat`) are a
//! follow-up: 8 different `SkillBuilder` call sites across 3 crates would
//! need a project-root parameter they don't have today, and only two of
//! them (`harness run`/`chat`) have any "workspace root" concept at all.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;
use sqlx::Row;
use sqlx::sqlite::SqlitePool;

use crate::MemoryError;
pub use agentflow_store_spi::{ProjectFact, ProjectMemoryStore, project_key_for_path};

/// Ephemeral, process-lifetime [`ProjectMemoryStore`] — for tests and
/// short-lived usage that doesn't need facts to survive a restart.
#[derive(Default)]
pub struct InMemoryProjectMemoryStore {
  facts: Mutex<HashMap<String, Vec<ProjectFact>>>,
}

impl InMemoryProjectMemoryStore {
  pub fn new() -> Self {
    Self::default()
  }
}

#[async_trait]
impl ProjectMemoryStore for InMemoryProjectMemoryStore {
  async fn get_project_facts(&self, project_key: &str) -> Result<Vec<ProjectFact>, MemoryError> {
    let facts = self
      .facts
      .lock()
      .map_err(|e| MemoryError::StorageError(format!("project memory mutex poisoned: {e}")))?;
    let mut list = facts.get(project_key).cloned().unwrap_or_default();
    list.sort_by_key(|f| std::cmp::Reverse(f.last_seen));
    Ok(list)
  }

  async fn record_project_fact(
    &self,
    project_key: &str,
    tool: &str,
    command: &str,
  ) -> Result<(), MemoryError> {
    let mut facts = self
      .facts
      .lock()
      .map_err(|e| MemoryError::StorageError(format!("project memory mutex poisoned: {e}")))?;
    let list = facts.entry(project_key.to_string()).or_default();
    let now = Utc::now();
    if let Some(existing) = list
      .iter_mut()
      .find(|f| f.tool == tool && f.command == command)
    {
      existing.last_seen = now;
      existing.observation_count += 1;
    } else {
      list.push(ProjectFact {
        tool: tool.to_string(),
        command: command.to_string(),
        first_seen: now,
        last_seen: now,
        observation_count: 1,
      });
    }
    Ok(())
  }

  async fn clear_project_facts(&self, project_key: &str) -> Result<(), MemoryError> {
    self
      .facts
      .lock()
      .map_err(|e| MemoryError::StorageError(format!("project memory mutex poisoned: {e}")))?
      .remove(project_key);
    Ok(())
  }
}

/// Persistent, SQLite-backed [`ProjectMemoryStore`].
///
/// Schema (single table, one row per `(project_key, tool, command)`):
///
/// ```sql
/// CREATE TABLE project_facts (
///   project_key       TEXT NOT NULL,
///   tool              TEXT NOT NULL,
///   command           TEXT NOT NULL,
///   first_seen        TEXT NOT NULL,
///   last_seen         TEXT NOT NULL,
///   observation_count INTEGER NOT NULL DEFAULT 1,
///   PRIMARY KEY (project_key, tool, command)
/// );
/// ```
pub struct SqliteProjectMemoryStore {
  pool: SqlitePool,
}

impl SqliteProjectMemoryStore {
  pub async fn open<P: AsRef<Path>>(path: P) -> Result<Self, MemoryError> {
    let pool = crate::sqlite_pool::build_pool(path).await?;
    let store = Self { pool };
    store.init_schema().await?;
    Ok(store)
  }

  pub async fn in_memory() -> Result<Self, MemoryError> {
    let pool = crate::sqlite_pool::build_in_memory_pool().await?;
    let store = Self { pool };
    store.init_schema().await?;
    Ok(store)
  }

  async fn init_schema(&self) -> Result<(), MemoryError> {
    sqlx::query(
      "CREATE TABLE IF NOT EXISTS project_facts (
        project_key       TEXT NOT NULL,
        tool              TEXT NOT NULL,
        command           TEXT NOT NULL,
        first_seen        TEXT NOT NULL,
        last_seen         TEXT NOT NULL,
        observation_count INTEGER NOT NULL DEFAULT 1,
        PRIMARY KEY (project_key, tool, command)
      );",
    )
    .execute(&self.pool)
    .await
    .map_err(|e| MemoryError::StorageError(e.to_string()))?;
    Ok(())
  }
}

#[async_trait]
impl ProjectMemoryStore for SqliteProjectMemoryStore {
  async fn get_project_facts(&self, project_key: &str) -> Result<Vec<ProjectFact>, MemoryError> {
    let rows = sqlx::query(
      "SELECT tool, command, first_seen, last_seen, observation_count
       FROM project_facts WHERE project_key = ? ORDER BY last_seen DESC",
    )
    .bind(project_key)
    .fetch_all(&self.pool)
    .await
    .map_err(|e| MemoryError::StorageError(e.to_string()))?;

    rows
      .into_iter()
      .map(|row| {
        let first_seen: String = row
          .try_get("first_seen")
          .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        let last_seen: String = row
          .try_get("last_seen")
          .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        Ok(ProjectFact {
          tool: row
            .try_get("tool")
            .map_err(|e| MemoryError::StorageError(e.to_string()))?,
          command: row
            .try_get("command")
            .map_err(|e| MemoryError::StorageError(e.to_string()))?,
          first_seen: parse_ts(&first_seen),
          last_seen: parse_ts(&last_seen),
          observation_count: row
            .try_get::<i64, _>("observation_count")
            .map_err(|e| MemoryError::StorageError(e.to_string()))?
            as u32,
        })
      })
      .collect()
  }

  async fn record_project_fact(
    &self,
    project_key: &str,
    tool: &str,
    command: &str,
  ) -> Result<(), MemoryError> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
      "INSERT INTO project_facts (project_key, tool, command, first_seen, last_seen, observation_count)
       VALUES (?, ?, ?, ?, ?, 1)
       ON CONFLICT(project_key, tool, command) DO UPDATE SET
         last_seen = excluded.last_seen,
         observation_count = observation_count + 1",
    )
    .bind(project_key)
    .bind(tool)
    .bind(command)
    .bind(&now)
    .bind(&now)
    .execute(&self.pool)
    .await
    .map_err(|e| MemoryError::StorageError(e.to_string()))?;
    Ok(())
  }

  async fn clear_project_facts(&self, project_key: &str) -> Result<(), MemoryError> {
    sqlx::query("DELETE FROM project_facts WHERE project_key = ?")
      .bind(project_key)
      .execute(&self.pool)
      .await
      .map_err(|e| MemoryError::StorageError(e.to_string()))?;
    Ok(())
  }
}

fn parse_ts(s: &str) -> chrono::DateTime<Utc> {
  chrono::DateTime::parse_from_rfc3339(s)
    .map(|dt| dt.with_timezone(&Utc))
    .unwrap_or_else(|_| Utc::now())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn in_memory_store_upserts_repeated_commands() {
    let store = InMemoryProjectMemoryStore::new();
    store
      .record_project_fact("proj", "shell", "cargo build")
      .await
      .unwrap();
    store
      .record_project_fact("proj", "shell", "cargo build")
      .await
      .unwrap();
    store
      .record_project_fact("proj", "shell", "cargo test")
      .await
      .unwrap();

    let facts = store.get_project_facts("proj").await.unwrap();
    assert_eq!(facts.len(), 2);
    let build = facts.iter().find(|f| f.command == "cargo build").unwrap();
    assert_eq!(build.observation_count, 2);
    let test = facts.iter().find(|f| f.command == "cargo test").unwrap();
    assert_eq!(test.observation_count, 1);
  }

  #[tokio::test]
  async fn in_memory_store_isolates_projects() {
    let store = InMemoryProjectMemoryStore::new();
    store
      .record_project_fact("proj-a", "shell", "cargo build")
      .await
      .unwrap();
    store
      .record_project_fact("proj-b", "shell", "npm run build")
      .await
      .unwrap();

    assert_eq!(store.get_project_facts("proj-a").await.unwrap().len(), 1);
    assert_eq!(store.get_project_facts("proj-b").await.unwrap().len(), 1);
  }

  #[tokio::test]
  async fn in_memory_store_clear_removes_all_facts_for_a_project() {
    let store = InMemoryProjectMemoryStore::new();
    store
      .record_project_fact("proj", "shell", "cargo build")
      .await
      .unwrap();
    store.clear_project_facts("proj").await.unwrap();
    assert!(store.get_project_facts("proj").await.unwrap().is_empty());
  }

  #[tokio::test]
  async fn sqlite_store_upserts_repeated_commands() {
    let store = SqliteProjectMemoryStore::in_memory().await.unwrap();
    store
      .record_project_fact("proj", "shell", "cargo build")
      .await
      .unwrap();
    store
      .record_project_fact("proj", "shell", "cargo build")
      .await
      .unwrap();

    let facts = store.get_project_facts("proj").await.unwrap();
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].observation_count, 2);
  }

  #[tokio::test]
  async fn sqlite_store_persists_across_reopen() {
    let dir = tempfile::TempDir::new().unwrap();
    let db_path = dir.path().join("project_facts.db");
    {
      let store = SqliteProjectMemoryStore::open(&db_path).await.unwrap();
      store
        .record_project_fact("proj", "shell", "cargo build")
        .await
        .unwrap();
    }
    let reopened = SqliteProjectMemoryStore::open(&db_path).await.unwrap();
    let facts = reopened.get_project_facts("proj").await.unwrap();
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].command, "cargo build");
  }
}

#![allow(clippy::doc_lazy_continuation)]
//! Shared SQLite pool construction for every memory backend.
//!
//! Q2.1.1 + Q2.1.2: all four SQLite backends (`SqliteMemory`,
//! `SqliteEntityFactStore`, `SqlitePreferenceStore`, `SemanticMemory`)
//! used to hand-roll `format!("sqlite://{}", path)` and ship a pool
//! without `PRAGMA journal_mode = WAL` / `busy_timeout` /
//! `foreign_keys`. Two bugs collapsed into one helper:
//!
//! 1.  Paths containing `?`, `#`, spaces, or backslashes turned the
//!     `sqlite://...` string into an invalid URI — sqlx silently
//!     treated the path as relative or failed obscurely. The helper
//!     constructs the connect string with `SqliteConnectOptions::new()`
//!     + `.filename(...)` instead, so paths are passed byte-for-byte
//!     without URI escaping.
//!
//! 2.  Without WAL + a busy timeout, CLI + agent concurrency hit
//!     `SQLITE_BUSY` under load; foreign-key checks were left disabled,
//!     so an orphan row never tripped a constraint.

use std::path::Path;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

use crate::MemoryError;

/// Default `busy_timeout` for SQLite memory backends.
/// 5 seconds matches Postgres's default lock wait and is long enough
/// to absorb an in-flight write while still surfacing genuine
/// deadlocks.
pub const DEFAULT_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Build connect options that share the Q2.1 hardening across every
/// backend in this crate. Caller passes the on-disk path (or
/// `:memory:` for ephemeral in-memory dbs).
pub(crate) fn connect_options<P: AsRef<Path>>(
  path: P,
  create_if_missing: bool,
) -> SqliteConnectOptions {
  SqliteConnectOptions::new()
    .filename(path)
    .create_if_missing(create_if_missing)
    // Q2.1.1: WAL is the only journal_mode that lets readers and
    // writers run concurrently without a global file lock. CLI +
    // agent workloads hit the lock contention in `DELETE` (default)
    // mode within ~2 concurrent writers.
    .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
    // Q2.1.1: 5 s busy timeout — sqlx surfaces `SQLITE_BUSY` as a
    // hard error otherwise. Long enough to ride out a concurrent
    // write, short enough that a genuine deadlock is surfaced
    // quickly.
    .busy_timeout(DEFAULT_BUSY_TIMEOUT)
    // Q2.1.1: enforce FK constraints. SQLite defaults to "off" for
    // historical compatibility; we use FKs in semantic / entity
    // tables and need them honored at write time.
    .foreign_keys(true)
    // Q2.1.1: synchronous=NORMAL is the documented pairing for WAL
    // (NORMAL is durable across application crashes, only loses
    // committed transactions on a host power loss). FULL adds a
    // significant write-amplification penalty for no correctness
    // gain in this workload.
    .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
}

/// Build an in-memory pool with the same PRAGMA hardening as on-disk
/// backends. Used by `*::in_memory` helpers; `max_connections = 1`
/// because `sqlite::memory:` is private to one connection — multiple
/// readers see disjoint dbs otherwise.
pub(crate) async fn build_in_memory_pool() -> Result<SqlitePool, MemoryError> {
  let options = SqliteConnectOptions::new()
    .filename(":memory:")
    .journal_mode(sqlx::sqlite::SqliteJournalMode::Memory)
    .busy_timeout(DEFAULT_BUSY_TIMEOUT)
    .foreign_keys(true);
  SqlitePoolOptions::new()
    .max_connections(1)
    .connect_with(options)
    .await
    .map_err(|e| MemoryError::StorageError(e.to_string()))
}

/// Build an on-disk pool with the Q2.1 hardening. `max_connections`
/// defaults to 5, which matches the original per-backend value and is
/// the sweet spot for CLI + skill workloads — higher numbers run
/// into SQLite's single-writer ceiling.
pub(crate) async fn build_pool<P: AsRef<Path>>(path: P) -> Result<SqlitePool, MemoryError> {
  let options = connect_options(path, true);
  SqlitePoolOptions::new()
    .max_connections(5)
    .connect_with(options)
    .await
    .map_err(|e| MemoryError::StorageError(e.to_string()))
}

#[cfg(test)]
mod tests {
  use super::*;
  use sqlx::Row;

  /// Q2.1.2 regression: paths containing characters that would
  /// fall apart inside a `sqlite://...` URI now connect cleanly
  /// because `SqliteConnectOptions::filename` accepts the raw path.
  #[tokio::test]
  async fn pool_handles_path_with_special_characters() {
    let dir = tempfile::tempdir().unwrap();
    // Spaces, `?`, `#`, percent — all would have broken the
    // `format!("sqlite://{}", path)` URL.
    let path = dir.path().join("db with spaces? and # signs.sqlite");
    let pool = build_pool(&path)
      .await
      .expect("pool builds on special-char path");
    let row = sqlx::query("SELECT 1 AS v")
      .fetch_one(&pool)
      .await
      .expect("trivial query runs");
    let v: i64 = row.try_get("v").unwrap();
    assert_eq!(v, 1);
  }

  /// Q2.1.1 regression: PRAGMA settings actually apply. We probe
  /// `journal_mode`, `busy_timeout`, and `foreign_keys` on a fresh
  /// pool and assert the configured values land.
  #[tokio::test]
  async fn pool_applies_wal_busy_timeout_and_fk_pragmas() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pragma-probe.sqlite");
    let pool = build_pool(&path).await.unwrap();

    let journal: String = sqlx::query_scalar("PRAGMA journal_mode;")
      .fetch_one(&pool)
      .await
      .unwrap();
    assert_eq!(journal.to_lowercase(), "wal");

    let busy: i64 = sqlx::query_scalar("PRAGMA busy_timeout;")
      .fetch_one(&pool)
      .await
      .unwrap();
    assert!(
      busy >= 5_000,
      "busy_timeout should be at least 5000 ms, got {busy}"
    );

    let fk: i64 = sqlx::query_scalar("PRAGMA foreign_keys;")
      .fetch_one(&pool)
      .await
      .unwrap();
    assert_eq!(fk, 1, "foreign_keys must be enabled");
  }

  /// Q2.1.1 regression: under WAL + busy_timeout the pool tolerates
  /// concurrent writers without surfacing `SQLITE_BUSY`. We spawn
  /// 5 tasks that race to insert 20 rows each and check every
  /// insert lands.
  #[tokio::test]
  async fn pool_handles_concurrent_writers_without_busy_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("concurrent.sqlite");
    let pool = build_pool(&path).await.unwrap();

    sqlx::query("CREATE TABLE probe (id INTEGER PRIMARY KEY AUTOINCREMENT, who TEXT NOT NULL)")
      .execute(&pool)
      .await
      .unwrap();

    let mut handles = Vec::new();
    for task_id in 0..5 {
      let pool = pool.clone();
      handles.push(tokio::spawn(async move {
        for row in 0..20 {
          sqlx::query("INSERT INTO probe (who) VALUES (?)")
            .bind(format!("task-{task_id}-row-{row}"))
            .execute(&pool)
            .await
            .map_err(|e| format!("insert failed: {e}"))?;
        }
        Ok::<_, String>(())
      }));
    }
    for handle in handles {
      handle.await.unwrap().unwrap();
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM probe")
      .fetch_one(&pool)
      .await
      .unwrap();
    assert_eq!(count, 100, "every concurrent insert must land");
  }
}

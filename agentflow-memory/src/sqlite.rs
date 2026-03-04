use std::path::Path;

use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions, SqliteRow};
use sqlx::Row;
use std::str::FromStr;

use crate::{MemoryError, MemoryStore, Message, Role};

/// Persistent SQLite-backed memory store for long-term conversation history.
///
/// Uses `sqlx` async I/O — no blocking thread overhead.
pub struct SqliteMemory {
    pool: SqlitePool,
}

impl SqliteMemory {
    /// Open (or create) a SQLite database file at `path`.
    pub async fn open<P: AsRef<Path>>(path: P) -> Result<Self, MemoryError> {
        let url = format!(
            "sqlite://{}",
            path.as_ref()
                .to_str()
                .ok_or_else(|| MemoryError::StorageError("Invalid path".to_string()))?
        );
        let options = SqliteConnectOptions::from_str(&url)
            .map_err(|e| MemoryError::StorageError(e.to_string()))?
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        let store = Self { pool };
        store.init_schema().await?;
        Ok(store)
    }

    /// In-memory SQLite database — useful for tests or ephemeral sessions.
    pub async fn in_memory() -> Result<Self, MemoryError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        let store = Self { pool };
        store.init_schema().await?;
        Ok(store)
    }

    async fn init_schema(&self) -> Result<(), MemoryError> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS messages (
                id          TEXT NOT NULL PRIMARY KEY,
                session_id  TEXT NOT NULL,
                role        TEXT NOT NULL,
                content     TEXT NOT NULL,
                timestamp   TEXT NOT NULL,
                tool_name   TEXT,
                token_count INTEGER NOT NULL DEFAULT 1
            );",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_messages_session
                ON messages (session_id, timestamp);",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        Ok(())
    }
}

fn row_to_message(row: &SqliteRow) -> Result<Message, MemoryError> {
    let id_str: String = row
        .try_get("id")
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;
    let session_id: String = row
        .try_get("session_id")
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;
    let role_str: String = row
        .try_get("role")
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;
    let content: String = row
        .try_get("content")
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;
    let ts_str: String = row
        .try_get("timestamp")
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;
    let tool_name: Option<String> = row
        .try_get("tool_name")
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;
    let token_count: i64 = row
        .try_get("token_count")
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;

    let id = uuid::Uuid::parse_str(&id_str).unwrap_or_else(|_| uuid::Uuid::new_v4());
    let role = Role::from(role_str.as_str());
    let timestamp = chrono::DateTime::parse_from_rfc3339(&ts_str)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now());

    Ok(Message {
        id,
        session_id,
        role,
        content,
        timestamp,
        tool_name,
        token_count: token_count as u32,
    })
}

#[async_trait]
impl MemoryStore for SqliteMemory {
    async fn add_message(&mut self, message: Message) -> Result<(), MemoryError> {
        sqlx::query(
            "INSERT OR REPLACE INTO messages
                (id, session_id, role, content, timestamp, tool_name, token_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(message.id.to_string())
        .bind(&message.session_id)
        .bind(message.role.as_str())
        .bind(&message.content)
        .bind(message.timestamp.to_rfc3339())
        .bind(&message.tool_name)
        .bind(message.token_count as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        Ok(())
    }

    async fn get_history(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<Message>, MemoryError> {
        let limit = limit.min(i64::MAX as usize) as i64;
        let rows = sqlx::query(
            "SELECT id, session_id, role, content, timestamp, tool_name, token_count
             FROM messages
             WHERE session_id = ?1
             ORDER BY timestamp DESC
             LIMIT ?2",
        )
        .bind(session_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        let mut messages: Vec<Message> = rows
            .iter()
            .map(row_to_message)
            .collect::<Result<_, _>>()?;

        messages.reverse(); // oldest-first
        Ok(messages)
    }

    async fn get_all(&self, session_id: &str) -> Result<Vec<Message>, MemoryError> {
        self.get_history(session_id, usize::MAX).await
    }

    async fn search(
        &self,
        session_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Message>, MemoryError> {
        let limit = limit.min(i64::MAX as usize) as i64;
        let like = format!("%{}%", query);
        let rows = sqlx::query(
            "SELECT id, session_id, role, content, timestamp, tool_name, token_count
             FROM messages
             WHERE session_id = ?1 AND content LIKE ?2
             ORDER BY timestamp DESC
             LIMIT ?3",
        )
        .bind(session_id)
        .bind(&like)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        let mut messages: Vec<Message> = rows
            .iter()
            .map(row_to_message)
            .collect::<Result<_, _>>()?;

        messages.reverse();
        Ok(messages)
    }

    async fn clear_session(&mut self, session_id: &str) -> Result<(), MemoryError> {
        sqlx::query("DELETE FROM messages WHERE session_id = ?1")
            .bind(session_id)
            .execute(&self.pool)
            .await
            .map_err(|e| MemoryError::StorageError(e.to_string()))?;
        Ok(())
    }

    async fn session_token_count(&self, session_id: &str) -> Result<u32, MemoryError> {
        let row = sqlx::query(
            "SELECT COALESCE(SUM(token_count), 0) as total FROM messages WHERE session_id = ?1",
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| MemoryError::StorageError(e.to_string()))?;

        let total: i64 = row.try_get("total").unwrap_or(0);
        Ok(total as u32)
    }
}

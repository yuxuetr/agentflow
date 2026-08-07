//! V2.3: `DbLoopCheckpointer` — the server's Postgres-backed
//! implementation of `agentflow_agent_spi::checkpoint::AgentLoopCheckpointer`.
//!
//! Distinct from `HarnessSessionRepo`/`HarnessEventRepo`: those own the
//! `harness_sessions`/`harness_session_events` audit-trail tables, while
//! this owns `harness_loop_checkpoints` — a single row per session,
//! overwritten in place, holding only the *latest* loop position. See
//! `agentflow-db/migrations/0007_harness_loop_checkpoints.sql`.

use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use agentflow_agent_spi::checkpoint::{
  AgentLoopCheckpoint, AgentLoopCheckpointError, AgentLoopCheckpointer,
};

/// Postgres-backed [`AgentLoopCheckpointer`]. `session_id` strings are
/// parsed as UUIDs (matching `harness_sessions.id`'s column type) — every
/// session this checkpointer is ever asked about originates from the
/// server (`LiveHarnessExecutor`), where session ids are always
/// server-generated UUIDs, unlike the CLI's free-form `--session <id>`.
pub struct DbLoopCheckpointer {
  pool: PgPool,
}

impl DbLoopCheckpointer {
  pub fn new(pool: PgPool) -> Self {
    Self { pool }
  }

  fn parse_session_id(session_id: &str) -> Result<Uuid, AgentLoopCheckpointError> {
    Uuid::parse_str(session_id).map_err(|_| AgentLoopCheckpointError::InvalidSessionId {
      session_id: session_id.to_string(),
    })
  }
}

#[async_trait]
impl AgentLoopCheckpointer for DbLoopCheckpointer {
  async fn save(&self, checkpoint: &AgentLoopCheckpoint) -> Result<(), AgentLoopCheckpointError> {
    let session_id = Self::parse_session_id(&checkpoint.session_id)?;
    let payload = serde_json::to_value(checkpoint).map_err(|e| AgentLoopCheckpointError::Io {
      message: format!("failed to serialize loop checkpoint: {e}"),
    })?;
    sqlx::query(
      r#"INSERT INTO harness_loop_checkpoints (session_id, schema_version, payload, updated_at)
         VALUES ($1, $2, $3, NOW())
         ON CONFLICT (session_id)
         DO UPDATE SET schema_version = EXCLUDED.schema_version,
                        payload = EXCLUDED.payload,
                        updated_at = NOW()"#,
    )
    .bind(session_id)
    .bind(checkpoint.schema_version as i32)
    .bind(payload)
    .execute(&self.pool)
    .await
    .map_err(|e| AgentLoopCheckpointError::Io {
      message: format!("failed to save loop checkpoint: {e}"),
    })?;
    Ok(())
  }

  async fn load(
    &self,
    session_id: &str,
  ) -> Result<Option<AgentLoopCheckpoint>, AgentLoopCheckpointError> {
    let uuid = Self::parse_session_id(session_id)?;
    let row: Option<(serde_json::Value,)> =
      sqlx::query_as("SELECT payload FROM harness_loop_checkpoints WHERE session_id = $1")
        .bind(uuid)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AgentLoopCheckpointError::Io {
          message: format!("failed to load loop checkpoint: {e}"),
        })?;
    let Some((payload,)) = row else {
      return Ok(None);
    };
    let checkpoint: AgentLoopCheckpoint =
      serde_json::from_value(payload).map_err(|e| AgentLoopCheckpointError::Io {
        message: format!("failed to parse loop checkpoint: {e}"),
      })?;
    if checkpoint.schema_version
      > agentflow_agent_spi::checkpoint::AGENT_LOOP_CHECKPOINT_SCHEMA_VERSION
    {
      return Err(AgentLoopCheckpointError::UnsupportedSchemaVersion {
        found: checkpoint.schema_version,
        supported: agentflow_agent_spi::checkpoint::AGENT_LOOP_CHECKPOINT_SCHEMA_VERSION,
      });
    }
    Ok(Some(checkpoint))
  }

  async fn clear(&self, session_id: &str) -> Result<(), AgentLoopCheckpointError> {
    let uuid = Self::parse_session_id(session_id)?;
    sqlx::query("DELETE FROM harness_loop_checkpoints WHERE session_id = $1")
      .bind(uuid)
      .execute(&self.pool)
      .await
      .map_err(|e| AgentLoopCheckpointError::Io {
        message: format!("failed to clear loop checkpoint: {e}"),
      })?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse_session_id_rejects_non_uuid_strings() {
    assert!(matches!(
      DbLoopCheckpointer::parse_session_id("not-a-uuid"),
      Err(AgentLoopCheckpointError::InvalidSessionId { .. })
    ));
    assert!(matches!(
      DbLoopCheckpointer::parse_session_id(""),
      Err(AgentLoopCheckpointError::InvalidSessionId { .. })
    ));
  }

  #[test]
  fn parse_session_id_accepts_a_real_uuid() {
    let id = Uuid::new_v4().to_string();
    assert_eq!(
      DbLoopCheckpointer::parse_session_id(&id)
        .unwrap()
        .to_string(),
      id
    );
  }
}

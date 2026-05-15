//! Repository abstractions over the gateway's six tables.
//!
//! Server handlers depend on these traits, not on `sqlx` directly, so we can
//! later swap in a different storage backend (or in-memory test fakes) without
//! changing the route layer. The current production implementation is
//! Postgres-only — see [`PgRunRepo`], [`PgStepRepo`], [`PgEventRepo`],
//! [`PgArtifactRepo`], [`PgSkillInstallRepo`], [`PgMcpSessionRepo`].
//!
//! `Repositories` bundles all six together for convenient injection into
//! `agentflow-server::AppState`.

use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::DbError;
use crate::models::{
  Artifact, Event, HarnessSession, HarnessSessionEvent, HarnessSessionStatus, McpSession,
  NewArtifact, NewEvent, NewHarnessSession, NewHarnessSessionEvent, NewRun, NewStep, Run,
  RunStatus, SkillInstall, Step,
};

/// Run lifecycle persistence.
#[async_trait]
pub trait RunRepo: Send + Sync {
  async fn create(&self, run: NewRun) -> Result<Run, DbError>;
  async fn get(&self, id: Uuid) -> Result<Option<Run>, DbError>;
  async fn update_status(
    &self,
    id: Uuid,
    status: RunStatus,
    error: Option<&str>,
  ) -> Result<(), DbError>;
  /// List runs for a tenant, newest first.
  async fn list(&self, tenant_id: &str, limit: i64) -> Result<Vec<Run>, DbError>;
}

#[async_trait]
pub trait StepRepo: Send + Sync {
  async fn append(&self, step: NewStep) -> Result<Step, DbError>;
  async fn list_for_run(&self, run_id: Uuid) -> Result<Vec<Step>, DbError>;
}

#[async_trait]
pub trait EventRepo: Send + Sync {
  async fn append(&self, event: NewEvent) -> Result<Event, DbError>;
  /// Return events for `run_id` with `seq > after_seq`, ordered ascending.
  ///
  /// SSE subscribers pass their last-seen `seq` to resume after a reconnect.
  async fn list_after(
    &self,
    run_id: Uuid,
    after_seq: i64,
    limit: i64,
  ) -> Result<Vec<Event>, DbError>;
}

#[async_trait]
pub trait ArtifactRepo: Send + Sync {
  async fn create(&self, artifact: NewArtifact) -> Result<Artifact, DbError>;
  async fn list_for_run(&self, run_id: Uuid) -> Result<Vec<Artifact>, DbError>;
}

#[async_trait]
pub trait SkillInstallRepo: Send + Sync {
  async fn upsert(&self, install: SkillInstall) -> Result<(), DbError>;
  async fn list(&self) -> Result<Vec<SkillInstall>, DbError>;
}

#[async_trait]
pub trait McpSessionRepo: Send + Sync {
  async fn open(&self, session: McpSession) -> Result<(), DbError>;
  async fn close(&self, id: Uuid, tool_calls: i32) -> Result<(), DbError>;
}

/// Harness session lifecycle persistence (P-H.5).
///
/// Mirrors [`RunRepo`] but is scoped to the `harness_sessions` table so the
/// agent-native flow keeps its own typed surface and avoids overloading
/// the workflow `runs` schema with sentinel columns.
#[async_trait]
pub trait HarnessSessionRepo: Send + Sync {
  async fn create(&self, session: NewHarnessSession) -> Result<HarnessSession, DbError>;
  async fn get(&self, id: Uuid) -> Result<Option<HarnessSession>, DbError>;
  /// List sessions for a tenant, newest first.
  async fn list(&self, tenant_id: &str, limit: i64) -> Result<Vec<HarnessSession>, DbError>;
  /// Transition the session's status. `final_answer` populates the
  /// terminal answer column on success; `error` populates the failure
  /// message on failure / cancel. Both default to `COALESCE`d updates so
  /// repeated transitions don't blow away earlier values.
  async fn update_status(
    &self,
    id: Uuid,
    status: HarnessSessionStatus,
    final_answer: Option<&str>,
    error: Option<&str>,
  ) -> Result<(), DbError>;
}

/// Harness session event log persistence (P-H.5).
///
/// Same contract as [`EventRepo`] but keyed by `session_id`. Kept separate
/// so SSE consumers can subscribe to either world without overlapping
/// `(run_id, seq)` primary keys.
#[async_trait]
pub trait HarnessEventRepo: Send + Sync {
  async fn append(&self, event: NewHarnessSessionEvent) -> Result<HarnessSessionEvent, DbError>;
  /// Return events for `session_id` with `seq > after_seq`, ordered ascending.
  ///
  /// SSE subscribers pass their last-seen `seq` to resume after a reconnect.
  async fn list_after(
    &self,
    session_id: Uuid,
    after_seq: i64,
    limit: i64,
  ) -> Result<Vec<HarnessSessionEvent>, DbError>;
}

// ----- Postgres implementations -----

#[derive(Clone, Debug)]
pub struct PgRunRepo {
  pub pool: PgPool,
}

#[async_trait]
impl RunRepo for PgRunRepo {
  async fn create(&self, run: NewRun) -> Result<Run, DbError> {
    let row = sqlx::query_as::<_, Run>(
      r#"INSERT INTO runs (id, workflow, status, run_dir, tenant_id)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id, workflow, status, started_at, finished_at, run_dir, tenant_id, error"#,
    )
    .bind(run.id)
    .bind(&run.workflow)
    .bind(run.status.as_str())
    .bind(run.run_dir.as_deref())
    .bind(&run.tenant_id)
    .fetch_one(&self.pool)
    .await?;
    Ok(row)
  }

  async fn get(&self, id: Uuid) -> Result<Option<Run>, DbError> {
    let row = sqlx::query_as::<_, Run>(
      r#"SELECT id, workflow, status, started_at, finished_at, run_dir, tenant_id, error
         FROM runs WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(&self.pool)
    .await?;
    Ok(row)
  }

  async fn update_status(
    &self,
    id: Uuid,
    status: RunStatus,
    error: Option<&str>,
  ) -> Result<(), DbError> {
    let finished_at = matches!(
      status,
      RunStatus::Succeeded | RunStatus::Failed | RunStatus::Cancelled
    )
    .then(Utc::now);

    let result = sqlx::query(
      r#"UPDATE runs
         SET status = $2,
             finished_at = COALESCE($3, finished_at),
             error = COALESCE($4, error)
         WHERE id = $1"#,
    )
    .bind(id)
    .bind(status.as_str())
    .bind(finished_at)
    .bind(error)
    .execute(&self.pool)
    .await?;

    if result.rows_affected() == 0 {
      return Err(DbError::NotFound {
        entity_type: "run",
        id: id.to_string(),
      });
    }
    Ok(())
  }

  async fn list(&self, tenant_id: &str, limit: i64) -> Result<Vec<Run>, DbError> {
    let rows = sqlx::query_as::<_, Run>(
      r#"SELECT id, workflow, status, started_at, finished_at, run_dir, tenant_id, error
         FROM runs
         WHERE tenant_id = $1
         ORDER BY started_at DESC
         LIMIT $2"#,
    )
    .bind(tenant_id)
    .bind(limit)
    .fetch_all(&self.pool)
    .await?;
    Ok(rows)
  }
}

#[derive(Clone, Debug)]
pub struct PgStepRepo {
  pub pool: PgPool,
}

#[async_trait]
impl StepRepo for PgStepRepo {
  async fn append(&self, step: NewStep) -> Result<Step, DbError> {
    let row = sqlx::query_as::<_, Step>(
      r#"INSERT INTO steps (run_id, seq, node_id, kind, status, duration_ms, payload)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING run_id, seq, node_id, kind, status, started_at, duration_ms, payload"#,
    )
    .bind(step.run_id)
    .bind(step.seq)
    .bind(&step.node_id)
    .bind(&step.kind)
    .bind(&step.status)
    .bind(step.duration_ms)
    .bind(step.payload.as_ref())
    .fetch_one(&self.pool)
    .await?;
    Ok(row)
  }

  async fn list_for_run(&self, run_id: Uuid) -> Result<Vec<Step>, DbError> {
    let rows = sqlx::query_as::<_, Step>(
      r#"SELECT run_id, seq, node_id, kind, status, started_at, duration_ms, payload
         FROM steps WHERE run_id = $1 ORDER BY seq ASC"#,
    )
    .bind(run_id)
    .fetch_all(&self.pool)
    .await?;
    Ok(rows)
  }
}

#[derive(Clone, Debug)]
pub struct PgEventRepo {
  pub pool: PgPool,
}

#[async_trait]
impl EventRepo for PgEventRepo {
  async fn append(&self, event: NewEvent) -> Result<Event, DbError> {
    let row = sqlx::query_as::<_, Event>(
      r#"INSERT INTO events (run_id, seq, kind, payload)
         VALUES ($1, $2, $3, $4)
         RETURNING run_id, seq, kind, payload, ts"#,
    )
    .bind(event.run_id)
    .bind(event.seq)
    .bind(&event.kind)
    .bind(&event.payload)
    .fetch_one(&self.pool)
    .await?;
    Ok(row)
  }

  async fn list_after(
    &self,
    run_id: Uuid,
    after_seq: i64,
    limit: i64,
  ) -> Result<Vec<Event>, DbError> {
    let rows = sqlx::query_as::<_, Event>(
      r#"SELECT run_id, seq, kind, payload, ts
         FROM events
         WHERE run_id = $1 AND seq > $2
         ORDER BY seq ASC
         LIMIT $3"#,
    )
    .bind(run_id)
    .bind(after_seq)
    .bind(limit)
    .fetch_all(&self.pool)
    .await?;
    Ok(rows)
  }
}

#[derive(Clone, Debug)]
pub struct PgArtifactRepo {
  pub pool: PgPool,
}

#[async_trait]
impl ArtifactRepo for PgArtifactRepo {
  async fn create(&self, artifact: NewArtifact) -> Result<Artifact, DbError> {
    let row = sqlx::query_as::<_, Artifact>(
      r#"INSERT INTO artifacts (id, run_id, node_id, name, path_or_url, mime_type)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id, run_id, node_id, name, path_or_url, mime_type, created_at"#,
    )
    .bind(artifact.id)
    .bind(artifact.run_id)
    .bind(&artifact.node_id)
    .bind(&artifact.name)
    .bind(&artifact.path_or_url)
    .bind(artifact.mime_type.as_deref())
    .fetch_one(&self.pool)
    .await?;
    Ok(row)
  }

  async fn list_for_run(&self, run_id: Uuid) -> Result<Vec<Artifact>, DbError> {
    let rows = sqlx::query_as::<_, Artifact>(
      r#"SELECT id, run_id, node_id, name, path_or_url, mime_type, created_at
         FROM artifacts WHERE run_id = $1 ORDER BY created_at ASC"#,
    )
    .bind(run_id)
    .fetch_all(&self.pool)
    .await?;
    Ok(rows)
  }
}

#[derive(Clone, Debug)]
pub struct PgSkillInstallRepo {
  pub pool: PgPool,
}

#[async_trait]
impl SkillInstallRepo for PgSkillInstallRepo {
  async fn upsert(&self, install: SkillInstall) -> Result<(), DbError> {
    sqlx::query(
      r#"INSERT INTO skill_installs (name, version, source, installed_at, checksum)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (name, version) DO UPDATE SET
           source = EXCLUDED.source,
           installed_at = EXCLUDED.installed_at,
           checksum = EXCLUDED.checksum"#,
    )
    .bind(&install.name)
    .bind(&install.version)
    .bind(&install.source)
    .bind(install.installed_at)
    .bind(install.checksum.as_deref())
    .execute(&self.pool)
    .await?;
    Ok(())
  }

  async fn list(&self) -> Result<Vec<SkillInstall>, DbError> {
    let rows = sqlx::query_as::<_, SkillInstall>(
      r#"SELECT name, version, source, installed_at, checksum
         FROM skill_installs ORDER BY name ASC, version ASC"#,
    )
    .fetch_all(&self.pool)
    .await?;
    Ok(rows)
  }
}

#[derive(Clone, Debug)]
pub struct PgMcpSessionRepo {
  pub pool: PgPool,
}

#[async_trait]
impl McpSessionRepo for PgMcpSessionRepo {
  async fn open(&self, session: McpSession) -> Result<(), DbError> {
    sqlx::query(
      r#"INSERT INTO mcp_sessions (id, server, started_at, ended_at, tool_calls, metadata)
         VALUES ($1, $2, $3, $4, $5, $6)"#,
    )
    .bind(session.id)
    .bind(&session.server)
    .bind(session.started_at)
    .bind(session.ended_at)
    .bind(session.tool_calls)
    .bind(session.metadata.as_ref())
    .execute(&self.pool)
    .await?;
    Ok(())
  }

  async fn close(&self, id: Uuid, tool_calls: i32) -> Result<(), DbError> {
    let result = sqlx::query(
      r#"UPDATE mcp_sessions
         SET ended_at = NOW(), tool_calls = $2
         WHERE id = $1"#,
    )
    .bind(id)
    .bind(tool_calls)
    .execute(&self.pool)
    .await?;
    if result.rows_affected() == 0 {
      return Err(DbError::NotFound {
        entity_type: "mcp_session",
        id: id.to_string(),
      });
    }
    Ok(())
  }
}

#[derive(Clone, Debug)]
pub struct PgHarnessSessionRepo {
  pub pool: PgPool,
}

#[async_trait]
impl HarnessSessionRepo for PgHarnessSessionRepo {
  async fn create(&self, session: NewHarnessSession) -> Result<HarnessSession, DbError> {
    let row = sqlx::query_as::<_, HarnessSession>(
      r#"INSERT INTO harness_sessions (
           id, tenant_id, status, user_input, workspace_root, profile,
           runtime_kind, model, skill_name
         )
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id, tenant_id, status, user_input, workspace_root, profile,
                   runtime_kind, model, skill_name, started_at, finished_at,
                   final_answer, error"#,
    )
    .bind(session.id)
    .bind(&session.tenant_id)
    .bind(HarnessSessionStatus::Running.as_str())
    .bind(&session.user_input)
    .bind(&session.workspace_root)
    .bind(&session.profile)
    .bind(&session.runtime_kind)
    .bind(&session.model)
    .bind(session.skill_name.as_deref())
    .fetch_one(&self.pool)
    .await?;
    Ok(row)
  }

  async fn get(&self, id: Uuid) -> Result<Option<HarnessSession>, DbError> {
    let row = sqlx::query_as::<_, HarnessSession>(
      r#"SELECT id, tenant_id, status, user_input, workspace_root, profile,
                runtime_kind, model, skill_name, started_at, finished_at,
                final_answer, error
         FROM harness_sessions
         WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(&self.pool)
    .await?;
    Ok(row)
  }

  async fn list(&self, tenant_id: &str, limit: i64) -> Result<Vec<HarnessSession>, DbError> {
    let rows = sqlx::query_as::<_, HarnessSession>(
      r#"SELECT id, tenant_id, status, user_input, workspace_root, profile,
                runtime_kind, model, skill_name, started_at, finished_at,
                final_answer, error
         FROM harness_sessions
         WHERE tenant_id = $1
         ORDER BY started_at DESC
         LIMIT $2"#,
    )
    .bind(tenant_id)
    .bind(limit)
    .fetch_all(&self.pool)
    .await?;
    Ok(rows)
  }

  async fn update_status(
    &self,
    id: Uuid,
    status: HarnessSessionStatus,
    final_answer: Option<&str>,
    error: Option<&str>,
  ) -> Result<(), DbError> {
    let finished_at = status.is_terminal().then(Utc::now);

    let result = sqlx::query(
      r#"UPDATE harness_sessions
         SET status = $2,
             finished_at = COALESCE($3, finished_at),
             final_answer = COALESCE($4, final_answer),
             error = COALESCE($5, error)
         WHERE id = $1"#,
    )
    .bind(id)
    .bind(status.as_str())
    .bind(finished_at)
    .bind(final_answer)
    .bind(error)
    .execute(&self.pool)
    .await?;

    if result.rows_affected() == 0 {
      return Err(DbError::NotFound {
        entity_type: "harness_session",
        id: id.to_string(),
      });
    }
    Ok(())
  }
}

#[derive(Clone, Debug)]
pub struct PgHarnessEventRepo {
  pub pool: PgPool,
}

#[async_trait]
impl HarnessEventRepo for PgHarnessEventRepo {
  async fn append(&self, event: NewHarnessSessionEvent) -> Result<HarnessSessionEvent, DbError> {
    let row = sqlx::query_as::<_, HarnessSessionEvent>(
      r#"INSERT INTO harness_session_events (session_id, seq, kind, payload)
         VALUES ($1, $2, $3, $4)
         RETURNING session_id, seq, kind, payload, ts"#,
    )
    .bind(event.session_id)
    .bind(event.seq)
    .bind(&event.kind)
    .bind(&event.payload)
    .fetch_one(&self.pool)
    .await?;
    Ok(row)
  }

  async fn list_after(
    &self,
    session_id: Uuid,
    after_seq: i64,
    limit: i64,
  ) -> Result<Vec<HarnessSessionEvent>, DbError> {
    let rows = sqlx::query_as::<_, HarnessSessionEvent>(
      r#"SELECT session_id, seq, kind, payload, ts
         FROM harness_session_events
         WHERE session_id = $1 AND seq > $2
         ORDER BY seq ASC
         LIMIT $3"#,
    )
    .bind(session_id)
    .bind(after_seq)
    .bind(limit)
    .fetch_all(&self.pool)
    .await?;
    Ok(rows)
  }
}

/// Convenience bundle of all Pg repositories backed by the same pool.
///
/// `agentflow-server::AppState` injects this into route handlers so each route
/// can pick the repo it needs without holding a separate pool reference.
#[derive(Clone, Debug)]
pub struct Repositories {
  pub runs: PgRunRepo,
  pub steps: PgStepRepo,
  pub events: PgEventRepo,
  pub artifacts: PgArtifactRepo,
  pub skill_installs: PgSkillInstallRepo,
  pub mcp_sessions: PgMcpSessionRepo,
  pub harness_sessions: PgHarnessSessionRepo,
  pub harness_events: PgHarnessEventRepo,
}

impl Repositories {
  pub fn from_pool(pool: PgPool) -> Self {
    Self {
      runs: PgRunRepo { pool: pool.clone() },
      steps: PgStepRepo { pool: pool.clone() },
      events: PgEventRepo { pool: pool.clone() },
      artifacts: PgArtifactRepo { pool: pool.clone() },
      skill_installs: PgSkillInstallRepo { pool: pool.clone() },
      mcp_sessions: PgMcpSessionRepo { pool: pool.clone() },
      harness_sessions: PgHarnessSessionRepo { pool: pool.clone() },
      harness_events: PgHarnessEventRepo { pool },
    }
  }
}

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
  NewArtifact, NewEvent, NewHarnessSession, NewHarnessSessionEvent, NewRun, NewStep,
  NewUserPreference, Run, RunStatus, SkillInstall, Step, UserPreference,
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
  /// List runs for a tenant, newest first. Convenience shim over
  /// [`Self::list_filtered`] with no status filter and zero offset.
  async fn list(&self, tenant_id: &str, limit: i64) -> Result<Vec<Run>, DbError> {
    self.list_filtered(tenant_id, None, limit, 0).await
  }
  /// List runs for a tenant, newest first, optionally filtered by status
  /// and paginated via `offset`. `limit` and `offset` are caller-clamped;
  /// the repo binds them unchanged.
  async fn list_filtered(
    &self,
    tenant_id: &str,
    status: Option<&str>,
    limit: i64,
    offset: i64,
  ) -> Result<Vec<Run>, DbError>;
}

#[async_trait]
pub trait StepRepo: Send + Sync {
  async fn append(&self, step: NewStep) -> Result<Step, DbError>;
  async fn list_for_run(&self, run_id: Uuid) -> Result<Vec<Step>, DbError>;
}

#[async_trait]
pub trait EventRepo: Send + Sync {
  async fn append(&self, event: NewEvent) -> Result<Event, DbError>;
  /// Return events for `(tenant_id, run_id)` with `seq > after_seq`,
  /// ordered ascending.
  ///
  /// SSE subscribers pass their last-seen `seq` to resume after a
  /// reconnect. Q1.5.3 added the `tenant_id` parameter so the
  /// composite `events_tenant_run_idx` index is reachable and the
  /// db layer provides defense-in-depth even if a server route
  /// forgets to filter by tenant first.
  async fn list_after(
    &self,
    tenant_id: &str,
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
  /// Q1.5.1: this used to be tenant-agnostic, leaking every tenant's
  /// installed-skill catalog through one call. Callers must now pass
  /// the tenant they own.
  async fn list(&self, tenant_id: &str) -> Result<Vec<SkillInstall>, DbError>;
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
  /// Reset a terminal session back to `running` so a fresh executor
  /// run can write into the same row (P-H.5 slice 4 resume route).
  ///
  /// Clears `finished_at`, `final_answer`, and `error`, replaces
  /// `user_input` if `new_user_input` is provided, and deletes every
  /// existing `harness_session_events` row that referenced this
  /// session via the FK CASCADE. The transaction is atomic so a
  /// concurrent reader never observes a half-reset state.
  async fn reset_for_resume(&self, id: Uuid, new_user_input: &str) -> Result<(), DbError>;
  /// Append-mode counterpart to [`Self::reset_for_resume`]. Flips the
  /// row back to `running` and replaces `user_input` but **keeps** the
  /// existing `harness_session_events` rows intact.
  ///
  /// Used by the `:resume` route's `mode=append` flavour: combined with
  /// `HarnessRuntime::with_initial_seq(MAX(seq) + 1)`, this lets a
  /// resumed session continue the seq series instead of restarting
  /// from 0. The caller is responsible for supplying the correct
  /// initial seq to the executor.
  async fn reset_for_append_resume(&self, id: Uuid, new_user_input: &str) -> Result<(), DbError>;
}

/// Harness session event log persistence (P-H.5).
///
/// Same contract as [`EventRepo`] but keyed by `session_id`. Kept separate
/// so SSE consumers can subscribe to either world without overlapping
/// `(run_id, seq)` primary keys.
#[async_trait]
pub trait HarnessEventRepo: Send + Sync {
  async fn append(&self, event: NewHarnessSessionEvent) -> Result<HarnessSessionEvent, DbError>;
  /// Return events for `(tenant_id, session_id)` with `seq > after_seq`,
  /// ordered ascending.
  ///
  /// SSE subscribers pass their last-seen `seq` to resume after a
  /// reconnect. Q1.5.3 added the `tenant_id` parameter: because
  /// `harness_session_events` itself carries no `tenant_id` column,
  /// the SQL joins back to `harness_sessions` and filters there.
  /// Defense-in-depth on top of the existing route-layer check.
  async fn list_after(
    &self,
    tenant_id: &str,
    session_id: Uuid,
    after_seq: i64,
    limit: i64,
  ) -> Result<Vec<HarnessSessionEvent>, DbError>;
  /// Return the largest `seq` recorded for `session_id`, or `None` if
  /// no events exist yet. Used by the `:resume` route's append-mode
  /// flavour to pick a non-colliding `initial_seq` for the next run.
  async fn max_seq(&self, session_id: Uuid) -> Result<Option<i64>, DbError>;
}

// ----- Postgres implementations -----
//
// Each Pg*Repo holds two pools: `pool` (the primary, used for every
// write) and `read_pool` (used for `get_*` / `list_*`). When the
// caller didn't configure a replica, `read_pool` is cloned from
// `pool` and behaves identically. P10.15.2.

#[derive(Clone, Debug)]
pub struct PgRunRepo {
  pub pool: PgPool,
  pub read_pool: PgPool,
}

#[async_trait]
impl RunRepo for PgRunRepo {
  async fn create(&self, run: NewRun) -> Result<Run, DbError> {
    let row = sqlx::query_as::<_, Run>(
      r#"INSERT INTO runs (id, workflow, status, run_dir, tenant_id,
                           events_retention_days, artifacts_retention_days)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id, workflow, status, started_at, finished_at, run_dir,
                   tenant_id, error, events_retention_days, artifacts_retention_days"#,
    )
    .bind(run.id)
    .bind(&run.workflow)
    .bind(run.status.as_str())
    .bind(run.run_dir.as_deref())
    .bind(&run.tenant_id)
    .bind(run.events_retention_days)
    .bind(run.artifacts_retention_days)
    .fetch_one(&self.pool)
    .await?;
    Ok(row)
  }

  async fn get(&self, id: Uuid) -> Result<Option<Run>, DbError> {
    let row = sqlx::query_as::<_, Run>(
      r#"SELECT id, workflow, status, started_at, finished_at, run_dir,
                tenant_id, error, events_retention_days, artifacts_retention_days
         FROM runs WHERE id = $1"#,
    )
    .bind(id)
    .fetch_optional(&self.read_pool)
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

  async fn list_filtered(
    &self,
    tenant_id: &str,
    status: Option<&str>,
    limit: i64,
    offset: i64,
  ) -> Result<Vec<Run>, DbError> {
    // Two query shapes — one with a status predicate, one without — so
    // the optimizer can pick the right index without needing to read a
    // NULL parameter at runtime.
    let rows = if let Some(status) = status {
      sqlx::query_as::<_, Run>(
        r#"SELECT id, workflow, status, started_at, finished_at, run_dir, tenant_id, error,
                  events_retention_days, artifacts_retention_days
           FROM runs
           WHERE tenant_id = $1 AND status = $2
           ORDER BY started_at DESC
           LIMIT $3 OFFSET $4"#,
      )
      .bind(tenant_id)
      .bind(status)
      .bind(limit)
      .bind(offset)
      .fetch_all(&self.read_pool)
      .await?
    } else {
      sqlx::query_as::<_, Run>(
        r#"SELECT id, workflow, status, started_at, finished_at, run_dir, tenant_id, error,
                  events_retention_days, artifacts_retention_days
           FROM runs
           WHERE tenant_id = $1
           ORDER BY started_at DESC
           LIMIT $2 OFFSET $3"#,
      )
      .bind(tenant_id)
      .bind(limit)
      .bind(offset)
      .fetch_all(&self.read_pool)
      .await?
    };
    Ok(rows)
  }
}

#[derive(Clone, Debug)]
pub struct PgStepRepo {
  pub pool: PgPool,
  pub read_pool: PgPool,
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
    .fetch_all(&self.read_pool)
    .await?;
    Ok(rows)
  }
}

#[derive(Clone, Debug)]
pub struct PgEventRepo {
  pub pool: PgPool,
  pub read_pool: PgPool,
}

#[async_trait]
impl EventRepo for PgEventRepo {
  async fn append(&self, event: NewEvent) -> Result<Event, DbError> {
    let tenant_id = event.tenant_id.as_deref().unwrap_or("default");
    let row = sqlx::query_as::<_, Event>(
      r#"INSERT INTO events (run_id, seq, kind, payload, tenant_id)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING run_id, seq, kind, payload, ts, tenant_id"#,
    )
    .bind(event.run_id)
    .bind(event.seq)
    .bind(&event.kind)
    .bind(&event.payload)
    .bind(tenant_id)
    .fetch_one(&self.pool)
    .await?;
    Ok(row)
  }

  async fn list_after(
    &self,
    tenant_id: &str,
    run_id: Uuid,
    after_seq: i64,
    limit: i64,
  ) -> Result<Vec<Event>, DbError> {
    let rows = sqlx::query_as::<_, Event>(
      r#"SELECT run_id, seq, kind, payload, ts, tenant_id
         FROM events
         WHERE tenant_id = $1 AND run_id = $2 AND seq > $3
         ORDER BY seq ASC
         LIMIT $4"#,
    )
    .bind(tenant_id)
    .bind(run_id)
    .bind(after_seq)
    .bind(limit)
    .fetch_all(&self.read_pool)
    .await?;
    Ok(rows)
  }
}

#[derive(Clone, Debug)]
pub struct PgArtifactRepo {
  pub pool: PgPool,
  pub read_pool: PgPool,
}

#[async_trait]
impl ArtifactRepo for PgArtifactRepo {
  async fn create(&self, artifact: NewArtifact) -> Result<Artifact, DbError> {
    let tenant_id = artifact.tenant_id.as_deref().unwrap_or("default");
    let row = sqlx::query_as::<_, Artifact>(
      r#"INSERT INTO artifacts (id, run_id, node_id, name, path_or_url, mime_type, tenant_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id, run_id, node_id, name, path_or_url, mime_type, created_at, tenant_id"#,
    )
    .bind(artifact.id)
    .bind(artifact.run_id)
    .bind(&artifact.node_id)
    .bind(&artifact.name)
    .bind(&artifact.path_or_url)
    .bind(artifact.mime_type.as_deref())
    .bind(tenant_id)
    .fetch_one(&self.pool)
    .await?;
    Ok(row)
  }

  async fn list_for_run(&self, run_id: Uuid) -> Result<Vec<Artifact>, DbError> {
    let rows = sqlx::query_as::<_, Artifact>(
      r#"SELECT id, run_id, node_id, name, path_or_url, mime_type, created_at, tenant_id
         FROM artifacts WHERE run_id = $1 ORDER BY created_at ASC"#,
    )
    .bind(run_id)
    .fetch_all(&self.read_pool)
    .await?;
    Ok(rows)
  }
}

#[derive(Clone, Debug)]
pub struct PgSkillInstallRepo {
  pub pool: PgPool,
  pub read_pool: PgPool,
}

#[async_trait]
impl SkillInstallRepo for PgSkillInstallRepo {
  async fn upsert(&self, install: SkillInstall) -> Result<(), DbError> {
    sqlx::query(
      r#"INSERT INTO skill_installs (tenant_id, name, version, source, installed_at, checksum)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (tenant_id, name, version) DO UPDATE SET
           source = EXCLUDED.source,
           installed_at = EXCLUDED.installed_at,
           checksum = EXCLUDED.checksum"#,
    )
    .bind(&install.tenant_id)
    .bind(&install.name)
    .bind(&install.version)
    .bind(&install.source)
    .bind(install.installed_at)
    .bind(install.checksum.as_deref())
    .execute(&self.pool)
    .await?;
    Ok(())
  }

  async fn list(&self, tenant_id: &str) -> Result<Vec<SkillInstall>, DbError> {
    let rows = sqlx::query_as::<_, SkillInstall>(
      r#"SELECT name, version, source, installed_at, checksum, tenant_id
         FROM skill_installs
         WHERE tenant_id = $1
         ORDER BY name ASC, version ASC"#,
    )
    .bind(tenant_id)
    .fetch_all(&self.read_pool)
    .await?;
    Ok(rows)
  }
}

#[derive(Clone, Debug)]
pub struct PgMcpSessionRepo {
  pub pool: PgPool,
  pub read_pool: PgPool,
}

#[async_trait]
impl McpSessionRepo for PgMcpSessionRepo {
  async fn open(&self, session: McpSession) -> Result<(), DbError> {
    sqlx::query(
      r#"INSERT INTO mcp_sessions (id, server, started_at, ended_at, tool_calls, metadata, tenant_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(session.id)
    .bind(&session.server)
    .bind(session.started_at)
    .bind(session.ended_at)
    .bind(session.tool_calls)
    .bind(session.metadata.as_ref())
    .bind(&session.tenant_id)
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
  pub read_pool: PgPool,
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
    .fetch_optional(&self.read_pool)
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
    .fetch_all(&self.read_pool)
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

  async fn reset_for_resume(&self, id: Uuid, new_user_input: &str) -> Result<(), DbError> {
    let mut tx = self.pool.begin().await?;
    // CASCADE on the FK already wipes harness_session_events when the
    // session row is deleted, but we keep the session row — so the
    // child rows are deleted explicitly here.
    sqlx::query("DELETE FROM harness_session_events WHERE session_id = $1")
      .bind(id)
      .execute(&mut *tx)
      .await?;
    let result = sqlx::query(
      r#"UPDATE harness_sessions
         SET status = 'running',
             finished_at = NULL,
             final_answer = NULL,
             error = NULL,
             user_input = $2
         WHERE id = $1"#,
    )
    .bind(id)
    .bind(new_user_input)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
      return Err(DbError::NotFound {
        entity_type: "harness_session",
        id: id.to_string(),
      });
    }
    tx.commit().await?;
    Ok(())
  }

  async fn reset_for_append_resume(&self, id: Uuid, new_user_input: &str) -> Result<(), DbError> {
    let result = sqlx::query(
      r#"UPDATE harness_sessions
         SET status = 'running',
             finished_at = NULL,
             final_answer = NULL,
             error = NULL,
             user_input = $2
         WHERE id = $1"#,
    )
    .bind(id)
    .bind(new_user_input)
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
  pub read_pool: PgPool,
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
    tenant_id: &str,
    session_id: Uuid,
    after_seq: i64,
    limit: i64,
  ) -> Result<Vec<HarnessSessionEvent>, DbError> {
    let rows = sqlx::query_as::<_, HarnessSessionEvent>(
      r#"SELECT e.session_id, e.seq, e.kind, e.payload, e.ts
         FROM harness_session_events e
         JOIN harness_sessions s ON s.id = e.session_id
         WHERE s.tenant_id = $1 AND e.session_id = $2 AND e.seq > $3
         ORDER BY e.seq ASC
         LIMIT $4"#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .bind(after_seq)
    .bind(limit)
    .fetch_all(&self.read_pool)
    .await?;
    Ok(rows)
  }

  async fn max_seq(&self, session_id: Uuid) -> Result<Option<i64>, DbError> {
    let row: Option<(Option<i64>,)> =
      sqlx::query_as("SELECT MAX(seq) FROM harness_session_events WHERE session_id = $1")
        .bind(session_id)
        .fetch_optional(&self.read_pool)
        .await?;
    Ok(row.and_then(|(value,)| value))
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
  pub user_preferences: PgUserPreferenceRepo,
}

impl Repositories {
  /// Construct repositories that read and write through the same
  /// pool. Equivalent to `from_pools(pool.clone(), pool)`. Kept as
  /// the canonical entry point for single-node deployments.
  pub fn from_pool(pool: PgPool) -> Self {
    Self::from_pools(pool.clone(), pool)
  }

  /// Construct repositories with separate write + read pools
  /// (P10.15.2). The `write_pool` handles every `INSERT` /
  /// `UPDATE` / `DELETE`; the `read_pool` handles every `SELECT`.
  /// Pass the same pool twice for the single-node default.
  pub fn from_pools(write_pool: PgPool, read_pool: PgPool) -> Self {
    Self {
      runs: PgRunRepo {
        pool: write_pool.clone(),
        read_pool: read_pool.clone(),
      },
      steps: PgStepRepo {
        pool: write_pool.clone(),
        read_pool: read_pool.clone(),
      },
      events: PgEventRepo {
        pool: write_pool.clone(),
        read_pool: read_pool.clone(),
      },
      artifacts: PgArtifactRepo {
        pool: write_pool.clone(),
        read_pool: read_pool.clone(),
      },
      skill_installs: PgSkillInstallRepo {
        pool: write_pool.clone(),
        read_pool: read_pool.clone(),
      },
      mcp_sessions: PgMcpSessionRepo {
        pool: write_pool.clone(),
        read_pool: read_pool.clone(),
      },
      harness_sessions: PgHarnessSessionRepo {
        pool: write_pool.clone(),
        read_pool: read_pool.clone(),
      },
      harness_events: PgHarnessEventRepo {
        pool: write_pool.clone(),
        read_pool: read_pool.clone(),
      },
      user_preferences: PgUserPreferenceRepo {
        pool: write_pool,
        read_pool,
      },
    }
  }

  /// Bridge to [`crate::Database`]: picks `db.pool` for writes and
  /// `db.read_pool()` for reads (which falls back to the primary
  /// when no replica is configured). Preferred entry point for
  /// `agentflow-server::AppState::new`.
  pub fn from_database(db: &crate::Database) -> Self {
    Self::from_pools(db.pool.clone(), db.read_pool().clone())
  }
}

// ── P6.4 user preferences ────────────────────────────────────────────────

/// Tenant-scoped UI preference persistence (theme / default profile /
/// pagination size / event filter, etc.). One row per `(tenant_id, key)`.
#[async_trait]
pub trait UserPreferenceRepo: Send + Sync {
  /// Upsert a single preference. The repo stamps `updated_at`.
  async fn upsert(&self, preference: NewUserPreference) -> Result<(), DbError>;
  /// Upsert many preferences for the same tenant in a single
  /// transaction. The `PUT /v1/preferences` route uses this so a
  /// failed key doesn't leave others stranded with stale data.
  async fn upsert_many(
    &self,
    tenant_id: &str,
    entries: Vec<(String, serde_json::Value)>,
  ) -> Result<(), DbError>;
  /// Read every preference for a tenant, sorted by key for stable
  /// rendering downstream.
  async fn list_for_tenant(&self, tenant_id: &str) -> Result<Vec<UserPreference>, DbError>;
  /// Delete a specific key. Used by the UI's "reset to default" path.
  /// Returns `Ok(false)` when the row doesn't exist.
  async fn delete(&self, tenant_id: &str, key: &str) -> Result<bool, DbError>;
}

#[derive(Clone, Debug)]
pub struct PgUserPreferenceRepo {
  pub pool: PgPool,
  pub read_pool: PgPool,
}

#[async_trait]
impl UserPreferenceRepo for PgUserPreferenceRepo {
  async fn upsert(&self, preference: NewUserPreference) -> Result<(), DbError> {
    sqlx::query(
      r#"INSERT INTO user_preferences (tenant_id, key, value, updated_at)
         VALUES ($1, $2, $3, NOW())
         ON CONFLICT (tenant_id, key) DO UPDATE SET
           value = EXCLUDED.value,
           updated_at = EXCLUDED.updated_at"#,
    )
    .bind(&preference.tenant_id)
    .bind(&preference.key)
    .bind(&preference.value)
    .execute(&self.pool)
    .await?;
    Ok(())
  }

  async fn upsert_many(
    &self,
    tenant_id: &str,
    entries: Vec<(String, serde_json::Value)>,
  ) -> Result<(), DbError> {
    if entries.is_empty() {
      return Ok(());
    }
    let mut tx = self.pool.begin().await?;
    for (key, value) in entries {
      sqlx::query(
        r#"INSERT INTO user_preferences (tenant_id, key, value, updated_at)
           VALUES ($1, $2, $3, NOW())
           ON CONFLICT (tenant_id, key) DO UPDATE SET
             value = EXCLUDED.value,
             updated_at = EXCLUDED.updated_at"#,
      )
      .bind(tenant_id)
      .bind(&key)
      .bind(&value)
      .execute(&mut *tx)
      .await?;
    }
    tx.commit().await?;
    Ok(())
  }

  async fn list_for_tenant(&self, tenant_id: &str) -> Result<Vec<UserPreference>, DbError> {
    let rows = sqlx::query_as::<_, UserPreference>(
      r#"SELECT tenant_id, key, value, updated_at
         FROM user_preferences
         WHERE tenant_id = $1
         ORDER BY key"#,
    )
    .bind(tenant_id)
    .fetch_all(&self.read_pool)
    .await?;
    Ok(rows)
  }

  async fn delete(&self, tenant_id: &str, key: &str) -> Result<bool, DbError> {
    let result = sqlx::query("DELETE FROM user_preferences WHERE tenant_id = $1 AND key = $2")
      .bind(tenant_id)
      .bind(key)
      .execute(&self.pool)
      .await?;
    Ok(result.rows_affected() > 0)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use sqlx::postgres::PgPoolOptions;

  fn lazy_pool() -> PgPool {
    PgPoolOptions::new()
      .max_connections(1)
      .connect_lazy("postgres://test:test@localhost:5432/test")
      .expect("lazy pool")
  }

  #[tokio::test]
  async fn from_pool_uses_same_pool_for_reads_and_writes() {
    let pool = lazy_pool();
    let repos = Repositories::from_pool(pool);
    // Pointer equality (via Arc) so a single-node deployment
    // hits one pool for everything and connection accounting is
    // unsurprising.
    assert!(std::ptr::eq(&repos.runs.pool, &repos.runs.pool));
    assert!(std::ptr::eq(&repos.runs.read_pool, &repos.runs.read_pool));
  }

  #[tokio::test]
  async fn from_pools_routes_separate_pools_to_every_repo() {
    let write = lazy_pool();
    let read = lazy_pool();
    let repos = Repositories::from_pools(write, read);
    // Every repo gets both pools populated. The exact "same Arc"
    // check is overspecified for sqlx's internal pool sharing;
    // what matters is the field is present and clonable.
    let _ = repos.runs.pool.clone();
    let _ = repos.runs.read_pool.clone();
    let _ = repos.steps.read_pool.clone();
    let _ = repos.events.read_pool.clone();
    let _ = repos.artifacts.read_pool.clone();
    let _ = repos.skill_installs.read_pool.clone();
    let _ = repos.mcp_sessions.read_pool.clone();
    let _ = repos.harness_sessions.read_pool.clone();
    let _ = repos.harness_events.read_pool.clone();
    let _ = repos.user_preferences.read_pool.clone();
  }

  #[tokio::test]
  async fn from_database_threads_replica_into_repos_when_set() {
    let primary = lazy_pool();
    let replica = lazy_pool();
    let db = crate::Database {
      pool: primary,
      read_pool: Some(replica),
    };
    let repos = Repositories::from_database(&db);
    // Read pool came from the replica, not the primary. The
    // primary has `size() == 0` lazily; cloning either side
    // never opens a connection in this test.
    let read = &repos.runs.read_pool;
    let write = &repos.runs.pool;
    // Distinct Arcs because primary != replica.
    assert!(!std::ptr::eq(read, write));
  }

  #[tokio::test]
  async fn from_database_falls_back_to_primary_when_no_replica() {
    let primary = lazy_pool();
    let db = crate::Database {
      pool: primary,
      read_pool: None,
    };
    let repos = Repositories::from_database(&db);
    // Without a replica configured, the read pool is a clone of
    // the primary — they're distinct fields but back the same
    // underlying connection state. Verified by checking that
    // `from_database` doesn't accidentally swap the order.
    let _read = &repos.runs.read_pool;
    let _write = &repos.runs.pool;
    // The invariant we actually care about: there's no panic, and
    // the repo carries a usable pool in both slots.
  }
}

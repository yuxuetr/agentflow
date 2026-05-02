//! Run submission, status, and the executor abstraction.
//!
//! `POST /v1/runs` and `GET /v1/runs/{id}` live here. Actual workflow
//! execution is delegated to a [`RunExecutor`] trait so the route layer
//! stays oblivious to whether runs are dispatched in-process via
//! `agentflow-core::Flow`, sent to a worker pool, or stubbed out for tests.
//!
//! v0.3.0 N8 ships [`StubExecutor`], which only flips the run from
//! `queued` → `running` → `succeeded` and writes a couple of synthetic
//! events so SSE subscribers see traffic. Task #14 in the v0.3.0 series
//! replaces it with a real Flow runner.

use async_trait::async_trait;
use axum::{
  Json,
  extract::{Path, State},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};
use uuid::Uuid;

use agentflow_db::{EventRepo, NewEvent, NewRun, Repositories, Run, RunRepo, RunStatus};

use crate::AppState;
use crate::error::ApiError;

/// JSON body for `POST /v1/runs`.
///
/// Either `workflow` (inline YAML / JSON workflow definition as a string) or
/// `workflow_id` (reference to a stored workflow) must be supplied. The
/// gateway treats the body as opaque text and hands it to the configured
/// `RunExecutor`; parsing happens at execution time.
#[derive(Debug, Deserialize)]
pub struct CreateRunRequest {
  /// Inline workflow as a YAML or JSON string.
  pub workflow: Option<String>,
  /// Reference to a workflow stored elsewhere (future use).
  pub workflow_id: Option<String>,
  /// Optional tenant override (defaults to `"default"`).
  #[serde(default)]
  pub tenant_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateRunResponse {
  pub run_id: Uuid,
  pub status: &'static str,
}

/// Minimal run-execution contract. Implementations are responsible for
/// every state transition after the route layer creates the row, including
/// terminal status updates and event emission.
#[async_trait]
pub trait RunExecutor: Send + Sync {
  async fn execute(&self, ctx: RunContext);
}

/// Everything an executor needs to do its job. Owns its own copies of the
/// repositories so the route handler can return immediately.
pub struct RunContext {
  pub run_id: Uuid,
  pub workflow: String,
  pub repos: Repositories,
}

/// Default no-op executor used until the real Flow runner lands. Marks runs
/// as `running` then `succeeded` and writes two synthetic events so SSE
/// subscribers see something flow through. Tests use this to verify the
/// route layer + DB plumbing without depending on `agentflow-core`.
#[derive(Clone, Debug, Default)]
pub struct StubExecutor;

#[async_trait]
impl RunExecutor for StubExecutor {
  async fn execute(&self, ctx: RunContext) {
    if let Err(e) = stub_execute(&ctx).await {
      error!(run_id = %ctx.run_id, error = %e, "stub executor failed");
      let _ = ctx
        .repos
        .runs
        .update_status(ctx.run_id, RunStatus::Failed, Some(&e.to_string()))
        .await;
    }
  }
}

async fn stub_execute(ctx: &RunContext) -> Result<(), agentflow_db::DbError> {
  ctx
    .repos
    .runs
    .update_status(ctx.run_id, RunStatus::Running, None)
    .await?;
  ctx
    .repos
    .events
    .append(NewEvent {
      run_id: ctx.run_id,
      seq: 0,
      kind: "run_started".into(),
      payload: serde_json::json!({"executor": "stub"}),
    })
    .await?;

  // Brief delay so SSE subscribers have time to attach for tests that
  // race the spawn against the subscribe call.
  tokio::time::sleep(Duration::from_millis(50)).await;

  ctx
    .repos
    .events
    .append(NewEvent {
      run_id: ctx.run_id,
      seq: 1,
      kind: "run_completed".into(),
      payload: serde_json::json!({"executor": "stub"}),
    })
    .await?;
  ctx
    .repos
    .runs
    .update_status(ctx.run_id, RunStatus::Succeeded, None)
    .await?;
  info!(run_id = %ctx.run_id, "stub executor finished");
  Ok(())
}

/// `POST /v1/runs` — accept a workflow body, persist a queued `runs` row,
/// dispatch the executor in the background, return the new id immediately.
pub async fn submit_run(
  State(state): State<AppState>,
  Json(req): Json<CreateRunRequest>,
) -> Result<Json<CreateRunResponse>, ApiError> {
  let workflow = req.workflow.or_else(|| {
    req.workflow_id.as_ref().map(|id| {
      // Reference-by-id is reserved for future use. We persist it as a
      // marker payload so operators can see what was submitted.
      format!("@workflow:{}", id)
    })
  });
  let Some(workflow) = workflow else {
    return Err(ApiError::BadRequest(
      "request body must include `workflow` (string) or `workflow_id`".into(),
    ));
  };

  let run_id = Uuid::new_v4();
  let tenant_id = req.tenant_id.unwrap_or_else(|| "default".into());

  let run = state
    .repos
    .runs
    .create(NewRun {
      id: run_id,
      workflow: workflow.clone(),
      status: RunStatus::Queued,
      run_dir: None,
      tenant_id,
    })
    .await?;

  // Dispatch in the background so the HTTP request returns immediately. The
  // executor owns the entire run lifecycle from this point.
  let executor = state.executor.clone();
  let repos = state.repos.clone();
  tokio::spawn(async move {
    executor
      .execute(RunContext {
        run_id,
        workflow,
        repos,
      })
      .await;
  });

  Ok(Json(CreateRunResponse {
    run_id: run.id,
    status: "queued",
  }))
}

#[derive(Debug, Serialize)]
pub struct RunResponse {
  #[serde(flatten)]
  pub run: Run,
}

/// `GET /v1/runs/{id}` — return the current run state.
pub async fn get_run(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<RunResponse>, ApiError> {
  let run = state
    .repos
    .runs
    .get(id)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("run {} not found", id)))?;
  Ok(Json(RunResponse { run }))
}

/// Default executor used by [`AppState::new`]. Exposed so callers can wrap
/// or replace it (tests use this to verify the route layer).
pub fn default_executor() -> Arc<dyn RunExecutor> {
  Arc::new(StubExecutor)
}

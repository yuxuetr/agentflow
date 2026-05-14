//! Run submission, status, and the executor abstraction.
//!
//! `POST /v1/runs` and `GET /v1/runs/{id}` live here. Actual workflow
//! execution is delegated to a [`RunExecutor`] trait so the route layer
//! stays oblivious to whether runs are dispatched in-process via
//! `agentflow-core::Flow`, sent to a worker pool, or stubbed out for tests.
//!
//! Production state uses [`FlowRunExecutor`] to run config-first workflows
//! in-process. Tests can still inject [`StubExecutor`] when they only need
//! route / persistence plumbing.

use async_trait::async_trait;
use axum::{
  Json,
  extract::{Path, Query, State},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path as FsPath, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{error, info};
use uuid::Uuid;

use agentflow_core::{
  FlowCancellationToken, FlowExecutionConfig, ResumePlan, ResumePlanOptions,
  async_node::AsyncNodeResult,
  build_resume_plan,
  checkpoint::{CheckpointConfig, CheckpointManager},
};
use agentflow_db::{EventRepo, NewEvent, NewRun, Repositories, Run, RunRepo, RunStatus};
use agentflow_viz::{NodeStatus, OutputFormat, from_yaml, render};

use crate::AppState;
use crate::error::ApiError;
use crate::events_stream::{EventBroker, WorkflowEventListener, publish_through};

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

#[derive(Debug, Serialize)]
pub struct CancelRunResponse {
  #[serde(flatten)]
  pub run: Run,
  pub cancelled: bool,
}

/// Minimal run-execution contract. Implementations are responsible for
/// every state transition after the route layer creates the row, including
/// terminal status updates and event emission.
#[async_trait]
pub trait RunExecutor: Send + Sync {
  async fn execute(&self, ctx: RunContext);
}

/// Everything an executor needs to do its job. Owns its own copies of the
/// repositories and broker so the route handler can return immediately.
pub struct RunContext {
  pub run_id: Uuid,
  pub workflow: String,
  pub repos: Repositories,
  pub run_base_dir: Option<PathBuf>,
  pub cancellation_token: FlowCancellationToken,
  /// Forwards events to live SSE subscribers. Persisting to the DB still
  /// has to happen — use [`publish_through`](crate::events_stream::publish_through)
  /// for the standard path.
  pub broker: EventBroker,
}

#[derive(Clone, Default)]
pub struct RunCancellationRegistry {
  inner: Arc<Mutex<HashMap<Uuid, RunCancellationEntry>>>,
}

impl std::fmt::Debug for RunCancellationRegistry {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let len = self.inner.lock().map(|entries| entries.len()).unwrap_or(0);
    f.debug_struct("RunCancellationRegistry")
      .field("active_runs", &len)
      .finish()
  }
}

#[derive(Clone)]
struct RunCancellationEntry {
  token: FlowCancellationToken,
  abort_handle: tokio::task::AbortHandle,
}

impl RunCancellationRegistry {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn register(
    &self,
    run_id: Uuid,
    token: FlowCancellationToken,
    abort_handle: tokio::task::AbortHandle,
  ) {
    let mut entries = self.inner.lock().expect("run cancellation mutex poisoned");
    entries.insert(
      run_id,
      RunCancellationEntry {
        token,
        abort_handle,
      },
    );
  }

  pub fn cancel(&self, run_id: Uuid) -> bool {
    let Some(entry) = self
      .inner
      .lock()
      .expect("run cancellation mutex poisoned")
      .get(&run_id)
      .cloned()
    else {
      return false;
    };

    entry.token.cancel();
    entry.abort_handle.abort();
    true
  }

  pub fn complete(&self, run_id: Uuid) {
    let mut entries = self.inner.lock().expect("run cancellation mutex poisoned");
    entries.remove(&run_id);
  }
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

/// In-process executor for config-first DAG workflows.
#[derive(Clone, Debug, Default)]
pub struct FlowRunExecutor;

#[async_trait]
impl RunExecutor for FlowRunExecutor {
  async fn execute(&self, ctx: RunContext) {
    if let Err(e) = flow_execute(&ctx).await {
      error!(run_id = %ctx.run_id, error = %e, "flow executor failed");
      let status = if e.is_cancelled() {
        RunStatus::Cancelled
      } else {
        RunStatus::Failed
      };
      let _ = ctx
        .repos
        .runs
        .update_status(ctx.run_id, status, Some(&e.to_string()))
        .await;
      ctx.broker.finalise(ctx.run_id);
    }
  }
}

async fn flow_execute(ctx: &RunContext) -> Result<(), anyhow_like::FlowRunError> {
  ctx
    .repos
    .runs
    .update_status(ctx.run_id, RunStatus::Running, None)
    .await?;

  let run_id = ctx.run_id.to_string();
  let mut flow = agentflow_cli::executor::build_flow_from_yaml(&ctx.workflow, None)?;
  let listener = Arc::new(WorkflowEventListener::from_state(
    ctx.run_id,
    ctx.repos.clone(),
    ctx.broker.clone(),
    0,
  ));
  flow = flow.with_event_listener(listener);

  let execution_config =
    server_execution_config(ctx.run_base_dir.clone(), ctx.cancellation_token.clone());
  let state = flow
    .execute_from_inputs_with_id_and_config(run_id, HashMap::new(), execution_config)
    .await?;

  // The listener bridges sync Flow events to async DB/SSE writes. Give the
  // drain task a bounded chance to persist terminal workflow events before
  // closing the broker channel for subscribers.
  tokio::time::sleep(Duration::from_millis(50)).await;

  if let Some(error) = first_state_error(&state) {
    ctx
      .repos
      .runs
      .update_status(ctx.run_id, RunStatus::Failed, Some(&error))
      .await?;
  } else {
    ctx
      .repos
      .runs
      .update_status(ctx.run_id, RunStatus::Succeeded, None)
      .await?;
  }

  ctx.broker.finalise(ctx.run_id);
  info!(run_id = %ctx.run_id, "flow executor finished");
  Ok(())
}

fn server_execution_config(
  run_base_dir: Option<PathBuf>,
  cancellation_token: FlowCancellationToken,
) -> FlowExecutionConfig {
  let base_dir = run_base_dir.unwrap_or_else(default_run_base_dir);
  FlowExecutionConfig::serial()
    .with_run_base_dir(base_dir)
    .with_cancellation_token(cancellation_token)
}

fn default_run_base_dir() -> PathBuf {
  if let Ok(path) = std::env::var("AGENTFLOW_RUN_DIR")
    && !path.trim().is_empty()
  {
    return PathBuf::from(path);
  }

  dirs::home_dir()
    .map(|home| home.join(".agentflow").join("runs"))
    .unwrap_or_else(|| std::env::temp_dir().join("agentflow-runs"))
}

fn run_base_dir_for_request() -> PathBuf {
  default_run_base_dir()
}

fn run_dir_for_run(base_dir: &FsPath, run_id: Uuid) -> PathBuf {
  base_dir.join(run_id.to_string())
}

fn first_state_error(state: &HashMap<String, AsyncNodeResult>) -> Option<String> {
  state
    .iter()
    .find_map(|(node_id, result)| result.as_ref().err().map(|err| format!("{node_id}: {err}")))
}

async fn stub_execute(ctx: &RunContext) -> Result<(), agentflow_db::DbError> {
  ctx
    .repos
    .runs
    .update_status(ctx.run_id, RunStatus::Running, None)
    .await?;
  publish_through(
    &ctx.repos,
    &ctx.broker,
    NewEvent {
      run_id: ctx.run_id,
      seq: 0,
      kind: "run_started".into(),
      payload: serde_json::json!({"executor": "stub"}),
    },
  )
  .await?;

  // Brief delay so SSE subscribers have time to attach for tests that
  // race the spawn against the subscribe call.
  tokio::time::sleep(Duration::from_millis(50)).await;

  publish_through(
    &ctx.repos,
    &ctx.broker,
    NewEvent {
      run_id: ctx.run_id,
      seq: 1,
      kind: "run_completed".into(),
      payload: serde_json::json!({"executor": "stub"}),
    },
  )
  .await?;
  ctx
    .repos
    .runs
    .update_status(ctx.run_id, RunStatus::Succeeded, None)
    .await?;
  // Drop the per-run broadcast channel so live subscribers see EOF after
  // any in-flight events drain.
  ctx.broker.finalise(ctx.run_id);
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
  let run_base_dir = run_base_dir_for_request();
  let run_dir = run_dir_for_run(&run_base_dir, run_id);

  let run = state
    .repos
    .runs
    .create(NewRun {
      id: run_id,
      workflow: workflow.clone(),
      status: RunStatus::Queued,
      run_dir: Some(run_dir.display().to_string()),
      tenant_id,
    })
    .await?;

  // Dispatch in the background so the HTTP request returns immediately. The
  // executor owns the entire run lifecycle from this point.
  let executor = state.executor.clone();
  let repos = state.repos.clone();
  let broker = state.event_broker.clone();
  let cancellation_registry = state.cancellation_registry.clone();
  let cancellation_token = FlowCancellationToken::new();
  let task_token = cancellation_token.clone();
  let handle = tokio::spawn(async move {
    executor
      .execute(RunContext {
        run_id,
        workflow,
        repos,
        run_base_dir: Some(run_base_dir),
        cancellation_token: task_token,
        broker,
      })
      .await;
    cancellation_registry.complete(run_id);
  });
  state
    .cancellation_registry
    .register(run_id, cancellation_token, handle.abort_handle());
  if handle.is_finished() {
    state.cancellation_registry.complete(run_id);
  }

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

#[derive(Debug, Deserialize)]
pub struct ListRunsQuery {
  /// Tenant to list. Defaults to the single-tenant local-dev bucket.
  #[serde(default)]
  pub tenant_id: Option<String>,
  /// Max rows to return, clamped to 1..=100.
  #[serde(default)]
  pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ListRunsResponse {
  pub runs: Vec<Run>,
}

#[derive(Debug, Serialize)]
pub struct RunGraphResponse {
  pub graph: serde_json::Value,
  pub mermaid: String,
  pub active_node: Option<String>,
}

/// Query string for `GET /v1/runs/{id}/resume-plan`.
#[derive(Debug, Deserialize, Default)]
pub struct ResumePlanQuery {
  /// Override the checkpoint directory. Defaults to the
  /// `CheckpointConfig::default()` path
  /// (`~/.agentflow/checkpoints` for the server's user).
  pub checkpoint_dir: Option<String>,
  /// Treat `Unknown` idempotency calls as safe to replay.
  #[serde(default)]
  pub force_replay: bool,
}

/// `GET /v1/runs` — list recent runs for a tenant, newest first.
pub async fn list_runs(
  State(state): State<AppState>,
  Query(params): Query<ListRunsQuery>,
) -> Result<Json<ListRunsResponse>, ApiError> {
  let tenant_id = params.tenant_id.unwrap_or_else(|| "default".into());
  let limit = params.limit.unwrap_or(25).clamp(1, 100);
  let runs = state.repos.runs.list(&tenant_id, limit).await?;
  Ok(Json(ListRunsResponse { runs }))
}

/// `POST /v1/runs/{id}:cancel` — idempotently cancel a queued/running run.
pub async fn cancel_run(
  State(state): State<AppState>,
  Path(id_cancel): Path<String>,
) -> Result<Json<CancelRunResponse>, ApiError> {
  let id_raw = id_cancel
    .strip_suffix(":cancel")
    .ok_or_else(|| ApiError::BadRequest("run cancellation route must end with :cancel".into()))?;
  let id = Uuid::parse_str(id_raw)
    .map_err(|_| ApiError::BadRequest(format!("invalid run id '{}'", id_raw)))?;

  let run = state
    .repos
    .runs
    .get(id)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("run {} not found", id)))?;

  if is_terminal_status(&run.status) {
    return Ok(Json(CancelRunResponse {
      run,
      cancelled: false,
    }));
  }

  state.cancellation_registry.cancel(id);
  state
    .repos
    .runs
    .update_status(id, RunStatus::Cancelled, Some("cancel requested"))
    .await?;
  publish_cancellation_event(&state.repos, &state.event_broker, id).await?;
  state.event_broker.finalise(id);
  state.cancellation_registry.complete(id);

  let run = state
    .repos
    .runs
    .get(id)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("run {} not found", id)))?;
  Ok(Json(CancelRunResponse {
    run,
    cancelled: true,
  }))
}

fn is_terminal_status(status: &str) -> bool {
  matches!(status, "succeeded" | "failed" | "cancelled")
}

async fn publish_cancellation_event(
  repos: &Repositories,
  broker: &EventBroker,
  run_id: Uuid,
) -> Result<(), ApiError> {
  let seq = next_event_seq(repos, run_id).await?;
  publish_through(
    repos,
    broker,
    NewEvent {
      run_id,
      seq,
      kind: "run.cancelled".to_string(),
      payload: serde_json::json!({
        "workflow_id": run_id.to_string(),
        "reason": "cancel requested",
      }),
    },
  )
  .await?;
  Ok(())
}

async fn next_event_seq(repos: &Repositories, run_id: Uuid) -> Result<i64, ApiError> {
  let events = repos.events.list_after(run_id, -1, 10_000).await?;
  Ok(
    events
      .iter()
      .map(|event| event.seq)
      .max()
      .map(|seq| seq + 1)
      .unwrap_or(0),
  )
}

/// `GET /v1/runs/{id}/resume-plan` — derive a structured resume plan
/// from the persisted checkpoint for this run.
///
/// Returns the same envelope produced by `agentflow workflow
/// resume-plan` so CLI / UI / Harness approval consumers share one
/// wire shape. Loading the plan does **not** execute anything; it
/// only reads the checkpoint state.
pub async fn get_run_resume_plan(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  Query(params): Query<ResumePlanQuery>,
) -> Result<Json<ResumePlan>, ApiError> {
  // Confirm the run exists so the route returns a meaningful 404 even
  // when no checkpoint has been written yet.
  let _run = state
    .repos
    .runs
    .get(id)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("run {} not found", id)))?;

  let mut config = CheckpointConfig::default();
  if let Some(dir) = params.checkpoint_dir.as_ref() {
    config = config.with_checkpoint_dir(PathBuf::from(dir));
  }
  let manager = CheckpointManager::new(config)
    .map_err(|e| ApiError::Internal(format!("checkpoint manager init failed: {e}")))?;
  let checkpoint = manager
    .load_latest_checkpoint(&id.to_string())
    .await
    .map_err(|e| ApiError::Internal(format!("failed to load checkpoint: {e}")))?
    .ok_or_else(|| ApiError::NotFound(format!("no checkpoint found for run {}", id)))?;

  let plan = build_resume_plan(
    &checkpoint,
    &ResumePlanOptions {
      force_replay: params.force_replay,
    },
  )
  .map_err(|e| ApiError::Internal(format!("failed to build resume plan: {e}")))?;

  Ok(Json(plan))
}

/// `GET /v1/runs/{id}/graph` — convert the stored workflow to
/// `agentflow-viz` JSON/Mermaid and overlay status from persisted events.
pub async fn get_run_graph(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<RunGraphResponse>, ApiError> {
  let run = state
    .repos
    .runs
    .get(id)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("run {} not found", id)))?;

  let mut graph = from_yaml(&run.workflow)
    .map_err(|e| ApiError::BadRequest(format!("workflow cannot be visualized: {}", e)))?;
  let events = state.repos.events.list_after(id, -1, 1_000).await?;
  let mut active_node = None;
  for event in events {
    let Some(node_id) = event
      .payload
      .get("node_id")
      .and_then(|value| value.as_str())
    else {
      continue;
    };
    let status = match event.kind.as_str() {
      "node.started" => Some(NodeStatus::Running),
      "node.completed" => Some(NodeStatus::Completed),
      "node.failed" => Some(NodeStatus::Failed),
      "node.skipped" => Some(NodeStatus::Skipped),
      _ => None,
    };
    if let Some(status) = status {
      graph.update_node_status(node_id, status);
      active_node = Some(node_id.to_string());
    }
  }

  let graph_json = render(&graph, OutputFormat::Json)
    .and_then(|json| {
      serde_json::from_str(&json)
        .map_err(|e| agentflow_viz::RenderError::InvalidGraph(e.to_string()))
    })
    .map_err(|e| ApiError::Internal(format!("failed to render graph json: {}", e)))?;
  let mermaid = render(&graph, OutputFormat::Mermaid)
    .map_err(|e| ApiError::Internal(format!("failed to render mermaid graph: {}", e)))?;

  Ok(Json(RunGraphResponse {
    graph: graph_json,
    mermaid,
    active_node,
  }))
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
  Arc::new(FlowRunExecutor)
}

mod anyhow_like {
  #[derive(Debug, thiserror::Error)]
  pub enum FlowRunError {
    #[error(transparent)]
    Db(#[from] agentflow_db::DbError),
    #[error(transparent)]
    Build(#[from] anyhow::Error),
    #[error(transparent)]
    Flow(#[from] agentflow_core::error::AgentFlowError),
  }

  impl FlowRunError {
    pub fn is_cancelled(&self) -> bool {
      matches!(
        self,
        Self::Flow(agentflow_core::error::AgentFlowError::TaskCancelled)
      )
    }
  }
}

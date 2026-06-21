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

use agentflow_core::FlowExt;
use async_trait::async_trait;
use axum::{
  Extension, Json,
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
  FlowCancellationToken, FlowExecutionConfig, MultiListener, ResumePlan, ResumePlanOptions,
  async_node::AsyncNodeResult,
  build_resume_plan,
  checkpoint::{CheckpointConfig, CheckpointManager},
  events::EventListener,
};
use agentflow_tracing::{TraceCollector, TraceConfig, storage::file::FileTraceStorage};

use crate::events_stream::broker_finalize_grace;
use agentflow_db::{EventRepo, NewEvent, NewRun, Repositories, Run, RunRepo, RunStatus};

use crate::AppState;
use crate::error::{ApiError, JsonReq};
use crate::events_stream::{EventBroker, WorkflowEventListener, publish_through};
use crate::tenant::TenantId;

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
  /// Optional tenant echo. Q1.4.3: this is no longer authoritative —
  /// the auth-middleware-bound `X-Agentflow-Tenant` header is the
  /// only source of truth. When the body still carries `tenant_id`
  /// it must match the header, otherwise the request is rejected
  /// with 403. Leaving this field in the body shape preserves the
  /// wire compatibility for existing clients during the transition.
  #[serde(default)]
  pub tenant_id: Option<String>,
  /// Per-run retention overrides (P10.14.1). Either field can pin
  /// the corresponding resource (events / artifacts) for at least
  /// the specified number of days, regardless of the tenant +
  /// profile default. Pinning is *additive*: the cleanup sweep
  /// uses `max(global, override)` so an override can only ever
  /// extend retention, never shorten it.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub retention_overrides: Option<RetentionOverrides>,
}

/// Body shape for `retention_overrides:` on `POST /v1/runs`
/// (P10.14.1). Both fields are optional; absent fields fall back
/// entirely to the tenant default.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RetentionOverrides {
  /// Keep `events` rows for this run for at least N days. Must be
  /// `>= 0`. `0` is accepted as a no-op (equivalent to absent) for
  /// caller convenience.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub events_days: Option<i32>,
  /// Keep `artifacts` rows for this run for at least N days. Same
  /// semantics as `events_days`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub artifacts_days: Option<i32>,
}

impl RetentionOverrides {
  /// Validate that no override is negative. The cleanup-sweep SQL
  /// uses `GREATEST(global, COALESCE(override, 0))`, so a negative
  /// override would otherwise silently degrade to the global
  /// default — better to surface the obvious request error at the
  /// API layer.
  pub fn validate(&self) -> Result<(), &'static str> {
    if matches!(self.events_days, Some(n) if n < 0) {
      return Err("retention_overrides.events_days must be >= 0");
    }
    if matches!(self.artifacts_days, Some(n) if n < 0) {
      return Err("retention_overrides.artifacts_days must be >= 0");
    }
    Ok(())
  }

  /// Treat `Some(0)` the same as absent (caller convenience). The
  /// SQL `GREATEST(global, 0)` is already a no-op vs `GREATEST(global)`,
  /// but normalizing here keeps the DB row tidy and the audit story
  /// honest (only meaningful overrides appear in the column).
  fn normalize_nonzero(value: Option<i32>) -> Option<i32> {
    value.filter(|n| *n > 0)
  }

  pub fn into_pair(self) -> (Option<i32>, Option<i32>) {
    (
      Self::normalize_nonzero(self.events_days),
      Self::normalize_nonzero(self.artifacts_days),
    )
  }
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
  /// has to happen — use [`publish_through`] for the standard path.
  pub broker: EventBroker,
  /// Tenant the run was created under. Mirrors `runs.tenant_id` so
  /// every event the executor emits gets stamped with the correct
  /// scope without re-querying the run row.
  pub tenant_id: String,
  /// Process-local registry the executor writes live state-pool sizes
  /// into (P10.14.2-FU6). `None` skips the gauge wiring — the `StubExecutor`
  /// path and tests that bypass `AppState::new` use this. Real submissions
  /// always carry the `AppState`'s shared registry so the `/metrics`
  /// scrape can read what's running.
  pub live_state_registry: Option<crate::live_state_registry::LiveStateRegistry>,
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

  /// Lock the entry map, recovering on poison so a panicked caller can't
  /// strand every subsequent cancellation request. Same poison-recovery
  /// pattern as [`crate::events_stream::EventBroker::lock_inner`]. (Q5.1)
  fn lock_inner(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, RunCancellationEntry>> {
    match self.inner.lock() {
      Ok(g) => g,
      Err(poisoned) => poisoned.into_inner(),
    }
  }

  pub fn register(
    &self,
    run_id: Uuid,
    token: FlowCancellationToken,
    abort_handle: tokio::task::AbortHandle,
  ) {
    let mut entries = self.lock_inner();
    entries.insert(
      run_id,
      RunCancellationEntry {
        token,
        abort_handle,
      },
    );
  }

  pub fn cancel(&self, run_id: Uuid) -> bool {
    let Some(entry) = self.lock_inner().get(&run_id).cloned() else {
      return false;
    };

    entry.token.cancel();
    entry.abort_handle.abort();
    true
  }

  pub fn complete(&self, run_id: Uuid) {
    let mut entries = self.lock_inner();
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
      // P10.14.2-FU6: drop the live-state gauge entry even on failure
      // so the cardinality stays bounded. (The happy path in
      // `flow_execute` deregisters after the success status update;
      // this branch covers cancellation, panic-via-Err, build_flow
      // failure, etc.)
      if let Some(registry) = &ctx.live_state_registry {
        registry.deregister(&ctx.run_id);
      }
      ctx
        .broker
        .finalise_with_grace(ctx.run_id, broker_finalize_grace());
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
  let mut flow = agentflow_config::executor::build_flow_from_yaml(&ctx.workflow, None)?;
  // The gateway always streams workflow events into Postgres + the SSE
  // broker. `AGENTFLOW_TRACE_DIR` opts in to *additionally* writing a
  // file-backed `ExecutionTrace` JSON so operators can run `agentflow
  // trace tui <run_id>` against the same run. Kept opt-in because the
  // gateway is long-running and unmanaged trace files would accumulate
  // — the existing run/event retention sweep does not cover this dir.
  let mut listeners: Vec<Box<dyn EventListener>> =
    vec![Box::new(WorkflowEventListener::from_state(
      ctx.run_id,
      ctx.tenant_id.clone(),
      ctx.repos.clone(),
      ctx.broker.clone(),
      0,
    ))];
  if let Some(trace_dir) = resolve_server_trace_dir() {
    match attach_file_trace_storage(&trace_dir) {
      Ok(collector) => {
        info!(
          run_id = %ctx.run_id,
          trace_dir = %trace_dir.display(),
          "tracing: writing file trace for this run",
        );
        listeners.push(Box::new(collector));
      }
      Err(err) => {
        // Trace IO is best-effort; degrade to DB-only rather than fail
        // the workflow because the operator's disk is unhappy.
        error!(
          run_id = %ctx.run_id,
          trace_dir = %trace_dir.display(),
          error = %err,
          "tracing: file trace storage unavailable; continuing without it",
        );
      }
    }
  }
  flow = flow.with_event_listener(Arc::new(MultiListener::new(listeners)));

  // P10.14.2-FU6: attach a state-size observer when one is wired in.
  // The observer keeps the live `agentflow_state_size_bytes{run_id}`
  // gauge fresh; on terminal transitions below we explicitly
  // deregister so the gauge stops emitting for this run.
  if let Some(registry) = &ctx.live_state_registry {
    flow = flow.with_state_size_observer(registry.observer_for(ctx.run_id));
  }

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

  if let Some(registry) = &ctx.live_state_registry {
    registry.deregister(&ctx.run_id);
  }

  ctx
    .broker
    .finalise_with_grace(ctx.run_id, broker_finalize_grace());
  info!(run_id = %ctx.run_id, "flow executor finished");
  Ok(())
}

/// Resolve the gateway's opt-in file-backed trace dir. Returns `None`
/// when `AGENTFLOW_TRACE_DIR` is unset / empty so the default deployment
/// does not silently accumulate JSON files outside the cleanup sweep.
fn resolve_server_trace_dir() -> Option<PathBuf> {
  std::env::var("AGENTFLOW_TRACE_DIR")
    .ok()
    .filter(|v| !v.is_empty())
    .map(PathBuf::from)
}

/// Build a `TraceCollector` rooted at `trace_dir`. Wrapped in its own
/// helper so the call site stays small and the error path is uniform.
fn attach_file_trace_storage(trace_dir: &FsPath) -> Result<TraceCollector, anyhow::Error> {
  std::fs::create_dir_all(trace_dir)?;
  let storage = Arc::new(FileTraceStorage::new(trace_dir.to_path_buf())?);
  // Production config: skips capturing prompts / IO bodies so trace
  // files don't fan out to the size of every per-node payload. The
  // server already persists the full event stream to Postgres; the
  // file-trace is a portable summary for `agentflow trace tui`.
  Ok(TraceCollector::new(storage, TraceConfig::production()))
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
      tenant_id: Some(ctx.tenant_id.clone()),
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
      tenant_id: Some(ctx.tenant_id.clone()),
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
  ctx
    .broker
    .finalise_with_grace(ctx.run_id, broker_finalize_grace());
  info!(run_id = %ctx.run_id, "stub executor finished");
  Ok(())
}

/// `POST /v1/runs` — accept a workflow body, persist a queued `runs` row,
/// dispatch the executor in the background, return the new id immediately.
pub async fn submit_run(
  State(state): State<AppState>,
  Extension(tenant): Extension<TenantId>,
  JsonReq(req): JsonReq<CreateRunRequest>,
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

  let (events_retention_days, artifacts_retention_days) = match req.retention_overrides {
    Some(overrides) => {
      if let Err(msg) = overrides.validate() {
        return Err(ApiError::BadRequest(msg.into()));
      }
      overrides.into_pair()
    }
    None => (None, None),
  };

  let tenant_id = tenant.as_str().to_string();
  // Q1.4.3: refuse to accept a body tenant_id that disagrees with the
  // auth-bound tenant. We don't silently override (that masks bugs in
  // the client); instead force the client to either omit the field or
  // align it with the header.
  if let Some(body_tenant) = &req.tenant_id
    && body_tenant != &tenant_id
  {
    return Err(ApiError::TenantMismatch(format!(
      "request body tenant_id '{body_tenant}' does not match authenticated tenant '{tenant_id}'"
    )));
  }
  let run_id = Uuid::new_v4();
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
      tenant_id: tenant_id.clone(),
      events_retention_days,
      artifacts_retention_days,
    })
    .await?;

  // Dispatch in the background so the HTTP request returns immediately. The
  // executor owns the entire run lifecycle from this point.
  let executor = state.executor.clone();
  let repos = state.repos.clone();
  let broker = state.event_broker.clone();
  let cancellation_registry = state.cancellation_registry.clone();
  let live_state_registry = state.live_state_registry.clone();
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
        tenant_id,
        live_state_registry: Some(live_state_registry),
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
  /// Max rows to return, clamped to 1..=100.
  #[serde(default)]
  pub limit: Option<i64>,
  /// Skip the first N rows (after the limit clamp). Lets clients
  /// paginate with `?limit=N&offset=M`. Clamped to ≥ 0.
  #[serde(default)]
  pub offset: Option<i64>,
  /// Optional run-status filter. Accepts the canonical `RunStatus`
  /// strings: `queued`, `running`, `succeeded`, `failed`, `cancelled`.
  #[serde(default)]
  pub status: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ListRunsResponse {
  pub runs: Vec<Run>,
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
///
/// Tenant resolution (Q1.4.1): the `X-Agentflow-Tenant` header bound by
/// the auth middleware is the only source of truth. The previous
/// `?tenant_id=` query parameter is gone — it overrode the header and
/// let any authenticated client list arbitrary tenants' runs.
///
/// Query parameters:
/// - `limit` (default 25, clamped to 1..=100)
/// - `offset` (default 0, clamped to ≥ 0)
/// - `status` (one of the canonical [`RunStatus`] strings; rejects
///   anything else with a 400). Omit to list all statuses.
pub async fn list_runs(
  State(state): State<AppState>,
  Extension(tenant): Extension<TenantId>,
  Query(params): Query<ListRunsQuery>,
) -> Result<Json<ListRunsResponse>, ApiError> {
  let tenant_id = tenant.as_str();
  let limit = params.limit.unwrap_or(25).clamp(1, 100);
  let offset = params.offset.unwrap_or(0).max(0);
  let status = match params.status.as_deref() {
    Some(s) => Some(parse_status_filter(s)?),
    None => None,
  };
  let runs = state
    .repos
    .runs
    .list_filtered(tenant_id, status, limit, offset)
    .await?;
  Ok(Json(ListRunsResponse { runs }))
}

/// Validate the `?status=` query parameter against the closed
/// [`RunStatus`] set. Rejects unknown values with a 400 so a typo never
/// silently returns "no runs found".
fn parse_status_filter(raw: &str) -> Result<&str, ApiError> {
  match raw {
    "queued" | "running" | "succeeded" | "failed" | "cancelled" => Ok(raw),
    other => Err(ApiError::BadRequest(format!(
      "invalid status filter '{other}'; expected one of queued|running|succeeded|failed|cancelled"
    ))),
  }
}

/// `POST /v1/runs/{id}:cancel` — idempotently cancel a queued/running run.
pub async fn cancel_run(
  State(state): State<AppState>,
  Extension(tenant): Extension<TenantId>,
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
  // P2.6 tenant boundary: pretend the row doesn't exist when the caller's
  // tenant doesn't own it. 404 (not 403) so a cross-tenant probe can't
  // infer existence by status code.
  if run.tenant_id != tenant.as_str() {
    return Err(ApiError::NotFound(format!("run {} not found", id)));
  }

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
  publish_cancellation_event(&state.repos, &state.event_broker, id, &run.tenant_id).await?;
  state
    .event_broker
    .finalise_with_grace(id, broker_finalize_grace());
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
  tenant_id: &str,
) -> Result<(), ApiError> {
  let seq = next_event_seq(repos, tenant_id, run_id).await?;
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
      tenant_id: Some(tenant_id.to_string()),
    },
  )
  .await?;
  Ok(())
}

async fn next_event_seq(
  repos: &Repositories,
  tenant_id: &str,
  run_id: Uuid,
) -> Result<i64, ApiError> {
  // Q3.11.1: O(1) `MAX(seq)` aggregate instead of paging
  // `list_after(..., 10_000)`. A run with > 10 000 events would
  // silently roll the seq counter back to a value already in
  // `events.(run_id, seq)` and collide the primary key on the next
  // `append`. Mirrors the long-standing pattern already used by
  // `harness_events.max_seq`.
  let max = repos.events.max_seq(tenant_id, run_id).await?;
  Ok(max.map(|seq| seq + 1).unwrap_or(0))
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
  Extension(tenant): Extension<TenantId>,
  Path(id): Path<Uuid>,
  Query(params): Query<ResumePlanQuery>,
) -> Result<Json<ResumePlan>, ApiError> {
  // Confirm the run exists so the route returns a meaningful 404 even
  // when no checkpoint has been written yet.
  let run = state
    .repos
    .runs
    .get(id)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("run {} not found", id)))?;
  // P2.6 tenant boundary.
  if run.tenant_id != tenant.as_str() {
    return Err(ApiError::NotFound(format!("run {} not found", id)));
  }

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

/// `GET /v1/runs/{id}` — return the current run state.
pub async fn get_run(
  State(state): State<AppState>,
  Extension(tenant): Extension<TenantId>,
  Path(id): Path<Uuid>,
) -> Result<Json<RunResponse>, ApiError> {
  let run = state
    .repos
    .runs
    .get(id)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("run {} not found", id)))?;
  // P2.6 tenant boundary: hide cross-tenant rows behind 404.
  if run.tenant_id != tenant.as_str() {
    return Err(ApiError::NotFound(format!("run {} not found", id)));
  }
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

#[cfg(test)]
mod retention_overrides_tests {
  use super::RetentionOverrides;

  #[test]
  fn validate_rejects_negative_events_days() {
    let o = RetentionOverrides {
      events_days: Some(-1),
      artifacts_days: None,
    };
    assert!(o.validate().is_err());
  }

  #[test]
  fn validate_rejects_negative_artifacts_days() {
    let o = RetentionOverrides {
      events_days: None,
      artifacts_days: Some(-7),
    };
    assert!(o.validate().is_err());
  }

  #[test]
  fn validate_accepts_zero_and_positive() {
    let o = RetentionOverrides {
      events_days: Some(0),
      artifacts_days: Some(180),
    };
    assert!(o.validate().is_ok());
  }

  #[test]
  fn into_pair_normalizes_zero_to_none() {
    let o = RetentionOverrides {
      events_days: Some(0),
      artifacts_days: Some(180),
    };
    // The cleanup SQL treats 0 the same as absent via GREATEST(...,
    // COALESCE(override, 0)). Normalizing in `into_pair` keeps the
    // DB row honest (only meaningful overrides are persisted) and
    // makes the audit story unambiguous.
    assert_eq!(o.into_pair(), (None, Some(180)));
  }

  #[test]
  fn into_pair_passes_through_positive_values() {
    let o = RetentionOverrides {
      events_days: Some(30),
      artifacts_days: Some(60),
    };
    assert_eq!(o.into_pair(), (Some(30), Some(60)));
  }

  #[test]
  fn deserialize_accepts_partial_body() {
    let parsed: RetentionOverrides =
      serde_json::from_str(r#"{"events_days": 90}"#).expect("valid body");
    assert_eq!(parsed.events_days, Some(90));
    assert!(parsed.artifacts_days.is_none());
  }

  #[test]
  fn deserialize_accepts_empty_object() {
    let parsed: RetentionOverrides = serde_json::from_str("{}").expect("empty body ok");
    assert!(parsed.events_days.is_none());
    assert!(parsed.artifacts_days.is_none());
  }
}

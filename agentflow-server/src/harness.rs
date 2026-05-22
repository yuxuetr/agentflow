//! Harness Mode HTTP surface (P-H.5).
//!
//! Exposes agent-native sessions (`agentflow-harness`) over the same
//! gateway Axum shell that hosts the workflow `/v1/runs` routes. The
//! initial slice mirrors the workflow runs surface:
//!
//! - `POST /v1/harness/sessions` — create a session, return id.
//! - `GET  /v1/harness/sessions` — list sessions for a tenant.
//! - `GET  /v1/harness/sessions/{id}` — read current state.
//! - `POST /v1/harness/sessions/{id}:cancel` — request cancellation.
//! - `GET  /v1/harness/sessions/{id}/events` — SSE with backfill.
//! - `GET  /v1/harness/sessions/{id}/events/history` — JSON history.
//!
//! Approval routes (`/approvals`, `/approvals/{id}`) and the
//! LLM-backed executor are deferred to follow-up P-H.5 slices so this
//! initial commit can land the schema + route plumbing without
//! standing up the full agent stack on the server.
//!
//! The route layer stays narrow: it owns the session row + event log
//! transitions, dispatches a [`HarnessSessionExecutor`] in the
//! background, and surfaces persisted events via a process-local
//! [`HarnessEventBroker`]. Real execution lives behind the executor
//! trait so we can swap [`StubHarnessExecutor`] for a real
//! `HarnessRuntime` later without touching the route layer.

use async_trait::async_trait;
use axum::{
  Json,
  extract::{Path, Query, State},
  response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_stream::wrappers::{BroadcastStream, errors::BroadcastStreamRecvError};
use tracing::{error, info};
use uuid::Uuid;

use agentflow_db::{
  DbError, HarnessEventRepo, HarnessSession, HarnessSessionRepo, HarnessSessionStatus,
  NewHarnessSession, NewHarnessSessionEvent, Repositories,
};

use crate::AppState;
use crate::error::{ApiError, JsonReq};
use crate::events_stream::broker_finalize_grace;

/// Channel capacity per session. Slow subscribers drop oldest events when
/// they fall this far behind; the SSE handler logs a warning and lets the
/// client reconnect with `?after_seq=` to refill from the DB.
const SESSION_CHANNEL_CAPACITY: usize = 256;

/// Default profile assigned when the request body omits one. Matches the
/// CLI default so the surface is consistent across entry points.
const DEFAULT_PROFILE: &str = "local";

/// Default runtime kind when the body omits one.
const DEFAULT_RUNTIME_KIND: &str = "react";

/// Default model handle the stub executor records on the session row.
/// Real executor will sniff this from the request body or skill config.
const DEFAULT_MODEL: &str = "stub";

/// Wire shape published over SSE. Mirrors `agentflow_db::HarnessSessionEvent`
/// but stays minimal so we don't tie SSE consumers to DB columns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamedHarnessEvent {
  pub session_id: Uuid,
  pub seq: i64,
  pub kind: String,
  pub payload: serde_json::Value,
  pub ts: chrono::DateTime<chrono::Utc>,
}

impl From<agentflow_db::HarnessSessionEvent> for StreamedHarnessEvent {
  fn from(e: agentflow_db::HarnessSessionEvent) -> Self {
    Self {
      session_id: e.session_id,
      seq: e.seq,
      kind: e.kind,
      payload: e.payload,
      ts: e.ts,
    }
  }
}

/// Process-local broker over a sharded broadcast channel keyed by
/// `session_id`. Same pattern as the workflow `EventBroker`; the two are
/// intentionally distinct types so a slow workflow subscriber can't lag a
/// harness session and vice versa.
#[derive(Clone, Default)]
pub struct HarnessEventBroker {
  inner: Arc<Mutex<HashMap<Uuid, broadcast::Sender<StreamedHarnessEvent>>>>,
}

impl std::fmt::Debug for HarnessEventBroker {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let len = self.inner.lock().map(|g| g.len()).unwrap_or(0);
    f.debug_struct("HarnessEventBroker")
      .field("active_sessions", &len)
      .finish()
  }
}

impl HarnessEventBroker {
  pub fn new() -> Self {
    Self::default()
  }

  /// Subscribe to live events for `session_id`. Creates the channel if no
  /// subscriber has registered for this session yet so publishers don't
  /// need to coordinate with subscribers.
  pub fn subscribe(&self, session_id: Uuid) -> broadcast::Receiver<StreamedHarnessEvent> {
    let mut map = self
      .inner
      .lock()
      .expect("harness event broker mutex poisoned");
    map
      .entry(session_id)
      .or_insert_with(|| broadcast::channel(SESSION_CHANNEL_CAPACITY).0)
      .subscribe()
  }

  /// Publish without persisting. Use [`Self::publish_through`] (free
  /// function below) when you also want a DB row.
  pub fn publish(&self, event: StreamedHarnessEvent) {
    let mut map = self
      .inner
      .lock()
      .expect("harness event broker mutex poisoned");
    let sender = map
      .entry(event.session_id)
      .or_insert_with(|| broadcast::channel(SESSION_CHANNEL_CAPACITY).0);
    let _ = sender.send(event);
  }

  /// Drop the channel for a finished session so it doesn't leak. Safe to
  /// call multiple times.
  pub fn finalise(&self, session_id: Uuid) {
    let mut map = self
      .inner
      .lock()
      .expect("harness event broker mutex poisoned");
    map.remove(&session_id);
  }

  /// Like [`Self::finalise`] but defers the actual channel removal by
  /// `grace`. Mirrors the workflow broker's grace-window pattern so
  /// in-flight SSE subscribers can drain the terminal event from the
  /// broadcast buffer before the sender is dropped.
  pub fn finalise_with_grace(&self, session_id: Uuid, grace: Duration) {
    let broker = self.clone();
    tokio::spawn(async move {
      if !grace.is_zero() {
        tokio::time::sleep(grace).await;
      }
      broker.finalise(session_id);
    });
  }

  /// Snapshot of the active per-session channel count. Cheap to call;
  /// used by tests and lightweight diagnostics.
  pub fn active_sessions(&self) -> usize {
    self.inner.lock().map(|m| m.len()).unwrap_or(0)
  }

  /// Returns the number of receivers currently subscribed to
  /// `session_id`. `0` when the channel is missing entirely.
  pub fn receiver_count(&self, session_id: Uuid) -> usize {
    let map = self
      .inner
      .lock()
      .expect("harness event broker mutex poisoned");
    map
      .get(&session_id)
      .map(|sender| sender.receiver_count())
      .unwrap_or(0)
  }
}

/// Persist + publish a harness event in one shot.
pub async fn publish_through(
  repos: &Repositories,
  broker: &HarnessEventBroker,
  event: NewHarnessSessionEvent,
) -> Result<(), DbError> {
  let stored = repos.harness_events.append(event).await?;
  broker.publish(StreamedHarnessEvent::from(stored));
  Ok(())
}

/// JSON body for `POST /v1/harness/sessions`.
#[derive(Debug, Deserialize)]
pub struct CreateHarnessSessionRequest {
  /// Operator-facing prompt or instruction handed to the agent.
  pub user_input: String,
  /// Workspace root the agent may access. Stored verbatim; sandbox
  /// enforcement is the executor's responsibility, not the route layer.
  pub workspace_root: String,
  /// Tenant scope. Defaults to `"default"` so single-tenant deployments
  /// don't need to spell it out.
  #[serde(default)]
  pub tenant_id: Option<String>,
  /// Security profile (`dev` / `local` / `production`). Defaults to
  /// `local` to match the CLI default and keep the surface conservative.
  #[serde(default)]
  pub profile: Option<String>,
  /// Runtime kind (`react`, `plan_execute`, ...). Defaults to `react`.
  #[serde(default)]
  pub runtime_kind: Option<String>,
  /// Optional model handle. The stub executor stores it verbatim; the
  /// real executor will pass it through `AgentFlow::model(...)`.
  #[serde(default)]
  pub model: Option<String>,
  /// Optional named skill to load.
  #[serde(default)]
  pub skill_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateHarnessSessionResponse {
  pub session_id: Uuid,
  pub status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct HarnessSessionResponse {
  #[serde(flatten)]
  pub session: HarnessSession,
}

#[derive(Debug, Serialize)]
pub struct ListHarnessSessionsResponse {
  pub sessions: Vec<HarnessSession>,
}

#[derive(Debug, Deserialize)]
pub struct ListHarnessSessionsQuery {
  /// Tenant to list. Defaults to the single-tenant local-dev bucket.
  #[serde(default)]
  pub tenant_id: Option<String>,
  /// Max rows to return, clamped to 1..=100.
  #[serde(default)]
  pub limit: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct CancelHarnessSessionResponse {
  #[serde(flatten)]
  pub session: HarnessSession,
  pub cancelled: bool,
}

#[derive(Debug, Deserialize)]
pub struct HarnessEventsQuery {
  /// Resume after this `seq`. SSE clients reconnecting after a network
  /// blip pass the last seq they saw to avoid duplicates and gaps.
  #[serde(default)]
  pub after_seq: Option<i64>,
}

/// Background execution contract for harness sessions.
///
/// Implementations own every state transition after the route layer
/// inserts the session row, including terminal status updates and event
/// emission. Today the only impl is [`StubHarnessExecutor`]; wiring
/// `agentflow-harness::HarnessRuntime` end-to-end through this trait is
/// tracked as a P-H.5 follow-up so we can land the schema + routes
/// without standing up an LLM-backed agent on the server.
#[async_trait]
pub trait HarnessSessionExecutor: Send + Sync {
  async fn execute(&self, ctx: HarnessSessionContext);
}

/// Everything an executor needs to do its job. Owns its own copies of the
/// repositories and broker so the route handler can return immediately.
pub struct HarnessSessionContext {
  pub session_id: Uuid,
  pub user_input: String,
  pub workspace_root: String,
  pub profile: String,
  pub runtime_kind: String,
  pub model: String,
  pub skill_name: Option<String>,
  pub repos: Repositories,
  /// Forwards events to live SSE subscribers. Persisting to the DB still
  /// has to happen — use [`publish_through`] for the standard path.
  pub broker: HarnessEventBroker,
  /// Seq number the executor must use for the first event it emits.
  /// Fresh submissions and rerun resumes use `0`; append-mode resumes
  /// pass `MAX(existing seq) + 1` so new events extend the persisted
  /// log without colliding with prior `(session_id, seq)` rows.
  pub initial_seq: u64,
}

/// Default executor used until the real LLM-backed runner lands.
///
/// Emits `session_started` + `stopped` events, marks the session as
/// `failed` with a clear error message, and finalises the broker. The
/// goal is end-to-end plumbing verification: routes accept submissions,
/// the row + events round-trip through Postgres, and SSE subscribers see
/// the lifecycle. Real execution wiring is tracked under the P-H.5
/// follow-up slice.
#[derive(Clone, Debug, Default)]
pub struct StubHarnessExecutor;

#[async_trait]
impl HarnessSessionExecutor for StubHarnessExecutor {
  async fn execute(&self, ctx: HarnessSessionContext) {
    if let Err(err) = stub_execute(&ctx).await {
      error!(session_id = %ctx.session_id, error = %err, "stub harness executor failed");
      // Best-effort: persist a terminal failure on the row even if the
      // event writes themselves failed.
      let _ = ctx
        .repos
        .harness_sessions
        .update_status(
          ctx.session_id,
          HarnessSessionStatus::Failed,
          None,
          Some(&err.to_string()),
        )
        .await;
      ctx
        .broker
        .finalise_with_grace(ctx.session_id, broker_finalize_grace());
    }
  }
}

async fn stub_execute(ctx: &HarnessSessionContext) -> Result<(), DbError> {
  let started_seq = ctx.initial_seq as i64;
  let stopped_seq = started_seq + 1;
  publish_through(
    &ctx.repos,
    &ctx.broker,
    NewHarnessSessionEvent {
      session_id: ctx.session_id,
      seq: started_seq,
      kind: "session_started".into(),
      payload: serde_json::json!({
        "executor": "stub",
        "profile": ctx.profile,
        "runtime_kind": ctx.runtime_kind,
        "model": ctx.model,
        "skill_name": ctx.skill_name,
        "workspace_root": ctx.workspace_root,
      }),
    },
  )
  .await?;

  // Brief delay so SSE subscribers have time to attach before the
  // session completes. Same trick as `StubExecutor::stub_execute` in
  // `agentflow-server::runs`.
  tokio::time::sleep(Duration::from_millis(50)).await;

  let stop_reason = "executor_not_yet_wired";
  publish_through(
    &ctx.repos,
    &ctx.broker,
    NewHarnessSessionEvent {
      session_id: ctx.session_id,
      seq: stopped_seq,
      kind: "stopped".into(),
      payload: serde_json::json!({
        "executor": "stub",
        "reason": stop_reason,
        "message": "LLM-backed harness executor not yet wired into the server; \
                    follow-up P-H.5 slice will replace this stub.",
      }),
    },
  )
  .await?;

  ctx
    .repos
    .harness_sessions
    .update_status(
      ctx.session_id,
      HarnessSessionStatus::Failed,
      None,
      Some(stop_reason),
    )
    .await?;

  ctx
    .broker
    .finalise_with_grace(ctx.session_id, broker_finalize_grace());
  info!(session_id = %ctx.session_id, "stub harness executor finished");
  Ok(())
}

/// Default executor used by [`AppState::new`]. Exposed so callers can wrap
/// or replace it (tests use this to verify the route layer in isolation
/// from the real LLM-backed runtime).
pub fn default_harness_executor() -> Arc<dyn HarnessSessionExecutor> {
  Arc::new(StubHarnessExecutor)
}

/// `POST /v1/harness/sessions` — accept a session submission, persist a
/// `running` session row, dispatch the executor in the background, return
/// the new id immediately.
pub async fn submit_harness_session(
  State(state): State<AppState>,
  JsonReq(req): JsonReq<CreateHarnessSessionRequest>,
) -> Result<Json<CreateHarnessSessionResponse>, ApiError> {
  let user_input = req.user_input.trim();
  if user_input.is_empty() {
    return Err(ApiError::BadRequest(
      "`user_input` must be a non-empty prompt".into(),
    ));
  }
  let workspace_root = req.workspace_root.trim();
  if workspace_root.is_empty() {
    return Err(ApiError::BadRequest(
      "`workspace_root` must be a non-empty path".into(),
    ));
  }

  let session_id = Uuid::new_v4();
  let tenant_id = req.tenant_id.unwrap_or_else(|| "default".into());
  let profile = req.profile.unwrap_or_else(|| DEFAULT_PROFILE.into());
  let runtime_kind = req
    .runtime_kind
    .unwrap_or_else(|| DEFAULT_RUNTIME_KIND.into());
  let model = req.model.unwrap_or_else(|| DEFAULT_MODEL.into());

  let session = state
    .repos
    .harness_sessions
    .create(NewHarnessSession {
      id: session_id,
      tenant_id,
      user_input: user_input.to_string(),
      workspace_root: workspace_root.to_string(),
      profile: profile.clone(),
      runtime_kind: runtime_kind.clone(),
      model: model.clone(),
      skill_name: req.skill_name.clone(),
    })
    .await?;

  // Dispatch in the background so the HTTP request returns immediately.
  // The executor owns the entire session lifecycle from this point.
  let executor = state.harness_executor.clone();
  let repos = state.repos.clone();
  let broker = state.harness_broker.clone();
  let workspace_root_owned = workspace_root.to_string();
  let user_input_owned = user_input.to_string();
  tokio::spawn(async move {
    executor
      .execute(HarnessSessionContext {
        session_id,
        user_input: user_input_owned,
        workspace_root: workspace_root_owned,
        profile,
        runtime_kind,
        model,
        skill_name: req.skill_name,
        repos,
        broker,
        initial_seq: 0,
      })
      .await;
  });

  Ok(Json(CreateHarnessSessionResponse {
    session_id: session.id,
    status: "running",
  }))
}

/// `GET /v1/harness/sessions` — list recent sessions for a tenant, newest
/// first.
pub async fn list_harness_sessions(
  State(state): State<AppState>,
  Query(params): Query<ListHarnessSessionsQuery>,
) -> Result<Json<ListHarnessSessionsResponse>, ApiError> {
  let tenant_id = params.tenant_id.unwrap_or_else(|| "default".into());
  let limit = params.limit.unwrap_or(25).clamp(1, 100);
  let sessions = state.repos.harness_sessions.list(&tenant_id, limit).await?;
  Ok(Json(ListHarnessSessionsResponse { sessions }))
}

/// `GET /v1/harness/sessions/{id}` — return the current session state.
pub async fn get_harness_session(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<HarnessSessionResponse>, ApiError> {
  let session = state
    .repos
    .harness_sessions
    .get(id)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("harness session {} not found", id)))?;
  Ok(Json(HarnessSessionResponse { session }))
}

/// Dispatcher for the two POST actions on `/v1/harness/sessions/{id}`.
///
/// Axum can't bind two POST handlers to the same path pattern, so the
/// router routes both `:cancel` and `:resume` here and the handler
/// dispatches on the suffix. The raw `:id` capture includes the
/// suffix verbatim (e.g. `<uuid>:cancel`); the matching handler strips
/// it before parsing the UUID.
pub async fn post_harness_session_action(
  state: State<AppState>,
  Path(id_action): Path<String>,
  body: Option<Json<ResumeHarnessSessionRequest>>,
) -> Result<axum::response::Response, ApiError> {
  use axum::response::IntoResponse;
  if id_action.ends_with(":cancel") {
    return cancel_harness_session(state, Path(id_action))
      .await
      .map(IntoResponse::into_response);
  }
  if id_action.ends_with(":resume") {
    let body = body.map(|Json(value)| value).unwrap_or_default();
    return resume_harness_session(state, Path(id_action), Json(body))
      .await
      .map(IntoResponse::into_response);
  }
  Err(ApiError::BadRequest(
    "harness session action route must end with :cancel or :resume".into(),
  ))
}

/// `POST /v1/harness/sessions/{id}:cancel` — idempotently cancel a running
/// session. Terminal sessions return the current row with
/// `cancelled: false`.
pub async fn cancel_harness_session(
  State(state): State<AppState>,
  Path(id_cancel): Path<String>,
) -> Result<Json<CancelHarnessSessionResponse>, ApiError> {
  let id_raw = id_cancel.strip_suffix(":cancel").ok_or_else(|| {
    ApiError::BadRequest("harness session cancel route must end with :cancel".into())
  })?;
  let id = Uuid::parse_str(id_raw)
    .map_err(|_| ApiError::BadRequest(format!("invalid harness session id '{}'", id_raw)))?;

  let session = state
    .repos
    .harness_sessions
    .get(id)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("harness session {} not found", id)))?;

  if HarnessSessionStatus::parse(&session.status)
    .map(HarnessSessionStatus::is_terminal)
    .unwrap_or(false)
  {
    return Ok(Json(CancelHarnessSessionResponse {
      session,
      cancelled: false,
    }));
  }

  // The stub executor never honours cancellation (it finishes in <50ms
  // anyway). Once the real executor lands, a parallel cancellation
  // registry (mirroring `RunCancellationRegistry`) will signal the
  // running agent. For now we mark the row + emit a stopped event so
  // SSE consumers see the lifecycle end.
  state
    .repos
    .harness_sessions
    .update_status(
      id,
      HarnessSessionStatus::Cancelled,
      None,
      Some("cancel requested"),
    )
    .await?;
  publish_cancel_event(&state.repos, &state.harness_broker, id).await?;
  state
    .harness_broker
    .finalise_with_grace(id, broker_finalize_grace());

  let session = state
    .repos
    .harness_sessions
    .get(id)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("harness session {} not found", id)))?;
  Ok(Json(CancelHarnessSessionResponse {
    session,
    cancelled: true,
  }))
}

/// Resume flavour selected by the request body.
///
/// `Rerun` (default) clears the prior event log and restarts the seq
/// series at `0` — the original v1 semantic. `Append` preserves the
/// prior events and continues the seq series at `MAX(seq) + 1` so SSE
/// consumers see the new run as an extension of the old one rather
/// than a fresh shell. The two modes are mutually exclusive on a
/// single resume call: pick the rerun flavour for "retry from
/// scratch" debugging, the append flavour for "extend the
/// conversation" workflows.
#[derive(Debug, Deserialize, Serialize, Default, Clone, Copy, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ResumeMode {
  #[default]
  Rerun,
  Append,
}

/// Body for `POST /v1/harness/sessions/{id}:resume`.
///
/// `user_input` is optional: omitting it replays the original prompt.
/// Pass a new prompt to extend the session with a follow-up
/// instruction. Other lifecycle fields (workspace, profile, runtime,
/// model, skill) are taken from the existing row — operators rerun
/// against the same shape, not a new shape.
///
/// `mode` defaults to [`ResumeMode::Rerun`] for full backwards
/// compatibility with pre-append callers.
#[derive(Debug, Deserialize, Default, Clone)]
pub struct ResumeHarnessSessionRequest {
  #[serde(default)]
  pub user_input: Option<String>,
  #[serde(default)]
  pub mode: ResumeMode,
}

#[derive(Debug, Serialize)]
pub struct ResumeHarnessSessionResponse {
  #[serde(flatten)]
  pub session: HarnessSession,
  pub resumed: bool,
  /// Echoes the resume flavour the server actually applied, so a
  /// caller that omits the field can confirm the default.
  pub mode: ResumeMode,
}

/// `POST /v1/harness/sessions/{id}:resume` — restart or extend a
/// terminated session.
///
/// **Rerun semantic (default, `mode = "rerun"`):** clear prior
/// persisted events, flip the row back to `running`, spawn a fresh
/// `HarnessSessionExecutor` run with the same `session_id`. The
/// original `user_input` is reused unless the request body overrides
/// it. Useful for retry-with-tweak debugging or replaying after a
/// transient LLM failure.
///
/// **Append semantic (`mode = "append"`):** keep the prior event log
/// intact, flip the row back to `running`, spawn a fresh executor run
/// that emits new events at `MAX(existing seq) + 1`. SSE consumers see
/// one continuous timeline. This is the natural shape for follow-up
/// instructions ("after that finished, also do X") and for resuming
/// from a forced cancel without losing the trace.
///
/// Returns 409 Conflict when the session is still running so two
/// reruns can't race the same row. Cancel first if needed.
pub async fn resume_harness_session(
  State(state): State<AppState>,
  Path(id_resume): Path<String>,
  Json(body): Json<ResumeHarnessSessionRequest>,
) -> Result<Json<ResumeHarnessSessionResponse>, ApiError> {
  let id_raw = id_resume.strip_suffix(":resume").ok_or_else(|| {
    ApiError::BadRequest("harness session resume route must end with :resume".into())
  })?;
  let id = Uuid::parse_str(id_raw)
    .map_err(|_| ApiError::BadRequest(format!("invalid harness session id '{}'", id_raw)))?;

  let session = state
    .repos
    .harness_sessions
    .get(id)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("harness session {} not found", id)))?;

  if !HarnessSessionStatus::parse(&session.status)
    .map(HarnessSessionStatus::is_terminal)
    .unwrap_or(false)
  {
    return Err(ApiError::BadRequest(format!(
      "harness session {} is still {}; cancel before resuming",
      id, session.status
    )));
  }

  // Pick the next prompt: explicit override, else the original.
  let user_input = body
    .user_input
    .as_deref()
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .map(str::to_string)
    .unwrap_or_else(|| session.user_input.clone());

  // Resolve the seq offset and reset the row in one place per mode so
  // the spawned executor only sees a fully-prepared session.
  let initial_seq: u64 = match body.mode {
    ResumeMode::Rerun => {
      // Clear the prior event log — destructive by design, see the
      // function's docstring. The inner runtime's seq counter then
      // restarts cleanly at 0.
      state
        .repos
        .harness_sessions
        .reset_for_resume(id, &user_input)
        .await?;
      0
    }
    ResumeMode::Append => {
      // Keep prior events; pick `MAX(seq) + 1` as the offset for the
      // next run so seq numbers remain monotonic across the resumes.
      let max_seq = state.repos.harness_events.max_seq(id).await?;
      state
        .repos
        .harness_sessions
        .reset_for_append_resume(id, &user_input)
        .await?;
      // Map None (no prior events) → 0 so the append flavour degrades
      // gracefully into a clean first run if the operator is calling
      // it on a session that somehow has zero rows. Otherwise the
      // u64 cast is safe because seqs are stored as non-negative
      // bigints by the append path.
      max_seq.map(|m| (m as u64) + 1).unwrap_or(0)
    }
  };

  // Spawn the executor in the background. The HTTP request returns
  // immediately with the refreshed row.
  let executor = state.harness_executor.clone();
  let repos = state.repos.clone();
  let broker = state.harness_broker.clone();
  let workspace_root = session.workspace_root.clone();
  let profile = session.profile.clone();
  let runtime_kind = session.runtime_kind.clone();
  let model = session.model.clone();
  let skill_name = session.skill_name.clone();
  let user_input_owned = user_input.clone();
  tokio::spawn(async move {
    executor
      .execute(HarnessSessionContext {
        session_id: id,
        user_input: user_input_owned,
        workspace_root,
        profile,
        runtime_kind,
        model,
        skill_name,
        repos,
        broker,
        initial_seq,
      })
      .await;
  });

  // Refetch so the response reflects the freshly-reset row (status,
  // cleared finished_at / final_answer / error, optional new prompt).
  let session = state
    .repos
    .harness_sessions
    .get(id)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("harness session {} not found", id)))?;
  Ok(Json(ResumeHarnessSessionResponse {
    session,
    resumed: true,
    mode: body.mode,
  }))
}

async fn publish_cancel_event(
  repos: &Repositories,
  broker: &HarnessEventBroker,
  session_id: Uuid,
) -> Result<(), ApiError> {
  let seq = next_event_seq(repos, session_id).await?;
  publish_through(
    repos,
    broker,
    NewHarnessSessionEvent {
      session_id,
      seq,
      kind: "stopped".to_string(),
      payload: serde_json::json!({
        "reason": "cancelled",
        "source": "operator",
      }),
    },
  )
  .await?;
  Ok(())
}

async fn next_event_seq(repos: &Repositories, session_id: Uuid) -> Result<i64, ApiError> {
  let events = repos
    .harness_events
    .list_after(session_id, -1, 10_000)
    .await?;
  Ok(
    events
      .iter()
      .map(|event| event.seq)
      .max()
      .map(|seq| seq + 1)
      .unwrap_or(0),
  )
}

/// `GET /v1/harness/sessions/{id}/events` — SSE stream.
///
/// 1. Verifies the session exists; 404s if not.
/// 2. Subscribes to the broker first so events emitted while we're still
///    setting up don't fall on the floor.
/// 3. Replays any events with `seq > after_seq` (default `-1`) from the DB.
/// 4. Forwards live broker events for as long as the channel stays open.
pub async fn stream_harness_events(
  State(state): State<AppState>,
  Path(session_id): Path<Uuid>,
  Query(params): Query<HarnessEventsQuery>,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, ApiError> {
  let _session = state
    .repos
    .harness_sessions
    .get(session_id)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("harness session {} not found", session_id)))?;

  let mut after_seq = params.after_seq.unwrap_or(-1);
  let receiver = state.harness_broker.subscribe(session_id);

  let mut backfill: Vec<StreamedHarnessEvent> = Vec::new();
  loop {
    let page = state
      .repos
      .harness_events
      .list_after(session_id, after_seq, 200)
      .await?;
    if page.is_empty() {
      break;
    }
    after_seq = page.last().map(|e| e.seq).unwrap_or(after_seq);
    backfill.extend(page.into_iter().map(StreamedHarnessEvent::from));
    if backfill.len() >= 1_000 {
      break;
    }
  }

  let backfill_stream = futures::stream::iter(backfill).map(HarnessBrokerItem::Event);
  let live_stream = BroadcastStream::new(receiver).map(|res| match res {
    Ok(event) => HarnessBrokerItem::Event(event),
    Err(BroadcastStreamRecvError::Lagged(_)) => HarnessBrokerItem::Lagged,
  });
  let stream = backfill_stream
    .chain(live_stream)
    .map(serialise_item)
    .map(Ok::<_, Infallible>);

  Ok(
    Sse::new(stream).keep_alive(
      KeepAlive::new()
        .interval(Duration::from_secs(15))
        .text("keep-alive"),
    ),
  )
}

/// `GET /v1/harness/sessions/{id}/events/history` — JSON list of events,
/// optionally filtered by `?after_seq=`. Same contract as the workflow
/// `list_events` route.
pub async fn list_harness_events(
  State(state): State<AppState>,
  Path(session_id): Path<Uuid>,
  Query(params): Query<HarnessEventsQuery>,
) -> Result<Json<Vec<StreamedHarnessEvent>>, ApiError> {
  let _session = state
    .repos
    .harness_sessions
    .get(session_id)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("harness session {} not found", session_id)))?;

  let after_seq = params.after_seq.unwrap_or(-1);
  let events = state
    .repos
    .harness_events
    .list_after(session_id, after_seq, 1_000)
    .await?
    .into_iter()
    .map(StreamedHarnessEvent::from)
    .collect();
  Ok(Json(events))
}

enum HarnessBrokerItem {
  Event(StreamedHarnessEvent),
  Lagged,
}

fn serialise_item(item: HarnessBrokerItem) -> Event {
  match item {
    HarnessBrokerItem::Event(event) => serialise_event(&event),
    HarnessBrokerItem::Lagged => {
      Event::default().comment("lagged: reconnect with ?after_seq=<last_seen>")
    }
  }
}

fn serialise_event(event: &StreamedHarnessEvent) -> Event {
  let json = serde_json::to_string(event).unwrap_or_else(|_| "{}".to_string());
  Event::default()
    .id(event.seq.to_string())
    .event(event.kind.clone())
    .data(json)
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::Utc;

  fn sample_event(session_id: Uuid, seq: i64) -> StreamedHarnessEvent {
    StreamedHarnessEvent {
      session_id,
      seq,
      kind: "test".into(),
      payload: serde_json::json!({"seq": seq}),
      ts: Utc::now(),
    }
  }

  #[tokio::test]
  async fn broker_subscribe_then_publish_delivers_event() {
    let broker = HarnessEventBroker::new();
    let session_id = Uuid::new_v4();
    let mut rx = broker.subscribe(session_id);
    broker.publish(sample_event(session_id, 0));
    let received = rx.recv().await.expect("event delivered");
    assert_eq!(received.seq, 0);
  }

  #[tokio::test]
  async fn broker_isolates_events_per_session_id() {
    let broker = HarnessEventBroker::new();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let mut rx_a = broker.subscribe(a);
    let _rx_b = broker.subscribe(b);
    broker.publish(sample_event(b, 0));
    assert!(rx_a.try_recv().is_err());
  }

  #[tokio::test]
  async fn broker_finalise_closes_subscribers() {
    let broker = HarnessEventBroker::new();
    let session_id = Uuid::new_v4();
    let mut rx = broker.subscribe(session_id);
    broker.finalise(session_id);
    let result = rx.recv().await;
    assert!(matches!(result, Err(broadcast::error::RecvError::Closed)));
  }

  #[tokio::test]
  async fn broker_finalise_with_grace_preserves_terminal_event() {
    let broker = HarnessEventBroker::new();
    let session_id = Uuid::new_v4();
    let mut rx = broker.subscribe(session_id);
    broker.publish(sample_event(session_id, 0));
    broker.finalise_with_grace(session_id, Duration::from_millis(50));

    let received = tokio::time::timeout(Duration::from_millis(200), rx.recv())
      .await
      .expect("recv did not time out")
      .expect("terminal event delivered before channel teardown");
    assert_eq!(received.seq, 0);

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(broker.active_sessions(), 0);
  }

  #[tokio::test]
  async fn broker_receiver_count_tracks_subscriber_drops() {
    let broker = HarnessEventBroker::new();
    let session_id = Uuid::new_v4();
    let rx_one = broker.subscribe(session_id);
    let rx_two = broker.subscribe(session_id);
    assert_eq!(broker.receiver_count(session_id), 2);
    drop(rx_one);
    broker.publish(sample_event(session_id, 0));
    assert_eq!(broker.receiver_count(session_id), 1);
    drop(rx_two);
    broker.publish(sample_event(session_id, 1));
    assert_eq!(broker.receiver_count(session_id), 0);
  }
}

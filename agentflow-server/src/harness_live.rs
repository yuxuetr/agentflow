//! Real LLM-backed [`HarnessSessionExecutor`] (P-H.5 slice 2).
//!
//! Replaces the [`StubHarnessExecutor`] for deployments that have an LLM
//! provider configured. Wires `agentflow-harness::HarnessRuntime` around
//! a `ReActAgent`, hooks tool execution through `wrap_registry` so the
//! shared `ServerApprovalProvider` can park decisions, and routes the
//! resulting `HarnessEvent` stream into the server's
//! [`HarnessEventBroker`] + Postgres event log.
//!
//! [`StubHarnessExecutor`]: crate::harness::StubHarnessExecutor

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::OnceCell;
use tracing::{error, info, warn};

use agentflow_agents::react::{ReActAgent, ReActConfig};
use agentflow_agents::runtime::AgentStopReason;
use agentflow_harness::{
  ApprovalProvider, HarnessEvent, HarnessEventBody, HarnessEventSink, HarnessProfile,
  HarnessRunOptions, HarnessRuntime, HarnessRuntimeKind, HookConfig, SeqAllocator, SinkChain,
  StopReason, StoppedPayload, default_providers, wrap_registry,
};
use agentflow_llm::AgentFlow;
use agentflow_memory::{MemoryStore, SessionMemory, SqliteMemory};
use agentflow_tools::ToolRegistry;

use agentflow_db::{
  HarnessEventRepo, HarnessSessionRepo, HarnessSessionStatus, NewHarnessSessionEvent, Repositories,
};

use crate::events_stream::broker_finalize_grace;
use crate::harness::{
  HarnessEventBroker, HarnessSessionContext, HarnessSessionExecutor, StreamedHarnessEvent,
};
use crate::harness_approval::{PendingApprovalRegistry, ServerApprovalProvider};

/// HarnessEventSink that fans every envelope out to:
///
/// - the `harness_session_events` Postgres table (durable history,
///   serves SSE backfill and JSON history);
/// - the process-local [`HarnessEventBroker`] (live SSE push).
///
/// Failures persist as `tracing::warn!` and are otherwise non-fatal:
/// the agent run continues even if the event log is briefly unavailable,
/// since dropping a synthetic event is safer than aborting a real
/// session. Subscribers can reconnect with `?after_seq=` to refill from
/// the DB once writes recover.
pub struct ServerHarnessEventSink {
  repos: Repositories,
  broker: HarnessEventBroker,
}

impl ServerHarnessEventSink {
  pub fn new(repos: Repositories, broker: HarnessEventBroker) -> Self {
    Self { repos, broker }
  }
}

#[async_trait]
impl HarnessEventSink for ServerHarnessEventSink {
  fn name(&self) -> &str {
    "server"
  }

  async fn write(&self, event: &HarnessEvent) -> Result<(), agentflow_harness::HarnessError> {
    let Ok(session_uuid) = uuid::Uuid::parse_str(&event.session_id) else {
      // Non-UUID session id arrived (test runtime or external caller).
      // Drop with a warning; the contract guarantees server-managed
      // sessions always pass through `Uuid::new_v4()`.
      warn!(
        session_id = %event.session_id,
        seq = event.seq,
        "harness event sink: session id is not a UUID, skipping persistence"
      );
      return Ok(());
    };
    let kind = harness_event_kind(&event.body);
    let payload = serde_json::to_value(&event.body).unwrap_or(serde_json::Value::Null);

    let new_event = NewHarnessSessionEvent {
      session_id: session_uuid,
      seq: event.seq as i64,
      kind: kind.to_string(),
      payload,
    };
    match self.repos.harness_events.append(new_event).await {
      Ok(stored) => {
        self.broker.publish(StreamedHarnessEvent::from(stored));
        Ok(())
      }
      Err(err) => {
        warn!(
          session_id = %event.session_id,
          seq = event.seq,
          error = %err,
          "harness event sink: persist failed"
        );
        // Surface to the runtime as Ok so the agent keeps running. The
        // event is lost from the live stream; subscribers can pull
        // history once persistence recovers.
        Ok(())
      }
    }
  }
}

fn harness_event_kind(body: &HarnessEventBody) -> &'static str {
  match body {
    HarnessEventBody::SessionStarted(_) => "session_started",
    HarnessEventBody::StepStarted(_) => "step_started",
    HarnessEventBody::ToolCallRequested(_) => "tool_call_requested",
    HarnessEventBody::ApprovalRequested(_) => "approval_requested",
    HarnessEventBody::ApprovalDecided(_) => "approval_decided",
    HarnessEventBody::ToolCallCompleted(_) => "tool_call_completed",
    HarnessEventBody::BackgroundTaskUpdated(_) => "background_task_updated",
    HarnessEventBody::MemorySummaryAdded(_) => "memory_summary_added",
    HarnessEventBody::Stopped(_) => "stopped",
  }
}

/// LLM-backed harness executor.
///
/// Each `execute` call assembles a fresh `ReActAgent` + `HarnessRuntime`
/// around the session's context (workspace_root, profile, runtime kind,
/// model). The executor calls [`AgentFlow::init`] lazily on first use so
/// the test suite doesn't pay for provider config when running the stub
/// path.
///
/// Tool registry is currently empty: tools come in via subsequent slices
/// (skill loading, MCP capability, plugin spawn). The approval pipeline
/// is still wired (`wrap_registry` with `ServerApprovalProvider`) so the
/// surface area is ready once tools land — confirmed by the
/// `harness_routes` integration tests that drive the registry directly.
#[derive(Clone)]
pub struct LiveHarnessExecutor {
  approval_registry: PendingApprovalRegistry,
  approval_timeout: Duration,
  /// Q3.4.3: bounds concurrent harness sessions. Each session spawns
  /// a dedicated OS thread via `spawn_blocking`; uncapped that's a
  /// DoS vector (`/v1/harness/sessions` has no rate limit). The
  /// semaphore caps in-flight executions; callers wait for a permit
  /// before starting their session's blocking runtime. Default cap
  /// is set by `default_max_concurrent_sessions()` (32) and is
  /// overridable via `with_max_concurrent_sessions()`.
  concurrency_limit: Arc<tokio::sync::Semaphore>,
}

/// Q3.4.3: production-safe default cap on concurrent live harness
/// sessions. Each session burns an OS thread for the duration of its
/// run, so the cap is the upper bound on extra OS threads the live
/// executor will materialize. 32 is a balance between local-dev
/// ergonomics (rarely hit) and shared-infra survival.
pub fn default_max_concurrent_sessions() -> usize {
  32
}

impl LiveHarnessExecutor {
  pub fn new(approval_registry: PendingApprovalRegistry, approval_timeout: Duration) -> Self {
    Self {
      approval_registry,
      approval_timeout,
      concurrency_limit: Arc::new(tokio::sync::Semaphore::new(
        default_max_concurrent_sessions(),
      )),
    }
  }

  /// Q3.4.3: override the default concurrency cap. `0` is treated as
  /// "1" so the executor always permits forward progress.
  pub fn with_max_concurrent_sessions(mut self, max: usize) -> Self {
    let max = max.max(1);
    self.concurrency_limit = Arc::new(tokio::sync::Semaphore::new(max));
    self
  }
}

impl std::fmt::Debug for LiveHarnessExecutor {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("LiveHarnessExecutor")
      .field("approval_timeout", &self.approval_timeout)
      .finish()
  }
}

#[async_trait]
impl HarnessSessionExecutor for LiveHarnessExecutor {
  async fn execute(&self, ctx: HarnessSessionContext) {
    // Q3.4.3: acquire the per-process concurrency permit before
    // spawning the OS thread that runs this session. Without this gate
    // a flood of `POST /v1/harness/sessions` would spawn one OS thread
    // per request — `spawn_blocking` doesn't bound thread count by
    // itself. Permit is dropped when the future resolves, freeing the
    // slot for the next session.
    let permit = match self.concurrency_limit.clone().acquire_owned().await {
      Ok(permit) => permit,
      Err(_closed) => {
        // Semaphore is permanently closed — collector shutdown.
        warn!(session_id = %ctx.session_id, "harness concurrency semaphore closed; rejecting session");
        return;
      }
    };
    let _permit_guard = permit; // released on drop
    if let Err(err) = live_execute(self, &ctx).await {
      let err_msg = err.to_string();
      error!(session_id = %ctx.session_id, error = %err_msg, "live harness executor failed");
      let _ = ctx
        .repos
        .harness_sessions
        .update_status(
          ctx.session_id,
          HarnessSessionStatus::Failed,
          None,
          Some(&err_msg),
        )
        .await;
      // Emit a terminal `stopped` event so SSE subscribers and event-log
      // consumers see the H0 contract's required close signal. Two
      // failure shapes need this:
      //   1. `live_execute` errored before `HarnessRuntime::run` could
      //      start (e.g. LLM init / model resolution failed), so the
      //      runtime never wrote anything but `session_started` —
      //      sometimes not even that.
      //   2. `HarnessRuntime::run` errored mid-way (inner agent failed)
      //      and the runtime itself does not currently emit `stopped`
      //      on its error path.
      // Both leave the broker open and the event history missing a
      // terminal kind, which the closed kind set documented in
      // `docs/HARNESS_MODE.md` promises is always present.
      emit_failure_stopped_event(&ctx, &err_msg).await;
      ctx
        .broker
        .finalise_with_grace(ctx.session_id, broker_finalize_grace());
    }
  }
}

/// Persist + publish a synthetic `stopped` event with
/// `StopReason::Failed` for a session whose execution failed before the
/// runtime could emit its own terminal event. seq is computed from the
/// current `MAX(seq)` in the event log so the synthetic event always
/// lands after whatever the runtime did manage to write (typically a
/// solitary `session_started`).
async fn emit_failure_stopped_event(ctx: &HarnessSessionContext, err_msg: &str) {
  let next_seq = match ctx.repos.harness_events.max_seq(ctx.session_id).await {
    Ok(Some(max)) => (max as u64).saturating_add(1),
    Ok(None) => 0,
    Err(err) => {
      warn!(
        session_id = %ctx.session_id,
        error = %err,
        "harness failure-stopped emit: max_seq lookup failed, skipping",
      );
      return;
    }
  };
  let event = HarnessEvent {
    seq: next_seq,
    session_id: ctx.session_id.to_string(),
    ts: chrono::Utc::now(),
    body: HarnessEventBody::Stopped(StoppedPayload {
      reason: StopReason::Failed,
      final_answer: None,
      error: Some(err_msg.to_string()),
    }),
  };
  let sink = ServerHarnessEventSink::new(ctx.repos.clone(), ctx.broker.clone());
  if let Err(err) = sink.write(&event).await {
    warn!(
      session_id = %ctx.session_id,
      error = %err,
      "harness failure-stopped emit: sink write failed",
    );
  }
}

/// Lazy AgentFlow init guard so the LLM registry is loaded at most once
/// per process. Subsequent calls are no-ops and return immediately.
async fn ensure_llm_initialized() -> Result<(), LiveExecutorError> {
  static INIT: OnceCell<()> = OnceCell::const_new();
  INIT
    .get_or_try_init(|| async { AgentFlow::init().await.map_err(LiveExecutorError::from) })
    .await
    .map(|_| ())
}

/// Snapshot of the inputs the inner harness session needs. We move a
/// fresh owned copy onto the blocking thread so the spawned task is
/// `'static` and doesn't carry a borrow of [`HarnessSessionContext`].
#[derive(Clone)]
struct RunInputs {
  session_id: uuid::Uuid,
  user_input: String,
  workspace_root: String,
  profile: String,
  runtime_kind: String,
  model: String,
  skill_name: Option<String>,
  repos: Repositories,
  broker: HarnessEventBroker,
  initial_seq: u64,
}

fn clone_run_inputs(ctx: &HarnessSessionContext) -> RunInputs {
  RunInputs {
    session_id: ctx.session_id,
    user_input: ctx.user_input.clone(),
    workspace_root: ctx.workspace_root.clone(),
    profile: ctx.profile.clone(),
    runtime_kind: ctx.runtime_kind.clone(),
    model: ctx.model.clone(),
    skill_name: ctx.skill_name.clone(),
    repos: ctx.repos.clone(),
    broker: ctx.broker.clone(),
    initial_seq: ctx.initial_seq,
  }
}

/// Runs `HarnessRuntime::run` on a dedicated current-thread Tokio
/// runtime hosted inside `tokio::task::spawn_blocking`.
///
/// **Why:** `HarnessRuntime::run` holds `&self` across `.await` points
/// (it calls `self.collect_context(...).await` and friends). For its
/// future to be `Send`, `HarnessRuntime: Sync` would have to hold — but
/// the inner `Box<dyn AgentRuntime>` is `Send`-only because
/// `AgentRuntime: Send`. The smoke test in `agentflow-harness` works
/// around this by being a `current_thread` tokio test (no `Send`
/// requirement on the test future). Server-side we want the same
/// relaxed-Send execution environment without forcing the rest of the
/// server onto a current-thread runtime, so we offload onto
/// `spawn_blocking` and start an isolated current-thread runtime
/// there. The cost is one OS thread per concurrent harness session,
/// which is acceptable for now and is removed once `HarnessRuntime` is
/// updated to thread `&mut self` (or `Sync` is added to
/// `AgentRuntime`).
/// Build the harness agent's conversation memory.
///
/// When `AGENTFLOW_HARNESS_MEMORY_DB` is set to a non-empty path, use a
/// persistent SQLite store (keyed by session_id, WAL + busy_timeout via
/// the shared sqlite pool) so a `:resume` reads the prior turns back
/// across process restarts — long-lived server sessions. Otherwise the
/// in-process default (unchanged behaviour). Opt-in because a shared
/// SQLite file is a single-node assumption; multi-node deployments should
/// front it with their own backend.
async fn build_harness_memory() -> Result<Box<dyn MemoryStore>, LiveExecutorError> {
  match std::env::var("AGENTFLOW_HARNESS_MEMORY_DB")
    .ok()
    .filter(|p| !p.trim().is_empty())
  {
    Some(path) => open_persistent_harness_memory(path.trim()).await,
    None => Ok(Box::new(SessionMemory::default_window()) as Box<dyn MemoryStore>),
  }
}

/// Open a persistent SQLite conversation store at `path`, creating the
/// parent directory if needed. Keyed by session_id, so a resumed session
/// reads the prior turns back.
async fn open_persistent_harness_memory(
  path: &str,
) -> Result<Box<dyn MemoryStore>, LiveExecutorError> {
  let path = std::path::PathBuf::from(path);
  if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
    std::fs::create_dir_all(parent).map_err(|e| {
      LiveExecutorError::Harness(agentflow_harness::HarnessError::Other(format!(
        "could not create harness memory dir {}: {e}",
        parent.display()
      )))
    })?;
  }
  let store = SqliteMemory::open(&path).await.map_err(|e| {
    LiveExecutorError::Harness(agentflow_harness::HarnessError::Other(format!(
      "failed to open harness memory db {}: {e}",
      path.display()
    )))
  })?;
  Ok(Box::new(store))
}

async fn run_harness_blocking(
  executor: LiveHarnessExecutor,
  inputs: RunInputs,
) -> Result<agentflow_harness::HarnessRunResult, LiveExecutorError> {
  let join = tokio::task::spawn_blocking(move || -> Result<_, LiveExecutorError> {
    let rt = tokio::runtime::Builder::new_current_thread()
      .enable_all()
      .build()
      .map_err(|err| {
        LiveExecutorError::Harness(agentflow_harness::HarnessError::Other(format!(
          "failed to build inner runtime: {err}"
        )))
      })?;
    rt.block_on(run_harness_inner(executor, inputs))
  });
  match join.await {
    Ok(result) => result,
    Err(err) => Err(LiveExecutorError::Harness(
      agentflow_harness::HarnessError::Other(format!("harness task panicked: {err}")),
    )),
  }
}

async fn run_harness_inner(
  executor: LiveHarnessExecutor,
  inputs: RunInputs,
) -> Result<agentflow_harness::HarnessRunResult, LiveExecutorError> {
  let session_id_string = inputs.session_id.to_string();
  let profile = parse_profile(&inputs.profile);
  let runtime_kind = parse_runtime_kind(&inputs.runtime_kind);

  let server_sink: Arc<dyn HarnessEventSink> = Arc::new(ServerHarnessEventSink::new(
    inputs.repos.clone(),
    inputs.broker.clone(),
  ));
  let sinks = SinkChain::new().push(server_sink.clone());

  // Q1.7.1 + P-A3.4: one shared `SeqAllocator` for both the hook layer and the
  // runtime. Pre-Q1.7.1 they each owned an independent counter and mixed events
  // would collide on the JSON-Lines sink's `(session_id, seq)` PK. P-A3.4 adds
  // the emit lock to the shared unit so the hook layer's concurrent tool /
  // approval events and the runtime's live bridge events also reach the sink in
  // seq order, not just carry monotonic seq numbers.
  let seq_allocator = SeqAllocator::with_initial(inputs.initial_seq);

  let approval_provider: Arc<dyn ApprovalProvider> = Arc::new(ServerApprovalProvider::new(
    executor.approval_registry.clone(),
    executor.approval_timeout,
  ));

  let hook_config = HookConfig::new(session_id_string.clone(), approval_provider, sinks.clone())
    .with_profile(profile)
    .with_seq_allocator(seq_allocator.clone())
    .with_approval_timeout(executor.approval_timeout);

  let registry = wrap_registry(ToolRegistry::new(), hook_config);

  let react_config = ReActConfig::new(&inputs.model).with_max_iterations(4);
  // Conversation memory: persistent (keyed by session_id) when the
  // operator configures it, so `:resume` continues prior turns across
  // restarts; otherwise the in-process default (back-compat).
  let memory = build_harness_memory().await?;
  let agent = ReActAgent::new(react_config, memory, Arc::new(registry));

  let mut runtime = HarnessRuntime::new(Box::new(agent))
    .with_event_sink(server_sink.clone())
    .with_context_providers(default_providers())
    .with_seq_allocator(seq_allocator.clone());

  let options = HarnessRunOptions::new(
    inputs.user_input,
    PathBuf::from(&inputs.workspace_root),
    inputs.model,
  )
  .with_profile(profile)
  .with_runtime_kind(runtime_kind)
  .with_session_id(session_id_string);
  let options = match inputs.skill_name.as_ref() {
    Some(name) => options.with_skill_name(name.clone()),
    None => options,
  };

  let result = runtime.run(options).await?;
  Ok(result)
}

async fn live_execute(
  executor: &LiveHarnessExecutor,
  ctx: &HarnessSessionContext,
) -> Result<(), LiveExecutorError> {
  ensure_llm_initialized().await?;
  let result = run_harness_blocking(executor.clone(), clone_run_inputs(ctx)).await?;

  // Map the inner agent's stop reason back to the session row's
  // terminal state. The closed `AgentStopReason` enum keeps the match
  // exhaustive at compile time, so new variants surface as errors here
  // rather than silently turning into `Failed`.
  let (status, final_answer, error) = match &result.stop_reason {
    AgentStopReason::FinalAnswer | AgentStopReason::StopCondition { .. } => {
      (HarnessSessionStatus::Completed, result.answer.clone(), None)
    }
    AgentStopReason::MaxSteps { max_steps } => (
      HarnessSessionStatus::Failed,
      result.answer.clone(),
      Some(format!("max_steps_reached:{max_steps}")),
    ),
    AgentStopReason::MaxToolCalls { max_tool_calls } => (
      HarnessSessionStatus::Failed,
      result.answer.clone(),
      Some(format!("max_tool_calls_reached:{max_tool_calls}")),
    ),
    AgentStopReason::Timeout { timeout_ms } => (
      HarnessSessionStatus::Failed,
      result.answer.clone(),
      Some(format!("timeout:{timeout_ms}ms")),
    ),
    AgentStopReason::Cancelled { message } => (
      HarnessSessionStatus::Cancelled,
      None,
      Some(format!("cancelled:{message}")),
    ),
    AgentStopReason::TokenBudgetExceeded { used, budget } => (
      HarnessSessionStatus::Failed,
      result.answer.clone(),
      Some(format!("token_budget_exceeded:{used}/{budget}")),
    ),
    AgentStopReason::CostLimitExceeded {
      used_usd,
      budget_usd,
    } => (
      HarnessSessionStatus::Failed,
      result.answer.clone(),
      Some(format!(
        "cost_limit_exceeded:${used_usd:.4}/${budget_usd:.4}"
      )),
    ),
    AgentStopReason::Error { message } => (
      HarnessSessionStatus::Failed,
      None,
      Some(format!("agent_error:{message}")),
    ),
  };
  ctx
    .repos
    .harness_sessions
    .update_status(
      ctx.session_id,
      status,
      final_answer.as_deref(),
      error.as_deref(),
    )
    .await?;

  ctx
    .broker
    .finalise_with_grace(ctx.session_id, broker_finalize_grace());
  info!(session_id = %ctx.session_id, "live harness executor finished");
  Ok(())
}

fn parse_profile(value: &str) -> HarnessProfile {
  match value {
    "dev" => HarnessProfile::Dev,
    "production" => HarnessProfile::Production,
    _ => HarnessProfile::Local,
  }
}

fn parse_runtime_kind(value: &str) -> HarnessRuntimeKind {
  match value {
    "plan_execute" => HarnessRuntimeKind::PlanExecute,
    _ => HarnessRuntimeKind::React,
  }
}

#[derive(Debug, thiserror::Error)]
enum LiveExecutorError {
  #[error(transparent)]
  Llm(#[from] agentflow_llm::LLMError),
  #[error(transparent)]
  Harness(#[from] agentflow_harness::HarnessError),
  #[error(transparent)]
  Db(#[from] agentflow_db::DbError),
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Resume contract: a persistent harness store at a path keeps a
  /// session's turns across re-opens (keyed by session_id), so a `:resume`
  /// against the same DB reads the prior conversation back. Tested via
  /// the path helper (no env) to stay race-free.
  #[tokio::test]
  async fn persistent_harness_memory_survives_reopen() {
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("harness-memory.sqlite");
    let path = db.to_string_lossy().into_owned();

    let first = open_persistent_harness_memory(&path).await.unwrap();
    first
      .add_message(agentflow_memory::Message::user(
        "sess-srv",
        "remember the deploy key",
      ))
      .await
      .unwrap();
    drop(first);

    let second = open_persistent_harness_memory(&path).await.unwrap();
    let history = second.get_all("sess-srv").await.unwrap();
    assert!(
      history
        .iter()
        .any(|m| m.content.contains("remember the deploy key")),
      "resume must restore the prior conversation from the persistent store"
    );
  }

  use agentflow_harness::{
    HarnessEvent, HarnessEventBody, SessionStartedPayload, StopReason, StoppedPayload,
  };
  use chrono::Utc;

  #[test]
  fn parse_profile_falls_back_to_local() {
    assert!(matches!(parse_profile("dev"), HarnessProfile::Dev));
    assert!(matches!(
      parse_profile("production"),
      HarnessProfile::Production
    ));
    assert!(matches!(parse_profile("local"), HarnessProfile::Local));
    assert!(matches!(parse_profile(""), HarnessProfile::Local));
    assert!(matches!(parse_profile("wat"), HarnessProfile::Local));
  }

  #[test]
  fn parse_runtime_kind_defaults_to_react() {
    assert!(matches!(
      parse_runtime_kind("react"),
      HarnessRuntimeKind::React
    ));
    assert!(matches!(
      parse_runtime_kind("plan_execute"),
      HarnessRuntimeKind::PlanExecute
    ));
    assert!(matches!(
      parse_runtime_kind("unknown"),
      HarnessRuntimeKind::React
    ));
  }

  #[test]
  fn harness_event_kind_covers_every_variant() {
    // Sanity check: each variant's kind() matches the canonical wire
    // name. The closed enum guarantees this exhaustively at compile
    // time; the assertions guard against future renames.
    let started = HarnessEvent {
      seq: 0,
      session_id: "s".into(),
      ts: chrono::Utc::now(),
      body: HarnessEventBody::SessionStarted(SessionStartedPayload {
        workspace_root: "/".into(),
        runtime: HarnessRuntimeKind::React,
        profile: HarnessProfile::Local,
        model: "m".into(),
        skills: Vec::new(),
        context_item_count: 0,
        context_token_estimate: 0,
      }),
    };
    let stopped = HarnessEvent {
      seq: 1,
      session_id: "s".into(),
      ts: Utc::now(),
      body: HarnessEventBody::Stopped(StoppedPayload {
        reason: StopReason::Completed,
        final_answer: None,
        error: None,
      }),
    };
    assert_eq!(harness_event_kind(&started.body), "session_started");
    assert_eq!(harness_event_kind(&stopped.body), "stopped");
  }
}

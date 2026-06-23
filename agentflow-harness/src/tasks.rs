//! Background task runtime (Phase H4 / `P-H.4`).
//!
//! Lets a Harness session spawn child agents in the background, query
//! their status, capture their output, and cancel them — all through
//! standard [`agentflow_tools::Tool`] implementations the agent calls
//! like any other tool.
//!
//! Design choices for v1:
//!
//! - **In-process runtime.** Each task is a `tokio::spawn`-backed
//!   future running an inner [`AgentRuntime`]. No subprocess, no
//!   queue. Server-backed tasks arrive with Phase H5.
//! - **Caller-supplied factory.** The harness can't build a fresh
//!   agent on its own (the registry, memory, and LLM config live
//!   outside the harness), so [`TaskAgentFactory::build`] is the
//!   integration seam.
//! - **Nested spawn rejection.** A task that's currently running
//!   inside the runtime cannot spawn another task via
//!   [`TaskCreateTool`]. Enforced through `tokio::task_local!` so
//!   the rejection is automatic even when nested code paths reach
//!   the runtime through cloned `Arc`s.
//! - **Bounded output buffer.** Each task captures up to
//!   `max_output_bytes` of incremental output through the [`TaskWriter`]
//!   handed to the factory. Overflow is recorded but not fatal.
//! - **Trace emission.** Every lifecycle transition emits one
//!   [`HarnessEventBody::BackgroundTaskUpdated`] through the same
//!   [`SinkChain`] the parent session uses, with the parent
//!   session's `seq_counter` so the order interleaves cleanly with
//!   approval and tool events.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use agentflow_agent_spi::runtime::{
  AgentCancellationToken, AgentContext, AgentRuntime, AgentStopReason,
};
use agentflow_tools::{Tool, ToolError, ToolMetadata, ToolOutput, ToolPermissionSet, ToolSource};

use crate::error::HarnessError;
use crate::event::{BackgroundTaskStatus, BackgroundTaskUpdatedPayload, HarnessEventBody};
use crate::persistence::SinkChain;
use crate::seq::SeqAllocator;

tokio::task_local! {
  /// Set inside the spawned `tokio::task` that runs a background
  /// task. Used by [`TaskCreateTool`] to reject nested spawns.
  static IN_BACKGROUND_TASK: ();
}

/// Default cap on bytes captured in [`TaskHandle::output`].
pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 64 * 1024;

/// Lifecycle states emitted as the runtime transitions a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
  /// Spawned but not yet running.
  Pending,
  /// Inner agent is currently executing.
  Running,
  /// Inner agent produced a final answer.
  Completed,
  /// Inner agent failed or stopped with an error reason.
  Failed,
  /// Cancellation was signalled before completion.
  Cancelled,
}

impl TaskStatus {
  pub fn is_terminal(self) -> bool {
    matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
  }

  fn envelope(self) -> BackgroundTaskStatus {
    match self {
      Self::Pending => BackgroundTaskStatus::Pending,
      Self::Running => BackgroundTaskStatus::Running,
      Self::Completed => BackgroundTaskStatus::Completed,
      Self::Failed => BackgroundTaskStatus::Failed,
      Self::Cancelled => BackgroundTaskStatus::Cancelled,
    }
  }
}

/// Stable identifier + lifecycle snapshot returned by `task_get` /
/// `task_list`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskHandle {
  pub id: String,
  pub prompt: String,
  pub status: TaskStatus,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub skill: Option<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub tools_allowed: Vec<String>,
  pub created_at: DateTime<Utc>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub started_at: Option<DateTime<Utc>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub completed_at: Option<DateTime<Utc>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub final_answer: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub error: Option<String>,
  /// Captured output lines (newline-stripped). Capped at
  /// `max_output_bytes` of the parent runtime; overflow sets
  /// [`Self::output_truncated`].
  #[serde(default)]
  pub output: Vec<String>,
  #[serde(default)]
  pub output_truncated: bool,
}

/// Inputs to [`TaskAgentFactory::build`].
#[derive(Debug, Clone)]
pub struct TaskSpec {
  pub task_id: String,
  pub prompt: String,
  pub skill: Option<String>,
  pub tools_allowed: Vec<String>,
}

/// Output handed back from [`TaskAgentFactory::build`].
pub struct TaskAgentBundle {
  /// Inner agent the runtime will drive with [`AgentRuntime::run`].
  pub agent: Box<dyn AgentRuntime>,
  /// Context to pass to that `run`. The factory is responsible for
  /// resolving model id + persona; the runtime threads the
  /// cancellation token in before invoking the agent.
  pub context: AgentContext,
}

/// Caller-supplied way to construct the inner agent that runs a
/// task. The factory keeps `TaskRuntime` agnostic to LLM config,
/// memory backend, and tool registry.
#[async_trait]
pub trait TaskAgentFactory: Send + Sync {
  async fn build(
    &self,
    spec: &TaskSpec,
    cancellation: AgentCancellationToken,
  ) -> Result<TaskAgentBundle, HarnessError>;
}

/// Snapshot of task state shared between the runtime, the spawned
/// tokio task, and the [`TaskWriter`] handed to the factory.
struct TaskState {
  handle: TaskHandle,
  cancellation: AgentCancellationToken,
  output_bytes: usize,
}

/// Cheap, cloneable handle the factory can use to stream output back
/// to the runtime. Each `push_line` is bounded by the parent's
/// `max_output_bytes`.
#[derive(Clone)]
pub struct TaskWriter {
  task_id: String,
  inner: Arc<Mutex<TaskRuntimeInner>>,
  max_output_bytes: usize,
}

impl TaskWriter {
  /// Append a single line to the captured output. Lines that would
  /// push the buffer past `max_output_bytes` are dropped and the
  /// `output_truncated` flag is set.
  pub async fn push_line(&self, line: impl Into<String>) {
    let mut line: String = line.into();
    if line.ends_with('\n') {
      line.pop();
    }
    let line_bytes = line.len();
    let mut inner = self.inner.lock().await;
    if let Some(state) = inner.tasks.get_mut(&self.task_id) {
      if state.output_bytes + line_bytes > self.max_output_bytes {
        state.handle.output_truncated = true;
        return;
      }
      state.output_bytes += line_bytes;
      state.handle.output.push(line);
    }
  }
}

struct TaskRuntimeInner {
  tasks: HashMap<String, TaskState>,
}

/// Owns the running and historical task table. Cheap to clone (Arc
/// internally) so each `Task*Tool` can share one runtime.
#[derive(Clone)]
pub struct TaskRuntime {
  session_id: String,
  sinks: SinkChain,
  seq_allocator: SeqAllocator,
  max_output_bytes: usize,
  agent_factory: Arc<dyn TaskAgentFactory>,
  inner: Arc<Mutex<TaskRuntimeInner>>,
}

impl std::fmt::Debug for TaskRuntime {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("TaskRuntime")
      .field("session_id", &self.session_id)
      .field("max_output_bytes", &self.max_output_bytes)
      .finish_non_exhaustive()
  }
}

impl TaskRuntime {
  pub fn new(
    session_id: impl Into<String>,
    sinks: SinkChain,
    seq_counter: Arc<AtomicU64>,
    agent_factory: Arc<dyn TaskAgentFactory>,
  ) -> Self {
    Self {
      session_id: session_id.into(),
      sinks,
      // P-A3.4: wrap the shared counter so background-task lifecycle events
      // serialize their (allocate, dispatch) against each other. Concurrent
      // tasks emit `BackgroundTaskUpdated` through this one runtime.
      seq_allocator: SeqAllocator::from_counter(seq_counter),
      max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
      agent_factory,
      inner: Arc::new(Mutex::new(TaskRuntimeInner {
        tasks: HashMap::new(),
      })),
    }
  }

  pub fn with_max_output_bytes(mut self, bytes: usize) -> Self {
    self.max_output_bytes = bytes;
    self
  }

  /// Spawn a new task. Refuses with [`HarnessError::InvalidState`]
  /// when called from within a running task (nested spawn rejection).
  pub async fn create_task(&self, spec: TaskSpec) -> Result<String, HarnessError> {
    if IN_BACKGROUND_TASK.try_with(|_| ()).is_ok() {
      return Err(HarnessError::InvalidState(
        "nested background tasks are not allowed in Phase H4".into(),
      ));
    }

    let task_id = if spec.task_id.is_empty() {
      uuid::Uuid::new_v4().to_string()
    } else {
      spec.task_id.clone()
    };
    let cancellation = AgentCancellationToken::new();
    let now = Utc::now();
    let handle = TaskHandle {
      id: task_id.clone(),
      prompt: spec.prompt.clone(),
      status: TaskStatus::Pending,
      skill: spec.skill.clone(),
      tools_allowed: spec.tools_allowed.clone(),
      created_at: now,
      started_at: None,
      completed_at: None,
      final_answer: None,
      error: None,
      output: Vec::new(),
      output_truncated: false,
    };
    {
      let mut inner = self.inner.lock().await;
      if inner.tasks.contains_key(&task_id) {
        return Err(HarnessError::InvalidState(format!(
          "task '{task_id}' already exists"
        )));
      }
      inner.tasks.insert(
        task_id.clone(),
        TaskState {
          handle,
          cancellation: cancellation.clone(),
          output_bytes: 0,
        },
      );
    }
    self
      .emit_status(&task_id, TaskStatus::Pending, None, None)
      .await?;

    let runtime = self.clone();
    let spec_clone = TaskSpec {
      task_id: task_id.clone(),
      ..spec
    };
    let cancellation_for_task = cancellation.clone();

    tokio::spawn(IN_BACKGROUND_TASK.scope((), async move {
      runtime.drive_task(spec_clone, cancellation_for_task).await;
    }));

    Ok(task_id)
  }

  /// Fetch a snapshot of a task's [`TaskHandle`].
  pub async fn get(&self, task_id: &str) -> Result<TaskHandle, HarnessError> {
    let inner = self.inner.lock().await;
    inner
      .tasks
      .get(task_id)
      .map(|state| state.handle.clone())
      .ok_or_else(|| HarnessError::SessionNotFound(format!("task '{task_id}'")))
  }

  /// List every task in the runtime. Sorted by `created_at` ascending.
  pub async fn list(&self, status: Option<TaskStatus>) -> Vec<TaskHandle> {
    let inner = self.inner.lock().await;
    let mut handles: Vec<TaskHandle> = inner
      .tasks
      .values()
      .filter(|state| status.is_none_or(|want| state.handle.status == want))
      .map(|state| state.handle.clone())
      .collect();
    handles.sort_by_key(|a| a.created_at);
    handles
  }

  /// Signal cancellation on a running task. Returns the latest
  /// handle snapshot after the signal is delivered.
  pub async fn stop(&self, task_id: &str) -> Result<TaskHandle, HarnessError> {
    let cancel = {
      let inner = self.inner.lock().await;
      let state = inner
        .tasks
        .get(task_id)
        .ok_or_else(|| HarnessError::SessionNotFound(format!("task '{task_id}'")))?;
      if state.handle.status.is_terminal() {
        return Ok(state.handle.clone());
      }
      state.cancellation.clone()
    };
    cancel.cancel();
    // Read again so the caller sees the (possibly already-flipped)
    // status set by `drive_task`.
    self.get(task_id).await
  }

  /// Returns the last `tail` captured output lines. `tail = 0` returns
  /// all captured output.
  pub async fn output(
    &self,
    task_id: &str,
    tail: usize,
  ) -> Result<TaskOutputSnapshot, HarnessError> {
    let inner = self.inner.lock().await;
    let state = inner
      .tasks
      .get(task_id)
      .ok_or_else(|| HarnessError::SessionNotFound(format!("task '{task_id}'")))?;
    let lines = if tail == 0 || tail >= state.handle.output.len() {
      state.handle.output.clone()
    } else {
      state.handle.output[state.handle.output.len() - tail..].to_vec()
    };
    Ok(TaskOutputSnapshot {
      task_id: task_id.to_owned(),
      status: state.handle.status,
      truncated: state.handle.output_truncated,
      lines,
    })
  }

  async fn drive_task(self, spec: TaskSpec, cancellation: AgentCancellationToken) {
    // Mark as running.
    self
      .update_state(&spec.task_id, |state| {
        state.handle.status = TaskStatus::Running;
        state.handle.started_at = Some(Utc::now());
      })
      .await;
    let _ = self
      .emit_status(&spec.task_id, TaskStatus::Running, None, None)
      .await;

    let bundle = match self.agent_factory.build(&spec, cancellation.clone()).await {
      Ok(bundle) => bundle,
      Err(err) => {
        let message = err.to_string();
        self
          .update_state(&spec.task_id, |state| {
            state.handle.status = TaskStatus::Failed;
            state.handle.error = Some(message.clone());
            state.handle.completed_at = Some(Utc::now());
          })
          .await;
        let _ = self
          .emit_status(&spec.task_id, TaskStatus::Failed, None, Some(message))
          .await;
        return;
      }
    };

    let TaskAgentBundle {
      mut agent,
      mut context,
    } = bundle;
    // Always thread the cancellation token in. The factory may have
    // already populated one; the runtime's token wins so `task_stop`
    // is honored.
    context = context.with_cancellation_token(cancellation.clone());

    let run_result = agent.run(context).await;
    let now = Utc::now();
    match run_result {
      Err(err) => {
        let message = err.to_string();
        self
          .update_state(&spec.task_id, |state| {
            state.handle.status = TaskStatus::Failed;
            state.handle.error = Some(message.clone());
            state.handle.completed_at = Some(now);
          })
          .await;
        let _ = self
          .emit_status(&spec.task_id, TaskStatus::Failed, None, Some(message))
          .await;
      }
      Ok(result) => {
        let cancelled_now = cancellation.is_cancelled()
          || matches!(result.stop_reason, AgentStopReason::Cancelled { .. });
        let final_status = if cancelled_now {
          TaskStatus::Cancelled
        } else if result.stop_reason.is_success() {
          TaskStatus::Completed
        } else {
          TaskStatus::Failed
        };
        let summary = result.answer.clone();
        let error = if matches!(final_status, TaskStatus::Failed) {
          Some(format!("agent stopped with {:?}", result.stop_reason))
        } else {
          None
        };
        self
          .update_state(&spec.task_id, |state| {
            state.handle.status = final_status;
            state.handle.final_answer = result.answer.clone();
            state.handle.completed_at = Some(now);
            state.handle.error = error.clone();
          })
          .await;
        let _ = self
          .emit_status(&spec.task_id, final_status, summary, error)
          .await;
      }
    }
  }

  async fn update_state(&self, task_id: &str, f: impl FnOnce(&mut TaskState)) {
    let mut inner = self.inner.lock().await;
    if let Some(state) = inner.tasks.get_mut(task_id) {
      f(state);
    }
  }

  async fn emit_status(
    &self,
    task_id: &str,
    status: TaskStatus,
    summary: Option<String>,
    error: Option<String>,
  ) -> Result<(), HarnessError> {
    // P-A3.4: stamp under the shared emit lock so concurrent background tasks'
    // lifecycle events reach the sink in seq order.
    self
      .seq_allocator
      .stamp(
        &self.sinks,
        &self.session_id,
        Utc::now(),
        HarnessEventBody::BackgroundTaskUpdated(BackgroundTaskUpdatedPayload {
          task_id: task_id.to_owned(),
          status: status.envelope(),
          summary,
          error,
        }),
      )
      .await
      .map(|_| ())
  }

  /// Build a [`TaskWriter`] for the caller's factory to stream output
  /// back into the captured buffer.
  pub fn writer(&self, task_id: impl Into<String>) -> TaskWriter {
    TaskWriter {
      task_id: task_id.into(),
      inner: self.inner.clone(),
      max_output_bytes: self.max_output_bytes,
    }
  }
}

/// Snapshot returned by `task_output`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskOutputSnapshot {
  pub task_id: String,
  pub status: TaskStatus,
  pub truncated: bool,
  pub lines: Vec<String>,
}

// ── Built-in tools ────────────────────────────────────────────────

const TASK_CREATE_TOOL_NAME: &str = "task_create";
const TASK_GET_TOOL_NAME: &str = "task_get";
const TASK_LIST_TOOL_NAME: &str = "task_list";
const TASK_STOP_TOOL_NAME: &str = "task_stop";
const TASK_OUTPUT_TOOL_NAME: &str = "task_output";

/// Tool exposing [`TaskRuntime::create_task`] to an agent.
pub struct TaskCreateTool {
  runtime: Arc<TaskRuntime>,
}

impl TaskCreateTool {
  pub fn new(runtime: Arc<TaskRuntime>) -> Self {
    Self { runtime }
  }
}

#[async_trait]
impl Tool for TaskCreateTool {
  fn name(&self) -> &str {
    TASK_CREATE_TOOL_NAME
  }
  fn description(&self) -> &str {
    "Spawn a background task that runs a child agent with the given prompt. \
     Returns the task_id. Tasks cannot create nested tasks."
  }
  fn parameters_schema(&self) -> serde_json::Value {
    serde_json::json!({
      "type": "object",
      "properties": {
        "prompt": {"type": "string"},
        "skill": {"type": "string"},
        "tools_allowed": {"type": "array", "items": {"type": "string"}}
      },
      "required": ["prompt"]
    })
  }
  fn metadata(&self) -> ToolMetadata {
    ToolMetadata {
      source: ToolSource::Builtin,
      permissions: ToolPermissionSet::default(),
      idempotency: agentflow_tools::ToolIdempotency::NonIdempotent,
      mcp_server_name: None,
      mcp_tool_name: None,
    }
  }
  async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
    let prompt = params
      .get("prompt")
      .and_then(|v| v.as_str())
      .ok_or_else(|| ToolError::ExecutionFailed {
        message: "task_create: `prompt` is required and must be a string".into(),
      })?
      .to_owned();
    let skill = params
      .get("skill")
      .and_then(|v| v.as_str())
      .map(ToOwned::to_owned);
    let tools_allowed = params
      .get("tools_allowed")
      .and_then(|v| v.as_array())
      .map(|arr| {
        arr
          .iter()
          .filter_map(|v| v.as_str().map(ToOwned::to_owned))
          .collect::<Vec<_>>()
      })
      .unwrap_or_default();
    let spec = TaskSpec {
      task_id: String::new(),
      prompt,
      skill,
      tools_allowed,
    };
    let task_id =
      self
        .runtime
        .create_task(spec)
        .await
        .map_err(|err| ToolError::ExecutionFailed {
          message: err.to_string(),
        })?;
    Ok(ToolOutput::success(
      serde_json::json!({"task_id": task_id}).to_string(),
    ))
  }
}

/// Tool exposing [`TaskRuntime::get`].
pub struct TaskGetTool {
  runtime: Arc<TaskRuntime>,
}

impl TaskGetTool {
  pub fn new(runtime: Arc<TaskRuntime>) -> Self {
    Self { runtime }
  }
}

#[async_trait]
impl Tool for TaskGetTool {
  fn name(&self) -> &str {
    TASK_GET_TOOL_NAME
  }
  fn description(&self) -> &str {
    "Return the current status snapshot for a background task."
  }
  fn parameters_schema(&self) -> serde_json::Value {
    serde_json::json!({
      "type": "object",
      "properties": {"task_id": {"type": "string"}},
      "required": ["task_id"]
    })
  }
  fn metadata(&self) -> ToolMetadata {
    ToolMetadata {
      source: ToolSource::Builtin,
      permissions: ToolPermissionSet::default(),
      idempotency: agentflow_tools::ToolIdempotency::Idempotent,
      mcp_server_name: None,
      mcp_tool_name: None,
    }
  }
  async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
    let task_id = require_task_id(&params, "task_get")?;
    let handle = self
      .runtime
      .get(&task_id)
      .await
      .map_err(|err| ToolError::ExecutionFailed {
        message: err.to_string(),
      })?;
    Ok(ToolOutput::success(
      serde_json::to_string(&handle).unwrap_or_default(),
    ))
  }
}

/// Tool exposing [`TaskRuntime::list`].
pub struct TaskListTool {
  runtime: Arc<TaskRuntime>,
}

impl TaskListTool {
  pub fn new(runtime: Arc<TaskRuntime>) -> Self {
    Self { runtime }
  }
}

#[async_trait]
impl Tool for TaskListTool {
  fn name(&self) -> &str {
    TASK_LIST_TOOL_NAME
  }
  fn description(&self) -> &str {
    "List all background tasks. Optional `status` filter accepts pending|running|completed|failed|cancelled."
  }
  fn parameters_schema(&self) -> serde_json::Value {
    serde_json::json!({
      "type": "object",
      "properties": {"status": {"type": "string"}}
    })
  }
  fn metadata(&self) -> ToolMetadata {
    ToolMetadata {
      source: ToolSource::Builtin,
      permissions: ToolPermissionSet::default(),
      idempotency: agentflow_tools::ToolIdempotency::Idempotent,
      mcp_server_name: None,
      mcp_tool_name: None,
    }
  }
  async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
    let filter = params.get("status").and_then(|v| v.as_str());
    let filter = match filter {
      Some("pending") => Some(TaskStatus::Pending),
      Some("running") => Some(TaskStatus::Running),
      Some("completed") => Some(TaskStatus::Completed),
      Some("failed") => Some(TaskStatus::Failed),
      Some("cancelled") => Some(TaskStatus::Cancelled),
      Some(other) => {
        return Err(ToolError::ExecutionFailed {
          message: format!("task_list: unknown status filter '{other}'"),
        });
      }
      None => None,
    };
    let handles = self.runtime.list(filter).await;
    Ok(ToolOutput::success(
      serde_json::to_string(&handles).unwrap_or_default(),
    ))
  }
}

/// Tool exposing [`TaskRuntime::stop`].
pub struct TaskStopTool {
  runtime: Arc<TaskRuntime>,
}

impl TaskStopTool {
  pub fn new(runtime: Arc<TaskRuntime>) -> Self {
    Self { runtime }
  }
}

#[async_trait]
impl Tool for TaskStopTool {
  fn name(&self) -> &str {
    TASK_STOP_TOOL_NAME
  }
  fn description(&self) -> &str {
    "Signal cancellation on a background task. The task transitions to `cancelled` once its inner agent observes the signal."
  }
  fn parameters_schema(&self) -> serde_json::Value {
    serde_json::json!({
      "type": "object",
      "properties": {"task_id": {"type": "string"}},
      "required": ["task_id"]
    })
  }
  fn metadata(&self) -> ToolMetadata {
    ToolMetadata {
      source: ToolSource::Builtin,
      permissions: ToolPermissionSet::default(),
      idempotency: agentflow_tools::ToolIdempotency::NonIdempotent,
      mcp_server_name: None,
      mcp_tool_name: None,
    }
  }
  async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
    let task_id = require_task_id(&params, "task_stop")?;
    let handle = self
      .runtime
      .stop(&task_id)
      .await
      .map_err(|err| ToolError::ExecutionFailed {
        message: err.to_string(),
      })?;
    Ok(ToolOutput::success(
      serde_json::to_string(&handle).unwrap_or_default(),
    ))
  }
}

/// Tool exposing [`TaskRuntime::output`].
pub struct TaskOutputTool {
  runtime: Arc<TaskRuntime>,
}

impl TaskOutputTool {
  pub fn new(runtime: Arc<TaskRuntime>) -> Self {
    Self { runtime }
  }
}

#[async_trait]
impl Tool for TaskOutputTool {
  fn name(&self) -> &str {
    TASK_OUTPUT_TOOL_NAME
  }
  fn description(&self) -> &str {
    "Return captured output lines from a background task. Optional `tail_lines` returns just the last N lines."
  }
  fn parameters_schema(&self) -> serde_json::Value {
    serde_json::json!({
      "type": "object",
      "properties": {
        "task_id": {"type": "string"},
        "tail_lines": {"type": "integer", "minimum": 0}
      },
      "required": ["task_id"]
    })
  }
  fn metadata(&self) -> ToolMetadata {
    ToolMetadata {
      source: ToolSource::Builtin,
      permissions: ToolPermissionSet::default(),
      idempotency: agentflow_tools::ToolIdempotency::Idempotent,
      mcp_server_name: None,
      mcp_tool_name: None,
    }
  }
  async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
    let task_id = require_task_id(&params, "task_output")?;
    let tail = params
      .get("tail_lines")
      .and_then(|v| v.as_u64())
      .unwrap_or(0) as usize;
    let snap =
      self
        .runtime
        .output(&task_id, tail)
        .await
        .map_err(|err| ToolError::ExecutionFailed {
          message: err.to_string(),
        })?;
    Ok(ToolOutput::success(
      serde_json::to_string(&snap).unwrap_or_default(),
    ))
  }
}

fn require_task_id(params: &serde_json::Value, tool: &str) -> Result<String, ToolError> {
  params
    .get("task_id")
    .and_then(|v| v.as_str())
    .map(ToOwned::to_owned)
    .ok_or_else(|| ToolError::ExecutionFailed {
      message: format!("{tool}: `task_id` is required and must be a string"),
    })
}

/// Convenience helper: produce `Arc<dyn Tool>` instances for every
/// background-task tool, ready to register in a `ToolRegistry`.
pub fn task_tools(runtime: Arc<TaskRuntime>) -> Vec<Arc<dyn Tool>> {
  vec![
    Arc::new(TaskCreateTool::new(runtime.clone())) as Arc<dyn Tool>,
    Arc::new(TaskGetTool::new(runtime.clone())),
    Arc::new(TaskListTool::new(runtime.clone())),
    Arc::new(TaskStopTool::new(runtime.clone())),
    Arc::new(TaskOutputTool::new(runtime)),
  ]
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::persistence::{HarnessEventSink, InMemoryEventSink};
  use agentflow_agent_spi::runtime::{
    AgentEvent, AgentRunResult, AgentRuntimeError, AgentStep, AgentStepKind,
  };

  // ── Test fixtures ─────────────────────────────────────────────

  struct EchoFactory {
    fail: bool,
    cancel_only: bool,
  }

  #[async_trait]
  impl TaskAgentFactory for EchoFactory {
    async fn build(
      &self,
      spec: &TaskSpec,
      cancellation: AgentCancellationToken,
    ) -> Result<TaskAgentBundle, HarnessError> {
      let agent: Box<dyn AgentRuntime> = Box::new(EchoAgent {
        fail: self.fail,
        cancel_only: self.cancel_only,
      });
      let context = AgentContext::new(&spec.task_id, &spec.prompt, "mock-task")
        .with_cancellation_token(cancellation);
      Ok(TaskAgentBundle { agent, context })
    }
  }

  struct EchoAgent {
    fail: bool,
    cancel_only: bool,
  }

  #[async_trait]
  impl AgentRuntime for EchoAgent {
    async fn run(&mut self, context: AgentContext) -> Result<AgentRunResult, AgentRuntimeError> {
      if self.cancel_only {
        // Wait until cancellation, then return Cancelled stop reason.
        if let Some(token) = &context.cancellation_token {
          token.cancelled().await;
        }
        return Ok(AgentRunResult {
          session_id: context.session_id,
          answer: None,
          stop_reason: AgentStopReason::Cancelled {
            message: "task_stop".into(),
          },
          steps: vec![],
          events: vec![AgentEvent::RunStopped {
            session_id: "".into(),
            reason: AgentStopReason::Cancelled {
              message: "task_stop".into(),
            },
            timestamp: Utc::now(),
          }],
        });
      }
      if self.fail {
        return Err(AgentRuntimeError::ExecutionFailed {
          message: "scripted failure".into(),
        });
      }
      let answer = format!("echo: {}", context.input);
      Ok(AgentRunResult {
        session_id: context.session_id,
        answer: Some(answer.clone()),
        stop_reason: AgentStopReason::FinalAnswer,
        steps: vec![AgentStep::new(0, AgentStepKind::FinalAnswer { answer })],
        events: vec![],
      })
    }
    fn runtime_name(&self) -> &'static str {
      "echo_task"
    }
  }

  fn build_runtime(factory: EchoFactory) -> (TaskRuntime, Arc<InMemoryEventSink>) {
    let sink = Arc::new(InMemoryEventSink::new());
    let sinks = SinkChain::new().push(sink.clone() as Arc<dyn HarnessEventSink>);
    let runtime = TaskRuntime::new(
      "sess",
      sinks,
      Arc::new(AtomicU64::new(0)),
      Arc::new(factory),
    );
    (runtime, sink)
  }

  async fn wait_until_terminal(runtime: &TaskRuntime, task_id: &str) -> TaskHandle {
    for _ in 0..200 {
      let handle = runtime.get(task_id).await.unwrap();
      if handle.status.is_terminal() {
        return handle;
      }
      tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("task '{task_id}' did not reach terminal state");
  }

  // ── Tests ─────────────────────────────────────────────────────

  #[tokio::test]
  async fn spawn_to_complete_records_lifecycle_events() {
    let (runtime, sink) = build_runtime(EchoFactory {
      fail: false,
      cancel_only: false,
    });
    let task_id = runtime
      .create_task(TaskSpec {
        task_id: String::new(),
        prompt: "hi".into(),
        skill: None,
        tools_allowed: Vec::new(),
      })
      .await
      .unwrap();
    let handle = wait_until_terminal(&runtime, &task_id).await;
    assert_eq!(handle.status, TaskStatus::Completed);
    assert_eq!(handle.final_answer.as_deref(), Some("echo: hi"));
    assert!(handle.error.is_none());
    let events = sink.snapshot().await;
    let statuses: Vec<BackgroundTaskStatus> = events
      .iter()
      .filter_map(|event| match &event.body {
        HarnessEventBody::BackgroundTaskUpdated(payload) => Some(payload.status),
        _ => None,
      })
      .collect();
    assert_eq!(
      statuses,
      vec![
        BackgroundTaskStatus::Pending,
        BackgroundTaskStatus::Running,
        BackgroundTaskStatus::Completed,
      ]
    );
  }

  #[tokio::test]
  async fn spawn_to_fail_marks_task_failed() {
    let (runtime, _sink) = build_runtime(EchoFactory {
      fail: true,
      cancel_only: false,
    });
    let task_id = runtime
      .create_task(TaskSpec {
        task_id: String::new(),
        prompt: "x".into(),
        skill: None,
        tools_allowed: Vec::new(),
      })
      .await
      .unwrap();
    let handle = wait_until_terminal(&runtime, &task_id).await;
    assert_eq!(handle.status, TaskStatus::Failed);
    assert!(handle.error.is_some());
  }

  #[tokio::test]
  async fn spawn_then_stop_yields_cancelled() {
    let (runtime, _sink) = build_runtime(EchoFactory {
      fail: false,
      cancel_only: true,
    });
    let task_id = runtime
      .create_task(TaskSpec {
        task_id: String::new(),
        prompt: "wait".into(),
        skill: None,
        tools_allowed: Vec::new(),
      })
      .await
      .unwrap();
    // Give the agent a moment to start before signalling cancel so
    // we exercise the active-state stop path.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    runtime.stop(&task_id).await.unwrap();
    let handle = wait_until_terminal(&runtime, &task_id).await;
    assert_eq!(handle.status, TaskStatus::Cancelled);
  }

  #[tokio::test]
  async fn nested_task_creation_is_rejected() {
    let (runtime, _sink) = build_runtime(EchoFactory {
      fail: false,
      cancel_only: false,
    });
    let runtime_arc = Arc::new(runtime);
    let spec = TaskSpec {
      task_id: String::new(),
      prompt: "outer".into(),
      skill: None,
      tools_allowed: Vec::new(),
    };
    // Run a `create_task` call from within the IN_BACKGROUND_TASK
    // scope to simulate a nested spawn.
    let inner_runtime = runtime_arc.clone();
    let result = IN_BACKGROUND_TASK
      .scope((), async move {
        inner_runtime
          .create_task(TaskSpec {
            task_id: String::new(),
            prompt: "nested".into(),
            skill: None,
            tools_allowed: Vec::new(),
          })
          .await
      })
      .await;
    assert!(matches!(result, Err(HarnessError::InvalidState(_))));
    // Top-level create still works.
    runtime_arc.create_task(spec).await.unwrap();
  }

  #[tokio::test]
  async fn task_list_filters_by_status_and_returns_sorted_handles() {
    let (runtime, _sink) = build_runtime(EchoFactory {
      fail: false,
      cancel_only: false,
    });
    for i in 0..3 {
      runtime
        .create_task(TaskSpec {
          task_id: format!("t{i}"),
          prompt: format!("p{i}"),
          skill: None,
          tools_allowed: Vec::new(),
        })
        .await
        .unwrap();
    }
    // Wait for all to finish.
    for i in 0..3 {
      wait_until_terminal(&runtime, &format!("t{i}")).await;
    }
    let completed = runtime.list(Some(TaskStatus::Completed)).await;
    assert_eq!(completed.len(), 3);
    let none_pending = runtime.list(Some(TaskStatus::Pending)).await;
    assert!(none_pending.is_empty());
  }

  #[tokio::test]
  async fn task_output_truncates_at_max_output_bytes() {
    let (runtime, _sink) = build_runtime(EchoFactory {
      fail: false,
      cancel_only: false,
    });
    let runtime = runtime.with_max_output_bytes(10);
    runtime
      .create_task(TaskSpec {
        task_id: "t-out".into(),
        prompt: "p".into(),
        skill: None,
        tools_allowed: Vec::new(),
      })
      .await
      .unwrap();
    wait_until_terminal(&runtime, "t-out").await;
    let writer = runtime.writer("t-out");
    writer.push_line("short").await;
    writer
      .push_line("this is way too long to fit in the buffer")
      .await;
    let snap = runtime.output("t-out", 0).await.unwrap();
    assert_eq!(snap.lines, vec!["short".to_string()]);
    assert!(snap.truncated);
  }

  #[tokio::test]
  async fn task_create_tool_routes_through_runtime() {
    let (runtime, _sink) = build_runtime(EchoFactory {
      fail: false,
      cancel_only: false,
    });
    let runtime = Arc::new(runtime);
    let tool = TaskCreateTool::new(runtime.clone());
    let output = tool
      .execute(serde_json::json!({"prompt": "via tool"}))
      .await
      .unwrap();
    let body: serde_json::Value = serde_json::from_str(&output.content).unwrap();
    let task_id = body["task_id"].as_str().unwrap().to_string();
    wait_until_terminal(&runtime, &task_id).await;
    let handle = runtime.get(&task_id).await.unwrap();
    assert_eq!(handle.status, TaskStatus::Completed);
    assert_eq!(handle.final_answer.as_deref(), Some("echo: via tool"));
  }

  #[tokio::test]
  async fn task_tools_helper_registers_all_five_names() {
    let (runtime, _sink) = build_runtime(EchoFactory {
      fail: false,
      cancel_only: false,
    });
    let tools = task_tools(Arc::new(runtime));
    let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    assert_eq!(
      names,
      vec![
        "task_create",
        "task_get",
        "task_list",
        "task_stop",
        "task_output",
      ]
    );
  }
}

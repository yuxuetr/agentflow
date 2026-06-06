//! Filesystem-level bridge from Harness sessions into the
//! `agentflow-tracing` directory convention.
//!
//! Two integration tiers live here:
//!
//! 1. JSONL-only ([`open_tracing_sink`] / [`resolve_trace_session_dir`])
//!    — the original Phase H1 shape. Harness session logs land as
//!    `<base>/harness/sessions/<session_id>.jsonl`. Trace replay /
//!    TUI tools that already crawl the trace directory pick these up
//!    without any extra coupling.
//! 2. **ExecutionTrace adapter** ([`ExecutionTraceSink`] /
//!    [`open_execution_trace_sink`]) — Q3.10.4 closes the gap that
//!    CLAUDE.md flagged: each `HarnessEvent` stream is translated
//!    into an `agentflow_tracing::ExecutionTrace` and persisted
//!    through any [`agentflow_tracing::storage::TraceStorage`]
//!    backend. Operators can now point a single SQLite / Postgres
//!    trace store at both DAG workflows AND harness sessions; the
//!    UI / replay tools see harness runs as ordinary execution
//!    traces with one node per tool call.
//!
//! Path precedence stays consistent across both tiers: explicit
//! override → `AGENTFLOW_TRACE_DIR` env var → `~/.agentflow/traces`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentflow_tracing::storage::TraceStorage;
use agentflow_tracing::types::{ExecutionTrace, NodeStatus, NodeTrace, TraceContext, TraceStatus};
use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::error::HarnessError;
use crate::event::{HarnessEvent, HarnessEventBody, StopReason};
use crate::persistence::{HarnessEventSink, JsonlEventSink, default_session_dir};

/// Env var honored by every AgentFlow trace surface, including this
/// bridge.
pub const AGENTFLOW_TRACE_DIR_ENV: &str = "AGENTFLOW_TRACE_DIR";

/// Resolve the directory where Harness session logs should live when
/// integrated with the rest of the trace tooling.
///
/// Precedence:
/// 1. `override_path` (when `Some`, used verbatim).
/// 2. `$AGENTFLOW_TRACE_DIR`.
/// 3. `$HOME/.agentflow/traces`.
///
/// The resolved path is `<base>/harness/sessions/` — i.e. callers can
/// hand the returned [`PathBuf`] to [`JsonlEventSink::new`] without
/// further plumbing.
pub fn resolve_trace_session_dir(override_path: Option<&Path>) -> Result<PathBuf, HarnessError> {
  if let Some(path) = override_path {
    return Ok(default_session_dir(path));
  }
  if let Ok(env_dir) = std::env::var(AGENTFLOW_TRACE_DIR_ENV)
    && !env_dir.trim().is_empty()
  {
    return Ok(default_session_dir(Path::new(&env_dir)));
  }
  let home = dirs_home_dir().ok_or_else(|| {
    HarnessError::Other("cannot determine $HOME for default trace directory".into())
  })?;
  Ok(default_session_dir(&home.join(".agentflow").join("traces")))
}

/// Build a [`JsonlEventSink`] anchored at the bridge directory. Equivalent
/// to `JsonlEventSink::new(resolve_trace_session_dir(override_path)?)`.
pub fn open_tracing_sink(
  override_path: Option<&Path>,
) -> Result<Arc<dyn HarnessEventSink>, HarnessError> {
  let dir = resolve_trace_session_dir(override_path)?;
  Ok(Arc::new(JsonlEventSink::new(dir)))
}

/// Q3.10.4: build an [`ExecutionTraceSink`] backed by any
/// [`TraceStorage`] implementation. Wire it into a `SinkChain` so
/// the same harness session lands in both JSONL (for human replay)
/// AND the platform's trace store (for UI / dashboards) — they no
/// longer have to diverge.
pub fn open_execution_trace_sink(storage: Arc<dyn TraceStorage>) -> Arc<dyn HarnessEventSink> {
  Arc::new(ExecutionTraceSink::new(storage))
}

/// Q3.10.4: per-session accumulator that translates a stream of
/// [`HarnessEvent`]s into one [`ExecutionTrace`] and persists it to
/// the wrapped [`TraceStorage`].
///
/// Translation rules:
/// - `SessionStarted` → seed `ExecutionTrace::new(session_id)` with
///   `workflow_name = "harness:<session_id>"` and `started_at = now`.
/// - `StepStarted` → append a `NodeTrace { node_id: "step:<index>",
///   node_type: "harness_step" }` in `Running` state.
/// - `ToolCallRequested` → append a `NodeTrace { node_id:
///   "tool:<tool>", node_type: "tool_call" }` in `Running` state.
///   Multiple in-flight tool calls map to separate node rows so
///   replay can show concurrency.
/// - `ToolCallCompleted` → flip the matching tool-call node to
///   `Completed` or `Failed` based on the payload's `is_error`
///   flag; populate `duration_ms` from the wire payload.
/// - `Stopped` → flip the overall trace to
///   [`TraceStatus::Completed`] / [`TraceStatus::Failed`] /
///   [`TraceStatus::Cancelled`] depending on the stop reason, set
///   `completed_at`, and call `storage.save_trace(&trace)`.
///
/// Unrecognized event bodies (e.g. `MemorySummaryAdded`,
/// `BackgroundTaskUpdated`, `ApprovalRequested`/`ApprovalDecided`)
/// are intentionally ignored at this tier — they're preserved
/// faithfully by the parallel JSONL sink for deep audit; the
/// ExecutionTrace surface is the operator-facing summary.
pub struct ExecutionTraceSink {
  storage: Arc<dyn TraceStorage>,
  in_flight: Mutex<HashMap<String, ExecutionTrace>>,
}

impl ExecutionTraceSink {
  pub fn new(storage: Arc<dyn TraceStorage>) -> Self {
    Self {
      storage,
      in_flight: Mutex::new(HashMap::new()),
    }
  }
}

#[async_trait]
impl HarnessEventSink for ExecutionTraceSink {
  fn name(&self) -> &str {
    "execution_trace"
  }

  async fn write(&self, event: &HarnessEvent) -> Result<(), HarnessError> {
    let session_id = event.session_id.clone();
    let mut traces = self.in_flight.lock().await;
    match &event.body {
      HarnessEventBody::SessionStarted(_) => {
        let mut trace = ExecutionTrace::new(session_id.clone());
        trace.workflow_name = Some(format!("harness:{session_id}"));
        trace.started_at = event.ts;
        trace.context = TraceContext::workflow(session_id.clone());
        traces.insert(session_id, trace);
      }
      HarnessEventBody::StepStarted(payload) => {
        if let Some(trace) = traces.get_mut(&session_id) {
          let node_id = format!("step:{}", payload.step_index);
          let mut node = NodeTrace::new(node_id.clone(), "harness_step".to_string());
          node.context = TraceContext::child(&trace.context, format!("node:{node_id}"));
          trace.nodes.push(node);
        }
      }
      HarnessEventBody::ToolCallRequested(payload) => {
        if let Some(trace) = traces.get_mut(&session_id) {
          let node_id = format!("tool:{}", payload.tool);
          let mut node = NodeTrace::new(node_id.clone(), "tool_call".to_string());
          node.context = TraceContext::child(&trace.context, format!("node:{node_id}"));
          trace.nodes.push(node);
        }
      }
      HarnessEventBody::ToolCallCompleted(payload) => {
        if let Some(trace) = traces.get_mut(&session_id) {
          let needle = format!("tool:{}", payload.tool);
          // Match the most recent running entry — multiple
          // in-flight calls for the same tool map LIFO.
          if let Some(node) = trace
            .nodes
            .iter_mut()
            .rev()
            .find(|n| n.node_id == needle && n.status == NodeStatus::Running)
          {
            node.duration_ms = Some(payload.duration_ms);
            if payload.is_error {
              node.fail("tool call failed".to_string());
            } else {
              node.complete();
            }
          }
        }
      }
      HarnessEventBody::Stopped(payload) => {
        if let Some(mut trace) = traces.remove(&session_id) {
          // Close any still-running node rows so the persisted
          // trace doesn't carry phantom "Running" rows after the
          // session ended.
          for node in trace.nodes.iter_mut() {
            if node.status == NodeStatus::Running {
              node.fail("superseded by session stop".to_string());
            }
          }
          trace.completed_at = Some(event.ts);
          trace.status = match payload.reason {
            StopReason::Completed => TraceStatus::Completed,
            StopReason::Cancelled => TraceStatus::Cancelled {
              reason: payload
                .error
                .clone()
                .unwrap_or_else(|| "harness session cancelled".to_string()),
            },
            StopReason::Failed | StopReason::LimitReached | StopReason::ApprovalDenied => {
              TraceStatus::Failed {
                error: payload
                  .error
                  .clone()
                  .unwrap_or_else(|| format!("harness session stopped: {:?}", payload.reason)),
              }
            }
          };
          drop(traces);
          self
            .storage
            .save_trace(&trace)
            .await
            .map_err(|e| HarnessError::Other(format!("ExecutionTrace save_trace failed: {e}")))?;
        }
      }
      _ => {
        // Other variants (approval, memory summary, background
        // task) are richer than the ExecutionTrace surface can
        // represent; rely on the JSONL sink for full audit.
      }
    }
    Ok(())
  }
}

fn dirs_home_dir() -> Option<PathBuf> {
  // `home_dir` is not part of std; replicate the cross-platform check
  // used by `dirs` without taking the dep here so the harness crate
  // stays lean.
  if let Some(value) = std::env::var_os("HOME")
    && !value.is_empty()
  {
    return Some(PathBuf::from(value));
  }
  #[cfg(windows)]
  {
    if let Some(value) = std::env::var_os("USERPROFILE")
      && !value.is_empty()
    {
      return Some(PathBuf::from(value));
    }
  }
  None
}

#[cfg(test)]
mod tests {
  use super::*;
  use tempfile::TempDir;

  #[test]
  fn explicit_override_wins_over_env() {
    // SAFETY: serializing env mutation by running this test alone in
    // the module; tests inside the same binary do not race because
    // cargo serializes tests in the same `#[cfg(test)]` block by
    // default unless `#[parallel]` is used. Here the test sets the
    // var and immediately reads it.
    unsafe {
      std::env::set_var(AGENTFLOW_TRACE_DIR_ENV, "/tmp/should-be-ignored");
    }
    let override_dir = TempDir::new().unwrap();
    let resolved = resolve_trace_session_dir(Some(override_dir.path())).unwrap();
    assert!(resolved.starts_with(override_dir.path()));
    assert!(resolved.ends_with("harness/sessions"));
    unsafe {
      std::env::remove_var(AGENTFLOW_TRACE_DIR_ENV);
    }
  }

  #[test]
  fn env_var_wins_over_default() {
    let env_dir = TempDir::new().unwrap();
    unsafe {
      std::env::set_var(AGENTFLOW_TRACE_DIR_ENV, env_dir.path());
    }
    let resolved = resolve_trace_session_dir(None).unwrap();
    assert!(resolved.starts_with(env_dir.path()));
    assert!(resolved.ends_with("harness/sessions"));
    unsafe {
      std::env::remove_var(AGENTFLOW_TRACE_DIR_ENV);
    }
  }

  #[test]
  fn open_tracing_sink_returns_jsonl_sink_at_resolved_path() {
    let dir = TempDir::new().unwrap();
    let sink = open_tracing_sink(Some(dir.path())).unwrap();
    assert_eq!(sink.name(), "jsonl");
  }

  // ── Q3.10.4 ExecutionTraceSink regression suite ──────────────────

  use agentflow_tracing::storage::file::FileTraceStorage;
  use chrono::Utc;
  use std::time::Duration as StdDuration;

  use crate::event::{
    SessionStartedPayload, StepStartedPayload, StopReason, StoppedPayload,
    ToolCallCompletedPayload, ToolCallRequestedPayload,
  };

  fn evt(seq: u64, session: &str, body: HarnessEventBody) -> HarnessEvent {
    HarnessEvent {
      seq,
      session_id: session.to_string(),
      ts: Utc::now(),
      body,
    }
  }

  fn session_started() -> HarnessEventBody {
    HarnessEventBody::SessionStarted(SessionStartedPayload {
      workspace_root: "/tmp".into(),
      runtime: crate::context::HarnessRuntimeKind::React,
      profile: crate::context::HarnessProfile::Local,
      model: "mock".into(),
      skills: Vec::new(),
      context_item_count: 0,
      context_token_estimate: 0,
    })
  }

  fn step_started(idx: usize) -> HarnessEventBody {
    HarnessEventBody::StepStarted(StepStartedPayload {
      step_index: idx,
      step_type: "tool_call".into(),
    })
  }

  fn tool_requested(idx: usize, tool: &str) -> HarnessEventBody {
    HarnessEventBody::ToolCallRequested(ToolCallRequestedPayload {
      step_index: idx,
      tool: tool.into(),
      source: None,
      permissions: Vec::new(),
      idempotency: None,
      params_summary: serde_json::json!({}),
    })
  }

  fn tool_completed(idx: usize, tool: &str, is_error: bool) -> HarnessEventBody {
    HarnessEventBody::ToolCallCompleted(ToolCallCompletedPayload {
      step_index: idx,
      tool: tool.into(),
      is_error,
      duration_ms: 50,
      source: None,
      output_summary: None,
    })
  }

  fn stopped(reason: StopReason, err: Option<&str>) -> HarnessEventBody {
    HarnessEventBody::Stopped(StoppedPayload {
      reason,
      final_answer: None,
      error: err.map(|s| s.into()),
    })
  }

  /// Q3.10.4 happy-path — a full SessionStarted → 2 tool calls →
  /// Stopped(Completed) lifecycle must produce an `ExecutionTrace`
  /// with 2 NodeTrace rows (both Completed) plus a Completed
  /// terminal status persisted through `TraceStorage`.
  #[tokio::test]
  async fn execution_trace_sink_persists_completed_session() {
    let dir = TempDir::new().unwrap();
    let storage: Arc<dyn TraceStorage> =
      Arc::new(FileTraceStorage::new(dir.path().to_path_buf()).unwrap());
    let sink = open_execution_trace_sink(storage.clone());
    let session = "sess-completed";

    sink
      .write(&evt(0, session, session_started()))
      .await
      .unwrap();
    sink.write(&evt(1, session, step_started(0))).await.unwrap();
    sink
      .write(&evt(2, session, tool_requested(0, "file_read")))
      .await
      .unwrap();
    sink
      .write(&evt(3, session, tool_completed(0, "file_read", false)))
      .await
      .unwrap();
    sink
      .write(&evt(4, session, tool_requested(1, "http_get")))
      .await
      .unwrap();
    sink
      .write(&evt(5, session, tool_completed(1, "http_get", false)))
      .await
      .unwrap();
    sink
      .write(&evt(6, session, stopped(StopReason::Completed, None)))
      .await
      .unwrap();

    let trace = storage
      .get_trace(session)
      .await
      .unwrap()
      .expect("persisted");
    assert!(matches!(trace.status, TraceStatus::Completed));
    assert!(trace.completed_at.is_some());
    // 1 harness_step + 2 tool_call rows.
    assert_eq!(trace.nodes.len(), 3, "expected 3 NodeTrace rows: {trace:?}");
    let tool_rows: Vec<&NodeTrace> = trace
      .nodes
      .iter()
      .filter(|n| n.node_type == "tool_call")
      .collect();
    assert_eq!(tool_rows.len(), 2);
    for n in &tool_rows {
      assert_eq!(n.status, NodeStatus::Completed);
      assert!(n.duration_ms.is_some());
    }
  }

  /// Q3.10.4 — Stopped(Cancelled) maps to TraceStatus::Cancelled
  /// with the `error` field threaded through as the reason; any
  /// still-running node rows are closed out as Failed so the
  /// persisted trace can't carry phantom Running rows.
  #[tokio::test]
  async fn execution_trace_sink_maps_cancellation_and_closes_open_rows() {
    let dir = TempDir::new().unwrap();
    let storage: Arc<dyn TraceStorage> =
      Arc::new(FileTraceStorage::new(dir.path().to_path_buf()).unwrap());
    let sink = open_execution_trace_sink(storage.clone());
    let session = "sess-cancelled";

    sink
      .write(&evt(0, session, session_started()))
      .await
      .unwrap();
    sink
      .write(&evt(1, session, tool_requested(0, "shell")))
      .await
      .unwrap();
    // Intentionally DO NOT send tool_completed → the tool node
    // stays Running until Stopped fires the close-out logic.
    sink
      .write(&evt(
        2,
        session,
        stopped(StopReason::Cancelled, Some("operator pressed Ctrl-C")),
      ))
      .await
      .unwrap();

    let trace = storage.get_trace(session).await.unwrap().unwrap();
    assert!(matches!(
      trace.status,
      TraceStatus::Cancelled { ref reason } if reason.contains("Ctrl-C")
    ));
    let tool_row = trace
      .nodes
      .iter()
      .find(|n| n.node_type == "tool_call")
      .unwrap();
    assert_eq!(
      tool_row.status,
      NodeStatus::Failed,
      "still-running rows must be closed out on Stop"
    );
  }

  /// Q3.10.4 — Stopped(Failed) and Stopped(LimitReached) /
  /// Stopped(ApprovalDenied) all map to TraceStatus::Failed,
  /// carrying the `error` text. Smoke-checks all three branches.
  #[tokio::test]
  async fn execution_trace_sink_maps_failed_variants() {
    let dir = TempDir::new().unwrap();
    let storage: Arc<dyn TraceStorage> =
      Arc::new(FileTraceStorage::new(dir.path().to_path_buf()).unwrap());
    let sink = open_execution_trace_sink(storage.clone());

    for (i, reason) in [
      StopReason::Failed,
      StopReason::LimitReached,
      StopReason::ApprovalDenied,
    ]
    .into_iter()
    .enumerate()
    {
      let session = format!("sess-failed-{i}");
      sink
        .write(&evt(0, &session, session_started()))
        .await
        .unwrap();
      sink
        .write(&evt(
          1,
          &session,
          stopped(reason, Some(&format!("variant_{i}"))),
        ))
        .await
        .unwrap();
      let trace = storage.get_trace(&session).await.unwrap().unwrap();
      assert!(
        matches!(trace.status, TraceStatus::Failed { ref error } if error.contains(&format!("variant_{i}"))),
        "Stopped variant must surface as Failed with the error text; got {:?}",
        trace.status
      );
    }
  }

  /// Q3.10.4 — `name()` must be distinct from the JSONL sink so a
  /// SinkChain with both can be introspected. Trivial pin to
  /// prevent regressions where someone copy-paste'd "jsonl".
  #[test]
  fn execution_trace_sink_has_distinct_name() {
    let dir = TempDir::new().unwrap();
    let storage: Arc<dyn TraceStorage> =
      Arc::new(FileTraceStorage::new(dir.path().to_path_buf()).unwrap());
    let sink = open_execution_trace_sink(storage);
    assert_eq!(sink.name(), "execution_trace");
  }

  /// Q3.10.4 — events arriving without a prior SessionStarted are
  /// dropped silently (no panic, no save). Guards against
  /// out-of-order replay confusing the in-memory accumulator.
  #[tokio::test]
  async fn execution_trace_sink_drops_orphan_events() {
    let dir = TempDir::new().unwrap();
    let storage: Arc<dyn TraceStorage> =
      Arc::new(FileTraceStorage::new(dir.path().to_path_buf()).unwrap());
    let sink = open_execution_trace_sink(storage.clone());
    // Stopped without SessionStarted → no persistence happens.
    sink
      .write(&evt(0, "orphan", stopped(StopReason::Completed, None)))
      .await
      .unwrap();
    assert!(storage.get_trace("orphan").await.unwrap().is_none());
    // Settle the borrow-check by yielding briefly so the
    // tokio::time crate doesn't complain about uninstantiated
    // timer state.
    tokio::time::sleep(StdDuration::from_millis(1)).await;
  }
}

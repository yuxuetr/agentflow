//! P-A2.2 — the harness governs a deterministic `Flow` run.
//!
//! [`HarnessRuntime::run_flow`] brackets an `agentflow-graph::Flow` execution
//! with the Harness envelope (`session_started` → … → `stopped`) and runs it via
//! an injected [`FlowRunner`] (so the executor stays out of the harness — the
//! same contract `agentflow-agents` uses for dynamic workflows). It is the
//! orthogonal-governance counterpart to [`HarnessRuntime::run`], which governs
//! an agent loop.
//!
//! **Where governance comes from.** The harness governs tool calls at the
//! registry seam: tools inside the Flow's nodes are decorated by
//! [`wrap_registry`](crate::hooks_runtime::wrap_registry) (pre/post hooks +
//! approval). When the caller wires that `HookConfig` with the harness's shared
//! [`seq_counter`](HarnessRuntime::seq_counter) and event sinks, every
//! `tool_call_requested` / `approval_requested` / `tool_call_completed` a node
//! emits interleaves on the same monotonic event stream between the
//! `session_started` and `stopped` this method emits. The harness therefore
//! governs the Flow's tool calls without owning the executor or the node loop.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use agentflow_graph::events::{EventListener, WorkflowEvent};
use agentflow_graph::{AsyncNodeInputs, AsyncNodeResult, Flow, FlowRunner};
use chrono::Utc;

use crate::context::{HarnessProfile, HarnessRuntimeKind};
use crate::error::HarnessError;
use crate::event::{
  HarnessEvent, HarnessEventBody, SessionStartedPayload, StepStartedPayload, StopReason,
  StoppedPayload,
};
use crate::runtime::HarnessRuntime;

/// `EventListener` that forwards each node's start (by id) onto a channel so
/// the harness can emit a `step_started` event for it. `on_event` is sync, so
/// it uses a non-blocking unbounded send; the harness drains the receiver.
struct HarnessFlowListener {
  tx: tokio::sync::mpsc::UnboundedSender<String>,
}

impl EventListener for HarnessFlowListener {
  fn on_event(&self, event: &WorkflowEvent) {
    if let WorkflowEvent::NodeStarted { node_id, .. } = event {
      // Best-effort: a closed receiver (run already finished) just drops it.
      let _ = self.tx.send(node_id.clone());
    }
  }
}

/// Sleep until `deadline`, or never (for the no-timeout case) so the `select!`
/// timeout arm is inert when `options.timeout` is `None`.
async fn sleep_until_opt(deadline: Option<tokio::time::Instant>) {
  match deadline {
    Some(d) => tokio::time::sleep_until(d).await,
    None => std::future::pending::<()>().await,
  }
}

/// Options for a single [`HarnessRuntime::run_flow`] invocation.
#[derive(Debug, Clone)]
pub struct HarnessFlowRunOptions {
  /// Workspace root recorded in the `session_started` event.
  pub workspace_root: PathBuf,
  /// Security profile recorded in the `session_started` event. The actual
  /// approval escalation is applied by the wrapped registry's `HookConfig`.
  pub profile: HarnessProfile,
  /// Resume / correlate an existing session; defaults to a fresh UUID.
  pub session_id: Option<String>,
  /// Wall-clock cap on the whole Flow run. `None` imposes no harness timeout
  /// (the Flow's own per-node timeouts still apply).
  pub timeout: Option<Duration>,
  /// Free-form metadata; reserved for parity with the agent path.
  pub metadata: serde_json::Value,
}

impl Default for HarnessFlowRunOptions {
  fn default() -> Self {
    Self {
      workspace_root: PathBuf::from("."),
      profile: HarnessProfile::default(),
      session_id: None,
      timeout: None,
      metadata: serde_json::Value::Null,
    }
  }
}

/// Classify a successfully-returned state pool. The executor reports a
/// *node-level* failure as an `Err` entry in the state map (the outer run still
/// returns `Ok`), so a run is only `Completed` when every node succeeded;
/// otherwise the first failed node's error becomes the `Failed` reason. A
/// denied tool call (harness governance) lands here as a failed node.
fn classify_state(state: HashMap<String, AsyncNodeResult>) -> FlowRunOutcome {
  if let Some(err) = state.values().find_map(|r| r.as_ref().err()) {
    FlowRunOutcome::Failed(err.to_string())
  } else {
    FlowRunOutcome::Completed(state)
  }
}

/// Terminal outcome of a governed Flow run.
#[derive(Debug)]
pub enum FlowRunOutcome {
  /// The Flow ran to completion; carries the final node-output state pool.
  Completed(HashMap<String, AsyncNodeResult>),
  /// A node returned an error (or the run otherwise failed).
  Failed(String),
  /// The run exceeded the harness-imposed `timeout`.
  TimedOut,
}

/// Result of [`HarnessRuntime::run_flow`].
#[derive(Debug)]
pub struct HarnessFlowRunResult {
  /// Resolved session id (matches the emitted events).
  pub session_id: String,
  /// Seq of the terminal `stopped` event — the last seq this run wrote.
  pub final_event_seq: u64,
  /// What happened.
  pub outcome: FlowRunOutcome,
}

impl HarnessRuntime {
  /// Emit a `step_started` event for one Flow node, taking the next seq from
  /// the shared counter. `step_type` carries the node id (`node:<id>`) so the
  /// frozen payload still identifies which node began.
  async fn emit_step_started(
    &self,
    session_id: &str,
    step_index: usize,
    node_id: &str,
  ) -> Result<(), HarnessError> {
    let seq = self.seq_counter.fetch_add(1, Ordering::SeqCst);
    let event = HarnessEvent {
      seq,
      session_id: session_id.to_string(),
      ts: Utc::now(),
      body: HarnessEventBody::StepStarted(StepStartedPayload {
        step_index,
        step_type: format!("node:{node_id}"),
      }),
    };
    self.sinks.dispatch(&event).await
  }

  /// Govern a deterministic [`Flow`] run (P-A2.2).
  ///
  /// Emits `session_started` (runtime = [`HarnessRuntimeKind::Flow`]), executes
  /// the flow via `runner` (raced against `options.timeout` when set), then
  /// emits `stopped` classifying the outcome. Tool-call / approval events from
  /// the Flow's nodes interleave between the two when the node registry was
  /// wrapped with a `HookConfig` sharing this runtime's seq counter + sinks.
  ///
  /// Returns the resolved session id, the final event seq, and the outcome
  /// (including the Flow's state pool on success). Sink dispatch failures abort
  /// with [`HarnessError`].
  pub async fn run_flow(
    &mut self,
    flow: &Flow,
    runner: Arc<dyn FlowRunner>,
    inputs: AsyncNodeInputs,
    options: HarnessFlowRunOptions,
  ) -> Result<HarnessFlowRunResult, HarnessError> {
    let session_id = options
      .session_id
      .clone()
      .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // ── session_started ────────────────────────────────────────────────────
    let started_seq = self.seq_counter.fetch_add(1, Ordering::SeqCst);
    let started = HarnessEvent {
      seq: started_seq,
      session_id: session_id.clone(),
      ts: Utc::now(),
      body: HarnessEventBody::SessionStarted(SessionStartedPayload {
        workspace_root: options.workspace_root.to_string_lossy().into_owned(),
        runtime: HarnessRuntimeKind::Flow,
        profile: options.profile,
        model: String::new(),
        skills: Vec::new(),
        context_item_count: 0,
        context_token_estimate: 0,
      }),
    };
    self.sinks.dispatch(&started).await?;

    // ── execute (governance happens inside via the wrapped registry) ────────
    //
    // Instrument the flow with a listener that forwards each node's start onto
    // a channel; drain it concurrently with the run so `step_started` events
    // interleave in real time with the tool-call / approval events the wrapped
    // registry emits between `session_started` and `stopped`. Replaces any
    // listener the caller had attached (the harness owns the slot here).
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let instrumented = flow.clone().with_event_listener(Arc::new(HarnessFlowListener { tx }));

    let mut step_index: usize = 0;
    let deadline = options.timeout.map(|t| tokio::time::Instant::now() + t);
    let run_fut = runner.run(&instrumented, inputs);
    tokio::pin!(run_fut);

    let run_result: Option<Result<HashMap<String, AsyncNodeResult>, _>> = loop {
      tokio::select! {
        // Prefer draining node-start events so their seq precedes the run's
        // terminal handling.
        biased;
        Some(node_id) = rx.recv() => {
          self.emit_step_started(&session_id, step_index, &node_id).await?;
          step_index += 1;
        }
        res = &mut run_fut => {
          // The run finished; drain any node-starts still queued, then stop.
          while let Ok(node_id) = rx.try_recv() {
            self.emit_step_started(&session_id, step_index, &node_id).await?;
            step_index += 1;
          }
          break Some(res);
        }
        _ = sleep_until_opt(deadline) => break None,
      }
    };

    let outcome = match run_result {
      Some(Ok(state)) => classify_state(state),
      Some(Err(err)) => FlowRunOutcome::Failed(err.to_string()),
      None => FlowRunOutcome::TimedOut,
    };

    // ── stopped ─────────────────────────────────────────────────────────────
    let stopped_payload = match &outcome {
      FlowRunOutcome::Completed(_) => StoppedPayload {
        reason: StopReason::Completed,
        final_answer: None,
        error: None,
      },
      FlowRunOutcome::Failed(err) => StoppedPayload {
        reason: StopReason::Failed,
        final_answer: None,
        error: Some(err.clone()),
      },
      FlowRunOutcome::TimedOut => StoppedPayload {
        reason: StopReason::LimitReached,
        final_answer: None,
        error: Some("flow run exceeded the harness timeout".to_string()),
      },
    };
    let stopped_seq = self.seq_counter.fetch_add(1, Ordering::SeqCst);
    let stopped = HarnessEvent {
      seq: stopped_seq,
      session_id: session_id.clone(),
      ts: Utc::now(),
      body: HarnessEventBody::Stopped(stopped_payload),
    };
    self.sinks.dispatch(&stopped).await?;

    Ok(HarnessFlowRunResult {
      session_id,
      final_event_seq: stopped_seq,
      outcome,
    })
  }
}

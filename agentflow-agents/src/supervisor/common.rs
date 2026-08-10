//! Shared helpers for the multi-agent supervisors (`handoff` / `blackboard`
//! / `debate`).

use std::sync::Arc;

use tokio::sync::Mutex as AsyncMutex;

use crate::runtime::{AgentContext, AgentRuntime};

/// A supervisor participant, shared across concurrently-dispatched tasks
/// (e.g. `BlackboardSchedule::Parallel`, debate rounds) and type-erased
/// (W3.2) so any [`AgentRuntime`] — not just `ReActAgent` — can be a
/// `HandoffSupervisor`/`BlackboardSupervisor`/`DebateSupervisor`
/// participant or judge.
pub(crate) type SharedAgentRuntime = Arc<AsyncMutex<Box<dyn AgentRuntime>>>;

/// Build a child agent's `AgentContext` for one delegation step, forwarding
/// everything from `parent` a sub-agent needs to stay governed and
/// observable the same way the top-level run is (W3.1):
///
/// - `limits` / `cancellation_token` were already forwarded pre-W3.1.
/// - `event_sink` / `between_turn_hook` / `loop_checkpointer` /
///   `trace_context` were dropped — a supervisor delegating to a child
///   silently orphaned the parent's live event stream (Harness couldn't see
///   sub-agent steps), loop checkpointing (no multi-agent checkpoint
///   coverage), and OTel trace propagation (the trace broke at the
///   delegation boundary) even though every one of these fields is a cheap
///   `Option<Arc<dyn Trait>>`-shaped handle to clone.
///
/// All three supervisors (`HandoffSupervisor`, `BlackboardSupervisor`,
/// `DebateSupervisor`) call this instead of duplicating the same
/// hand-rolled construction, so a future new field on `AgentContext` only
/// needs to be forwarded here once.
pub(crate) fn build_child_context(
  parent: &AgentContext,
  agent_name: &str,
  input: &str,
) -> AgentContext {
  let session = format!("{}::{}", parent.session_id, agent_name);
  let mut ctx =
    AgentContext::new(session, input, parent.model.clone()).with_limits(parent.limits.clone());
  if let Some(token) = parent.cancellation_token.clone() {
    ctx = ctx.with_cancellation_token(token);
  }
  ctx.metadata = parent.metadata.clone();
  ctx.event_sink = parent.event_sink.clone();
  ctx.between_turn_hook = parent.between_turn_hook.clone();
  ctx.loop_checkpointer = parent.loop_checkpointer.clone();
  ctx.trace_context = parent.trace_context.clone();
  ctx
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::runtime::{AgentEvent, AgentEventSink, BetweenTurnHook};
  use agentflow_agent_spi::checkpoint::{
    AgentLoopCheckpoint, AgentLoopCheckpointError, AgentLoopCheckpointer,
  };
  use agentflow_memory::MemoryStore;
  use async_trait::async_trait;
  use std::sync::Arc;

  struct NoopSink;
  #[async_trait]
  impl AgentEventSink for NoopSink {
    async fn emit(&self, _event: &AgentEvent) {}
  }

  struct NoopHook;
  #[async_trait]
  impl BetweenTurnHook for NoopHook {
    async fn before_turn(&self, _turn_index: usize, _session_id: &str, _memory: &dyn MemoryStore) {}
  }

  struct NoopCheckpointer;
  #[async_trait]
  impl AgentLoopCheckpointer for NoopCheckpointer {
    async fn save(
      &self,
      _checkpoint: &AgentLoopCheckpoint,
    ) -> Result<(), AgentLoopCheckpointError> {
      Ok(())
    }
    async fn load(
      &self,
      _session_id: &str,
    ) -> Result<Option<AgentLoopCheckpoint>, AgentLoopCheckpointError> {
      Ok(None)
    }
    async fn clear(&self, _session_id: &str) -> Result<(), AgentLoopCheckpointError> {
      Ok(())
    }
  }

  /// W3.1 regression: pre-fix, `build_child_context` only forwarded
  /// `limits`/`cancellation_token`/`metadata` — `event_sink`,
  /// `between_turn_hook`, `loop_checkpointer`, and `trace_context` were
  /// silently dropped, so a supervisor's sub-agent ran with none of the
  /// parent's observability/checkpointing wiring.
  #[test]
  fn forwards_event_sink_hook_checkpointer_and_trace_context() {
    let trace = agentflow_value::LlmTraceContext::new("1".repeat(32), "1".repeat(16))
      .expect("valid trace context");
    let parent = AgentContext::new("sess", "goal", "gpt-4o")
      .with_event_sink(Arc::new(NoopSink))
      .with_between_turn_hook(Arc::new(NoopHook))
      .with_loop_checkpointer(Arc::new(NoopCheckpointer))
      .with_trace_context(trace);

    let child = build_child_context(&parent, "sub_agent", "sub-input");

    assert_eq!(
      child.event_sink, parent.event_sink,
      "event_sink must be the same shared handle as the parent's"
    );
    assert_eq!(
      child.between_turn_hook, parent.between_turn_hook,
      "between_turn_hook must be the same shared handle as the parent's"
    );
    assert_eq!(
      child.loop_checkpointer, parent.loop_checkpointer,
      "loop_checkpointer must be the same shared handle as the parent's"
    );
    assert_eq!(
      child.trace_context, parent.trace_context,
      "trace_context must propagate to the child"
    );
  }

  #[test]
  fn leaves_governance_fields_none_when_parent_has_none() {
    let parent = AgentContext::new("sess", "goal", "gpt-4o");
    let child = build_child_context(&parent, "sub_agent", "sub-input");
    assert!(child.event_sink.is_none());
    assert!(child.between_turn_hook.is_none());
    assert!(child.loop_checkpointer.is_none());
    assert!(child.trace_context.is_none());
  }
}

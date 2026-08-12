use tracing::warn;

use crate::runtime::{AgentStep, AgentStopReason};

use super::config::ReActError;
use super::core::ReActAgent;
use super::turn_driven::LoopState;

impl ReActAgent {
  /// V2.4: save an [`agentflow_agent_spi::checkpoint::AgentLoopCheckpoint`]
  /// for the current loop state, if a checkpointer is configured. A save
  /// failure is logged and swallowed — observability/durability must
  /// never abort an otherwise-successful turn (mirrors the DAG-level
  /// checkpoint's non-fatal posture).
  pub(crate) async fn save_loop_checkpoint(&self, st: &LoopState) {
    let Some(checkpointer) = self.live_checkpointer.as_ref() else {
      return;
    };
    let checkpoint = st.to_checkpoint(&self.session_id, None);
    if let Err(e) = checkpointer.0.save(&checkpoint).await {
      warn!(session = %self.session_id, error = %e, "agent loop checkpoint save failed");
    }
  }

  /// V2.4: clear the loop checkpoint (if any) when `reason` represents a
  /// genuine completion — see [`crate::checkpoint::should_clear_checkpoint`].
  pub(crate) async fn clear_loop_checkpoint_if_terminal(&self, reason: &AgentStopReason) {
    let Some(checkpointer) = self.live_checkpointer.as_ref() else {
      return;
    };
    if crate::checkpoint::should_clear_checkpoint(reason)
      && let Err(e) = checkpointer.0.clear(&self.session_id).await
    {
      warn!(session = %self.session_id, error = %e, "agent loop checkpoint clear failed");
    }
  }

  /// L3.1: extract + persist project facts from a completed run's steps.
  /// No-op if `with_project_memory` wasn't configured, or the generator
  /// found nothing worth recording.
  pub(crate) async fn record_project_facts(&self, steps: &[AgentStep]) -> Result<(), ReActError> {
    let (Some(store), Some(project_key)) = (&self.project_memory_store, &self.project_key) else {
      return Ok(());
    };
    let candidates = self.project_fact_generator.extract(steps).await;
    for candidate in candidates {
      store
        .record_project_fact(project_key, &candidate.tool, &candidate.command)
        .await?;
    }
    Ok(())
  }
}

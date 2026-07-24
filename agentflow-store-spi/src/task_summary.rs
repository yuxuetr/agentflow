//! Agent-loop task summary checkpoint (L2.1).
//!
//! Flow-level checkpoint (`agentflow-graph`/`agentflow-core`) recovers the
//! *state pool*; nothing recovers the agent-loop's *task narrative* once
//! older messages fall out of the prompt via compaction
//! (`MemorySummaryBackend`, `agentflow-agents`). [`TaskSummary`] is that
//! narrative: a small, structured record of what a long-horizon run has
//! established so far, updated whenever the conversation is compacted and
//! surfaced again whenever the run continues (fresh process, resumed
//! process, or a new run reusing the same session id).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::MemoryError;

/// A structured checkpoint of a long-horizon task's progress, distinct from
/// (and coarser than) the raw message history: what the run is trying to
/// do, what it has already established, and where it left off.
///
/// Every field is a plain list of short strings rather than free-form prose
/// so a generator can append to it deterministically across repeated
/// compaction events without needing to re-parse its own prior output.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TaskSummary {
  /// What the run is trying to accomplish — normally the original user
  /// input, fixed at first write and never overwritten by later updates.
  pub goal: String,
  /// Steps already completed, oldest first.
  pub completed_steps: Vec<String>,
  /// Short excerpts of key tool results established so far, oldest first.
  pub key_results: Vec<String>,
  /// Open questions the run hasn't resolved yet. A deterministic generator
  /// may leave this empty — inferring genuine open questions needs
  /// judgment a non-LLM extractor can't provide.
  pub open_questions: Vec<String>,
  /// The run's current plan for what to do next, if known.
  pub next_steps: Vec<String>,
  /// When this summary was last updated.
  pub updated_at: DateTime<Utc>,
}

/// Persistence contract for [`TaskSummary`], keyed by session id. Lives
/// alongside [`crate::MemoryStore`] rather than folded into it: task
/// summaries are a distinct concern (one structured checkpoint per
/// session) from the raw append-only message log, and not every
/// `MemoryStore` implementation needs to carry one (e.g. a caller that
/// never enables compaction has no summaries to persist).
#[async_trait]
pub trait TaskSummaryStore: Send + Sync {
  /// Fetch the current summary for `session_id`, if one has been written.
  async fn get_task_summary(&self, session_id: &str) -> Result<Option<TaskSummary>, MemoryError>;

  /// Overwrite the summary for `session_id`.
  async fn set_task_summary(
    &self,
    session_id: &str,
    summary: TaskSummary,
  ) -> Result<(), MemoryError>;

  /// Delete the summary for `session_id`, if any. Used by `harness chat`'s
  /// `/clear` when the operator wants a full reset rather than an
  /// in-place clear that preserves the summary.
  async fn clear_task_summary(&self, session_id: &str) -> Result<(), MemoryError>;
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_summary_is_empty() {
    let summary = TaskSummary::default();
    assert!(summary.goal.is_empty());
    assert!(summary.completed_steps.is_empty());
    assert!(summary.key_results.is_empty());
    assert!(summary.open_questions.is_empty());
    assert!(summary.next_steps.is_empty());
  }

  #[test]
  fn serde_round_trips() {
    let summary = TaskSummary {
      goal: "ship the feature".to_string(),
      completed_steps: vec!["read the spec".to_string()],
      key_results: vec!["found the bug in parser.rs:42".to_string()],
      open_questions: vec!["is the fix backwards compatible?".to_string()],
      next_steps: vec!["write a regression test".to_string()],
      updated_at: Utc::now(),
    };
    let json = serde_json::to_string(&summary).unwrap();
    let round_tripped: TaskSummary = serde_json::from_str(&json).unwrap();
    assert_eq!(summary, round_tripped);
  }
}

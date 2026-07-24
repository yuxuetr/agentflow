//! Deterministic [`TaskSummary`] generation (L2.1).
//!
//! Mirrors `agentflow-harness::ContextSummarizer`'s design: the default
//! implementation is LLM-free and deterministic so it's replay-safe and
//! adds no cost/latency to every compaction event, but generation is
//! pluggable — a caller that wants a richer, LLM-synthesized narrative
//! (genuine open questions, a real "what's next" plan) can supply its own
//! [`TaskSummaryGenerator`].
//!
//! What the deterministic generator can't do: infer semantic open
//! questions or a next-step plan from raw message content. It leaves
//! [`TaskSummary::open_questions`] and [`TaskSummary::next_steps`] as
//! carried-forward from the previous summary (unchanged) rather than
//! guessing — an honest gap, not a silently-wrong answer.

use async_trait::async_trait;
use chrono::Utc;

use agentflow_memory::{Message, Role, TaskSummary};

/// Input to a [`TaskSummaryGenerator`]: the task's goal, the previous
/// summary (if any — later calls fold into this one rather than starting
/// over), and the messages compaction is about to drop from the prompt.
pub struct TaskSummaryContext {
  pub goal: String,
  pub previous: Option<TaskSummary>,
  pub newly_omitted: Vec<Message>,
}

/// Produces (or updates) a [`TaskSummary`] from a compaction event.
#[async_trait]
pub trait TaskSummaryGenerator: Send + Sync {
  /// Stable identifier, e.g. for logging which generator produced a
  /// summary.
  fn name(&self) -> &str;

  /// `None` means "nothing worth recording this round" — the caller
  /// should leave the previously persisted summary as-is.
  async fn generate(&self, context: TaskSummaryContext) -> Option<TaskSummary>;
}

/// How many entries [`DeterministicTaskSummaryGenerator`] keeps in
/// `completed_steps`/`key_results` before dropping the oldest — the
/// summary itself must stay small, so it can't grow unboundedly across a
/// long run's repeated compaction events.
const MAX_ENTRIES_PER_LIST: usize = 20;
const MAX_ENTRY_CHARS: usize = 160;

/// LLM-free, deterministic [`TaskSummaryGenerator`]. Extracts
/// `completed_steps` from omitted `Assistant`-role messages and
/// `key_results` from omitted `Tool`-role messages; leaves
/// `open_questions`/`next_steps` as carried forward from the previous
/// summary (see module docs).
#[derive(Debug, Default, Clone)]
pub struct DeterministicTaskSummaryGenerator;

#[async_trait]
impl TaskSummaryGenerator for DeterministicTaskSummaryGenerator {
  fn name(&self) -> &str {
    "deterministic"
  }

  async fn generate(&self, context: TaskSummaryContext) -> Option<TaskSummary> {
    if context.newly_omitted.is_empty() {
      return None;
    }

    let mut completed_steps = context
      .previous
      .as_ref()
      .map(|p| p.completed_steps.clone())
      .unwrap_or_default();
    let mut key_results = context
      .previous
      .as_ref()
      .map(|p| p.key_results.clone())
      .unwrap_or_default();
    let open_questions = context
      .previous
      .as_ref()
      .map(|p| p.open_questions.clone())
      .unwrap_or_default();
    let next_steps = context
      .previous
      .as_ref()
      .map(|p| p.next_steps.clone())
      .unwrap_or_default();
    let goal = context
      .previous
      .as_ref()
      .map(|p| p.goal.clone())
      .filter(|g| !g.is_empty())
      .unwrap_or(context.goal);

    for message in &context.newly_omitted {
      match message.role {
        Role::Assistant => completed_steps.push(truncate(&message.content)),
        Role::Tool => {
          let tool = message.tool_name.as_deref().unwrap_or("tool");
          key_results.push(format!("{tool}: {}", truncate(&message.content)));
        }
        Role::User | Role::System => {}
      }
    }

    cap_front(&mut completed_steps, MAX_ENTRIES_PER_LIST);
    cap_front(&mut key_results, MAX_ENTRIES_PER_LIST);

    Some(TaskSummary {
      goal,
      completed_steps,
      key_results,
      open_questions,
      next_steps,
      updated_at: Utc::now(),
    })
  }
}

fn truncate(text: &str) -> String {
  let flattened = text.split_whitespace().collect::<Vec<_>>().join(" ");
  if flattened.chars().count() <= MAX_ENTRY_CHARS {
    flattened
  } else {
    let mut out: String = flattened.chars().take(MAX_ENTRY_CHARS).collect();
    out.push('\u{2026}');
    out
  }
}

fn cap_front(list: &mut Vec<String>, max_len: usize) {
  if list.len() > max_len {
    let drop = list.len() - max_len;
    list.drain(0..drop);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn tool_message(tool: &str, content: &str) -> Message {
    let mut m = Message::new("session", Role::Tool, content);
    m.tool_name = Some(tool.to_string());
    m
  }

  #[tokio::test]
  async fn empty_omitted_produces_no_summary() {
    let generator = DeterministicTaskSummaryGenerator;
    let result = generator
      .generate(TaskSummaryContext {
        goal: "goal".to_string(),
        previous: None,
        newly_omitted: vec![],
      })
      .await;
    assert!(result.is_none());
  }

  #[tokio::test]
  async fn extracts_completed_steps_and_key_results_from_first_round() {
    let generator = DeterministicTaskSummaryGenerator;
    let omitted = vec![
      Message::new(
        "session",
        Role::Assistant,
        "decided to read the config file",
      ),
      tool_message("file", "config contents: port=8080"),
    ];
    let summary = generator
      .generate(TaskSummaryContext {
        goal: "configure the server".to_string(),
        previous: None,
        newly_omitted: omitted,
      })
      .await
      .expect("summary");

    assert_eq!(summary.goal, "configure the server");
    assert_eq!(summary.completed_steps.len(), 1);
    assert!(summary.completed_steps[0].contains("read the config file"));
    assert_eq!(summary.key_results.len(), 1);
    assert!(summary.key_results[0].starts_with("file:"));
    assert!(summary.key_results[0].contains("port=8080"));
    assert!(summary.open_questions.is_empty());
  }

  /// The whole point: a second compaction round must fold onto the first
  /// summary's lists (accumulate), not discard them.
  #[tokio::test]
  async fn second_round_accumulates_onto_previous_summary() {
    let generator = DeterministicTaskSummaryGenerator;
    let first = generator
      .generate(TaskSummaryContext {
        goal: "goal".to_string(),
        previous: None,
        newly_omitted: vec![Message::new("session", Role::Assistant, "step one")],
      })
      .await
      .unwrap();

    let second = generator
      .generate(TaskSummaryContext {
        goal: "goal".to_string(),
        previous: Some(first),
        newly_omitted: vec![Message::new("session", Role::Assistant, "step two")],
      })
      .await
      .unwrap();

    assert_eq!(second.completed_steps.len(), 2);
    assert!(second.completed_steps[0].contains("step one"));
    assert!(second.completed_steps[1].contains("step two"));
  }

  #[tokio::test]
  async fn goal_is_fixed_at_first_write_not_overwritten() {
    let generator = DeterministicTaskSummaryGenerator;
    let first = generator
      .generate(TaskSummaryContext {
        goal: "original goal".to_string(),
        previous: None,
        newly_omitted: vec![Message::new("session", Role::Assistant, "step")],
      })
      .await
      .unwrap();

    let second = generator
      .generate(TaskSummaryContext {
        // A caller passing a different `goal` (e.g. re-derived from a
        // stale LoopState) must not clobber the fixed original.
        goal: "some other goal".to_string(),
        previous: Some(first),
        newly_omitted: vec![Message::new("session", Role::Assistant, "step 2")],
      })
      .await
      .unwrap();

    assert_eq!(second.goal, "original goal");
  }

  #[tokio::test]
  async fn open_questions_and_next_steps_carry_forward_unchanged() {
    let generator = DeterministicTaskSummaryGenerator;
    let previous = TaskSummary {
      goal: "goal".to_string(),
      completed_steps: vec![],
      key_results: vec![],
      open_questions: vec!["is X safe?".to_string()],
      next_steps: vec!["do Y".to_string()],
      updated_at: Utc::now(),
    };

    let updated = generator
      .generate(TaskSummaryContext {
        goal: "goal".to_string(),
        previous: Some(previous),
        newly_omitted: vec![Message::new("session", Role::Assistant, "did something")],
      })
      .await
      .unwrap();

    assert_eq!(updated.open_questions, vec!["is X safe?".to_string()]);
    assert_eq!(updated.next_steps, vec!["do Y".to_string()]);
  }

  #[tokio::test]
  async fn list_length_is_capped_dropping_oldest_first() {
    let generator = DeterministicTaskSummaryGenerator;
    let mut previous: Option<TaskSummary> = None;
    for i in 0..(MAX_ENTRIES_PER_LIST + 5) {
      previous = generator
        .generate(TaskSummaryContext {
          goal: "goal".to_string(),
          previous,
          newly_omitted: vec![Message::new(
            "session",
            Role::Assistant,
            format!("step {i}"),
          )],
        })
        .await;
    }
    let summary = previous.unwrap();
    assert_eq!(summary.completed_steps.len(), MAX_ENTRIES_PER_LIST);
    // The oldest entries (step 0..5) must have been dropped, newest kept.
    assert!(summary.completed_steps[0].contains("step 5"));
    assert!(
      summary.completed_steps[MAX_ENTRIES_PER_LIST - 1]
        .contains(&format!("step {}", MAX_ENTRIES_PER_LIST + 4))
    );
  }

  #[tokio::test]
  async fn long_content_is_truncated() {
    let generator = DeterministicTaskSummaryGenerator;
    let long_content = "x".repeat(500);
    let summary = generator
      .generate(TaskSummaryContext {
        goal: "goal".to_string(),
        previous: None,
        newly_omitted: vec![Message::new("session", Role::Assistant, long_content)],
      })
      .await
      .unwrap();
    assert!(summary.completed_steps[0].chars().count() <= MAX_ENTRY_CHARS + 1);
    assert!(summary.completed_steps[0].ends_with('\u{2026}'));
  }
}

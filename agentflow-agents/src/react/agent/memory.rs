use std::sync::Arc;

use agentflow_memory::{Message, Role};
use chrono::Utc;

use crate::runtime::{
  AgentEvent, AgentRunResult, AgentStepKind, MemoryHookContext, MemoryHookKind,
};

use super::config::{
  CompactMemorySummary, MemorySummaryBackend, MemorySummaryContext, MemorySummaryStrategy,
  ReActError, RecentOnlyMemorySummary,
};
use super::core::ReActAgent;

impl ReActAgent {
  pub(crate) async fn apply_memory_prompt_budget(
    &self,
    history: Vec<Message>,
  ) -> Result<(Option<String>, Vec<Message>), ReActError> {
    let Some(budget) = self.config.memory_prompt_token_budget else {
      return Ok((None, history));
    };
    if self.config.memory_summary_strategy == MemorySummaryStrategy::Disabled {
      return Ok((None, history));
    }

    let total_tokens: u32 = history.iter().map(|msg| msg.token_count).sum();
    if total_tokens <= budget {
      return Ok((None, history));
    }

    let mut kept_reversed = Vec::new();
    let mut kept_tokens = 0u32;
    for message in history.iter().rev() {
      if !kept_reversed.is_empty() && kept_tokens.saturating_add(message.token_count) > budget {
        break;
      }
      kept_tokens = kept_tokens.saturating_add(message.token_count);
      kept_reversed.push(message.clone());
    }
    kept_reversed.reverse();

    let omitted_count = history.len().saturating_sub(kept_reversed.len());
    let omitted_tokens = total_tokens.saturating_sub(kept_tokens);
    let omitted_messages = history[..omitted_count].to_vec();

    // L2.1: fold whatever this round of compaction is about to drop into
    // the persisted task-summary checkpoint, before the raw messages are
    // gone from the prompt for good.
    if let Some(store) = self.task_summary_store.clone() {
      self
        .update_task_summary(&store, &history, &omitted_messages)
        .await?;
    }

    let context = MemorySummaryContext {
      session_id: self.session_id.clone(),
      budget_tokens: budget,
      omitted_tokens,
      omitted_messages,
      kept_messages: kept_reversed.clone(),
    };

    let summary = match &self.memory_summary_backend {
      Some(backend) => backend.summarize(context).await?,
      None => match self.config.memory_summary_strategy {
        MemorySummaryStrategy::Disabled => None,
        MemorySummaryStrategy::RecentOnly => RecentOnlyMemorySummary.summarize(context).await?,
        MemorySummaryStrategy::Compact => CompactMemorySummary.summarize(context).await?,
      },
    };

    Ok((summary, kept_reversed))
  }

  async fn update_task_summary(
    &self,
    store: &Arc<dyn agentflow_memory::TaskSummaryStore>,
    history: &[Message],
    omitted_messages: &[Message],
  ) -> Result<(), ReActError> {
    if omitted_messages.is_empty() {
      return Ok(());
    }

    let previous = store.get_task_summary(&self.session_id).await?;
    let goal = previous
      .as_ref()
      .map(|p| p.goal.clone())
      .filter(|g| !g.is_empty())
      .or_else(|| {
        history
          .iter()
          .find(|m| m.role == Role::User)
          .map(|m| m.content.clone())
      })
      .unwrap_or_default();

    let Some(updated) = self
      .task_summary_generator
      .generate(crate::task_summary::TaskSummaryContext {
        goal,
        previous,
        newly_omitted: omitted_messages.to_vec(),
      })
      .await
    else {
      return Ok(());
    };

    store
      .set_task_summary(&self.session_id, updated.clone())
      .await?;

    if let Some(handle) = &self.live_sink {
      handle
        .0
        .emit(&AgentEvent::TaskSummaryUpdated {
          session_id: self.session_id.clone(),
          generator: self.task_summary_generator.name().to_string(),
          goal: updated.goal.clone(),
          completed_step_count: updated.completed_steps.len(),
          key_result_count: updated.key_results.len(),
          timestamp: Utc::now(),
        })
        .await;
    }

    Ok(())
  }

  fn notify_memory_read(
    &self,
    session_id: &str,
    kind: MemoryHookKind,
    query: Option<String>,
    limit: Option<usize>,
    messages: Vec<Message>,
  ) {
    if let Some(hook) = &self.memory_hook {
      hook.on_memory_read(&MemoryHookContext {
        session_id: session_id.to_string(),
        kind,
        query,
        limit,
        messages,
      });
    }
  }

  fn notify_memory_write(&self, message: Message) {
    if let Some(hook) = &self.memory_hook {
      hook.on_memory_write(&MemoryHookContext {
        session_id: message.session_id.clone(),
        kind: MemoryHookKind::Write,
        query: None,
        limit: None,
        messages: vec![message],
      });
    }
  }

  pub(crate) async fn add_memory_message(&mut self, message: Message) -> Result<(), ReActError> {
    self.memory.add_message(message.clone()).await?;
    self.notify_memory_write(message);
    Ok(())
  }

  pub(crate) async fn restore_trace_memory(
    &mut self,
    prior: &AgentRunResult,
  ) -> Result<(), ReActError> {
    self.memory.clear_session(&self.session_id).await?;
    for step in &prior.steps {
      match &step.kind {
        AgentStepKind::Observe { input } => {
          self
            .add_memory_message(Message::user_with_counter(
              &self.session_id,
              input,
              &*self.message_counter,
            ))
            .await?;
        }
        AgentStepKind::Plan { thought } => {
          self
            .add_memory_message(Message::assistant_with_counter(
              &self.session_id,
              thought,
              &*self.message_counter,
            ))
            .await?;
        }
        AgentStepKind::ToolCall { .. } => {}
        AgentStepKind::ToolResult {
          tool,
          content,
          is_error,
          ..
        } => {
          let observation = if *is_error {
            format!("[ERROR] {}", content)
          } else {
            content.clone()
          };
          self
            .add_memory_message(Message::tool_result_with_counter(
              &self.session_id,
              tool,
              observation,
              &*self.message_counter,
            ))
            .await?;
        }
        AgentStepKind::Reflect { content } => {
          self
            .add_memory_message(Message::assistant_with_counter(
              &self.session_id,
              content,
              &*self.message_counter,
            ))
            .await?;
        }
        AgentStepKind::FinalAnswer { answer } => {
          self
            .add_memory_message(Message::assistant_with_counter(
              &self.session_id,
              answer,
              &*self.message_counter,
            ))
            .await?;
        }
        AgentStepKind::Verify {
          approved, feedback, ..
        } => {
          // Only a rejection ever fed a message into memory live (see
          // `record_verification`): an approval is a pure gate with no
          // observation attached, so restoring it is a no-op.
          if !*approved && let Some(feedback) = feedback {
            self
              .add_memory_message(Message::tool_result_with_counter(
                &self.session_id,
                "verifier",
                feedback,
                &*self.message_counter,
              ))
              .await?;
          }
        }
        AgentStepKind::Handoff { .. }
        | AgentStepKind::BlackboardOp { .. }
        | AgentStepKind::DebateProposal { .. }
        | AgentStepKind::DebateVerdict { .. } => {
          // Multi-agent supervisor steps are not part of this ReActAgent's own
          // conversation, so they are dropped when restoring its memory.
        }
      }
    }
    Ok(())
  }

  pub(crate) async fn read_memory_history(
    &self,
    session_id: &str,
  ) -> Result<Vec<Message>, ReActError> {
    let messages = self.memory.get_all(session_id).await?;
    self.notify_memory_read(
      session_id,
      MemoryHookKind::ReadHistory,
      None,
      None,
      messages.clone(),
    );
    Ok(messages)
  }

  pub(crate) async fn search_memory(
    &self,
    session_id: &str,
    query: &str,
    limit: usize,
  ) -> Result<Vec<Message>, ReActError> {
    let messages = self.memory.search(session_id, query, limit).await?;
    self.notify_memory_read(
      session_id,
      MemoryHookKind::Search,
      Some(query.to_string()),
      Some(limit),
      messages.clone(),
    );
    Ok(messages)
  }

  /// Clear the current session's memory.
  pub async fn reset(&mut self) -> Result<(), ReActError> {
    self.memory.clear_session(&self.session_id).await?;
    self.session_id = uuid::Uuid::new_v4().to_string();
    Ok(())
  }

  /// Estimated tokens used in the current session.
  pub async fn token_count(&self) -> Result<u32, ReActError> {
    Ok(self.memory.session_token_count(&self.session_id).await?)
  }
}

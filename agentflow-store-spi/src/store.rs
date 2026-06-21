use async_trait::async_trait;

use crate::{MemoryError, Message};

/// Trait implemented by all memory backends.
///
/// Q2.10.2: every method takes `&self`. The two write paths
/// (`add_message`, `clear_session`) used to take `&mut self`, which
/// serialized concurrent writes through the borrow checker even on
/// backends whose underlying state was already lock-protected (sqlx
/// pool, dashmap, mutex). The ReAct H3 parallel tool-call dispatcher
/// in particular wanted to fan out multi-tool calls into concurrent
/// `memory.add_message` futures and was instead forced through the
/// outer lock. Switching to `&self` lets every backend use its own
/// internal concurrency primitive.
#[async_trait]
pub trait MemoryStore: Send + Sync {
  /// Append a message to the store.
  async fn add_message(&self, message: Message) -> Result<(), MemoryError>;

  /// Retrieve the most recent `limit` messages for a session (oldest first).
  async fn get_history(&self, session_id: &str, limit: usize) -> Result<Vec<Message>, MemoryError>;

  /// Retrieve all messages for a session (oldest first).
  async fn get_all(&self, session_id: &str) -> Result<Vec<Message>, MemoryError>;

  /// Simple keyword search over message content.
  async fn search(
    &self,
    session_id: &str,
    query: &str,
    limit: usize,
  ) -> Result<Vec<Message>, MemoryError>;

  /// Delete all messages for a session.
  async fn clear_session(&self, session_id: &str) -> Result<(), MemoryError>;

  /// Sum of `token_count` for all messages in a session.
  async fn session_token_count(&self, session_id: &str) -> Result<u32, MemoryError>;

  /// Build a prompt string from the session history.
  async fn to_prompt(&self, session_id: &str) -> Result<String, MemoryError> {
    let messages = self.get_all(session_id).await?;
    Ok(
      messages
        .iter()
        .map(|m| m.to_prompt_line())
        .collect::<Vec<_>>()
        .join("\n"),
    )
  }
}

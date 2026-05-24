use std::collections::HashMap;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::{MemoryError, MemoryStore, Message, Role};

/// In-memory session store with a sliding token-window.
///
/// Keeps all messages up to `max_tokens` total estimated tokens per session.
/// When the budget is exceeded the oldest **non-system** messages are evicted
/// first; system messages are always preserved.
///
/// Q2.10.2: the inner `HashMap` lives behind a `tokio::sync::Mutex` so
/// the [`MemoryStore`] impl can use `&self`. Concurrent
/// `add_message` calls (the ReAct H3 parallel tool-call path)
/// serialize through one mutex, which is fine for an in-memory dev
/// backend — sqlx-backed backends have their own pool-level
/// concurrency primitives.
pub struct SessionMemory {
  sessions: Mutex<HashMap<String, Vec<Message>>>,
  max_tokens: u32,
}

impl SessionMemory {
  pub fn new(max_tokens: u32) -> Self {
    Self {
      sessions: Mutex::new(HashMap::new()),
      max_tokens,
    }
  }

  /// 8 000-token context window — suitable for most chat models.
  pub fn default_window() -> Self {
    Self::new(8_000)
  }

  /// 128 000-token window — for long-context models (Claude, GPT-4o).
  pub fn large_window() -> Self {
    Self::new(128_000)
  }

  /// Evict oldest non-system messages until we are within the token budget.
  /// Caller holds the mutex.
  fn prune_locked(sessions: &mut HashMap<String, Vec<Message>>, session_id: &str, max_tokens: u32) {
    let Some(msgs) = sessions.get_mut(session_id) else {
      return;
    };

    let mut total: u32 = msgs.iter().map(|m| m.token_count).sum();

    while total > max_tokens {
      // Find the oldest non-system message
      let pos = msgs.iter().position(|m| m.role != Role::System);
      match pos {
        Some(i) => {
          total = total.saturating_sub(msgs[i].token_count);
          msgs.remove(i);
        }
        None => break, // Only system messages remain; stop pruning
      }
    }
  }
}

#[async_trait]
impl MemoryStore for SessionMemory {
  async fn add_message(&self, message: Message) -> Result<(), MemoryError> {
    let session_id = message.session_id.clone();
    let mut sessions = self.sessions.lock().await;
    sessions
      .entry(session_id.clone())
      .or_default()
      .push(message);
    Self::prune_locked(&mut sessions, &session_id, self.max_tokens);
    Ok(())
  }

  async fn get_history(&self, session_id: &str, limit: usize) -> Result<Vec<Message>, MemoryError> {
    let sessions = self.sessions.lock().await;
    let all = sessions.get(session_id).cloned().unwrap_or_default();
    let start = all.len().saturating_sub(limit);
    Ok(all[start..].to_vec())
  }

  async fn get_all(&self, session_id: &str) -> Result<Vec<Message>, MemoryError> {
    let sessions = self.sessions.lock().await;
    Ok(sessions.get(session_id).cloned().unwrap_or_default())
  }

  async fn search(
    &self,
    session_id: &str,
    query: &str,
    limit: usize,
  ) -> Result<Vec<Message>, MemoryError> {
    let query_lc = query.to_lowercase();
    let sessions = self.sessions.lock().await;
    let matches: Vec<Message> = sessions
      .get(session_id)
      .cloned()
      .unwrap_or_default()
      .into_iter()
      .filter(|m| m.content.to_lowercase().contains(&query_lc))
      .take(limit)
      .collect();
    Ok(matches)
  }

  async fn clear_session(&self, session_id: &str) -> Result<(), MemoryError> {
    let mut sessions = self.sessions.lock().await;
    sessions.remove(session_id);
    Ok(())
  }

  async fn session_token_count(&self, session_id: &str) -> Result<u32, MemoryError> {
    let sessions = self.sessions.lock().await;
    let total = sessions
      .get(session_id)
      .map(|msgs| msgs.iter().map(|m| m.token_count).sum())
      .unwrap_or(0);
    Ok(total)
  }
}

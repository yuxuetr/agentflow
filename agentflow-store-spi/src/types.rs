use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Role of a message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
  System,
  User,
  Assistant,
  /// Output produced by a tool invocation
  Tool,
}

impl Role {
  pub fn as_str(&self) -> &str {
    match self {
      Role::System => "system",
      Role::User => "user",
      Role::Assistant => "assistant",
      Role::Tool => "tool",
    }
  }
}

impl std::fmt::Display for Role {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.as_str())
  }
}

impl From<&str> for Role {
  fn from(s: &str) -> Self {
    match s {
      "system" => Role::System,
      "user" => Role::User,
      "assistant" => Role::Assistant,
      "tool" => Role::Tool,
      _ => Role::User,
    }
  }
}

/// Token-counter abstraction shared with `agentflow-llm::TokenCounter`
/// (P10.3.3-FU1). The memory crate doesn't depend on agentflow-llm
/// so the trait is defined locally — `agentflow_llm::counter_for_model`
/// returns a `Box<dyn agentflow_llm::TokenCounter>` whose
/// `count_tokens` method has the same `&str -> u32` shape, so an
/// adapter (`agentflow-agents::token_counter_adapter`) bridges the
/// two surfaces without forcing a workspace-wide trait merge.
///
/// Implementations should be cheap to call repeatedly — they're
/// invoked once per message construction in the hot path of every
/// ReAct turn.
pub trait TokenCounter: Send + Sync {
  fn count_tokens(&self, text: &str) -> u32;
}

/// Default counter: `(content.len() / 4).max(1)`. This is the
/// pre-FU1 behaviour preserved as the fallback for `Message::new`
/// and friends — call sites that don't know the model id (most
/// tests, the bare `Message::user` constructor) get this counter.
///
/// Over-estimates English text by ~10-20% and CJK by 3-5×;
/// under-estimates dense code. The counter-aware constructors
/// (`Message::*_with_counter`) are the precise path.
#[derive(Debug, Default, Clone, Copy)]
pub struct HeuristicCounter;

impl TokenCounter for HeuristicCounter {
  fn count_tokens(&self, text: &str) -> u32 {
    (text.len() / 4).max(1) as u32
  }
}

/// A single message in a conversation session.
///
/// `token_count` is populated either via the heuristic (4 chars
/// per token, the pre-P10.3.3-FU1 default) or via a real
/// tokenizer supplied through the `*_with_counter` constructors.
/// The latter is the precise path; agents that know their target
/// model id should build a counter via
/// `agentflow_llm::counter_for_model(model_id)` and route every
/// message through it so the budget enforcement in
/// `ReActAgent::apply_memory_prompt_budget` lines up with what
/// the LLM provider actually bills.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
  pub id: Uuid,
  /// Opaque session identifier (e.g. UUID string or user-provided name)
  pub session_id: String,
  pub role: Role,
  pub content: String,
  pub timestamp: DateTime<Utc>,
  /// For `Tool` messages: the name of the tool that produced this output
  pub tool_name: Option<String>,
  /// Token count for this message. Populated by either the
  /// heuristic (`HeuristicCounter`, 4 chars/token — pre-FU1
  /// default) or a real tokenizer via the `*_with_counter`
  /// constructors.
  pub token_count: u32,
}

impl Message {
  /// Construct a message using the heuristic counter (4 chars per
  /// token). Pre-P10.3.3-FU1 default — kept as the bare API for
  /// callers that don't have a model id handy (most tests, ad-hoc
  /// message construction).
  pub fn new(session_id: &str, role: Role, content: impl Into<String>) -> Self {
    Self::new_with_counter(session_id, role, content, &HeuristicCounter)
  }

  /// Construct a message with a precise token count from the
  /// given counter. The counter is dropped immediately after the
  /// count — it's not retained on the message.
  pub fn new_with_counter(
    session_id: &str,
    role: Role,
    content: impl Into<String>,
    counter: &dyn TokenCounter,
  ) -> Self {
    let content = content.into();
    let token_count = counter.count_tokens(&content).max(1);
    Self {
      id: Uuid::new_v4(),
      session_id: session_id.to_string(),
      role,
      content,
      timestamp: Utc::now(),
      tool_name: None,
      token_count,
    }
  }

  pub fn system(session_id: &str, content: impl Into<String>) -> Self {
    Self::new(session_id, Role::System, content)
  }

  pub fn user(session_id: &str, content: impl Into<String>) -> Self {
    Self::new(session_id, Role::User, content)
  }

  pub fn assistant(session_id: &str, content: impl Into<String>) -> Self {
    Self::new(session_id, Role::Assistant, content)
  }

  pub fn tool_result(session_id: &str, tool_name: &str, content: impl Into<String>) -> Self {
    let mut msg = Self::new(session_id, Role::Tool, content);
    msg.tool_name = Some(tool_name.to_string());
    msg
  }

  /// `Message::system` with a precise token count. Use when the
  /// caller knows the target model and has built a counter via
  /// `agentflow_llm::counter_for_model`.
  pub fn system_with_counter(
    session_id: &str,
    content: impl Into<String>,
    counter: &dyn TokenCounter,
  ) -> Self {
    Self::new_with_counter(session_id, Role::System, content, counter)
  }

  pub fn user_with_counter(
    session_id: &str,
    content: impl Into<String>,
    counter: &dyn TokenCounter,
  ) -> Self {
    Self::new_with_counter(session_id, Role::User, content, counter)
  }

  pub fn assistant_with_counter(
    session_id: &str,
    content: impl Into<String>,
    counter: &dyn TokenCounter,
  ) -> Self {
    Self::new_with_counter(session_id, Role::Assistant, content, counter)
  }

  pub fn tool_result_with_counter(
    session_id: &str,
    tool_name: &str,
    content: impl Into<String>,
    counter: &dyn TokenCounter,
  ) -> Self {
    let mut msg = Self::new_with_counter(session_id, Role::Tool, content, counter);
    msg.tool_name = Some(tool_name.to_string());
    msg
  }

  /// Format message for inclusion in a plain-text prompt
  pub fn to_prompt_line(&self) -> String {
    match &self.role {
      Role::System => format!("[System] {}", self.content),
      Role::User => format!("Human: {}", self.content),
      Role::Assistant => format!("Assistant: {}", self.content),
      Role::Tool => {
        let name = self.tool_name.as_deref().unwrap_or("tool");
        format!("Tool result ({}): {}", name, self.content)
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Toy precise counter for tests — pretends every space-
  /// delimited word is exactly 1 token, plus 1 for the message
  /// boundary. Lets us assert the *path* without depending on
  /// the real BPE tokenizer (which lives in agentflow-llm).
  #[derive(Default)]
  struct WordCounter;

  impl TokenCounter for WordCounter {
    fn count_tokens(&self, text: &str) -> u32 {
      text.split_whitespace().count() as u32 + 1
    }
  }

  #[test]
  fn message_new_uses_heuristic_4_chars_per_token() {
    let msg = Message::new("s", Role::User, "1234567890");
    // 10 chars / 4 = 2 tokens.
    assert_eq!(msg.token_count, 2);
  }

  #[test]
  fn message_new_with_counter_uses_supplied_counter() {
    let counter = WordCounter;
    let msg = Message::new_with_counter("s", Role::User, "hello world from rust", &counter);
    // 4 words + 1 boundary = 5 tokens.
    assert_eq!(msg.token_count, 5);
  }

  #[test]
  fn message_user_with_counter_preserves_role() {
    let counter = WordCounter;
    let msg = Message::user_with_counter("s", "test", &counter);
    assert_eq!(msg.role, Role::User);
    // 1 word + 1 boundary = 2.
    assert_eq!(msg.token_count, 2);
  }

  #[test]
  fn message_tool_result_with_counter_preserves_tool_name_and_role() {
    let counter = WordCounter;
    let msg = Message::tool_result_with_counter("s", "echo", "hello world", &counter);
    assert_eq!(msg.role, Role::Tool);
    assert_eq!(msg.tool_name.as_deref(), Some("echo"));
    // 2 words + 1 boundary = 3.
    assert_eq!(msg.token_count, 3);
  }

  #[test]
  fn token_count_floor_is_one_for_empty_content() {
    // `.max(1)` invariant: even an empty string yields 1
    // token. Preserved across both heuristic and counter paths
    // so the budget arithmetic never multiplies by zero.
    let heuristic = Message::new("s", Role::User, "");
    assert_eq!(heuristic.token_count, 1);
    // A counter that returns 0 for empty input still gets
    // floored to 1 by `new_with_counter`.
    struct ZeroCounter;
    impl TokenCounter for ZeroCounter {
      fn count_tokens(&self, _text: &str) -> u32 {
        0
      }
    }
    let from_counter = Message::new_with_counter("s", Role::User, "", &ZeroCounter);
    assert_eq!(from_counter.token_count, 1);
  }

  #[test]
  fn heuristic_and_counter_diverge_on_cjk_input() {
    // Sanity check that the counter path actually differs from
    // the heuristic for the case that motivated P10.3.3-FU1
    // (CJK text where 4-bytes-per-token over-estimates by
    // multiples).
    struct CjkAware;
    impl TokenCounter for CjkAware {
      fn count_tokens(&self, text: &str) -> u32 {
        // Pretend every Chinese character is 1 token.
        text.chars().count() as u32
      }
    }
    let cjk = "你好世界这是一个测试";
    let heuristic_count = Message::new("s", Role::User, cjk).token_count;
    let counter_count = Message::new_with_counter("s", Role::User, cjk, &CjkAware).token_count;
    assert_ne!(
      heuristic_count, counter_count,
      "heuristic and a CJK-aware counter must disagree on CJK input — \
       that's the precision win P10.3.3-FU1 is plumbed for"
    );
  }
}

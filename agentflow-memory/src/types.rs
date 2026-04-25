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

/// A single message in a conversation session
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
  /// Approximate token count (content.len() / 4)
  pub token_count: u32,
}

impl Message {
  pub fn new(session_id: &str, role: Role, content: impl Into<String>) -> Self {
    let content = content.into();
    let token_count = (content.len() / 4).max(1) as u32;
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

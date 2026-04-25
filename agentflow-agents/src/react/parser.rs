use serde_json::Value;

/// A parsed LLM response in the ReAct loop.
#[derive(Debug, Clone)]
pub enum AgentResponse {
  /// The model wants to call a tool.
  Action {
    thought: String,
    tool: String,
    params: Value,
  },
  /// The model has a final answer.
  Answer { thought: String, answer: String },
  /// The model responded with text that could not be parsed as JSON.
  /// Treated as a final answer so the loop terminates gracefully.
  Malformed(String),
}

impl AgentResponse {
  /// Parse a raw LLM response string into an `AgentResponse`.
  ///
  /// Handles:
  /// - JSON objects directly in the response
  /// - JSON wrapped in ` ```json … ``` ` code fences
  /// - Plain text (returned as `Malformed`)
  pub fn parse(text: &str) -> Self {
    let json_str = extract_json(text);

    match serde_json::from_str::<Value>(&json_str) {
      Ok(v) => {
        let thought = v["thought"].as_str().unwrap_or("").to_string();

        // Check for "action" key → tool call
        if let Some(action) = v.get("action") {
          let tool = action["tool"].as_str().unwrap_or("").to_string();
          let params = action
            .get("params")
            .cloned()
            .unwrap_or(Value::Object(serde_json::Map::new()));
          if !tool.is_empty() {
            return Self::Action {
              thought,
              tool,
              params,
            };
          }
        }

        // Check for "answer" key → final answer
        if let Some(answer_val) = v.get("answer") {
          let answer = match answer_val {
            Value::String(s) => s.clone(),
            other => other.to_string(),
          };
          return Self::Answer { thought, answer };
        }

        // JSON parsed but has neither "action" nor "answer" key
        Self::Malformed(text.to_string())
      }
      Err(_) => Self::Malformed(text.to_string()),
    }
  }

  pub fn is_terminal(&self) -> bool {
    matches!(self, Self::Answer { .. } | Self::Malformed(_))
  }
}

/// Try to pull a JSON object out of `text`, stripping markdown code fences if present.
fn extract_json(text: &str) -> String {
  // Strip ```json ... ``` blocks
  if let Some(start) = text.find("```json") {
    let after = &text[start + 7..];
    if let Some(end) = after.find("```") {
      return after[..end].trim().to_string();
    }
  }
  // Strip ``` ... ``` blocks
  if let Some(start) = text.find("```") {
    let after = &text[start + 3..];
    if let Some(end) = after.find("```") {
      return after[..end].trim().to_string();
    }
  }
  // Find the outermost { … } span
  if let Some(start) = text.find('{') {
    if let Some(end) = text.rfind('}') {
      if end > start {
        return text[start..=end].to_string();
      }
    }
  }
  text.to_string()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_action() {
    let raw = r#"{"thought": "I need to check the date", "action": {"tool": "shell", "params": {"command": "date"}}}"#;
    match AgentResponse::parse(raw) {
      AgentResponse::Action { thought, tool, .. } => {
        assert_eq!(thought, "I need to check the date");
        assert_eq!(tool, "shell");
      }
      other => panic!("Expected Action, got {:?}", other),
    }
  }

  #[test]
  fn parses_answer() {
    let raw = r#"{"thought": "I have enough info", "answer": "The answer is 42"}"#;
    match AgentResponse::parse(raw) {
      AgentResponse::Answer { answer, .. } => {
        assert_eq!(answer, "The answer is 42");
      }
      other => panic!("Expected Answer, got {:?}", other),
    }
  }

  #[test]
  fn parses_json_in_code_fence() {
    let raw = "Here is my response:\n```json\n{\"thought\": \"done\", \"answer\": \"ok\"}\n```";
    match AgentResponse::parse(raw) {
      AgentResponse::Answer { answer, .. } => assert_eq!(answer, "ok"),
      other => panic!("Expected Answer, got {:?}", other),
    }
  }

  /// An action JSON with a missing "tool" field falls through to Malformed.
  #[test]
  fn action_missing_tool_name_becomes_malformed() {
    let raw = r#"{"thought": "hmm", "action": {"params": {"command": "ls"}}}"#;
    assert!(
      matches!(AgentResponse::parse(raw), AgentResponse::Malformed(_)),
      "action without 'tool' should be Malformed"
    );
  }

  /// An action with an empty string for "tool" falls through to Malformed.
  #[test]
  fn action_empty_tool_name_becomes_malformed() {
    let raw = r#"{"thought": "hmm", "action": {"tool": "", "params": {}}}"#;
    assert!(
      matches!(AgentResponse::parse(raw), AgentResponse::Malformed(_)),
      "action with empty 'tool' should be Malformed"
    );
  }

  /// An action with deeply nested params is parsed correctly.
  #[test]
  fn action_with_nested_params() {
    let raw = r#"{
            "thought": "searching",
            "action": {
                "tool": "file",
                "params": {"path": "/tmp/x", "options": {"encoding": "utf-8", "mode": "r"}}
            }
        }"#;
    match AgentResponse::parse(raw) {
      AgentResponse::Action { tool, params, .. } => {
        assert_eq!(tool, "file");
        assert_eq!(params["path"], "/tmp/x");
        assert_eq!(params["options"]["encoding"], "utf-8");
      }
      other => panic!("Expected Action, got {:?}", other),
    }
  }

  /// Plain text (no JSON) becomes Malformed.
  #[test]
  fn plain_text_becomes_malformed() {
    let raw = "I don't know how to answer that.";
    assert!(
      matches!(AgentResponse::parse(raw), AgentResponse::Malformed(_)),
      "plain text should be Malformed"
    );
  }

  /// JSON with neither "action" nor "answer" key becomes Malformed.
  #[test]
  fn json_without_action_or_answer_is_malformed() {
    let raw = r#"{"thought": "just thinking", "note": "nothing actionable"}"#;
    assert!(
      matches!(AgentResponse::parse(raw), AgentResponse::Malformed(_)),
      "JSON without action or answer should be Malformed"
    );
  }

  /// is_terminal() returns false for Action.
  #[test]
  fn is_terminal_false_for_action() {
    let r = AgentResponse::Action {
      thought: "t".into(),
      tool: "shell".into(),
      params: serde_json::Value::Null,
    };
    assert!(!r.is_terminal());
  }

  /// is_terminal() returns true for Answer.
  #[test]
  fn is_terminal_true_for_answer() {
    let r = AgentResponse::Answer {
      thought: "t".into(),
      answer: "42".into(),
    };
    assert!(r.is_terminal());
  }

  /// is_terminal() returns true for Malformed.
  #[test]
  fn is_terminal_true_for_malformed() {
    let r = AgentResponse::Malformed("garbage".into());
    assert!(r.is_terminal());
  }

  /// JSON wrapped in plain ``` fences (no language tag) is also extracted.
  #[test]
  fn parses_json_in_plain_code_fence() {
    let raw = "Result:\n```\n{\"thought\": \"ok\", \"answer\": \"done\"}\n```";
    match AgentResponse::parse(raw) {
      AgentResponse::Answer { answer, .. } => assert_eq!(answer, "done"),
      other => panic!("Expected Answer, got {:?}", other),
    }
  }

  /// answer value that is not a string is coerced to its JSON representation.
  #[test]
  fn answer_non_string_coerced_to_string() {
    let raw = r#"{"thought": "counting", "answer": 42}"#;
    match AgentResponse::parse(raw) {
      AgentResponse::Answer { answer, .. } => assert_eq!(answer, "42"),
      other => panic!("Expected Answer, got {:?}", other),
    }
  }
}

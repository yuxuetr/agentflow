use serde_json::Value;
use tracing::warn;

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
      Err(_) => {
        // serde_json failed — most common cause in practice is the LLM
        // response getting truncated mid-JSON by `max_tokens` (A7/A2
        // dogfooding finding F-A2-1: 4096-token cap can truncate large
        // changelog / code-review outputs mid-string, leaving an
        // unclosed `"answer": "..."` value). Before falling back to
        // Malformed (which would expose the JSON envelope to the user),
        // try a best-effort extraction of just the `answer` field's
        // contents, treating the unbalanced JSON as a partial Answer.
        if let Some(answer) = try_extract_answer_field(&json_str) {
          let thought = extract_string_field(&json_str, "thought").unwrap_or_default();
          warn!(
            answer_chars = answer.chars().count(),
            "LLM response JSON was unparseable (likely truncated by `max_tokens`); \
             recovered partial answer via best-effort extraction. Consider raising \
             the model's `max_tokens` ceiling if this fires often."
          );
          return Self::Answer { thought, answer };
        }
        Self::Malformed(text.to_string())
      }
    }
  }

  pub fn is_terminal(&self) -> bool {
    matches!(self, Self::Answer { .. } | Self::Malformed(_))
  }
}

/// Best-effort extraction of the `"answer": "..."` field from a JSON-like
/// string whose top-level `serde_json::from_str` failed. Returns the
/// (unescaped) inner string. Designed for the truncated-mid-string case:
/// LLM emits `{"thought":"...","answer":"## Review summary..."` and gets
/// cut off by `max_tokens` before the closing `"` and `}` — we still
/// want the partial answer to reach the user instead of being treated
/// as JSON garbage and surfaced via the Malformed code path (which
/// would print the whole `{"thought":...` wrapper).
///
/// Returns `None` when:
/// - text doesn't contain a `"answer"` key (so it's not actually a
///   truncated Answer at all)
/// - the value isn't a string-typed field (e.g. `"answer": 42` —
///   handled by the strict-parse branch when JSON is valid)
fn try_extract_answer_field(text: &str) -> Option<String> {
  extract_string_field(text, "answer").filter(|s| !s.is_empty())
}

/// Find `"<key>": "..."` in `text` and return the unescaped inner string
/// even when the value is truncated (no closing quote). Returns `None`
/// when the key is missing or the value isn't followed by a string
/// literal opener (`"`).
fn extract_string_field(text: &str, key: &str) -> Option<String> {
  let needle = format!("\"{key}\"");
  let key_pos = text.find(&needle)?;
  let after_key = &text[key_pos + needle.len()..];
  let colon_pos = after_key.find(':')?;
  let after_colon = &after_key[colon_pos + 1..];
  // Skip whitespace then expect a string opener.
  let trimmed = after_colon.trim_start();
  if !trimmed.starts_with('"') {
    return None;
  }
  let inner = &trimmed[1..];
  Some(unescape_json_string_until_quote(inner))
}

/// Walk `text` consuming characters until an unescaped `"` (string terminator)
/// or end-of-input. Decodes the common JSON escape sequences. When input
/// terminates without a closing quote (truncation), returns whatever was
/// decoded so far.
fn unescape_json_string_until_quote(text: &str) -> String {
  let mut out = String::with_capacity(text.len());
  let mut chars = text.chars();
  while let Some(ch) = chars.next() {
    match ch {
      '"' => break,
      '\\' => match chars.next() {
        Some('n') => out.push('\n'),
        Some('t') => out.push('\t'),
        Some('r') => out.push('\r'),
        Some('"') => out.push('"'),
        Some('\\') => out.push('\\'),
        Some('/') => out.push('/'),
        Some('b') => out.push('\u{08}'),
        Some('f') => out.push('\u{0c}'),
        Some('u') => {
          // Try to read 4 hex digits; if the input is truncated, drop
          // the partial escape and keep what we have.
          let hex: String = chars.by_ref().take(4).collect();
          if hex.len() == 4
            && let Ok(code) = u32::from_str_radix(&hex, 16)
            && let Some(c) = char::from_u32(code)
          {
            out.push(c);
          }
        }
        Some(other) => {
          // Unknown escape — preserve verbatim.
          out.push('\\');
          out.push(other);
        }
        None => {
          // Truncation right after a backslash; drop it.
          break;
        }
      },
      other => out.push(other),
    }
  }
  out
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
  if let Some(start) = text.find('{')
    && let Some(end) = text.rfind('}')
    && end > start
  {
    return text[start..=end].to_string();
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

  /// F-A2-1 — `max_tokens` truncation inside the `answer` string:
  /// the LLM emitted a valid JSON envelope but Moonshot cut the response
  /// before the closing `"` and `}` of the answer value. Strict
  /// `serde_json::from_str` fails; before the parser fix the user
  /// would see the raw `{"thought":..,"answer":..` wrapper. The fix
  /// best-effort extracts the partial answer string.
  #[test]
  fn truncated_answer_string_extracted_as_answer() {
    let raw = "{\"thought\": \"I have enough info\", \"answer\": \"## Review summary\\n\\nThe code looks fine but";
    match AgentResponse::parse(raw) {
      AgentResponse::Answer { answer, .. } => {
        assert!(answer.starts_with("## Review summary"));
        assert!(answer.ends_with("looks fine but"));
        // Newlines were unescaped from `\n` to actual newlines.
        assert!(answer.contains('\n'));
      }
      other => panic!("Expected Answer (truncated recovery), got {:?}", other),
    }
  }

  /// Truncation at any earlier point (before the closing `}` of the
  /// outer object but after a complete answer string) still produces a
  /// clean Answer — the strict parse would also recover here if the
  /// closing `}` were present; the fallback extractor handles the case
  /// where it isn't.
  #[test]
  fn truncated_object_after_complete_answer_string_still_extracts() {
    let raw = "{\"thought\": \"ok\", \"answer\": \"complete content";
    match AgentResponse::parse(raw) {
      AgentResponse::Answer { answer, .. } => {
        assert_eq!(answer, "complete content");
      }
      other => panic!("Expected Answer, got {:?}", other),
    }
  }

  /// Truncation that lands mid-escape sequence (e.g. backslash at very end)
  /// drops the dangling escape rather than panicking or emitting a lone
  /// backslash.
  #[test]
  fn truncation_mid_escape_drops_dangling_backslash() {
    let raw = "{\"answer\": \"hello\\";
    match AgentResponse::parse(raw) {
      AgentResponse::Answer { answer, .. } => {
        assert_eq!(answer, "hello");
      }
      other => panic!("Expected Answer, got {:?}", other),
    }
  }

  /// JSON-like garbage with no `"answer"` field stays as Malformed —
  /// the fallback extractor only fires when there's a plausible answer
  /// string to recover, so non-Answer truncation still surfaces via
  /// the Malformed path.
  #[test]
  fn truncation_without_answer_field_stays_malformed() {
    // Note: top-level extractor first tries `{...}` boundary; with
    // no closing `}` the strict JSON parse fails. No `"answer"` key
    // means the fallback extractor returns None → Malformed.
    let raw = "{\"thought\": \"still thinking\",  \"action\": {\"tool\": \"shell";
    assert!(
      matches!(AgentResponse::parse(raw), AgentResponse::Malformed(_)),
      "truncated text without `answer` field should be Malformed"
    );
  }

  /// Unescape covers the common JSON sequences: \n / \t / \r / \" / \\ /
  /// \/ / \b / \f and \uXXXX.
  #[test]
  fn truncation_unescapes_common_sequences() {
    let raw = "{\"answer\": \"line1\\nline2\\t\\\"quoted\\\"\\\\back\\/slashé";
    match AgentResponse::parse(raw) {
      AgentResponse::Answer { answer, .. } => {
        assert_eq!(answer, "line1\nline2\t\"quoted\"\\back/slashé");
      }
      other => panic!("Expected Answer, got {:?}", other),
    }
  }

  /// Empty `"answer"` value (LLM hiccup, returned `{"answer":""}`)
  /// goes through the strict-parse path as an empty Answer. The
  /// fallback's `filter(|s| !s.is_empty())` only kicks in when JSON
  /// parsing fails AND extraction also yields empty — in that case
  /// Malformed is preferred over an empty Answer.
  #[test]
  fn empty_answer_field_from_strict_parse_returns_empty_answer() {
    let raw = r#"{"answer": ""}"#;
    match AgentResponse::parse(raw) {
      AgentResponse::Answer { answer, .. } => assert_eq!(answer, ""),
      other => panic!("Expected Answer(empty), got {:?}", other),
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

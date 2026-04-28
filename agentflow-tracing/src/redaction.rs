//! Trace redaction utilities.
//!
//! Redaction is applied before traces are persisted or exported. The defaults
//! are intentionally conservative around credentials, headers, environment
//! values, and tool parameters.

use crate::types::ExecutionTrace;
use serde_json::Value;

pub const REDACTED_VALUE: &str = "[REDACTED]";

#[derive(Debug, Clone)]
pub struct RedactionConfig {
  pub enabled: bool,
  pub replacement: String,
  pub sensitive_key_patterns: Vec<String>,
  pub max_value_bytes: Option<usize>,
}

impl Default for RedactionConfig {
  fn default() -> Self {
    Self {
      enabled: true,
      replacement: REDACTED_VALUE.to_string(),
      sensitive_key_patterns: default_sensitive_key_patterns(),
      max_value_bytes: None,
    }
  }
}

impl RedactionConfig {
  pub fn disabled() -> Self {
    Self {
      enabled: false,
      ..Self::default()
    }
  }

  pub fn with_max_value_bytes(mut self, max_value_bytes: usize) -> Self {
    self.max_value_bytes = Some(max_value_bytes);
    self
  }

  pub fn with_sensitive_key(mut self, key_pattern: impl Into<String>) -> Self {
    self.sensitive_key_patterns.push(key_pattern.into());
    self
  }
}

pub fn redact_trace(trace: &mut ExecutionTrace, config: &RedactionConfig) {
  if !config.enabled {
    return;
  }

  for node in &mut trace.nodes {
    redact_option_value(&mut node.input, config);
    redact_option_value(&mut node.output, config);

    if let Some(llm) = &mut node.llm_details {
      redact_string(&mut llm.system_prompt, config);
      redact_plain_string(&mut llm.user_prompt, config);
      redact_plain_string(&mut llm.response, config);
    }

    if let Some(agent) = &mut node.agent_details {
      redact_value(&mut agent.stop_reason, config);
      for step in &mut agent.steps {
        redact_value(step, config);
      }
      for event in &mut agent.events {
        redact_value(event, config);
      }
      for tool_call in &mut agent.tool_calls {
        redact_option_value(&mut tool_call.params, config);
      }
    }
  }
}

pub fn redact_value(value: &mut Value, config: &RedactionConfig) {
  if !config.enabled {
    return;
  }
  redact_value_at_key(value, None, config);
}

/// Redact sensitive fragments in plain text intended for CLI/log display.
///
/// This covers common inline forms such as `API_KEY=value`,
/// `Authorization: Bearer value`, and `--token=value`. Structured trace data
/// should still use [`redact_value`] or [`redact_trace`] first.
pub fn redact_text(value: &str, config: &RedactionConfig) -> String {
  if !config.enabled {
    return value.to_string();
  }

  let redacted = redact_bearer_tokens(value, config);
  redact_assignment_like_tokens(&redacted, config)
}

fn redact_option_value(value: &mut Option<Value>, config: &RedactionConfig) {
  if let Some(value) = value {
    redact_value(value, config);
  }
}

fn redact_value_at_key(value: &mut Value, key: Option<&str>, config: &RedactionConfig) {
  if let Some(key) = key {
    if is_sensitive_key(key, config) {
      *value = Value::String(config.replacement.clone());
      return;
    }
  }

  match value {
    Value::Object(map) => {
      for (child_key, child_value) in map.iter_mut() {
        redact_value_at_key(child_value, Some(child_key), config);
      }
    }
    Value::Array(values) => {
      for child in values {
        redact_value_at_key(child, None, config);
      }
    }
    Value::String(text) => redact_plain_string(text, config),
    _ => {}
  }
}

fn redact_string(value: &mut Option<String>, config: &RedactionConfig) {
  if let Some(value) = value {
    redact_plain_string(value, config);
  }
}

fn redact_plain_string(value: &mut String, config: &RedactionConfig) {
  *value = redact_text(value, config);
  if let Some(max_value_bytes) = config.max_value_bytes {
    if value.len() > max_value_bytes {
      *value = format!("[TRUNCATED: {} bytes]", value.len());
    }
  }
}

fn redact_bearer_tokens(value: &str, config: &RedactionConfig) -> String {
  let mut output = String::with_capacity(value.len());
  let mut redact_next = false;
  map_whitespace_tokens(
    value,
    |token| {
      if redact_next {
        redact_next = false;
        config.replacement.clone()
      } else {
        if token.eq_ignore_ascii_case("bearer") {
          redact_next = true;
        }
        token.to_string()
      }
    },
    &mut output,
  );
  output
}

fn redact_assignment_like_tokens(value: &str, config: &RedactionConfig) -> String {
  let mut output = String::with_capacity(value.len());
  map_whitespace_tokens(
    value,
    |token| redact_assignment_token(token, config),
    &mut output,
  );
  output
}

fn map_whitespace_tokens<F>(value: &str, mut map_token: F, output: &mut String)
where
  F: FnMut(&str) -> String,
{
  let mut token_start: Option<usize> = None;
  for (index, ch) in value.char_indices() {
    if ch.is_whitespace() {
      if let Some(start) = token_start.take() {
        output.push_str(&map_token(&value[start..index]));
      }
      output.push(ch);
    } else if token_start.is_none() {
      token_start = Some(index);
    }
  }

  if let Some(start) = token_start {
    output.push_str(&map_token(&value[start..]));
  }
}

fn redact_assignment_token(token: &str, config: &RedactionConfig) -> String {
  for delimiter in ['=', ':'] {
    if let Some(index) = token.find(delimiter) {
      let (key, value_with_delimiter) = token.split_at(index);
      let key = key
        .trim_start_matches('-')
        .trim_matches(|ch| matches!(ch, '"' | '\'' | '{' | '['));
      if is_sensitive_key(key, config) {
        let value = &value_with_delimiter[delimiter.len_utf8()..];
        let suffix = value
          .chars()
          .rev()
          .take_while(|ch| matches!(ch, ',' | ';' | ')' | ']' | '}'))
          .collect::<String>()
          .chars()
          .rev()
          .collect::<String>();
        return format!("{key}{delimiter}{}{}", config.replacement, suffix);
      }
    }
  }

  token.to_string()
}

fn is_sensitive_key(key: &str, config: &RedactionConfig) -> bool {
  let normalized = normalize_key(key);
  if is_environment_variable_name_key(&normalized) {
    return false;
  }
  config
    .sensitive_key_patterns
    .iter()
    .map(|pattern| normalize_key(pattern))
    .any(|pattern| normalized.contains(&pattern))
}

fn normalize_key(key: &str) -> String {
  key
    .chars()
    .filter(|ch| ch.is_ascii_alphanumeric())
    .flat_map(|ch| ch.to_lowercase())
    .collect()
}

fn is_environment_variable_name_key(normalized_key: &str) -> bool {
  matches!(
    normalized_key,
    "apikeyenv" | "tokenenv" | "secretenv" | "passwordenv" | "credentialenv"
  )
}

fn default_sensitive_key_patterns() -> Vec<String> {
  [
    "api_key",
    "apikey",
    "authorization",
    "auth_token",
    "bearer_token",
    "credential",
    "env_secret",
    "password",
    "private_key",
    "secret",
    "session_token",
    "token",
    "x_api_key",
  ]
  .into_iter()
  .map(ToString::to_string)
  .collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::types::{AgentTrace, ExecutionTrace, NodeTrace, ToolCallTrace};

  #[test]
  fn redacts_nested_sensitive_keys() {
    let mut value = serde_json::json!({
      "headers": {
        "Authorization": "Bearer abc",
        "x-api-key": "secret-key"
      },
      "nested": {
        "credentials": {
          "password": "p@ss"
        }
      },
      "safe": "visible"
    });

    redact_value(&mut value, &RedactionConfig::default());

    assert_eq!(value["headers"]["Authorization"], REDACTED_VALUE);
    assert_eq!(value["headers"]["x-api-key"], REDACTED_VALUE);
    assert_eq!(value["nested"]["credentials"], REDACTED_VALUE);
    assert_eq!(value["safe"], "visible");
  }

  #[test]
  fn keeps_environment_variable_names_visible() {
    let mut value = serde_json::json!({
      "api_key_env": "OPENAI_API_KEY",
      "api_key": "secret-key"
    });

    redact_value(&mut value, &RedactionConfig::default());

    assert_eq!(value["api_key_env"], "OPENAI_API_KEY");
    assert_eq!(value["api_key"], REDACTED_VALUE);
  }

  #[test]
  fn redacts_agent_step_and_tool_call_params() {
    let mut trace = ExecutionTrace::new("wf-redact".to_string());
    let mut node = NodeTrace::new("agent".to_string(), "agent".to_string());
    node.agent_details = Some(AgentTrace {
      session_id: "session-1".to_string(),
      answer: Some("done".to_string()),
      stop_reason: serde_json::json!({"reason": "final_answer"}),
      steps: vec![serde_json::json!({
        "index": 1,
        "kind": {
          "type": "tool_call",
          "tool": "http",
          "params": {"url": "https://example.test", "api_key": "abc"}
        }
      })],
      events: vec![serde_json::json!({
        "event": "tool_call_completed",
        "env_secret": "hidden"
      })],
      tool_calls: vec![ToolCallTrace {
        tool: "http".to_string(),
        params: Some(serde_json::json!({
          "headers": {"Authorization": "Bearer abc"}
        })),
        is_error: Some(false),
        duration_ms: Some(10),
        is_mcp: false,
      }],
    });
    trace.nodes.push(node);

    redact_trace(&mut trace, &RedactionConfig::default());

    let agent = trace.nodes[0].agent_details.as_ref().unwrap();
    assert_eq!(
      agent.steps[0]["kind"]["params"]["api_key"],
      serde_json::json!(REDACTED_VALUE)
    );
    assert_eq!(
      agent.events[0]["env_secret"],
      serde_json::json!(REDACTED_VALUE)
    );
    assert_eq!(
      agent.tool_calls[0].params.as_ref().unwrap()["headers"]["Authorization"],
      serde_json::json!(REDACTED_VALUE)
    );
  }

  #[test]
  fn redacts_sensitive_plain_text_fragments() {
    let redacted = redact_text(
      "call --api-key=abc Authorization: Bearer secret TOKEN:xyz safe=value",
      &RedactionConfig::default(),
    );

    assert!(redacted.contains("api-key=[REDACTED]"));
    assert!(redacted.contains("Bearer [REDACTED]"));
    assert!(redacted.contains("TOKEN:[REDACTED]"));
    assert!(redacted.contains("safe=value"));
    assert!(!redacted.contains("abc"));
    assert!(!redacted.contains("secret"));
    assert!(!redacted.contains("xyz"));
  }
}

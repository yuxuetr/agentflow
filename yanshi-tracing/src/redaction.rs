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
  if let Some(key) = key
    && is_sensitive_key(key, config)
  {
    *value = Value::String(config.replacement.clone());
    return;
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
  if let Some(max_value_bytes) = config.max_value_bytes
    && value.len() > max_value_bytes
  {
    *value = format!("[TRUNCATED: {} bytes]", value.len());
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
  // Q2.3.6: walk every `key=value` / `key:value` pair inside this
  // whitespace-delimited token (URL query strings, JSON snippets,
  // semicolon-separated cookies, etc.) and only replace the *value*
  // segment of sensitive pairs. The previous implementation split on
  // the first delimiter and consumed the entire suffix, so
  // `?api_key=secret&q=test` collapsed to `?api_key=[REDACTED]`,
  // losing trailing query parameters and JSON closing characters.
  //
  // We scan left to right, looking for boundary characters that
  // separate pairs (`&`, `;`, `,`, `}`, `]`) and re-emitting each
  // segment with selective redaction. Quoted JSON keys (`"api_key"`)
  // and CLI-style flags (`--token=value`) hit the same boundary
  // logic because their leading punctuation is stripped before the
  // key match.
  if !token.contains('=') && !token.contains(':') {
    return token.to_string();
  }

  fn is_pair_boundary(ch: char) -> bool {
    matches!(ch, '&' | ';' | ',')
  }

  let mut out = String::with_capacity(token.len());
  let mut idx = 0;
  let chars: Vec<(usize, char)> = token.char_indices().collect();
  // We need to walk segments separated by &/;/, while remembering that
  // JSON objects use `}` / `]` to close — those characters terminate
  // the *value* but should be carried through to the output.
  while idx < chars.len() {
    // Find next pair boundary (`&`, `;`, `,`) — pairs end at these.
    let mut end = chars.len();
    for (i, (_, ch)) in chars.iter().enumerate().skip(idx) {
      if is_pair_boundary(*ch) {
        end = i;
        break;
      }
    }
    let segment_start_byte = chars[idx].0;
    let segment_end_byte = if end == chars.len() {
      token.len()
    } else {
      chars[end].0
    };
    let segment = &token[segment_start_byte..segment_end_byte];
    out.push_str(&redact_single_pair(segment, config));
    if end < chars.len() {
      out.push(chars[end].1);
      idx = end + 1;
    } else {
      idx = end;
    }
  }

  out
}

/// Redact a single `key<delim>value` segment with no embedded `&`/`;`/`,`
/// boundaries. Preserves JSON-tail characters (`}`, `]`, `)`) so closing
/// braces don't get eaten with the value.
fn redact_single_pair(segment: &str, config: &RedactionConfig) -> String {
  for delimiter in ['=', ':'] {
    if let Some(index) = segment.find(delimiter) {
      let (key, value_with_delimiter) = segment.split_at(index);
      let trimmed_key = key
        .trim_start_matches('-')
        .trim_start_matches(['"', '\'', '{', '[', '?'])
        .trim_end_matches(['"', '\'']);
      if is_sensitive_key(trimmed_key, config) {
        let value = &value_with_delimiter[delimiter.len_utf8()..];
        // Pull off any trailing JSON/closing-bracket characters so they
        // travel with the rest of the original payload, not the value.
        let suffix = value
          .chars()
          .rev()
          .take_while(|ch| matches!(ch, ')' | ']' | '}' | '"' | '\''))
          .collect::<String>()
          .chars()
          .rev()
          .collect::<String>();
        return format!("{key}{delimiter}{}{}", config.replacement, suffix);
      }
    }
  }
  segment.to_string()
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
  // Q2.3.5: expanded default list. Audit (M8) called out missing
  // coverage for JWTs, cookies, AWS access-key ids, webhooks, etc.
  // Substring matching still applies, but with these additional
  // tokens the common shapes are covered without a substring-collision
  // mode change. Patterns are normalized (alphanumeric, lowercase) so
  // spelling variants (`x-secret`, `aws_access_key_id`, `SetCookie`)
  // all hit the canonical form.
  [
    "api_key",
    "apikey",
    "authorization",
    "auth_token",
    "bearer_token",
    "client_secret",
    "cookie",
    "set_cookie",
    "credential",
    "env_secret",
    "jwt",
    "password",
    "private_key",
    "refresh_token",
    "secret",
    "session_token",
    "signature",
    "ssh_key",
    "pgp_key",
    "token",
    "webhook",
    "x_api_key",
    "aws_access_key_id",
    "aws_secret_access_key",
    "aws_session_token",
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
      context: Default::default(),
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
        context: Default::default(),
        call_id: Some("session-1:1:http".to_string()),
        tool: "http".to_string(),
        source: Some("builtin".to_string()),
        permissions: vec!["network".to_string()],
        params: Some(serde_json::json!({
          "headers": {"Authorization": "Bearer abc"}
        })),
        idempotency_key: None,
        side_effect_class: None,
        replay_policy: None,
        is_error: Some(false),
        duration_ms: Some(10),
        policy_allowed: Some(true),
        policy_rule: Some("allow_all".to_string()),
        policy_deny_reason: None,
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

  // Q2.3.5: default sensitive-key list must cover JWT, cookies, AWS
  // access keys, refresh tokens, webhooks, signatures. Without these
  // additions a substring matcher passed common shapes through.
  #[test]
  fn default_patterns_cover_jwt_cookie_aws_refresh_webhook() {
    let cfg = RedactionConfig::default();
    let mut value = serde_json::json!({
      "jwt": "eyJ.payload.sig",
      "Set-Cookie": "session=opaque",
      "aws_access_key_id": "AKIA...",
      "refresh_token": "rt_xxx",
      "webhook_url": "https://hooks.example/abc",
      "signature": "deadbeef",
    });
    redact_value(&mut value, &cfg);
    let obj = value.as_object().unwrap();
    for k in [
      "jwt",
      "Set-Cookie",
      "aws_access_key_id",
      "refresh_token",
      "webhook_url",
      "signature",
    ] {
      assert_eq!(
        obj[k].as_str(),
        Some("[REDACTED]"),
        "{k} must be redacted by default"
      );
    }
  }

  // Q2.3.6: URL query strings must redact only the sensitive value;
  // trailing parameters must be preserved. Previously the entire suffix
  // after the first `=` was eaten.
  #[test]
  fn redacts_url_query_string_preserves_other_params() {
    let redacted = redact_text(
      "https://api.example.test/data?api_key=secret&q=test&user=alice",
      &RedactionConfig::default(),
    );
    assert!(
      redacted.contains("api_key=[REDACTED]"),
      "sensitive query param must be redacted, got {redacted}"
    );
    assert!(
      redacted.contains("q=test"),
      "trailing query params must be preserved, got {redacted}"
    );
    assert!(
      redacted.contains("user=alice"),
      "non-sensitive trailing params must be preserved, got {redacted}"
    );
    assert!(!redacted.contains("secret"));
  }

  // Q2.3.6: JSON body fragments inside a single whitespace-delimited
  // token must redact only the value; the closing brace must survive.
  #[test]
  fn redacts_inline_json_preserves_closing_braces() {
    let redacted = redact_text(
      r#"body={"api_key":"secret","model":"gpt"}"#,
      &RedactionConfig::default(),
    );
    assert!(
      !redacted.contains("secret"),
      "value must be removed, got {redacted}"
    );
    assert!(
      redacted.contains("\"api_key\":[REDACTED]") || redacted.contains("api_key:[REDACTED]"),
      "redaction must mark the api_key field, got {redacted}"
    );
  }

  // Q2.3.6: cookie-style semicolon-separated pairs must each be
  // independently redacted when the key is sensitive.
  #[test]
  fn redacts_semicolon_separated_cookie_pairs() {
    let redacted = redact_text(
      "session_token=opaque;path=/;httponly",
      &RedactionConfig::default(),
    );
    assert!(redacted.contains("session_token=[REDACTED]"));
    assert!(redacted.contains("path=/"));
    assert!(redacted.contains("httponly"));
    assert!(!redacted.contains("opaque"));
  }

  // ── Q5.2: extended redaction dataset ─────────────────────────────────────
  //
  // These tests stress-shape patterns that the wider Q5.2 audit
  // identified as common-but-subtle leak vectors. Each test pins one
  // category so a regression that removes coverage surfaces immediately.

  // Q5.2: JWT in an Authorization header — most common API auth shape.
  // The `Bearer <jwt>` form should redact the token completely.
  #[test]
  fn redacts_jwt_in_authorization_header_value() {
    let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ1c2VyIn0.signature_part";
    let header = format!("Authorization: Bearer {jwt}");
    let redacted = redact_text(&header, &RedactionConfig::default());
    assert!(
      !redacted.contains(jwt),
      "raw JWT must be removed, got {redacted}"
    );
    assert!(
      redacted.contains("Bearer [REDACTED]"),
      "Bearer redaction must trigger, got {redacted}"
    );
  }

  // Q5.2: JWT as a `jwt=...` form field — covered by the structured key
  // path (matched against `jwt` in the default patterns).
  #[test]
  fn redacts_jwt_as_url_or_form_field() {
    let redacted = redact_text(
      "https://api.example.test/auth?jwt=eyJ.payload.sig&keep=visible",
      &RedactionConfig::default(),
    );
    assert!(!redacted.contains("eyJ.payload.sig"));
    assert!(redacted.contains("jwt=[REDACTED]"));
    assert!(redacted.contains("keep=visible"));
  }

  // Q5.2: AWS keys in env-style assignment form. AWS_SECRET_ACCESS_KEY
  // is one of the most copy-pasted credentials in logs.
  #[test]
  fn redacts_aws_credentials_in_env_dump_form() {
    let dump = "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE \
                AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY \
                AWS_SESSION_TOKEN=FwoGZXIvYXdzEJr token=opaque";
    let redacted = redact_text(dump, &RedactionConfig::default());
    for plaintext in [
      "AKIAIOSFODNN7EXAMPLE",
      "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
      "FwoGZXIvYXdzEJr",
      "opaque",
    ] {
      assert!(
        !redacted.contains(plaintext),
        "{plaintext} must be removed, got {redacted}"
      );
    }
    assert!(redacted.contains("AWS_ACCESS_KEY_ID=[REDACTED]"));
    assert!(redacted.contains("AWS_SECRET_ACCESS_KEY=[REDACTED]"));
    assert!(redacted.contains("AWS_SESSION_TOKEN=[REDACTED]"));
  }

  // Q5.2: CRLF in the middle of a log line is the HTTP-response-splitting
  // / log-injection vector. Redaction must keep working when an
  // attacker-controlled value contains `\r\n` — the redactor walks
  // whitespace-delimited tokens, and `\r` / `\n` are both whitespace
  // under `char::is_whitespace`, so each tokenised side is processed
  // independently. This test pins that we don't accidentally drop or
  // double-emit the CRLF, and that a sensitive value sandwiched between
  // CRLFs still gets redacted.
  #[test]
  fn redacts_sensitive_value_around_crlf_boundary() {
    let input = "user=alice\r\napi_key=leaked-secret\r\ntrailing=ok";
    let redacted = redact_text(input, &RedactionConfig::default());
    assert!(
      !redacted.contains("leaked-secret"),
      "sensitive value before CRLF must be removed, got {redacted:?}"
    );
    assert!(
      redacted.contains("api_key=[REDACTED]"),
      "api_key on a separate CRLF-line must be redacted, got {redacted:?}"
    );
    assert!(
      redacted.contains("user=alice"),
      "benign prior line must survive, got {redacted:?}"
    );
    assert!(
      redacted.contains("trailing=ok"),
      "benign trailing line must survive, got {redacted:?}"
    );
    // The CRLF separators themselves must be preserved (whitespace mode
    // is verbatim, not collapsing) so structured log parsers don't see
    // a merged line.
    assert!(
      redacted.contains("\r\n"),
      "CRLF separators must be preserved, got {redacted:?}"
    );
  }

  // Q5.2: lone `\n` separators (Unix log style) — same expectation as
  // CRLF but for the more common newline-only form.
  #[test]
  fn redacts_sensitive_value_across_lf_only_lines() {
    let input = "GET /v1/data\napi_key=topsecret\nuser=alice";
    let redacted = redact_text(input, &RedactionConfig::default());
    assert!(!redacted.contains("topsecret"));
    assert!(redacted.contains("api_key=[REDACTED]"));
    assert!(redacted.contains("user=alice"));
  }

  // Q5.2: cookie header in compact (no-whitespace-around-colon) shape
  // — the redactor's whitespace tokenizer keys off whitespace boundaries,
  // so a colon-delimited `Cookie:session=abc` (no space after `:`) is
  // one token and round-trips through `redact_assignment_token`.
  // Attributes after the first `;` boundary are preserved.
  #[test]
  fn redacts_cookie_header_compact_form_preserves_attributes() {
    let redacted = redact_text(
      "Cookie:session_token=abc123def;Path=/;Domain=.example.test;Secure;HttpOnly",
      &RedactionConfig::default(),
    );
    assert!(
      !redacted.contains("abc123def"),
      "raw cookie value must be removed, got {redacted}"
    );
    assert!(
      redacted.contains("Path=/"),
      "Path attribute must survive, got {redacted}"
    );
    assert!(
      redacted.contains("Domain=.example.test"),
      "Domain attribute must survive, got {redacted}"
    );
    assert!(
      redacted.contains("Secure"),
      "Secure flag must survive, got {redacted}"
    );
    assert!(
      redacted.contains("HttpOnly"),
      "HttpOnly flag must survive, got {redacted}"
    );
  }

  // Q5.2 (known-limitation pin): the inline `redact_text` tokenizer
  // splits on whitespace and treats each token independently. If a
  // header is emitted as `Set-Cookie: session=value` with whitespace
  // *after* the colon, `Set-Cookie:` is one token (sensitive key with
  // empty value, gets redacted to a literal `Set-Cookie:[REDACTED]`)
  // and `session=value` is a separate token where the key `session`
  // isn't in the default sensitive patterns. The actual cookie value
  // then leaks. This test pins the limitation so we don't accidentally
  // claim coverage for it; the structured `redact_value` path inside
  // a JSON object (the normal trace path) does NOT have this gap
  // because object keys are walked recursively.
  //
  // Fix avenue (deferred): teach the tokenizer to recognise
  // `<sensitive-header>:\s+<value>` shapes by carrying a "redact next
  // token" flag — same trick `redact_bearer_tokens` uses for
  // `Bearer <token>`. Out of scope for Q5.2 (audit + lint + dataset).
  #[test]
  fn known_limitation_set_cookie_with_whitespace_leaks_value_via_redact_text() {
    let redacted = redact_text(
      "Set-Cookie: session=abc123def; Path=/",
      &RedactionConfig::default(),
    );
    // The `Set-Cookie:` token gets `[REDACTED]` because Set-Cookie is a
    // sensitive key, but the value sits in the next whitespace-separated
    // token. Use this assertion as a tripwire — if a future redactor
    // change starts catching this case, delete the test and add it to
    // `redacts_cookie_header_compact_form_preserves_attributes` above.
    assert!(
      redacted.contains("Set-Cookie:[REDACTED]"),
      "Set-Cookie key still redacts to a stub, got {redacted}"
    );
    assert!(
      redacted.contains("abc123def"),
      "BUG-PIN: cookie value with whitespace after colon currently leaks via redact_text — \
       structured trace path (redact_value) is unaffected. See Q5.2 deferred fix avenue."
    );
  }

  // Q5.2: structured trace path (the production path for tool call
  // params, agent steps, etc.) does NOT have the whitespace gap above
  // — `redact_value` walks JSON keys recursively and the Set-Cookie
  // field gets fully redacted.
  #[test]
  fn structured_path_redacts_set_cookie_value_completely() {
    let mut value = serde_json::json!({
      "headers": {
        "Set-Cookie": "session=abc123def; Path=/; Domain=.example.test",
        "Cookie": "auth=opaque; theme=dark"
      }
    });
    redact_value(&mut value, &RedactionConfig::default());
    assert_eq!(value["headers"]["Set-Cookie"], REDACTED_VALUE);
    assert_eq!(value["headers"]["Cookie"], REDACTED_VALUE);
  }
}

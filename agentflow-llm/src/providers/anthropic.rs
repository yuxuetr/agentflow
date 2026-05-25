use crate::{
  LLMError, Result,
  client::streaming::{StreamChunk, StreamingResponse, ToolCallDelta},
  providers::{ContentType, LLMProvider, ProviderRequest, ProviderResponse},
  thinking::ThinkingConfig,
  tool_calling::{StopReason, ToolCallRequest, ToolChoice, ToolSpec},
};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::pin::Pin;
use tokio_stream::Stream;

pub struct AnthropicProvider {
  client: Client,
  api_key: String,
  base_url: String,
}

impl AnthropicProvider {
  pub fn new(api_key: &str, base_url: Option<String>) -> Result<Self> {
    Self::with_client(super::default_http_client()?, api_key, base_url)
  }

  /// Construct with a caller-supplied [`reqwest::Client`].
  ///
  /// Useful when callers need a non-default client — for example tests that
  /// must disable the system proxy (`.no_proxy()` on the builder) to reach a
  /// localhost mock, or production deployments that share one HTTPS-pinned
  /// client across providers.
  pub fn with_client(client: Client, api_key: &str, base_url: Option<String>) -> Result<Self> {
    if api_key.is_empty() {
      return Err(LLMError::MissingApiKey {
        provider: "anthropic".to_string(),
      });
    }

    let base_url = base_url.unwrap_or_else(|| "https://api.anthropic.com".to_string());

    Ok(Self {
      client,
      api_key: api_key.to_string(),
      base_url,
    })
  }

  fn build_headers(&self) -> Result<reqwest::header::HeaderMap> {
    use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    // Q2.5.3: pre-fix this called `.expect("API key contains invalid
    // characters")` and panicked the entire runtime if a `.env` file
    // ended the value with a stray `\n`. Surface as
    // `ConfigurationError` so callers can degrade gracefully.
    headers.insert(
      "x-api-key",
      HeaderValue::from_str(&self.api_key).map_err(|err| LLMError::ConfigurationError {
        message: format!("Anthropic API key contains invalid characters: {err}"),
      })?,
    );
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
    crate::trace_context::inject_into_headers(&mut headers);
    Ok(headers)
  }

  fn build_request_body(&self, request: &ProviderRequest) -> Value {
    // Convert OpenAI-style messages to Anthropic format
    let mut system_message = None;
    let mut anthropic_messages = Vec::new();

    for message in &request.messages {
      if let Some(msg_obj) = message.as_object()
        && let (Some(role), Some(content)) = (msg_obj.get("role"), msg_obj.get("content"))
      {
        match role.as_str() {
          Some("system") => {
            system_message = content.as_str().map(|s| s.to_string());
          }
          Some("user") | Some("assistant") => {
            anthropic_messages.push(json!({
              "role": role,
              "content": content
            }));
          }
          _ => {}
        }
      }
    }

    let mut body = json!({
      "model": request.model,
      "messages": anthropic_messages,
      "stream": request.stream
    });

    if let Some(system) = system_message {
      body["system"] = json!(system);
    }

    // Add additional parameters
    for (key, value) in &request.parameters {
      match key.as_str() {
        "max_tokens" => body["max_tokens"] = value.clone(),
        "temperature" => body["temperature"] = value.clone(),
        "top_p" => body["top_p"] = value.clone(),
        "top_k" => body["top_k"] = value.clone(),
        _ => {
          // Store other parameters in metadata for now
        }
      }
    }

    // Anthropic requires max_tokens to be specified
    if !body.as_object().unwrap().contains_key("max_tokens") {
      body["max_tokens"] = json!(4096);
    }

    if let Some(tools) = &request.tools {
      body["tools"] = Value::Array(tools.iter().map(tool_spec_to_anthropic_value).collect());
    }
    if let Some(choice) = &request.tool_choice {
      body["tool_choice"] = tool_choice_to_anthropic_value(choice);
    }

    if let Some(thinking) = &request.thinking
      && let Some(block) = thinking_config_to_anthropic_value(thinking)
    {
      body["thinking"] = block;
    }

    body
  }
}

/// Encode a [`ThinkingConfig`] as Anthropic's `thinking` request block.
///
/// Anthropic only accepts `{ type: "enabled", budget_tokens: N }` or
/// `{ type: "disabled" }`. `Auto` maps to `enabled` with the model's
/// default budget — Anthropic uses 5000 tokens when none is specified
/// for Claude 3.7+, but we send an explicit baseline of 4096 so behaviour
/// is reproducible across `Auto` and `Medium`.
pub(crate) fn thinking_config_to_anthropic_value(config: &ThinkingConfig) -> Option<Value> {
  if config.is_disabled() {
    return Some(json!({ "type": "disabled" }));
  }
  let budget = config.to_token_budget().unwrap_or(4096);
  Some(json!({
    "type": "enabled",
    "budget_tokens": budget,
  }))
}

/// Encode a `ToolSpec` as Anthropic's `{ name, description, input_schema }`.
pub(crate) fn tool_spec_to_anthropic_value(spec: &ToolSpec) -> Value {
  json!({
    "name": spec.name,
    "description": spec.description,
    "input_schema": spec.parameters,
  })
}

/// Encode `ToolChoice` as Anthropic's `tool_choice` object.
pub(crate) fn tool_choice_to_anthropic_value(choice: &ToolChoice) -> Value {
  match choice {
    ToolChoice::Auto => json!({"type": "auto"}),
    // Anthropic uses `none` since 2024-11; older revisions reject it. The
    // value is sent verbatim because callers opting into ToolChoice::None
    // know what they're doing.
    ToolChoice::None => json!({"type": "none"}),
    // Anthropic spells this `any` (model must call at least one tool).
    ToolChoice::Required => json!({"type": "any"}),
    ToolChoice::Tool { name } => json!({"type": "tool", "name": name}),
  }
}

/// Pull `tool_use` content blocks out of an Anthropic response and convert
/// them to typed `ToolCallRequest`s.
pub(crate) fn parse_anthropic_tool_use_blocks(
  content: &[AnthropicContent],
) -> Vec<ToolCallRequest> {
  content
    .iter()
    .filter_map(|block| match block {
      AnthropicContent::ToolUse { id, name, input } => Some(ToolCallRequest {
        id: id.clone(),
        name: name.clone(),
        arguments: input.clone(),
      }),
      AnthropicContent::Text { .. } | AnthropicContent::Thinking { .. } => None,
    })
    .collect()
}

/// Concatenate the text from every `thinking` block in an Anthropic
/// response. Returns `None` when no thinking blocks were emitted (the
/// caller's thinking config may have been disabled, the model may not
/// have used the budget, or thinking simply wasn't requested).
pub(crate) fn parse_anthropic_thinking_blocks(content: &[AnthropicContent]) -> Option<String> {
  let joined: String = content
    .iter()
    .filter_map(|block| match block {
      AnthropicContent::Thinking { thinking, .. } => Some(thinking.as_str()),
      AnthropicContent::Text { .. } | AnthropicContent::ToolUse { .. } => None,
    })
    .collect::<Vec<_>>()
    .join("");
  if joined.is_empty() {
    None
  } else {
    Some(joined)
  }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
  fn name(&self) -> &str {
    "anthropic"
  }

  async fn execute(&self, request: &ProviderRequest) -> Result<ProviderResponse> {
    if request.stream {
      return Err(LLMError::InternalError {
        message: "Use execute_streaming for streaming requests".to_string(),
      });
    }

    let url = format!("{}/v1/messages", self.base_url);
    let body = self.build_request_body(request);

    let response = self
      .client
      .post(&url)
      .headers(self.build_headers()?)
      .json(&body)
      .send()
      .await?;

    if !response.status().is_success() {
      let status_code = response.status().as_u16();
      let error_text = response.text().await.unwrap_or_default();
      return Err(LLMError::HttpError {
        status_code,
        message: error_text,
      });
    }

    let anthropic_response: AnthropicResponse = response.json().await?;

    // Concatenate all text blocks; tool_use blocks are surfaced via
    // `tool_calls` instead of being stringified into content.
    let content_text = anthropic_response
      .content
      .iter()
      .filter_map(|block| match block {
        AnthropicContent::Text { text } => Some(text.as_str()),
        AnthropicContent::ToolUse { .. } | AnthropicContent::Thinking { .. } => None,
      })
      .collect::<Vec<_>>()
      .join("");

    let content = ContentType::Text(content_text);

    let usage = anthropic_response
      .usage
      .clone()
      .map(|u| crate::providers::TokenUsage {
        prompt_tokens: Some(u.input_tokens),
        completion_tokens: Some(u.output_tokens),
        total_tokens: Some(u.input_tokens + u.output_tokens),
      });

    let tool_calls = parse_anthropic_tool_use_blocks(&anthropic_response.content);
    let stop_reason = anthropic_response
      .stop_reason
      .as_deref()
      .map(StopReason::from_anthropic_stop_reason);
    // Anthropic claude-3.7+ emits `type: "thinking"` content blocks
    // alongside `text` blocks when extended thinking is enabled. Surface
    // their concatenated text on the typed channel.
    let thinking = parse_anthropic_thinking_blocks(&anthropic_response.content);

    Ok(ProviderResponse {
      content,
      usage,
      metadata: Some(serde_json::to_value(&anthropic_response)?),
      tool_calls,
      stop_reason,
      thinking,
    })
  }

  async fn execute_streaming(
    &self,
    request: &ProviderRequest,
  ) -> Result<Box<dyn StreamingResponse>> {
    if !request.stream {
      return Err(LLMError::InternalError {
        message: "Streaming not enabled in request".to_string(),
      });
    }

    let url = format!("{}/v1/messages", self.base_url);
    let body = self.build_request_body(request);

    let response = self
      .client
      .post(&url)
      .headers(self.build_headers()?)
      .json(&body)
      .send()
      .await?;

    if !response.status().is_success() {
      let status_code = response.status().as_u16();
      let error_text = response.text().await.unwrap_or_default();
      return Err(LLMError::HttpError {
        status_code,
        message: error_text,
      });
    }

    Ok(Box::new(AnthropicStreamingResponse::new(response)))
  }

  async fn validate_config(&self) -> Result<()> {
    // Simple validation by trying to create a minimal request
    let test_body = json!({
      "model": "claude-3-haiku-20240307",
      "messages": [{"role": "user", "content": "Hi"}],
      "max_tokens": 1
    });

    let url = format!("{}/v1/messages", self.base_url);
    let response = self
      .client
      .post(&url)
      .headers(self.build_headers()?)
      .json(&test_body)
      .send()
      .await?;

    if response.status().as_u16() == 401 {
      return Err(LLMError::AuthenticationError {
        provider: "anthropic".to_string(),
        message: "Invalid API key".to_string(),
      });
    }

    // Any other error is likely a configuration issue, but auth is valid
    Ok(())
  }

  fn base_url(&self) -> &str {
    &self.base_url
  }

  fn supported_models(&self) -> Vec<String> {
    vec![
      "claude-3-5-sonnet-20241022".to_string(),
      "claude-3-5-sonnet-20240620".to_string(),
      "claude-3-5-haiku-20241022".to_string(),
      "claude-3-opus-20240229".to_string(),
      "claude-3-sonnet-20240229".to_string(),
      "claude-3-haiku-20240307".to_string(),
    ]
  }
}

// Anthropic API response structures
#[derive(Debug, Deserialize, Serialize)]
struct AnthropicResponse {
  id: String,
  #[serde(rename = "type")]
  type_field: String,
  role: String,
  content: Vec<AnthropicContent>,
  model: String,
  stop_reason: Option<String>,
  stop_sequence: Option<String>,
  usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub(crate) enum AnthropicContent {
  #[serde(rename = "text")]
  Text { text: String },
  #[serde(rename = "tool_use")]
  ToolUse {
    id: String,
    name: String,
    input: Value,
  },
  /// Extended-thinking block emitted by Claude 3.7+ when `thinking: {
  /// type: "enabled" }` is set on the request. Carries the model's chain
  /// of thought; the `signature` is opaque and used by Anthropic for
  /// integrity verification on multi-turn replays. We don't act on the
  /// signature today but accept the field so deserialisation succeeds.
  #[serde(rename = "thinking")]
  Thinking {
    thinking: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signature: Option<String>,
  },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct AnthropicUsage {
  input_tokens: u32,
  output_tokens: u32,
}

// Streaming response structures
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct AnthropicStreamingEvent {
  #[serde(rename = "type")]
  event_type: String,
  data: Option<Value>,
}

pub struct AnthropicStreamingResponse {
  stream: Pin<Box<dyn Stream<Item = Result<String>> + Send>>,
  buffer: Option<String>,
  finished: bool,
}

// Make it Send + Sync
// Q2.5.4: `unsafe impl Send + Sync` removed (trait no longer needs Sync).

impl AnthropicStreamingResponse {
  fn new(response: reqwest::Response) -> Self {
    let byte_stream = response.bytes_stream();
    let string_stream = byte_stream.map(|chunk_result| {
      chunk_result
        .map_err(|e| LLMError::StreamingError {
          message: e.to_string(),
        })
        .map(|chunk| String::from_utf8_lossy(&chunk).to_string())
    });

    Self {
      stream: Box::pin(string_stream),
      buffer: Some(String::new()),
      finished: false,
    }
  }

  fn parse_sse_event(line: &str) -> Option<StreamChunk> {
    if line.starts_with("event: ") {
      return None; // Event type line, not data
    }

    if !line.starts_with("data: ") {
      return None;
    }

    let data = &line[6..]; // Remove "data: " prefix

    if let Ok(event) = serde_json::from_str::<Value>(data)
      && let Some(event_type) = event.get("type").and_then(|t| t.as_str())
    {
      match event_type {
        "content_block_start" => {
          // Q2.5.2: tool_use blocks emit their `id` and `name` here, before any
          // `input_json_delta`. Surface them as a ToolCallDelta with empty args
          // so downstream consumers learn the tool_call exists.
          if let Some(block) = event.get("content_block")
            && block.get("type").and_then(|t| t.as_str()) == Some("tool_use")
          {
            let index = event.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
            let id = block.get("id").and_then(|i| i.as_str()).map(String::from);
            let name = block.get("name").and_then(|n| n.as_str()).map(String::from);
            return Some(StreamChunk {
              content: String::new(),
              is_final: false,
              metadata: Some(event.clone()),
              usage: None,
              content_type: Some("tool_use".to_string()),
              tool_call_deltas: vec![ToolCallDelta {
                index,
                id,
                name,
                arguments_delta: None,
              }],
            });
          }
        }
        "content_block_delta" => {
          if let Some(delta) = event.get("delta") {
            let delta_type = delta.get("type").and_then(|t| t.as_str()).unwrap_or("");
            // Q2.5.2: `input_json_delta` carries partial JSON for a tool_use
            // block's `input` field. Concatenate per index downstream.
            if delta_type == "input_json_delta"
              && let Some(partial) = delta.get("partial_json").and_then(|p| p.as_str())
            {
              let index = event.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
              return Some(StreamChunk {
                content: String::new(),
                is_final: false,
                metadata: Some(event.clone()),
                usage: None,
                content_type: Some("tool_use".to_string()),
                tool_call_deltas: vec![ToolCallDelta {
                  index,
                  id: None,
                  name: None,
                  arguments_delta: Some(partial.to_string()),
                }],
              });
            }
            // Existing path: text_delta on a text block.
            if let Some(text) = delta.get("text")
              && let Some(text_str) = text.as_str()
            {
              return Some(StreamChunk {
                content: text_str.to_string(),
                is_final: false,
                metadata: Some(event.clone()),
                usage: None,
                content_type: Some("text".to_string()),
                tool_call_deltas: Vec::new(),
              });
            }
          }
        }
        "message_stop" => {
          // Q2.5.1: Anthropic emits `message_stop` exactly once at the end of the
          // entire response. `content_block_stop` fires after each block (text,
          // tool_use, …) and previously terminated the stream early, dropping
          // every block after the first one. Now only `message_stop` ends it.
          return Some(StreamChunk {
            content: String::new(),
            is_final: true,
            metadata: Some(event.clone()),
            usage: None,
            content_type: Some("text".to_string()),
            tool_call_deltas: Vec::new(),
          });
        }
        _ => {}
      }
    }

    None
  }
}

#[async_trait]
impl StreamingResponse for AnthropicStreamingResponse {
  async fn next_chunk(&mut self) -> Result<Option<StreamChunk>> {
    if self.finished {
      return Ok(None);
    }

    loop {
      match self.stream.next().await {
        Some(Ok(data)) => {
          if let Some(ref mut buffer) = self.buffer {
            buffer.push_str(&data);

            // Process complete lines
            while let Some(newline_pos) = buffer.find('\n') {
              let line = buffer[..newline_pos].trim().to_string();
              buffer.drain(..=newline_pos);

              if !line.is_empty()
                && let Some(chunk) = Self::parse_sse_event(&line)
              {
                if chunk.is_final {
                  self.finished = true;
                }
                return Ok(Some(chunk));
              }
            }
          }
        }
        Some(Err(e)) => return Err(e),
        None => {
          self.finished = true;
          return Ok(None);
        }
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_anthropic_provider_creation() {
    let provider = AnthropicProvider::new("test-key", None);
    assert!(provider.is_ok());

    let provider = AnthropicProvider::new("", None);
    assert!(provider.is_err());
  }

  #[tokio::test]
  async fn build_headers_injects_traceparent_when_scope_active() {
    use crate::trace_context::{LlmTraceContext, scope};

    let provider = AnthropicProvider::new("test-key", None).unwrap();
    let ctx = LlmTraceContext::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331").unwrap();

    let headers = scope(ctx.clone(), async { provider.build_headers() })
      .await
      .expect("headers must build under a well-formed ASCII API key");
    assert_eq!(
      headers.get("traceparent").and_then(|v| v.to_str().ok()),
      Some(ctx.to_traceparent().as_str()),
    );
  }

  /// Q2.5.3 regression: an API key with a stray non-ASCII byte
  /// (typical `.env` mistake — trailing `\n` from copy-paste)
  /// surfaces as `ConfigurationError` instead of panicking.
  #[test]
  fn build_headers_returns_err_on_invalid_api_key() {
    let provider = AnthropicProvider::new("bad\nkey", None).unwrap();
    let err = provider
      .build_headers()
      .expect_err("newline-in-key must fail closed");
    match err {
      LLMError::ConfigurationError { message } => {
        assert!(
          message.contains("invalid characters") || message.contains("API key"),
          "expected configuration error, got: {message}"
        );
      }
      other => panic!("expected ConfigurationError, got {other:?}"),
    }
  }

  #[test]
  fn test_build_request_body() {
    let provider = AnthropicProvider::new("test-key", None).unwrap();

    let mut params = std::collections::HashMap::new();
    params.insert("temperature".to_string(), json!(0.7));
    params.insert("max_tokens".to_string(), json!(100));

    let request = ProviderRequest {
      model: "claude-3-sonnet-20240229".to_string(),
      messages: vec![
        json!({"role": "system", "content": "You are helpful"}),
        json!({"role": "user", "content": "test"}),
      ],
      stream: false,
      parameters: params,
      tools: None,
      tool_choice: None,
      thinking: None,
    };

    let body = provider.build_request_body(&request);
    assert_eq!(body["model"], "claude-3-sonnet-20240229");
    assert_eq!(body["temperature"], 0.7);
    assert_eq!(body["max_tokens"], 100);
    assert_eq!(body["system"], "You are helpful");
    assert_eq!(body["messages"].as_array().unwrap().len(), 1); // Only user message
    assert!(body.get("tools").is_none());
  }

  #[test]
  fn build_request_body_serialises_tools() {
    let provider = AnthropicProvider::new("test-key", None).unwrap();
    let tool = ToolSpec::new(
      "get_weather",
      "Return the weather for a city",
      json!({
        "type": "object",
        "properties": {"city": {"type": "string"}},
        "required": ["city"]
      }),
    );
    let request = ProviderRequest {
      model: "claude-3-5-sonnet-20241022".to_string(),
      messages: vec![json!({"role": "user", "content": "weather?"})],
      stream: false,
      parameters: std::collections::HashMap::new(),
      tools: Some(vec![tool]),
      tool_choice: Some(ToolChoice::Required),
      thinking: None,
    };

    let body = provider.build_request_body(&request);
    let tools = body["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "get_weather");
    assert_eq!(tools[0]["input_schema"]["required"][0], "city");
    // Anthropic spells Required as "any".
    assert_eq!(body["tool_choice"]["type"], "any");
  }

  #[test]
  fn tool_choice_specific_uses_tool_type() {
    let body = tool_choice_to_anthropic_value(&ToolChoice::Tool {
      name: "x".to_string(),
    });
    assert_eq!(body["type"], "tool");
    assert_eq!(body["name"], "x");
  }

  #[test]
  fn parse_anthropic_tool_use_blocks_extracts_calls() {
    let raw = json!({
      "id": "msg_x",
      "type": "message",
      "role": "assistant",
      "model": "claude-3-5-sonnet-20241022",
      "content": [
        {"type": "text", "text": "I'll check the weather."},
        {
          "type": "tool_use",
          "id": "toolu_abc",
          "name": "get_weather",
          "input": {"city": "Tokyo"}
        }
      ],
      "stop_reason": "tool_use",
      "stop_sequence": null,
      "usage": {"input_tokens": 5, "output_tokens": 3}
    });
    let parsed: AnthropicResponse = serde_json::from_value(raw).unwrap();
    assert_eq!(parsed.stop_reason.as_deref(), Some("tool_use"));
    let tool_calls = parse_anthropic_tool_use_blocks(&parsed.content);
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].id, "toolu_abc");
    assert_eq!(tool_calls[0].name, "get_weather");
    assert_eq!(tool_calls[0].arguments["city"], "Tokyo");
  }

  #[test]
  fn parse_anthropic_text_only_returns_no_tool_calls() {
    let content = vec![AnthropicContent::Text {
      text: "hello".to_string(),
    }];
    assert!(parse_anthropic_tool_use_blocks(&content).is_empty());
  }

  // Q2.5.1: `content_block_stop` must NOT terminate the stream; only
  // `message_stop` may. Multi-block responses (text + tool_use) were
  // truncated to the first block before the fix.
  #[test]
  fn streaming_content_block_stop_does_not_finalize() {
    let chunk = AnthropicStreamingResponse::parse_sse_event(
      "data: {\"type\":\"content_block_stop\",\"index\":0}",
    );
    assert!(
      chunk.is_none(),
      "content_block_stop should be ignored, got {chunk:?}"
    );
  }

  #[test]
  fn streaming_message_stop_does_finalize() {
    let chunk =
      AnthropicStreamingResponse::parse_sse_event("data: {\"type\":\"message_stop\"}").unwrap();
    assert!(chunk.is_final);
  }

  // Q2.5.2: tool_use blocks stream their identity via `content_block_start`
  // and their JSON arguments via `input_json_delta` chunks. The parser must
  // surface both as `ToolCallDelta` entries so concatenated `arguments_delta`
  // values reconstruct the full argument JSON.
  #[test]
  fn streaming_tool_use_block_start_emits_id_and_name() {
    let chunk = AnthropicStreamingResponse::parse_sse_event(
      "data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_abc\",\"name\":\"get_weather\",\"input\":{}}}",
    ).unwrap();
    assert_eq!(chunk.tool_call_deltas.len(), 1);
    let delta = &chunk.tool_call_deltas[0];
    assert_eq!(delta.index, 1);
    assert_eq!(delta.id.as_deref(), Some("toolu_abc"));
    assert_eq!(delta.name.as_deref(), Some("get_weather"));
    assert!(delta.arguments_delta.is_none());
    assert!(!chunk.is_final);
  }

  #[test]
  fn streaming_input_json_delta_emits_arguments_fragment() {
    let chunk = AnthropicStreamingResponse::parse_sse_event(
      "data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"city\\\":\"}}",
    ).unwrap();
    assert_eq!(chunk.tool_call_deltas.len(), 1);
    let delta = &chunk.tool_call_deltas[0];
    assert_eq!(delta.index, 1);
    assert_eq!(delta.arguments_delta.as_deref(), Some("{\"city\":"));
    assert!(delta.id.is_none());
    assert!(delta.name.is_none());
  }

  // ----- Thinking serialisation + parse coverage -----

  /// `ThinkingConfig::Medium` must be emitted as Anthropic's typed
  /// `thinking: { type: "enabled", budget_tokens: 4096 }` block on the
  /// request body — bypassing the parameters whitelist that would
  /// otherwise silently drop it.
  #[test]
  fn build_request_body_serialises_thinking_medium() {
    let provider = AnthropicProvider::new("test-key", None).unwrap();
    let request = ProviderRequest {
      model: "claude-3-7-sonnet-20250219".to_string(),
      messages: vec![json!({"role": "user", "content": "reason"})],
      stream: false,
      parameters: std::collections::HashMap::new(),
      tools: None,
      tool_choice: None,
      thinking: Some(ThinkingConfig::Medium),
    };
    let body = provider.build_request_body(&request);
    assert_eq!(body["thinking"]["type"], "enabled");
    assert_eq!(body["thinking"]["budget_tokens"], 4096);
  }

  #[test]
  fn build_request_body_thinking_disabled_emits_disabled_type() {
    let provider = AnthropicProvider::new("test-key", None).unwrap();
    let request = ProviderRequest {
      model: "claude-3-7-sonnet-20250219".to_string(),
      messages: vec![json!({"role": "user", "content": "no thinking"})],
      stream: false,
      parameters: std::collections::HashMap::new(),
      tools: None,
      tool_choice: None,
      thinking: Some(ThinkingConfig::Disabled),
    };
    let body = provider.build_request_body(&request);
    assert_eq!(body["thinking"]["type"], "disabled");
    assert!(body["thinking"].get("budget_tokens").is_none());
  }

  #[test]
  fn build_request_body_no_thinking_omits_the_block() {
    let provider = AnthropicProvider::new("test-key", None).unwrap();
    let request = ProviderRequest {
      model: "claude-3-5-sonnet-20241022".to_string(),
      messages: vec![json!({"role": "user", "content": "hi"})],
      stream: false,
      parameters: std::collections::HashMap::new(),
      tools: None,
      tool_choice: None,
      thinking: None,
    };
    let body = provider.build_request_body(&request);
    assert!(
      body.get("thinking").is_none(),
      "no thinking config → no `thinking` key on the wire body"
    );
  }

  #[test]
  fn parse_anthropic_thinking_blocks_concatenates_when_present() {
    let raw = json!({
      "id": "msg_x",
      "type": "message",
      "role": "assistant",
      "model": "claude-sonnet-4-20250514",
      "content": [
        {"type": "thinking", "thinking": "Step 1: ", "signature": "sig_abc"},
        {"type": "thinking", "thinking": "Step 2 done."},
        {"type": "text", "text": "Final answer."}
      ],
      "stop_reason": "end_turn",
      "stop_sequence": null,
      "usage": {"input_tokens": 12, "output_tokens": 8}
    });
    let parsed: AnthropicResponse = serde_json::from_value(raw).unwrap();
    let thinking = parse_anthropic_thinking_blocks(&parsed.content);
    assert_eq!(thinking.as_deref(), Some("Step 1: Step 2 done."));
  }

  #[test]
  fn parse_anthropic_thinking_blocks_returns_none_when_absent() {
    let content = vec![AnthropicContent::Text {
      text: "no reasoning here".to_string(),
    }];
    assert!(parse_anthropic_thinking_blocks(&content).is_none());
  }

  #[test]
  fn streaming_text_delta_remains_unaffected() {
    let chunk = AnthropicStreamingResponse::parse_sse_event(
      "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}",
    ).unwrap();
    assert_eq!(chunk.content, "hi");
    assert!(chunk.tool_call_deltas.is_empty());
  }
}

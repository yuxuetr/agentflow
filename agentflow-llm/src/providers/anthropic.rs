use crate::{
  LLMError, Result,
  client::streaming::{StreamChunk, StreamingResponse},
  providers::{ContentType, LLMProvider, ProviderRequest, ProviderResponse},
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
    Self::with_client(Client::new(), api_key, base_url)
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

  fn build_headers(&self) -> reqwest::header::HeaderMap {
    use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    // Note: API key is validated in new(), so this should always succeed
    headers.insert(
      "x-api-key",
      HeaderValue::from_str(&self.api_key).expect("API key contains invalid characters"),
    );
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
    crate::trace_context::inject_into_headers(&mut headers);
    headers
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

    body
  }
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
      AnthropicContent::Text { .. } => None,
    })
    .collect()
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
      .headers(self.build_headers())
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
        AnthropicContent::ToolUse { .. } => None,
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

    Ok(ProviderResponse {
      content,
      usage,
      metadata: Some(serde_json::to_value(&anthropic_response)?),
      tool_calls,
      stop_reason,
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
      .headers(self.build_headers())
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
      .headers(self.build_headers())
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
unsafe impl Send for AnthropicStreamingResponse {}
unsafe impl Sync for AnthropicStreamingResponse {}

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
        "content_block_delta" => {
          if let Some(delta) = event.get("delta")
            && let Some(text) = delta.get("text")
            && let Some(text_str) = text.as_str()
          {
            return Some(StreamChunk {
              content: text_str.to_string(),
              is_final: false,
              metadata: Some(event.clone()),
              usage: None,
              content_type: Some("text".to_string()),
            });
          }
        }
        "message_stop" => {
          return Some(StreamChunk {
            content: String::new(),
            is_final: true,
            metadata: Some(event.clone()),
            usage: None,
            content_type: Some("text".to_string()),
          });
        }
        "content_block_stop" => {
          return Some(StreamChunk {
            content: String::new(),
            is_final: true,
            metadata: Some(event.clone()),
            usage: None,
            content_type: Some("text".to_string()),
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
    let ctx = LlmTraceContext::new(
      "0af7651916cd43dd8448eb211c80319c",
      "b7ad6b7169203331",
    )
    .unwrap();

    let headers = scope(ctx.clone(), async { provider.build_headers() }).await;
    assert_eq!(
      headers.get("traceparent").and_then(|v| v.to_str().ok()),
      Some(ctx.to_traceparent().as_str()),
    );
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
}

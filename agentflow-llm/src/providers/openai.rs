use crate::{
  LLMError, Result,
  client::streaming::{StreamChunk, StreamingResponse, TokenUsage},
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

pub struct OpenAIProvider {
  client: Client,
  api_key: String,
  base_url: String,
}

impl OpenAIProvider {
  pub fn new(api_key: &str, base_url: Option<String>) -> Result<Self> {
    if api_key.is_empty() {
      return Err(LLMError::MissingApiKey {
        provider: "openai".to_string(),
      });
    }

    let client = Client::new();
    let base_url = base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string());

    Ok(Self {
      client,
      api_key: api_key.to_string(),
      base_url,
    })
  }

  fn build_headers(&self) -> reqwest::header::HeaderMap {
    use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    // Note: API key is validated in new(), so this should always succeed
    headers.insert(
      AUTHORIZATION,
      HeaderValue::from_str(&format!("Bearer {}", self.api_key))
        .expect("API key contains invalid characters"),
    );
    crate::trace_context::inject_into_headers(&mut headers);
    headers
  }

  fn build_request_body(&self, request: &ProviderRequest) -> Value {
    let mut body = json!({
      "model": request.model,
      "messages": request.messages,
      "stream": request.stream
    });

    // Add additional parameters
    for (key, value) in &request.parameters {
      body[key] = value.clone();
    }

    if let Some(tools) = &request.tools {
      body["tools"] = Value::Array(tools.iter().map(tool_spec_to_openai_value).collect());
    }
    if let Some(choice) = &request.tool_choice {
      body["tool_choice"] = tool_choice_to_openai_value(choice);
    }

    body
  }
}

/// Encode a `ToolSpec` as the OpenAI `{ "type": "function", "function": ... }`
/// wire format.
pub(crate) fn tool_spec_to_openai_value(spec: &ToolSpec) -> Value {
  json!({
    "type": "function",
    "function": {
      "name": spec.name,
      "description": spec.description,
      "parameters": spec.parameters,
    }
  })
}

/// Encode a `ToolChoice` as the OpenAI `tool_choice` wire format.
pub(crate) fn tool_choice_to_openai_value(choice: &ToolChoice) -> Value {
  match choice {
    ToolChoice::Auto => Value::String("auto".to_string()),
    ToolChoice::None => Value::String("none".to_string()),
    ToolChoice::Required => Value::String("required".to_string()),
    ToolChoice::Tool { name } => json!({
      "type": "function",
      "function": { "name": name }
    }),
  }
}

/// Decode OpenAI `tool_calls` array into typed `ToolCallRequest`s.
///
/// OpenAI returns `arguments` as a JSON-encoded string; we parse it eagerly so
/// callers don't need to know the wire format. Malformed JSON falls back to a
/// `Value::String` of the raw payload so the call can still surface in traces.
pub(crate) fn parse_openai_tool_calls(value: &Value) -> Vec<ToolCallRequest> {
  let Some(items) = value.as_array() else {
    return Vec::new();
  };
  items
    .iter()
    .enumerate()
    .filter_map(|(idx, item)| {
      let function = item.get("function")?;
      let name = function.get("name")?.as_str()?.to_string();
      let id = item
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("call_{}", idx));
      let raw_args = function.get("arguments");
      let arguments = match raw_args {
        Some(Value::String(s)) => {
          serde_json::from_str(s).unwrap_or_else(|_| Value::String(s.clone()))
        }
        Some(other) => other.clone(),
        None => Value::Object(serde_json::Map::new()),
      };
      Some(ToolCallRequest {
        id,
        name,
        arguments,
      })
    })
    .collect()
}

#[async_trait]
impl LLMProvider for OpenAIProvider {
  fn name(&self) -> &str {
    "openai"
  }

  async fn execute(&self, request: &ProviderRequest) -> Result<ProviderResponse> {
    if request.stream {
      return Err(LLMError::InternalError {
        message: "Use execute_streaming for streaming requests".to_string(),
      });
    }

    let url = format!("{}/chat/completions", self.base_url);
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

    let openai_response: OpenAIResponse = response.json().await?;

    // Handle both string and array content formats
    let content_text = if let Some(first_choice) = openai_response.choices.first() {
      match &first_choice.message.content {
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(serde_json::Value::Array(_)) => {
          // For multimodal responses that return structured content,
          // extract text parts or convert to string representation
          first_choice
            .message
            .content
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_default()
        }
        _ => String::new(),
      }
    } else {
      String::new()
    };

    // Convert to ContentType - OpenAI currently only returns text
    let content = ContentType::Text(content_text);

    let usage = openai_response
      .usage
      .clone()
      .map(|u| crate::providers::TokenUsage {
        prompt_tokens: Some(u.prompt_tokens),
        completion_tokens: Some(u.completion_tokens),
        total_tokens: Some(u.total_tokens),
      });

    let first_choice = openai_response.choices.first();
    let tool_calls = first_choice
      .and_then(|c| c.message.tool_calls.as_ref())
      .map(parse_openai_tool_calls)
      .unwrap_or_default();
    let stop_reason = first_choice
      .and_then(|c| c.finish_reason.as_deref())
      .map(StopReason::from_openai_finish_reason);

    Ok(ProviderResponse {
      content,
      usage,
      metadata: Some(serde_json::to_value(&openai_response)?),
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

    let url = format!("{}/chat/completions", self.base_url);
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

    Ok(Box::new(OpenAIStreamingResponse::new(response)))
  }

  async fn validate_config(&self) -> Result<()> {
    // Simple health check - try to list models
    let url = format!("{}/models", self.base_url);

    let response = self
      .client
      .get(&url)
      .headers(self.build_headers())
      .send()
      .await?;

    if !response.status().is_success() {
      return Err(LLMError::AuthenticationError {
        provider: "openai".to_string(),
        message: "Failed to authenticate with OpenAI API".to_string(),
      });
    }

    Ok(())
  }

  fn base_url(&self) -> &str {
    &self.base_url
  }

  fn supported_models(&self) -> Vec<String> {
    vec![
      "gpt-4o".to_string(),
      "gpt-4o-mini".to_string(),
      "gpt-4-turbo".to_string(),
      "gpt-4".to_string(),
      "gpt-3.5-turbo".to_string(),
    ]
  }
}

// OpenAI API response structures
#[derive(Debug, Deserialize, Serialize)]
struct OpenAIResponse {
  id: String,
  object: String,
  created: u64,
  model: String,
  choices: Vec<OpenAIChoice>,
  usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize, Serialize)]
struct OpenAIChoice {
  index: u32,
  message: OpenAIMessage,
  finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct OpenAIMessage {
  role: String,
  content: Option<serde_json::Value>, // Can be string or array of content objects
  #[serde(default, skip_serializing_if = "Option::is_none")]
  tool_calls: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct OpenAIUsage {
  prompt_tokens: u32,
  completion_tokens: u32,
  total_tokens: u32,
}

// Streaming response structures
#[derive(Debug, Deserialize, Serialize)]
struct OpenAIStreamingChunk {
  id: String,
  object: String,
  created: u64,
  model: String,
  choices: Vec<OpenAIStreamingChoice>,
  usage: Option<OpenAIUsage>,
}

#[derive(Debug, Deserialize, Serialize)]
struct OpenAIStreamingChoice {
  index: u32,
  delta: OpenAIStreamingDelta,
  finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct OpenAIStreamingDelta {
  role: Option<String>,
  content: Option<serde_json::Value>, // Can be string or array of content objects
}

pub struct OpenAIStreamingResponse {
  stream: Pin<Box<dyn Stream<Item = Result<String>> + Send>>,
  buffer: Option<String>,
  finished: bool,
}

// Make it Send + Sync
unsafe impl Send for OpenAIStreamingResponse {}
unsafe impl Sync for OpenAIStreamingResponse {}

impl OpenAIStreamingResponse {
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

  fn parse_sse_chunk(line: &str) -> Option<StreamChunk> {
    if !line.starts_with("data: ") {
      return None;
    }

    let data = &line[6..]; // Remove "data: " prefix

    if data.trim() == "[DONE]" {
      return Some(StreamChunk {
        content: String::new(),
        is_final: true,
        metadata: None,
        usage: None,
        content_type: Some("text".to_string()),
      });
    }

    if let Ok(chunk) = serde_json::from_str::<OpenAIStreamingChunk>(data)
      && let Some(choice) = chunk.choices.first()
      && let Some(content) = &choice.delta.content
    {
      // Handle both string and array content in streaming
      let content_text = match content {
        serde_json::Value::String(text) => text.clone(),
        _ => content.to_string(), // Convert other types to string
      };

      return Some(StreamChunk {
        content: content_text,
        is_final: choice.finish_reason.is_some(),
        metadata: Some(serde_json::to_value(&chunk).ok()?),
        usage: chunk.usage.map(|u| TokenUsage {
          prompt_tokens: Some(u.prompt_tokens),
          completion_tokens: Some(u.completion_tokens),
          total_tokens: Some(u.total_tokens),
        }),
        content_type: Some("text".to_string()),
      });
    }

    None
  }
}

#[async_trait]
impl StreamingResponse for OpenAIStreamingResponse {
  async fn next_chunk(&mut self) -> Result<Option<StreamChunk>> {
    if self.finished {
      return Ok(None);
    }

    loop {
      // Try to get the next chunk from the stream
      match self.stream.next().await {
        Some(Ok(data)) => {
          // Add to buffer
          if let Some(ref mut buffer) = self.buffer {
            buffer.push_str(&data);

            // Process complete lines
            while let Some(newline_pos) = buffer.find('\n') {
              let line = buffer[..newline_pos].trim().to_string();
              buffer.drain(..=newline_pos);

              if !line.is_empty()
                && let Some(chunk) = Self::parse_sse_chunk(&line)
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
  fn test_openai_provider_creation() {
    let provider = OpenAIProvider::new("test-key", None);
    assert!(provider.is_ok());

    let provider = OpenAIProvider::new("", None);
    assert!(provider.is_err());
  }

  #[tokio::test]
  async fn build_headers_injects_traceparent_when_scope_active() {
    use crate::trace_context::{LlmTraceContext, scope};

    let provider = OpenAIProvider::new("test-key", None).unwrap();
    let ctx = LlmTraceContext::new(
      "0af7651916cd43dd8448eb211c80319c",
      "b7ad6b7169203331",
    )
    .unwrap();

    let headers = scope(ctx.clone(), async { provider.build_headers() }).await;
    assert_eq!(
      headers
        .get("traceparent")
        .and_then(|v| v.to_str().ok()),
      Some(ctx.to_traceparent().as_str()),
    );
  }

  #[test]
  fn build_headers_omits_traceparent_when_no_scope_active() {
    let provider = OpenAIProvider::new("test-key", None).unwrap();
    let headers = provider.build_headers();
    assert!(headers.get("traceparent").is_none());
  }

  #[test]
  fn test_build_request_body() {
    let provider = OpenAIProvider::new("test-key", None).unwrap();

    let mut params = std::collections::HashMap::new();
    params.insert("temperature".to_string(), json!(0.7));
    params.insert("max_tokens".to_string(), json!(100));

    let request = ProviderRequest {
      model: "gpt-4o".to_string(),
      messages: vec![json!({"role": "user", "content": "test"})],
      stream: false,
      parameters: params,
      tools: None,
      tool_choice: None,
    };

    let body = provider.build_request_body(&request);
    assert_eq!(body["model"], "gpt-4o");
    assert_eq!(body["temperature"], 0.7);
    assert_eq!(body["max_tokens"], 100);
    assert_eq!(body["stream"], false);
    assert!(body.get("tools").is_none());
    assert!(body.get("tool_choice").is_none());
  }

  #[test]
  fn build_request_body_serialises_tools() {
    let provider = OpenAIProvider::new("test-key", None).unwrap();
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
      model: "gpt-4o".to_string(),
      messages: vec![json!({"role": "user", "content": "weather in Tokyo?"})],
      stream: false,
      parameters: std::collections::HashMap::new(),
      tools: Some(vec![tool]),
      tool_choice: Some(ToolChoice::Required),
    };

    let body = provider.build_request_body(&request);
    let tools = body["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["type"], "function");
    assert_eq!(tools[0]["function"]["name"], "get_weather");
    assert_eq!(tools[0]["function"]["parameters"]["required"][0], "city");
    assert_eq!(body["tool_choice"], "required");
  }

  #[test]
  fn build_request_body_tool_choice_specific() {
    let provider = OpenAIProvider::new("test-key", None).unwrap();
    let request = ProviderRequest {
      model: "gpt-4o".to_string(),
      messages: vec![],
      stream: false,
      parameters: std::collections::HashMap::new(),
      tools: Some(vec![ToolSpec::new("x", "", json!({}))]),
      tool_choice: Some(ToolChoice::Tool {
        name: "x".to_string(),
      }),
    };

    let body = provider.build_request_body(&request);
    assert_eq!(body["tool_choice"]["type"], "function");
    assert_eq!(body["tool_choice"]["function"]["name"], "x");
  }

  #[test]
  fn parse_openai_tool_calls_decodes_arguments_json() {
    let raw = json!([
      {
        "id": "call_abc",
        "type": "function",
        "function": {
          "name": "get_weather",
          "arguments": "{\"city\": \"Tokyo\"}"
        }
      },
      {
        "id": "call_def",
        "type": "function",
        "function": {
          "name": "search",
          "arguments": "not json"
        }
      }
    ]);
    let calls = parse_openai_tool_calls(&raw);
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].id, "call_abc");
    assert_eq!(calls[0].name, "get_weather");
    assert_eq!(calls[0].arguments["city"], "Tokyo");
    // Unparseable arguments fall back to a string so traces still see them.
    assert_eq!(calls[1].arguments, Value::String("not json".to_string()));
  }

  #[test]
  fn parse_openai_tool_calls_synthesises_id_when_missing() {
    let raw = json!([
      { "type": "function", "function": { "name": "ping", "arguments": "{}" } }
    ]);
    let calls = parse_openai_tool_calls(&raw);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].id, "call_0");
  }

  #[test]
  fn openai_message_deserialises_tool_calls() {
    let raw = json!({
      "id": "x",
      "object": "chat.completion",
      "created": 0,
      "model": "gpt-4o",
      "choices": [
        {
          "index": 0,
          "message": {
            "role": "assistant",
            "content": null,
            "tool_calls": [
              {
                "id": "call_1",
                "type": "function",
                "function": {
                  "name": "get_weather",
                  "arguments": "{\"city\": \"NYC\"}"
                }
              }
            ]
          },
          "finish_reason": "tool_calls"
        }
      ],
      "usage": null
    });
    let parsed: OpenAIResponse = serde_json::from_value(raw).unwrap();
    let choice = &parsed.choices[0];
    assert_eq!(choice.finish_reason.as_deref(), Some("tool_calls"));
    let tool_calls = parse_openai_tool_calls(choice.message.tool_calls.as_ref().unwrap());
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].name, "get_weather");
    assert_eq!(tool_calls[0].arguments["city"], "NYC");
  }
}

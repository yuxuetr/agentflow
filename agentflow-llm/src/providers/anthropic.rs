use crate::{
  client::streaming::{StreamChunk, StreamingResponse},
  providers::{ContentType, LLMProvider, ProviderRequest, ProviderResponse},
  LLMError, Result,
};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::pin::Pin;
use tokio_stream::Stream;

pub struct AnthropicProvider {
  client: Client,
  api_key: String,
  base_url: String,
}

impl AnthropicProvider {
  pub fn new(api_key: &str, base_url: Option<String>) -> Result<Self> {
    if api_key.is_empty() {
      return Err(LLMError::MissingApiKey {
        provider: "anthropic".to_string(),
      });
    }

    let client = Client::new();
    let base_url = base_url.unwrap_or_else(|| "https://api.anthropic.com".to_string());

    Ok(Self {
      client,
      api_key: api_key.to_string(),
      base_url,
    })
  }

  fn build_headers(&self) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Content-Type", "application/json".parse().unwrap());
    headers.insert("x-api-key", self.api_key.parse().unwrap());
    headers.insert("anthropic-version", "2023-06-01".parse().unwrap());
    headers
  }

  fn build_request_body(&self, request: &ProviderRequest) -> Value {
    // Convert OpenAI-style messages to Anthropic format
    let mut system_message = None;
    let mut anthropic_messages = Vec::new();

    for message in &request.messages {
      if let Some(msg_obj) = message.as_object() {
        if let (Some(role), Some(content)) = (msg_obj.get("role"), msg_obj.get("content")) {
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

    body
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

    let content_text = anthropic_response
      .content
      .first()
      .and_then(|content| match content {
        AnthropicContent::Text { text } => Some(text.clone()),
      })
      .unwrap_or_default();

    // Convert to ContentType - Anthropic currently only returns text
    let content = ContentType::Text(content_text);

    let usage = anthropic_response
      .usage
      .clone()
      .map(|u| crate::providers::TokenUsage {
        prompt_tokens: Some(u.input_tokens),
        completion_tokens: Some(u.output_tokens),
        total_tokens: Some(u.input_tokens + u.output_tokens),
      });

    Ok(ProviderResponse {
      content,
      usage,
      metadata: Some(serde_json::to_value(&anthropic_response)?),
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
enum AnthropicContent {
  #[serde(rename = "text")]
  Text { text: String },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct AnthropicUsage {
  input_tokens: u32,
  output_tokens: u32,
}

// Streaming response structures
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

    if let Ok(event) = serde_json::from_str::<AnthropicStreamingEvent>(data) {
      match event.event_type.as_str() {
        "content_block_delta" => {
          if let Some(data) = event.data {
            if let Some(delta) = data.get("delta") {
              if let Some(text) = delta.get("text") {
                if let Some(text_str) = text.as_str() {
                  return Some(StreamChunk {
                    content: text_str.to_string(),
                    is_final: false,
                    metadata: Some(data),
                    usage: None,
                    content_type: Some("text".to_string()),
                  });
                }
              }
            }
          }
        }
        "message_stop" => {
          return Some(StreamChunk {
            content: String::new(),
            is_final: true,
            metadata: event.data,
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

              if !line.is_empty() {
                if let Some(chunk) = Self::parse_sse_event(&line) {
                  if chunk.is_final {
                    self.finished = true;
                  }
                  return Ok(Some(chunk));
                }
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
    };

    let body = provider.build_request_body(&request);
    assert_eq!(body["model"], "claude-3-sonnet-20240229");
    assert_eq!(body["temperature"], 0.7);
    assert_eq!(body["max_tokens"], 100);
    assert_eq!(body["system"], "You are helpful");
    assert_eq!(body["messages"].as_array().unwrap().len(), 1); // Only user message
  }
}

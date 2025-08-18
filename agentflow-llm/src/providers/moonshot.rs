use crate::{
  client::streaming::{StreamChunk, StreamingResponse, TokenUsage},
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

pub struct MoonshotProvider {
  client: Client,
  api_key: String,
  base_url: String,
}

impl MoonshotProvider {
  pub fn new(api_key: &str, base_url: Option<String>) -> Result<Self> {
    if api_key.is_empty() {
      return Err(LLMError::MissingApiKey {
        provider: "moonshot".to_string(),
      });
    }

    let client = Client::new();
    let base_url = base_url.unwrap_or_else(|| "https://api.moonshot.cn/v1".to_string());

    Ok(Self {
      client,
      api_key: api_key.to_string(),
      base_url,
    })
  }

  fn build_headers(&self) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Content-Type", "application/json".parse().unwrap());
    headers.insert(
      "Authorization",
      format!("Bearer {}", self.api_key).parse().unwrap(),
    );
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

    body
  }
}

#[async_trait]
impl LLMProvider for MoonshotProvider {
  fn name(&self) -> &str {
    "moonshot"
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

    let moonshot_response: MoonshotResponse = response.json().await?;

    let content_text = moonshot_response
      .choices
      .first()
      .and_then(|choice| choice.message.content.as_ref())
      .unwrap_or(&String::new())
      .clone();

    // Convert to ContentType - Moonshot currently only returns text
    let content = ContentType::Text(content_text);

    let usage = moonshot_response
      .usage
      .clone()
      .map(|u| crate::providers::TokenUsage {
        prompt_tokens: Some(u.prompt_tokens),
        completion_tokens: Some(u.completion_tokens),
        total_tokens: Some(u.total_tokens),
      });

    Ok(ProviderResponse {
      content,
      usage,
      metadata: Some(serde_json::to_value(&moonshot_response)?),
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

    Ok(Box::new(MoonshotStreamingResponse::new(response)))
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
        provider: "moonshot".to_string(),
        message: "Failed to authenticate with Moonshot API".to_string(),
      });
    }

    Ok(())
  }

  fn base_url(&self) -> &str {
    &self.base_url
  }

  fn supported_models(&self) -> Vec<String> {
    vec![
      "moonshot-v1-8k".to_string(),
      "moonshot-v1-32k".to_string(),
      "moonshot-v1-128k".to_string(),
    ]
  }
}

// Moonshot API response structures (similar to OpenAI format)
#[derive(Debug, Deserialize, Serialize)]
struct MoonshotResponse {
  id: String,
  object: String,
  created: u64,
  model: String,
  choices: Vec<MoonshotChoice>,
  usage: Option<MoonshotUsage>,
}

#[derive(Debug, Deserialize, Serialize)]
struct MoonshotChoice {
  index: u32,
  message: MoonshotMessage,
  finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct MoonshotMessage {
  role: String,
  content: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct MoonshotUsage {
  prompt_tokens: u32,
  completion_tokens: u32,
  total_tokens: u32,
}

// Streaming response structures
#[derive(Debug, Deserialize, Serialize)]
struct MoonshotStreamingChunk {
  id: String,
  object: String,
  created: u64,
  model: String,
  choices: Vec<MoonshotStreamingChoice>,
  usage: Option<MoonshotUsage>,
}

#[derive(Debug, Deserialize, Serialize)]
struct MoonshotStreamingChoice {
  index: u32,
  delta: MoonshotStreamingDelta,
  finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct MoonshotStreamingDelta {
  role: Option<String>,
  content: Option<String>,
}

pub struct MoonshotStreamingResponse {
  stream: Pin<Box<dyn Stream<Item = Result<String>> + Send>>,
  buffer: Option<String>,
  finished: bool,
}

// Make it Send + Sync
unsafe impl Send for MoonshotStreamingResponse {}
unsafe impl Sync for MoonshotStreamingResponse {}

impl MoonshotStreamingResponse {
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

    if let Ok(chunk) = serde_json::from_str::<MoonshotStreamingChunk>(data) {
      if let Some(choice) = chunk.choices.first() {
        if let Some(content) = &choice.delta.content {
          return Some(StreamChunk {
            content: content.clone(),
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
      }
    }

    None
  }
}

#[async_trait]
impl StreamingResponse for MoonshotStreamingResponse {
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

              if !line.is_empty() {
                if let Some(chunk) = Self::parse_sse_chunk(&line) {
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
  fn test_moonshot_provider_creation() {
    let provider = MoonshotProvider::new("test-key", None);
    assert!(provider.is_ok());

    let provider = MoonshotProvider::new("", None);
    assert!(provider.is_err());
  }

  #[test]
  fn test_build_request_body() {
    let provider = MoonshotProvider::new("test-key", None).unwrap();

    let mut params = std::collections::HashMap::new();
    params.insert("temperature".to_string(), json!(0.7));
    params.insert("max_tokens".to_string(), json!(100));

    let request = ProviderRequest {
      model: "moonshot-v1-8k".to_string(),
      messages: vec![json!({"role": "user", "content": "test"})],
      stream: false,
      parameters: params,
    };

    let body = provider.build_request_body(&request);
    assert_eq!(body["model"], "moonshot-v1-8k");
    assert_eq!(body["temperature"], 0.7);
    assert_eq!(body["max_tokens"], 100);
    assert_eq!(body["stream"], false);
  }

  #[test]
  fn test_supported_models() {
    let provider = MoonshotProvider::new("test-key", None).unwrap();
    let models = provider.supported_models();
    assert!(models.contains(&"moonshot-v1-8k".to_string()));
    assert!(models.contains(&"moonshot-v1-32k".to_string()));
    assert!(models.contains(&"moonshot-v1-128k".to_string()));
  }
}

use crate::{
  client::streaming::{StreamChunk, StreamingResponse, TokenUsage},
  providers::{LLMProvider, ProviderRequest, ProviderResponse, ContentType},
  LLMError, Result,
};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::pin::Pin;
use tokio_stream::Stream;

/// StepFun provider implementation (OpenAI-compatible but optimized for multimodal)
pub struct StepFunProvider {
  client: Client,
  api_key: String,
  base_url: String,
}

impl StepFunProvider {
  pub fn new(api_key: &str, base_url: Option<String>) -> Result<Self> {
    if api_key.is_empty() {
      return Err(LLMError::MissingApiKey {
        provider: "stepfun".to_string(),
      });
    }

    let client = Client::new();
    let base_url = base_url.unwrap_or_else(|| "https://api.stepfun.com/v1".to_string());

    Ok(Self {
      client,
      api_key: api_key.to_string(),
      base_url,
    })
  }

  fn build_headers(&self) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Content-Type", "application/json".parse().unwrap());
    headers.insert("Authorization", format!("Bearer {}", self.api_key).parse().unwrap());
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
impl LLMProvider for StepFunProvider {
  fn name(&self) -> &str {
    "stepfun"
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

    let stepfun_response: StepFunResponse = response.json().await?;
    
    // Handle both string and array content formats (StepFun supports multimodal)
    let content_text = if let Some(first_choice) = stepfun_response.choices.first() {
      match &first_choice.message.content {
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(serde_json::Value::Array(_)) => {
          // For multimodal responses that return structured content,
          // extract text parts or convert to string representation
          first_choice.message.content
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_default()
        },
        _ => String::new(),
      }
    } else {
      String::new()
    };

    // Convert to ContentType
    let content = ContentType::Text(content_text);

    let usage = stepfun_response.usage.clone().map(|u| crate::providers::TokenUsage {
      prompt_tokens: Some(u.prompt_tokens),
      completion_tokens: Some(u.completion_tokens),
      total_tokens: Some(u.total_tokens),
    });

    Ok(ProviderResponse {
      content,
      usage,
      metadata: Some(serde_json::to_value(&stepfun_response)?),
    })
  }

  async fn execute_streaming(&self, request: &ProviderRequest) -> Result<Box<dyn StreamingResponse>> {
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

    Ok(Box::new(StepFunStreamingResponse::new(response)))
  }

  async fn validate_config(&self) -> Result<()> {
    // Simple health check - try to make a minimal request
    let url = format!("{}/chat/completions", self.base_url);
    let test_body = json!({
      "model": "step-1-8k",
      "messages": [{"role": "user", "content": "test"}],
      "max_tokens": 1
    });
    
    let response = self
      .client
      .post(&url)
      .headers(self.build_headers())
      .json(&test_body)
      .send()
      .await?;

    if !response.status().is_success() {
      return Err(LLMError::AuthenticationError {
        provider: "stepfun".to_string(),
        message: "Failed to authenticate with StepFun API".to_string(),
      });
    }

    Ok(())
  }

  fn base_url(&self) -> &str {
    &self.base_url
  }

  fn supported_models(&self) -> Vec<String> {
    vec![
      "step-1-8k".to_string(),
      "step-1-32k".to_string(),
      "step-1-128k".to_string(),
      "step-1-256k".to_string(),
      "step-1o".to_string(),
      "step-1o-turbo".to_string(),
      "step-1o-turbo-vision".to_string(), // Multimodal model
      "step-2-16k".to_string(),
    ]
  }
}

// StepFun API response structures (similar to OpenAI but with StepFun specifics)
#[derive(Debug, Deserialize, Serialize)]
struct StepFunResponse {
  id: String,
  object: String,
  created: u64,
  model: String,
  choices: Vec<StepFunChoice>,
  usage: Option<StepFunUsage>,
  #[serde(skip_serializing_if = "Option::is_none")]
  service_tier: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  system_fingerprint: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct StepFunChoice {
  index: u32,
  message: StepFunMessage,
  finish_reason: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  logprobs: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct StepFunMessage {
  role: String,
  content: Option<serde_json::Value>, // Can be string or array for multimodal
  #[serde(skip_serializing_if = "Option::is_none")]
  refusal: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  function_call: Option<Value>,
  #[serde(skip_serializing_if = "Option::is_none")]
  tool_calls: Option<Vec<Value>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct StepFunUsage {
  prompt_tokens: u32,
  completion_tokens: u32,
  total_tokens: u32,
  #[serde(skip_serializing_if = "Option::is_none")]
  completion_tokens_details: Option<Value>,
  #[serde(skip_serializing_if = "Option::is_none")]
  cached_tokens: Option<u32>,
}

// Streaming response structures
#[derive(Debug, Deserialize, Serialize)]
struct StepFunStreamingChunk {
  id: String,
  object: String,
  created: u64,
  model: String,
  choices: Vec<StepFunStreamingChoice>,
  usage: Option<StepFunUsage>,
}

#[derive(Debug, Deserialize, Serialize)]
struct StepFunStreamingChoice {
  index: u32,
  delta: StepFunStreamingDelta,
  finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct StepFunStreamingDelta {
  role: Option<String>,
  content: Option<serde_json::Value>, // Can be string or array for multimodal
}

pub struct StepFunStreamingResponse {
  stream: Pin<Box<dyn Stream<Item = Result<String>> + Send>>,
  buffer: Option<String>,
  finished: bool,
}

// Make it Send + Sync
unsafe impl Send for StepFunStreamingResponse {}
unsafe impl Sync for StepFunStreamingResponse {}

impl StepFunStreamingResponse {
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

    if let Ok(chunk) = serde_json::from_str::<StepFunStreamingChunk>(data) {
      if let Some(choice) = chunk.choices.first() {
        if let Some(content) = &choice.delta.content {
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
      }
    }

    None
  }
}

#[async_trait]
impl StreamingResponse for StepFunStreamingResponse {
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
  fn test_stepfun_provider_creation() {
    let provider = StepFunProvider::new("test-key", None);
    assert!(provider.is_ok());

    let provider = StepFunProvider::new("", None);
    assert!(provider.is_err());
  }

  #[test]
  fn test_supported_models() {
    let provider = StepFunProvider::new("test-key", None).unwrap();
    let models = provider.supported_models();
    assert!(models.contains(&"step-1o-turbo-vision".to_string()));
    assert!(models.len() > 5);
  }
}
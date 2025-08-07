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

pub struct GoogleProvider {
  client: Client,
  api_key: String,
  base_url: String,
}

impl GoogleProvider {
  pub fn new(api_key: &str, base_url: Option<String>) -> Result<Self> {
    if api_key.is_empty() {
      return Err(LLMError::MissingApiKey {
        provider: "google".to_string(),
      });
    }

    let client = Client::new();
    let base_url = base_url.unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string());

    Ok(Self {
      client,
      api_key: api_key.to_string(),
      base_url,
    })
  }

  fn build_headers(&self) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Content-Type", "application/json".parse().unwrap());
    headers
  }

  fn build_request_body(&self, request: &ProviderRequest) -> Value {
    // Convert OpenAI-style messages to Gemini format
    let mut system_instruction = None;
    let mut gemini_contents = Vec::new();

    for message in &request.messages {
      if let Some(msg_obj) = message.as_object() {
        if let (Some(role), Some(content)) = (msg_obj.get("role"), msg_obj.get("content")) {
          match role.as_str() {
            Some("system") => {
              system_instruction = content.as_str().map(|s| json!({
                "parts": [{"text": s}]
              }));
            }
            Some("user") => {
              gemini_contents.push(json!({
                "role": "user",
                "parts": [{"text": content}]
              }));
            }
            Some("assistant") => {
              gemini_contents.push(json!({
                "role": "model",
                "parts": [{"text": content}]
              }));
            }
            _ => {}
          }
        }
      }
    }

    let mut body = json!({
      "contents": gemini_contents
    });

    if let Some(system) = system_instruction {
      body["systemInstruction"] = system;
    }

    // Add generation config
    let mut generation_config = json!({});

    for (key, value) in &request.parameters {
      match key.as_str() {
        "temperature" => generation_config["temperature"] = value.clone(),
        "max_tokens" => generation_config["maxOutputTokens"] = value.clone(),
        "top_p" => generation_config["topP"] = value.clone(),
        "top_k" => generation_config["topK"] = value.clone(),
        _ => {}
      }
    }

    if !generation_config.as_object().unwrap().is_empty() {
      body["generationConfig"] = generation_config;
    }

    body
  }

  fn get_model_endpoint(&self, model: &str, stream: bool) -> String {
    let method = if stream { "streamGenerateContent" } else { "generateContent" };
    format!("{}/v1beta/models/{}:{}?key={}", self.base_url, model, method, self.api_key)
  }
}

#[async_trait]
impl LLMProvider for GoogleProvider {
  fn name(&self) -> &str {
    "google"
  }

  async fn execute(&self, request: &ProviderRequest) -> Result<ProviderResponse> {
    if request.stream {
      return Err(LLMError::InternalError {
        message: "Use execute_streaming for streaming requests".to_string(),
      });
    }

    let url = self.get_model_endpoint(&request.model, false);
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

    let google_response: GoogleResponse = response.json().await?;
    
    let content_text = google_response
      .candidates
      .first()
      .and_then(|candidate| candidate.content.parts.first())
      .and_then(|part| part.text.as_ref())
      .unwrap_or(&String::new())
      .clone();

    // Convert to ContentType - Google currently only returns text
    let content = ContentType::Text(content_text);

    let usage = google_response.usage_metadata.clone().map(|u| crate::providers::TokenUsage {
      prompt_tokens: Some(u.prompt_token_count),
      completion_tokens: Some(u.candidates_token_count),
      total_tokens: Some(u.total_token_count),
    });

    Ok(ProviderResponse {
      content,
      usage,
      metadata: Some(serde_json::to_value(&google_response)?),
    })
  }

  async fn execute_streaming(&self, request: &ProviderRequest) -> Result<Box<dyn StreamingResponse>> {
    if !request.stream {
      return Err(LLMError::InternalError {
        message: "Streaming not enabled in request".to_string(),
      });
    }

    let url = self.get_model_endpoint(&request.model, true);
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

    Ok(Box::new(GoogleStreamingResponse::new(response)))
  }

  async fn validate_config(&self) -> Result<()> {
    // Test with a simple model list request
    let url = format!("{}/v1beta/models?key={}", self.base_url, self.api_key);
    
    let response = self
      .client
      .get(&url)
      .headers(self.build_headers())
      .send()
      .await?;

    if response.status().as_u16() == 401 || response.status().as_u16() == 403 {
      return Err(LLMError::AuthenticationError {
        provider: "google".to_string(),
        message: "Invalid API key".to_string(),
      });
    }

    Ok(())
  }

  fn base_url(&self) -> &str {
    &self.base_url
  }

  fn supported_models(&self) -> Vec<String> {
    vec![
      "gemini-1.5-pro".to_string(),
      "gemini-1.5-pro-002".to_string(),
      "gemini-1.5-flash".to_string(),
      "gemini-1.5-flash-002".to_string(),
      "gemini-1.0-pro".to_string(),
    ]
  }
}

// Google AI API response structures
#[derive(Debug, Deserialize, Serialize)]
struct GoogleResponse {
  candidates: Vec<GoogleCandidate>,
  #[serde(rename = "usageMetadata")]
  usage_metadata: Option<GoogleUsage>,
  #[serde(rename = "promptFeedback")]
  prompt_feedback: Option<Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct GoogleCandidate {
  content: GoogleContent,
  #[serde(rename = "finishReason")]
  finish_reason: Option<String>,
  index: Option<u32>,
  #[serde(rename = "safetyRatings")]
  safety_ratings: Option<Vec<Value>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct GoogleContent {
  parts: Vec<GooglePart>,
  role: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct GooglePart {
  text: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct GoogleUsage {
  #[serde(rename = "promptTokenCount")]
  prompt_token_count: u32,
  #[serde(rename = "candidatesTokenCount")]
  candidates_token_count: u32,
  #[serde(rename = "totalTokenCount")]
  total_token_count: u32,
}

pub struct GoogleStreamingResponse {
  stream: Pin<Box<dyn Stream<Item = Result<String>> + Send>>,
  buffer: Option<String>,
  finished: bool,
}

// Make it Send + Sync
unsafe impl Send for GoogleStreamingResponse {}
unsafe impl Sync for GoogleStreamingResponse {}

impl GoogleStreamingResponse {
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

  fn parse_json_chunk(line: &str) -> Option<StreamChunk> {
    if line.trim().is_empty() {
      return None;
    }

    if let Ok(response) = serde_json::from_str::<GoogleResponse>(line) {
      if let Some(candidate) = response.candidates.first() {
        if let Some(part) = candidate.content.parts.first() {
          if let Some(text) = &part.text {
            let is_final = candidate.finish_reason.is_some();
            
            return Some(StreamChunk {
              content: text.clone(),
              is_final,
              metadata: Some(serde_json::to_value(&response).ok()?),
              usage: response.usage_metadata.map(|u| TokenUsage {
                prompt_tokens: Some(u.prompt_token_count),
                completion_tokens: Some(u.candidates_token_count),
                total_tokens: Some(u.total_token_count),
              }),
              content_type: Some("text".to_string()),
            });
          }
        }
        
        // Check if this is a final chunk without text
        if candidate.finish_reason.is_some() {
          return Some(StreamChunk {
            content: String::new(),
            is_final: true,
            metadata: Some(serde_json::to_value(&response).ok()?),
            usage: response.usage_metadata.map(|u| TokenUsage {
              prompt_tokens: Some(u.prompt_token_count),
              completion_tokens: Some(u.candidates_token_count),
              total_tokens: Some(u.total_token_count),
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
impl StreamingResponse for GoogleStreamingResponse {
  async fn next_chunk(&mut self) -> Result<Option<StreamChunk>> {
    if self.finished {
      return Ok(None);
    }

    loop {
      match self.stream.next().await {
        Some(Ok(data)) => {
          if let Some(ref mut buffer) = self.buffer {
            buffer.push_str(&data);

            // Google streams JSON objects separated by newlines
            while let Some(newline_pos) = buffer.find('\n') {
              let line = buffer[..newline_pos].trim().to_string();
              buffer.drain(..=newline_pos);

              if !line.is_empty() {
                if let Some(chunk) = Self::parse_json_chunk(&line) {
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
  fn test_google_provider_creation() {
    let provider = GoogleProvider::new("test-key", None);
    assert!(provider.is_ok());

    let provider = GoogleProvider::new("", None);
    assert!(provider.is_err());
  }

  #[test]
  fn test_build_request_body() {
    let provider = GoogleProvider::new("test-key", None).unwrap();
    
    let mut params = std::collections::HashMap::new();
    params.insert("temperature".to_string(), json!(0.7));
    params.insert("max_tokens".to_string(), json!(100));

    let request = ProviderRequest {
      model: "gemini-1.5-pro".to_string(),
      messages: vec![
        json!({"role": "system", "content": "You are helpful"}),
        json!({"role": "user", "content": "test"})
      ],
      stream: false,
      parameters: params,
    };

    let body = provider.build_request_body(&request);
    assert!(body.get("systemInstruction").is_some());
    assert_eq!(body["contents"].as_array().unwrap().len(), 1); // Only user message in contents
    assert!(body.get("generationConfig").is_some());
  }

  #[test]
  fn test_model_endpoint() {
    let provider = GoogleProvider::new("test-key", None).unwrap();
    
    let endpoint = provider.get_model_endpoint("gemini-1.5-pro", false);
    assert!(endpoint.contains("generateContent"));
    assert!(endpoint.contains("test-key"));
    
    let streaming_endpoint = provider.get_model_endpoint("gemini-1.5-pro", true);
    assert!(streaming_endpoint.contains("streamGenerateContent"));
  }
}
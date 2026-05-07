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
    let base_url =
      base_url.unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string());

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
    crate::trace_context::inject_into_headers(&mut headers);
    headers
  }

  fn build_request_body(&self, request: &ProviderRequest) -> Value {
    // Convert OpenAI-style messages to Gemini format
    let mut system_instruction = None;
    let mut gemini_contents = Vec::new();

    for message in &request.messages {
      if let Some(msg_obj) = message.as_object()
        && let (Some(role), Some(content)) = (msg_obj.get("role"), msg_obj.get("content"))
      {
        match role.as_str() {
          Some("system") => {
            system_instruction = content.as_str().map(|s| {
              json!({
                "parts": [{"text": s}]
              })
            });
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

    if let Some(tools) = &request.tools {
      // Gemini wraps every function declaration list in a single `tools`
      // entry — we send one entry containing all functions.
      let declarations: Vec<Value> = tools.iter().map(tool_spec_to_google_value).collect();
      body["tools"] = json!([
        { "functionDeclarations": declarations }
      ]);
    }
    if let Some(choice) = &request.tool_choice {
      body["toolConfig"] = tool_choice_to_google_value(choice, request.tools.as_deref());
    }

    body
  }

  fn get_model_endpoint(&self, model: &str, stream: bool) -> String {
    let method = if stream {
      "streamGenerateContent"
    } else {
      "generateContent"
    };
    format!(
      "{}/v1beta/models/{}:{}?key={}",
      self.base_url, model, method, self.api_key
    )
  }
}

/// Encode a `ToolSpec` as a Gemini `functionDeclaration` entry.
pub(crate) fn tool_spec_to_google_value(spec: &ToolSpec) -> Value {
  json!({
    "name": spec.name,
    "description": spec.description,
    "parameters": spec.parameters,
  })
}

/// Encode `ToolChoice` as Gemini's `toolConfig.functionCallingConfig` block.
///
/// Specific-tool selection requires `allowedFunctionNames` to contain the
/// target name (mode is `ANY` so the model is forced to use a tool).
pub(crate) fn tool_choice_to_google_value(
  choice: &ToolChoice,
  _tools: Option<&[ToolSpec]>,
) -> Value {
  match choice {
    ToolChoice::Auto => json!({"functionCallingConfig": {"mode": "AUTO"}}),
    ToolChoice::None => json!({"functionCallingConfig": {"mode": "NONE"}}),
    ToolChoice::Required => json!({"functionCallingConfig": {"mode": "ANY"}}),
    ToolChoice::Tool { name } => json!({
      "functionCallingConfig": {
        "mode": "ANY",
        "allowedFunctionNames": [name],
      }
    }),
  }
}

/// Pull `functionCall` parts out of the first candidate and convert them to
/// typed `ToolCallRequest`s. Gemini does not include ids — we synthesise
/// stable `call_<index>` ids so downstream tool-result correlation works.
pub(crate) fn parse_google_function_calls(parts: &[GooglePart]) -> Vec<ToolCallRequest> {
  parts
    .iter()
    .enumerate()
    .filter_map(|(idx, part)| {
      let call = part.function_call.as_ref()?;
      let name = call.get("name").and_then(Value::as_str)?.to_string();
      let arguments = call
        .get("args")
        .cloned()
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
      Some(ToolCallRequest {
        id: format!("call_{}", idx),
        name,
        arguments,
      })
    })
    .collect()
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

    let first_candidate = google_response.candidates.first();
    // Concatenate all text parts; functionCall parts are surfaced via
    // `tool_calls` instead of being stringified into content.
    let content_text = first_candidate
      .map(|c| {
        c.content
          .parts
          .iter()
          .filter_map(|p| p.text.as_deref())
          .collect::<Vec<_>>()
          .join("")
      })
      .unwrap_or_default();

    let content = ContentType::Text(content_text);

    let usage = google_response
      .usage_metadata
      .clone()
      .map(|u| crate::providers::TokenUsage {
        prompt_tokens: Some(u.prompt_token_count),
        completion_tokens: Some(u.candidates_token_count),
        total_tokens: Some(u.total_token_count),
      });

    let tool_calls = first_candidate
      .map(|c| parse_google_function_calls(&c.content.parts))
      .unwrap_or_default();

    // Gemini emits no dedicated tool-call finish reason; presence of
    // functionCall parts is the signal. Override `STOP` to `ToolCalls` when
    // tool calls are present so callers branch correctly.
    let stop_reason = first_candidate.and_then(|c| {
      let raw = c.finish_reason.as_deref()?;
      let mapped = StopReason::from_google_finish_reason(raw);
      if !tool_calls.is_empty() && matches!(mapped, StopReason::Stop) {
        Some(StopReason::ToolCalls)
      } else {
        Some(mapped)
      }
    });

    Ok(ProviderResponse {
      content,
      usage,
      metadata: Some(serde_json::to_value(&google_response)?),
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
pub(crate) struct GooglePart {
  pub text: Option<String>,
  /// Native function call payload: `{ "name": "...", "args": { ... } }`.
  #[serde(
    rename = "functionCall",
    default,
    skip_serializing_if = "Option::is_none"
  )]
  pub function_call: Option<Value>,
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

    if let Ok(response) = serde_json::from_str::<GoogleResponse>(line)
      && let Some(candidate) = response.candidates.first()
    {
      if let Some(part) = candidate.content.parts.first()
        && let Some(text) = &part.text
      {
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

              if !line.is_empty()
                && let Some(chunk) = Self::parse_json_chunk(&line)
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
  fn test_google_provider_creation() {
    let provider = GoogleProvider::new("test-key", None);
    assert!(provider.is_ok());

    let provider = GoogleProvider::new("", None);
    assert!(provider.is_err());
  }

  #[tokio::test]
  async fn build_headers_injects_traceparent_when_scope_active() {
    use crate::trace_context::{LlmTraceContext, scope};

    let provider = GoogleProvider::new("test-key", None).unwrap();
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
    let provider = GoogleProvider::new("test-key", None).unwrap();

    let mut params = std::collections::HashMap::new();
    params.insert("temperature".to_string(), json!(0.7));
    params.insert("max_tokens".to_string(), json!(100));

    let request = ProviderRequest {
      model: "gemini-1.5-pro".to_string(),
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

  #[test]
  fn build_request_body_serialises_tools() {
    let provider = GoogleProvider::new("test-key", None).unwrap();
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
      model: "gemini-1.5-pro".to_string(),
      messages: vec![json!({"role": "user", "content": "weather?"})],
      stream: false,
      parameters: std::collections::HashMap::new(),
      tools: Some(vec![tool]),
      tool_choice: Some(ToolChoice::Required),
    };

    let body = provider.build_request_body(&request);
    let tools = body["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 1);
    let decls = tools[0]["functionDeclarations"]
      .as_array()
      .expect("functionDeclarations");
    assert_eq!(decls.len(), 1);
    assert_eq!(decls[0]["name"], "get_weather");
    assert_eq!(body["toolConfig"]["functionCallingConfig"]["mode"], "ANY");
  }

  #[test]
  fn tool_choice_specific_lists_allowed_function() {
    let body = tool_choice_to_google_value(
      &ToolChoice::Tool {
        name: "x".to_string(),
      },
      None,
    );
    assert_eq!(body["functionCallingConfig"]["mode"], "ANY");
    assert_eq!(
      body["functionCallingConfig"]["allowedFunctionNames"][0],
      "x"
    );
  }

  #[test]
  fn parse_google_function_calls_extracts_calls() {
    let raw = json!({
      "candidates": [
        {
          "content": {
            "parts": [
              {"text": "I'll check"},
              {"functionCall": {"name": "get_weather", "args": {"city": "Tokyo"}}}
            ],
            "role": "model"
          },
          "finishReason": "STOP"
        }
      ],
      "usageMetadata": {
        "promptTokenCount": 5,
        "candidatesTokenCount": 3,
        "totalTokenCount": 8
      }
    });
    let parsed: GoogleResponse = serde_json::from_value(raw).unwrap();
    let candidate = &parsed.candidates[0];
    let calls = parse_google_function_calls(&candidate.content.parts);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "get_weather");
    assert_eq!(calls[0].arguments["city"], "Tokyo");
    // Synthesised id when Gemini doesn't provide one.
    assert_eq!(calls[0].id, "call_1");
  }

  #[test]
  fn parse_google_function_calls_text_only_returns_empty() {
    let parts = vec![GooglePart {
      text: Some("hi".to_string()),
      function_call: None,
    }];
    assert!(parse_google_function_calls(&parts).is_empty());
  }
}

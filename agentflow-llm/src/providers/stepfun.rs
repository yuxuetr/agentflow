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

/// Model types supported by StepFun
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]  // Some variants like VoiceClone are defined but not yet used in routing logic
enum ModelType {
  Text,
  ImageUnderstand,
  Multimodal,
  TTS,         // Text-to-Speech: input text, output audio
  ASR,         // Automatic Speech Recognition: input audio, output text
  VoiceClone,  // Voice Cloning: input audio sample, output voice profile
  GenerateImage,
  EditImage,
}

/// StepFun provider implementation
/// 
/// Handles chat-compatible models (Text, ImageUnderstand, Multimodal) via /chat/completions endpoint.
/// For specialized APIs (TTS, ASR, Image Generation, etc.), use StepFunSpecializedClient instead.
/// 
/// Supported endpoints:
/// - Text Models: /chat/completions (streaming + non-streaming)
/// - Image Understanding: /chat/completions with multimodal content
/// - Multimodal: /chat/completions with enhanced capabilities
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

  fn get_model_type(&self, model: &str) -> ModelType {
    match model {
      // Text models
      "step-1-8k" | "step-1-32k" | "step-1-256k" | "step-2-16k" | "step-2-mini" | 
      "step-2-16k-202411" | "step-2-16k-exp" => ModelType::Text,
      
      // Image understanding models
      "step-1o-turbo-vision" | "step-1o-vision-32k" | "step-1v-8k" | "step-1v-32k" => ModelType::ImageUnderstand,
      
      // Multimodal models
      "step-3" => ModelType::Multimodal,
      
      // Text-to-Speech models
      "step-tts-vivid" | "step-tts-mini" => ModelType::TTS,
      
      // Automatic Speech Recognition models  
      "step-asr" => ModelType::ASR,
      
      // Image generation models
      "step-2x-large" | "step-1x-medium" => ModelType::GenerateImage,
      
      // Image editing models
      "step-1x-edit" => ModelType::EditImage,
      
      // Default to text for unknown models
      _ => ModelType::Text,
    }
  }

  async fn execute_chat_completion(&self, url: String, body: Value) -> Result<ProviderResponse> {
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

  async fn execute_streaming_chat(&self, url: String, body: Value) -> Result<Box<dyn StreamingResponse>> {
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

    // Route to appropriate API based on model type
    match self.get_model_type(&request.model) {
      ModelType::Text | ModelType::ImageUnderstand | ModelType::Multimodal => {
        // Use chat completions for text, image understanding, and multimodal models
        let url = format!("{}/chat/completions", self.base_url);
        let body = self.build_request_body(request);
        self.execute_chat_completion(url, body).await
      }
      ModelType::TTS | ModelType::ASR | ModelType::VoiceClone | ModelType::GenerateImage | ModelType::EditImage => {
        // These model types require specialized APIs that are not suitable for streaming chat interface
        return Err(LLMError::InternalError {
          message: format!("Model '{}' requires specialized API. Use StepFunSpecializedClient instead.", request.model),
        });
      }
    }
  }

  async fn execute_streaming(&self, request: &ProviderRequest) -> Result<Box<dyn StreamingResponse>> {
    if !request.stream {
      return Err(LLMError::InternalError {
        message: "Streaming not enabled in request".to_string(),
      });
    }

    // Check if model supports streaming
    match self.get_model_type(&request.model) {
      ModelType::Text | ModelType::ImageUnderstand | ModelType::Multimodal => {
        // These models support streaming via chat completions
        let url = format!("{}/chat/completions", self.base_url);
        let body = self.build_request_body(request);
        self.execute_streaming_chat(url, body).await
      }
      ModelType::TTS | ModelType::ASR | ModelType::VoiceClone | ModelType::GenerateImage | ModelType::EditImage => {
        // These model types don't support streaming
        return Err(LLMError::InternalError {
          message: format!("Model '{}' does not support streaming", request.model),
        });
      }
    }
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
      // Text models
      "step-1-8k".to_string(),
      "step-1-32k".to_string(),
      "step-1-256k".to_string(),
      "step-2-16k".to_string(),
      "step-2-mini".to_string(),
      "step-2-16k-202411".to_string(),
      "step-2-16k-exp".to_string(),
      
      // Image understanding models
      "step-1o-turbo-vision".to_string(),
      "step-1o-vision-32k".to_string(),
      "step-1v-8k".to_string(),
      "step-1v-32k".to_string(),
      
      // Multimodal models
      "step-3".to_string(),
      
      // Audio models (for chat completions interface)
      "step-tts-vivid".to_string(),
      "step-tts-mini".to_string(),
      "step-asr".to_string(),
      
      // Image generation models (for chat completions interface)
      "step-2x-large".to_string(),
      "step-1x-medium".to_string(),
      "step-1x-edit".to_string(),
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

// =============================================================================
// STEPFUN SPECIALIZED APIS
// =============================================================================
// 
// This section provides specialized support for StepFun's image generation,
// audio synthesis, and voice processing APIs beyond standard chat completions.

/// Image generation parameters for text2image
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Text2ImageRequest {
  pub model: String,
  pub prompt: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub size: Option<String>, // "1024x1024", "512x512", "1280x800", etc.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub n: Option<u32>, // Currently only supports 1
  #[serde(skip_serializing_if = "Option::is_none")]
  pub response_format: Option<String>, // "b64_json" or "url"
  #[serde(skip_serializing_if = "Option::is_none")]
  pub seed: Option<i32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub steps: Option<u32>, // 1-100
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cfg_scale: Option<f32>, // 1-10
  #[serde(skip_serializing_if = "Option::is_none")]
  pub style_reference: Option<StyleReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleReference {
  pub source_url: String, // URL or base64
  #[serde(skip_serializing_if = "Option::is_none")]
  pub weight: Option<f32>, // (0, 2], default 1
}

/// Image-to-image generation parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image2ImageRequest {
  pub model: String,
  pub prompt: String,
  pub source_url: String, // URL or base64
  pub source_weight: f32, // (0, 1]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub size: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub n: Option<u32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub response_format: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub seed: Option<i32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub steps: Option<u32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cfg_scale: Option<f32>,
}

/// Image edit parameters (multipart form data)
#[derive(Debug, Clone)]
pub struct ImageEditRequest {
  pub model: String,
  pub image_data: Vec<u8>, // Binary image data
  pub image_filename: String,
  pub prompt: String,
  pub seed: Option<i32>,
  pub steps: Option<u32>, // Default 28
  pub cfg_scale: Option<f32>, // Default 6
  pub size: Option<String>, // "512x512", "768x768", "1024x1024"
  pub response_format: Option<String>, // "b64_json" or "url"
}

/// Text-to-speech parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TTSRequest {
  pub model: String, // "step-tts-mini" or "step-tts-vivid"
  pub input: String, // Max 1000 characters
  pub voice: String, // Voice ID
  #[serde(skip_serializing_if = "Option::is_none")]
  pub response_format: Option<String>, // "wav", "mp3", "flac", "opus"
  #[serde(skip_serializing_if = "Option::is_none")]
  pub speed: Option<f32>, // 0.5-2.0
  #[serde(skip_serializing_if = "Option::is_none")]
  pub volume: Option<f32>, // 0.1-2.0
  #[serde(skip_serializing_if = "Option::is_none")]
  pub voice_label: Option<VoiceLabel>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub sample_rate: Option<u32>, // 8000, 16000, 22050, 24000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceLabel {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub language: Option<String>, // 粤语, 四川话, 日语
  #[serde(skip_serializing_if = "Option::is_none")]
  pub emotion: Option<String>, // 高兴, 非常高兴, 生气, etc.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub style: Option<String>, // 慢速, 极慢, 快速, 极快
}

/// Voice cloning parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceCloningRequest {
  pub model: String,
  pub text: String,
  pub file_id: String, // From file upload
  #[serde(skip_serializing_if = "Option::is_none")]
  pub sample_text: Option<String>, // Max 300 characters
}

/// ASR (Automatic Speech Recognition) parameters
#[derive(Debug, Clone)]
pub struct ASRRequest {
  pub model: String, // "step-asr"
  pub response_format: String, // "json", "text", "srt", "vtt"
  pub audio_data: Vec<u8>,
  pub filename: String,
}

/// Image generation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenerationResponse {
  pub created: u64,
  pub data: Vec<ImageData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageData {
  pub finish_reason: String, // "success" or "content_filtered"
  pub seed: i32,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub image: Option<String>, // Base64 when response_format is "b64_json"
  #[serde(skip_serializing_if = "Option::is_none")]
  pub url: Option<String>, // URL when response_format is "url"
  #[serde(skip_serializing_if = "Option::is_none")]
  pub b64_json: Option<String>, // Alternative field name for base64
}

/// Voice cloning response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceCloningResponse {
  pub id: String, // Voice ID for future use
  pub object: String, // "audio.voice"
  #[serde(skip_serializing_if = "Option::is_none")]
  pub duplicated: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub sample_text: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub sample_audio: Option<String>, // Base64 encoded wav
}

/// Voice list response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceListResponse {
  pub object: String, // "list"
  pub data: Vec<VoiceInfo>,
  pub has_more: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub first_id: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub last_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceInfo {
  pub id: String,
  pub file_id: String,
  pub created_at: u64,
}

/// StepFun specialized API client
pub struct StepFunSpecializedClient {
  client: Client,
  api_key: String,
  base_url: String,
}

impl StepFunSpecializedClient {
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

  fn build_auth_headers(&self) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Authorization", format!("Bearer {}", self.api_key).parse().unwrap());
    headers
  }

  /// Generate image from text prompt
  pub async fn text_to_image(&self, request: Text2ImageRequest) -> Result<ImageGenerationResponse> {
    let url = format!("{}/images/generations", self.base_url);
    
    let mut headers = self.build_auth_headers();
    headers.insert("Content-Type", "application/json".parse().unwrap());

    let response = self
      .client
      .post(&url)
      .headers(headers)
      .json(&request)
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

    let result: ImageGenerationResponse = response.json().await?;
    Ok(result)
  }

  /// Transform image using another image as reference
  pub async fn image_to_image(&self, request: Image2ImageRequest) -> Result<ImageGenerationResponse> {
    let url = format!("{}/images/image2image", self.base_url);
    
    let mut headers = self.build_auth_headers();
    headers.insert("Content-Type", "application/json".parse().unwrap());

    let response = self
      .client
      .post(&url)
      .headers(headers)
      .json(&request)
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

    let result: ImageGenerationResponse = response.json().await?;
    Ok(result)
  }

  /// Edit image with text instructions
  pub async fn edit_image(&self, request: ImageEditRequest) -> Result<ImageGenerationResponse> {
    let url = format!("{}/images/edits", self.base_url);
    
    let form = reqwest::multipart::Form::new()
      .text("model", request.model.clone())
      .text("prompt", request.prompt.clone())
      .part("image", 
        reqwest::multipart::Part::bytes(request.image_data)
          .file_name(request.image_filename)
          .mime_str("image/jpeg")?
      );

    let form = if let Some(seed) = request.seed {
      form.text("seed", seed.to_string())
    } else { form };

    let form = if let Some(steps) = request.steps {
      form.text("steps", steps.to_string())
    } else { form };

    let form = if let Some(cfg_scale) = request.cfg_scale {
      form.text("cfg_scale", cfg_scale.to_string())
    } else { form };

    let form = if let Some(size) = request.size {
      form.text("size", size)
    } else { form };

    let form = if let Some(response_format) = request.response_format {
      form.text("response_format", response_format)
    } else { form };

    let response = self
      .client
      .post(&url)
      .headers(self.build_auth_headers())
      .multipart(form)
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

    let result: ImageGenerationResponse = response.json().await?;
    Ok(result)
  }

  /// Convert text to speech
  pub async fn text_to_speech(&self, request: TTSRequest) -> Result<Vec<u8>> {
    let url = format!("{}/audio/speech", self.base_url);
    
    let mut headers = self.build_auth_headers();
    headers.insert("Content-Type", "application/json".parse().unwrap());

    let response = self
      .client
      .post(&url)
      .headers(headers)
      .json(&request)
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

    let audio_data = response.bytes().await?;
    Ok(audio_data.to_vec())
  }

  /// Create voice clone from audio sample
  pub async fn clone_voice(&self, request: VoiceCloningRequest) -> Result<VoiceCloningResponse> {
    let url = format!("{}/audio/voices", self.base_url);
    
    let mut headers = self.build_auth_headers();
    headers.insert("Content-Type", "application/json".parse().unwrap());

    let response = self
      .client
      .post(&url)
      .headers(headers)
      .json(&request)
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

    let result: VoiceCloningResponse = response.json().await?;
    Ok(result)
  }

  /// List available voices
  pub async fn list_voices(&self, limit: Option<u32>, order: Option<String>, 
                          before: Option<String>, after: Option<String>) -> Result<VoiceListResponse> {
    let mut url = format!("{}/audio/voices", self.base_url);
    let mut params = Vec::new();

    if let Some(limit) = limit {
      params.push(format!("limit={}", limit));
    }
    if let Some(order) = order {
      params.push(format!("order={}", order));
    }
    if let Some(before) = before {
      params.push(format!("before={}", before));
    }
    if let Some(after) = after {
      params.push(format!("after={}", after));
    }

    if !params.is_empty() {
      url.push('?');
      url.push_str(&params.join("&"));
    }

    let response = self
      .client
      .get(&url)
      .headers(self.build_auth_headers())
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

    let result: VoiceListResponse = response.json().await?;
    Ok(result)
  }

  /// Transcribe audio to text
  pub async fn speech_to_text(&self, request: ASRRequest) -> Result<String> {
    let url = format!("{}/audio/transcriptions", self.base_url);
    
    let form = reqwest::multipart::Form::new()
      .text("model", request.model.clone())
      .text("response_format", request.response_format.clone())
      .part("file", 
        reqwest::multipart::Part::bytes(request.audio_data)
          .file_name(request.filename)
          .mime_str("audio/mpeg")?
      );

    let response = self
      .client
      .post(&url)
      .headers(self.build_auth_headers())
      .multipart(form)
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

    // Handle different response formats
    match request.response_format.as_str() {
      "json" => {
        #[derive(Deserialize)]
        struct JsonResponse {
          text: String,
        }
        let json_result: JsonResponse = response.json().await?;
        Ok(json_result.text)
      },
      "text" | "srt" | "vtt" => {
        let text_result = response.text().await?;
        Ok(text_result)
      },
      _ => {
        let text_result = response.text().await?;
        Ok(text_result)
      }
    }
  }
}

/// Builder for Text2Image requests
pub struct Text2ImageBuilder {
  request: Text2ImageRequest,
}

impl Text2ImageBuilder {
  pub fn new(model: &str, prompt: &str) -> Self {
    Self {
      request: Text2ImageRequest {
        model: model.to_string(),
        prompt: prompt.to_string(),
        size: None,
        n: None,
        response_format: None,
        seed: None,
        steps: None,
        cfg_scale: None,
        style_reference: None,
      }
    }
  }

  pub fn size(mut self, size: &str) -> Self {
    self.request.size = Some(size.to_string());
    self
  }

  pub fn response_format(mut self, format: &str) -> Self {
    self.request.response_format = Some(format.to_string());
    self
  }

  pub fn seed(mut self, seed: i32) -> Self {
    self.request.seed = Some(seed);
    self
  }

  pub fn steps(mut self, steps: u32) -> Self {
    self.request.steps = Some(steps);
    self
  }

  pub fn cfg_scale(mut self, scale: f32) -> Self {
    self.request.cfg_scale = Some(scale);
    self
  }

  pub fn style_reference(mut self, source_url: &str, weight: Option<f32>) -> Self {
    self.request.style_reference = Some(StyleReference {
      source_url: source_url.to_string(),
      weight,
    });
    self
  }

  pub fn build(self) -> Text2ImageRequest {
    self.request
  }
}

/// Builder for TTS requests
pub struct TTSBuilder {
  request: TTSRequest,
}

impl TTSBuilder {
  pub fn new(model: &str, input: &str, voice: &str) -> Self {
    Self {
      request: TTSRequest {
        model: model.to_string(),
        input: input.to_string(),
        voice: voice.to_string(),
        response_format: None,
        speed: None,
        volume: None,
        voice_label: None,
        sample_rate: None,
      }
    }
  }

  pub fn response_format(mut self, format: &str) -> Self {
    self.request.response_format = Some(format.to_string());
    self
  }

  pub fn speed(mut self, speed: f32) -> Self {
    self.request.speed = Some(speed);
    self
  }

  pub fn volume(mut self, volume: f32) -> Self {
    self.request.volume = Some(volume);
    self
  }

  pub fn language(mut self, language: &str) -> Self {
    if self.request.voice_label.is_none() {
      self.request.voice_label = Some(VoiceLabel {
        language: None,
        emotion: None,
        style: None,
      });
    }
    self.request.voice_label.as_mut().unwrap().language = Some(language.to_string());
    self
  }

  pub fn emotion(mut self, emotion: &str) -> Self {
    if self.request.voice_label.is_none() {
      self.request.voice_label = Some(VoiceLabel {
        language: None,
        emotion: None,
        style: None,
      });
    }
    self.request.voice_label.as_mut().unwrap().emotion = Some(emotion.to_string());
    self
  }

  pub fn style(mut self, style: &str) -> Self {
    if self.request.voice_label.is_none() {
      self.request.voice_label = Some(VoiceLabel {
        language: None,
        emotion: None,
        style: None,
      });
    }
    self.request.voice_label.as_mut().unwrap().style = Some(style.to_string());
    self
  }

  pub fn sample_rate(mut self, rate: u32) -> Self {
    self.request.sample_rate = Some(rate);
    self
  }

  pub fn build(self) -> TTSRequest {
    self.request
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

  #[test]
  fn test_specialized_client_creation() {
    let client = StepFunSpecializedClient::new("test-key", None);
    assert!(client.is_ok());

    let client = StepFunSpecializedClient::new("", None);
    assert!(client.is_err());
  }

  #[test]
  fn test_text2image_builder() {
    let request = Text2ImageBuilder::new("step-1x-medium", "A beautiful landscape")
      .size("1024x1024")
      .response_format("b64_json")
      .seed(12345)
      .steps(50)
      .cfg_scale(7.5)
      .build();

    assert_eq!(request.model, "step-1x-medium");
    assert_eq!(request.prompt, "A beautiful landscape");
    assert_eq!(request.size, Some("1024x1024".to_string()));
    assert_eq!(request.seed, Some(12345));
  }

  #[test]
  fn test_tts_builder() {
    let request = TTSBuilder::new("step-tts-mini", "Hello world", "default_voice")
      .response_format("mp3")
      .speed(1.2)
      .emotion("高兴")
      .build();

    assert_eq!(request.model, "step-tts-mini");
    assert_eq!(request.input, "Hello world");
    assert_eq!(request.speed, Some(1.2));
    assert!(request.voice_label.is_some());
    assert_eq!(request.voice_label.unwrap().emotion, Some("高兴".to_string()));
  }
}
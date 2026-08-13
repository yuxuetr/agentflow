//! GLM (Zhipu AI / BigModel) text-to-image provider (CogView).
//!
//! Implements [`Text2ImageProvider`] via
//! `POST {base_url}/images/generations` — a synchronous request/response,
//! the same host GLM's already-registered chat provider uses
//! (`https://open.bigmodel.cn/api/paas/v4`), unlike Google/DashScope
//! where media endpoints live on a separate host from chat.
//!
//! CogVideo (GLM's text-to-video counterpart) is deliberately not
//! implemented here — it's async-task-shaped (submit + poll, like
//! DashScope's Wan) and is its own follow-up as a second
//! `Text2VideoProvider` implementation alongside Veo, not bundled into
//! this image-only batch.

use crate::{
  LLMError, Result,
  providers::modality::{
    GeneratedImage, ImageGenerationResponse, Text2ImageProvider, Text2ImageRequest,
  },
};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

pub struct GlmImageProvider {
  client: Client,
  api_key: String,
  base_url: String,
}

impl std::fmt::Debug for GlmImageProvider {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("GlmImageProvider")
      .field("base_url", &self.base_url)
      .field("api_key", &"<redacted>")
      .finish()
  }
}

impl GlmImageProvider {
  pub fn new(api_key: &str, base_url: Option<String>) -> Result<Self> {
    Self::with_client(super::default_http_client()?, api_key, base_url)
  }

  /// Construct with a caller-supplied [`reqwest::Client`]. Mirrors
  /// `OpenAIImageProvider::with_client`.
  pub fn with_client(client: Client, api_key: &str, base_url: Option<String>) -> Result<Self> {
    if api_key.is_empty() {
      return Err(LLMError::MissingApiKey {
        provider: "glm".to_string(),
      });
    }
    let base_url = base_url.unwrap_or_else(|| "https://open.bigmodel.cn/api/paas/v4".to_string());
    Ok(Self {
      client,
      api_key: api_key.to_string(),
      base_url,
    })
  }
}

/// Decode a GLM `{created, data: [{url}], content_filter: [...]}` image
/// response body into the modality [`ImageGenerationResponse`] envelope.
/// Pure function, independently unit-tested — mirrors
/// `openai_images::parse_image_response`'s style (kept as its own
/// self-contained parser rather than sharing code cross-provider, same
/// as every other batch this session).
pub(crate) fn parse_image_response(body: &str) -> Result<ImageGenerationResponse> {
  let value: Value = serde_json::from_str(body).map_err(|e| LLMError::ResponseParsingError {
    message: format!("GLM image response JSON parse failed: {e}"),
  })?;

  let created = value.get("created").and_then(|v| v.as_u64()).unwrap_or(0);

  let data = value
    .get("data")
    .and_then(|v| v.as_array())
    .ok_or_else(|| LLMError::ResponseParsingError {
      message: format!("GLM image response missing 'data' array. Body: {body}"),
    })?;

  let images = data
    .iter()
    .map(|entry| GeneratedImage {
      url: entry.get("url").and_then(|v| v.as_str()).map(String::from),
      b64_json: None,
      seed: None,
    })
    .collect();

  Ok(ImageGenerationResponse {
    created,
    images,
    metadata: Some(value),
  })
}

#[async_trait]
impl Text2ImageProvider for GlmImageProvider {
  fn name(&self) -> &str {
    "glm"
  }

  async fn generate(&self, request: Text2ImageRequest) -> Result<ImageGenerationResponse> {
    let url = format!("{}/images/generations", self.base_url);

    let mut body = serde_json::Map::new();
    body.insert("model".to_string(), Value::String(request.model.clone()));
    body.insert("prompt".to_string(), Value::String(request.prompt.clone()));
    if let Some(ref size) = request.size {
      body.insert("size".to_string(), Value::String(size.clone()));
    }

    let response = self
      .client
      .post(&url)
      .bearer_auth(&self.api_key)
      .json(&Value::Object(body))
      .send()
      .await?;

    if !response.status().is_success() {
      let status_code = response.status().as_u16();
      let message = response.text().await.unwrap_or_default();
      return Err(LLMError::HttpError {
        status_code,
        message,
      });
    }

    parse_image_response(&response.text().await?)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn empty_api_key_is_rejected_at_construction() {
    let err = GlmImageProvider::new("", None).unwrap_err();
    assert!(matches!(err, LLMError::MissingApiKey { ref provider } if provider == "glm"));
  }

  #[test]
  fn parse_image_response_extracts_url() {
    let body = json!({
      "created": 1700000000,
      "data": [{ "url": "https://example.com/img.png" }],
      "content_filter": []
    })
    .to_string();
    let response = parse_image_response(&body).expect("parse ok");
    assert_eq!(response.created, 1700000000);
    assert_eq!(response.images.len(), 1);
    assert_eq!(
      response.images[0].url.as_deref(),
      Some("https://example.com/img.png")
    );
  }

  #[test]
  fn parse_image_response_with_missing_data_returns_typed_error() {
    let body = json!({ "created": 1700000000 }).to_string();
    let err = parse_image_response(&body).unwrap_err();
    assert!(err.to_string().contains("missing 'data' array"));
  }

  #[test]
  fn parse_image_response_with_invalid_json_returns_typed_error() {
    let err = parse_image_response("{not json").unwrap_err();
    assert!(err.to_string().contains("JSON parse failed"));
  }
}

//! Google text-to-image + TTS provider, both via `generateContent`.
//!
//! Unlike the deprecated Imagen `:predict` endpoint (shutting down
//! 2026-08-17 per Google's own docs — not implemented here) and the
//! beta Interactions API (`/v1beta/interactions`, a different
//! request/response shape Google's own docs say to avoid for "stable
//! production deployments"), both modalities here go through the same
//! `POST {base_url}/v1beta/models/{model}:generateContent` endpoint
//! `GoogleProvider` (chat) already speaks — just with
//! `generationConfig.responseModalities` set to `["IMAGE"]` or
//! `["AUDIO"]`. The response carries generated media as base64
//! `inlineData` parts (`candidates[0].content.parts[].inlineData.{data,
//! mimeType}`), which this module decodes into the modality traits'
//! `Vec<u8>`/URL-or-b64_json shapes.
//!
//! No `AsrProvider` here: Gemini has no dedicated transcription REST
//! endpoint — audio input goes through `generateContent` as multimodal
//! chat content, not an ASR-shaped request/response.

use super::modality::{
  GeneratedImage, ImageGenerationResponse, Text2ImageProvider, Text2ImageRequest, TtsProvider,
  TtsRequest, TtsResponse,
};
use crate::{LLMError, Result};
use async_trait::async_trait;
use base64::Engine;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};

pub struct GoogleMediaProvider {
  client: Client,
  api_key: String,
  base_url: String,
}

impl std::fmt::Debug for GoogleMediaProvider {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("GoogleMediaProvider")
      .field("base_url", &self.base_url)
      .field("api_key", &"<redacted>")
      .finish()
  }
}

impl GoogleMediaProvider {
  pub fn new(api_key: &str, base_url: Option<String>) -> Result<Self> {
    Self::with_client(super::default_http_client()?, api_key, base_url)
  }

  /// Construct with a caller-supplied [`reqwest::Client`]. Mirrors
  /// `GoogleVeoClient::with_client`.
  pub fn with_client(client: Client, api_key: &str, base_url: Option<String>) -> Result<Self> {
    if api_key.is_empty() {
      return Err(LLMError::MissingApiKey {
        provider: "google".to_string(),
      });
    }
    let base_url =
      base_url.unwrap_or_else(|| "https://generativelanguage.googleapis.com".to_string());
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
    // Same rationale as `GoogleProvider::build_headers` (Q1.8.1): the
    // key rides in a header, never a URL.
    headers.insert(
      "x-goog-api-key",
      HeaderValue::from_str(&self.api_key).map_err(|err| LLMError::ConfigurationError {
        message: format!("Google API key contains non-ASCII bytes: {err}"),
      })?,
    );
    crate::trace_context::inject_into_headers(&mut headers);
    Ok(headers)
  }

  /// Shared core for both modalities: POST `generateContent` with a
  /// single user-text part and the given `generation_config`, then
  /// collect every response part carrying `inlineData`.
  async fn generate_content_media(
    &self,
    model: &str,
    text: &str,
    generation_config: Value,
  ) -> Result<Vec<InlineDataPart>> {
    let url = format!("{}/v1beta/models/{}:generateContent", self.base_url, model);
    let body = json!({
      "contents": [{"parts": [{"text": text}]}],
      "generationConfig": generation_config,
    });

    let response = self
      .client
      .post(&url)
      .headers(self.build_headers()?)
      .json(&body)
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

    parse_generate_content_media_response(&response.text().await?)
  }
}

/// A single `inlineData` part from a `generateContent` response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InlineDataPart {
  /// Base64-encoded media bytes, as returned by the API (not yet decoded).
  pub(crate) data: String,
  pub(crate) mime_type: String,
}

#[derive(Debug, Deserialize)]
struct GenerateContentResponse {
  #[serde(default)]
  candidates: Vec<Candidate>,
}

#[derive(Debug, Deserialize)]
struct Candidate {
  content: CandidateContent,
}

#[derive(Debug, Deserialize)]
struct CandidateContent {
  #[serde(default)]
  parts: Vec<CandidatePart>,
}

#[derive(Debug, Deserialize)]
struct CandidatePart {
  #[serde(default, rename = "inlineData")]
  inline_data: Option<InlineData>,
}

#[derive(Debug, Deserialize)]
struct InlineData {
  data: String,
  #[serde(rename = "mimeType")]
  mime_type: String,
}

/// Parse a `generateContent` response body, collecting every part that
/// carries `inlineData`. Pure function, independently unit-tested —
/// mirrors `openai_images::parse_image_response`'s style.
pub(crate) fn parse_generate_content_media_response(body: &str) -> Result<Vec<InlineDataPart>> {
  let parsed: GenerateContentResponse =
    serde_json::from_str(body).map_err(|e| LLMError::ResponseParsingError {
      message: format!("Google generateContent media response JSON parse failed: {e}"),
    })?;

  let parts: Vec<InlineDataPart> = parsed
    .candidates
    .into_iter()
    .flat_map(|c| c.content.parts)
    .filter_map(|p| p.inline_data)
    .map(|inline| InlineDataPart {
      data: inline.data,
      mime_type: inline.mime_type,
    })
    .collect();

  if parts.is_empty() {
    return Err(LLMError::ResponseParsingError {
      message: format!(
        "Google generateContent response carried no inlineData media parts. Body: {body}"
      ),
    });
  }

  Ok(parts)
}

#[async_trait]
impl Text2ImageProvider for GoogleMediaProvider {
  fn name(&self) -> &str {
    "google"
  }

  async fn generate(&self, request: Text2ImageRequest) -> Result<ImageGenerationResponse> {
    let generation_config = json!({ "responseModalities": ["IMAGE"] });
    let parts = self
      .generate_content_media(&request.model, &request.prompt, generation_config)
      .await?;

    let images = parts
      .into_iter()
      .map(|part| GeneratedImage {
        url: None,
        b64_json: Some(part.data),
        seed: None,
      })
      .collect();

    Ok(ImageGenerationResponse {
      created: 0,
      images,
      metadata: None,
    })
  }
}

#[async_trait]
impl TtsProvider for GoogleMediaProvider {
  fn name(&self) -> &str {
    "google"
  }

  async fn synthesize(&self, request: TtsRequest) -> Result<TtsResponse> {
    let generation_config = json!({
      "responseModalities": ["AUDIO"],
      "speechConfig": {
        "voiceConfig": {
          "prebuiltVoiceConfig": { "voiceName": request.voice }
        }
      }
    });
    let parts = self
      .generate_content_media(&request.model, &request.input, generation_config)
      .await?;

    let first = parts
      .into_iter()
      .next()
      .ok_or_else(|| LLMError::ResponseParsingError {
        message: "Google generateContent response carried no inlineData media parts".to_string(),
      })?;

    let audio = base64::engine::general_purpose::STANDARD
      .decode(&first.data)
      .map_err(|e| LLMError::ResponseParsingError {
        message: format!("Google TTS response inlineData.data was not valid base64: {e}"),
      })?;

    Ok(TtsResponse {
      audio,
      mime_type: first.mime_type,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn empty_api_key_is_rejected_at_construction() {
    let err = GoogleMediaProvider::new("", None).unwrap_err();
    assert!(matches!(err, LLMError::MissingApiKey { ref provider } if provider == "google"));
  }

  #[test]
  fn parse_response_collects_single_inline_data_part() {
    let body = json!({
      "candidates": [{
        "content": {
          "parts": [{
            "inlineData": { "data": "aGVsbG8=", "mimeType": "audio/pcm" }
          }]
        }
      }]
    })
    .to_string();
    let parts = parse_generate_content_media_response(&body).expect("parse ok");
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].data, "aGVsbG8=");
    assert_eq!(parts[0].mime_type, "audio/pcm");
  }

  #[test]
  fn parse_response_collects_multiple_inline_data_parts() {
    let body = json!({
      "candidates": [{
        "content": {
          "parts": [
            { "inlineData": { "data": "aGVsbG8=", "mimeType": "image/png" } },
            { "inlineData": { "data": "d29ybGQ=", "mimeType": "image/png" } }
          ]
        }
      }]
    })
    .to_string();
    let parts = parse_generate_content_media_response(&body).expect("parse ok");
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[1].data, "d29ybGQ=");
  }

  #[test]
  fn parse_response_with_no_inline_data_returns_typed_error() {
    let body = json!({
      "candidates": [{
        "content": { "parts": [{ "text": "no media here" }] }
      }]
    })
    .to_string();
    let err = parse_generate_content_media_response(&body).unwrap_err();
    assert!(err.to_string().contains("no inlineData media parts"));
  }

  #[test]
  fn parse_response_with_invalid_json_returns_typed_error() {
    let err = parse_generate_content_media_response("{not json").unwrap_err();
    assert!(err.to_string().contains("JSON parse failed"));
  }

  #[test]
  fn parse_response_with_empty_candidates_returns_typed_error() {
    let body = json!({ "candidates": [] }).to_string();
    let err = parse_generate_content_media_response(&body).unwrap_err();
    assert!(err.to_string().contains("no inlineData media parts"));
  }
}

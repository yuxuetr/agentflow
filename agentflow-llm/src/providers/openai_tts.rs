//! OpenAI text-to-speech provider.
//!
//! Implements [`TtsProvider`] via `POST {base_url}/audio/speech` — a
//! JSON request body, raw audio bytes response (unlike the JSON-wrapped
//! responses everywhere else in this crate). Known gap: OpenAI's
//! `instructions` field (natural-language tone/style guidance) has no
//! analog in [`TtsRequest`] today and is simply not sent — extending the
//! trait for one vendor's extra field is out of scope for this
//! vendor-implementation batch (P-LLM2.3).

use crate::{
  LLMError, Result,
  providers::modality::{TtsProvider, TtsRequest, TtsResponse},
};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

pub struct OpenAITtsProvider {
  client: Client,
  api_key: String,
  base_url: String,
}

impl std::fmt::Debug for OpenAITtsProvider {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("OpenAITtsProvider")
      .field("base_url", &self.base_url)
      .field("api_key", &"<redacted>")
      .finish()
  }
}

impl OpenAITtsProvider {
  pub fn new(api_key: &str, base_url: Option<String>) -> Result<Self> {
    Self::with_client(super::default_http_client()?, api_key, base_url)
  }

  /// Construct with a caller-supplied [`reqwest::Client`]. Mirrors
  /// `OpenAIAsrProvider::with_client`.
  pub fn with_client(client: Client, api_key: &str, base_url: Option<String>) -> Result<Self> {
    if api_key.is_empty() {
      return Err(LLMError::MissingApiKey {
        provider: "openai".to_string(),
      });
    }
    let base_url = base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string());
    Ok(Self {
      client,
      api_key: api_key.to_string(),
      base_url,
    })
  }

  fn build_request_body(request: &TtsRequest) -> Value {
    let mut body = serde_json::Map::new();
    body.insert("model".to_string(), Value::String(request.model.clone()));
    body.insert("input".to_string(), Value::String(request.input.clone()));
    body.insert("voice".to_string(), Value::String(request.voice.clone()));
    if let Some(ref response_format) = request.response_format {
      body.insert(
        "response_format".to_string(),
        Value::String(response_format.clone()),
      );
    }
    if let Some(speed) = request.speed {
      // f32 -> Value::Number can only fail for NaN/Inf; `speed` comes
      // from caller-supplied config, not computed floating point, so
      // this is not expected to fail in practice — fall back to
      // omitting the field rather than erroring the whole request.
      if let Some(number) = serde_json::Number::from_f64(speed as f64) {
        body.insert("speed".to_string(), Value::Number(number));
      }
    }
    Value::Object(body)
  }
}

/// Map an OpenAI TTS `response_format` to its MIME type. Falls back to
/// `audio/mpeg` (OpenAI's own documented default output is MP3) for an
/// unrecognized or missing format — mirrors
/// `stepfun::tts_mime_type_for`, but with OpenAI's actual format set and
/// default (StepFun defaults to WAV; OpenAI defaults to MP3).
fn openai_tts_mime_type_for(response_format: Option<&str>) -> &'static str {
  match response_format {
    Some("wav") => "audio/wav",
    Some("opus") => "audio/opus",
    Some("aac") => "audio/aac",
    Some("flac") => "audio/flac",
    Some("pcm") => "audio/pcm",
    Some("mp3") | None | Some(_) => "audio/mpeg",
  }
}

#[async_trait]
impl TtsProvider for OpenAITtsProvider {
  fn name(&self) -> &str {
    "openai"
  }

  async fn synthesize(&self, request: TtsRequest) -> Result<TtsResponse> {
    let url = format!("{}/audio/speech", self.base_url);
    let mime_type = openai_tts_mime_type_for(request.response_format.as_deref()).to_string();
    let body = Self::build_request_body(&request);

    let response = self
      .client
      .post(&url)
      .bearer_auth(&self.api_key)
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

    let audio = response.bytes().await?.to_vec();
    Ok(TtsResponse { audio, mime_type })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn empty_api_key_is_rejected_at_construction() {
    let err = OpenAITtsProvider::new("", None).unwrap_err();
    assert!(matches!(err, LLMError::MissingApiKey { ref provider } if provider == "openai"));
  }

  #[test]
  fn openai_tts_mime_type_covers_documented_formats_and_defaults_to_mp3() {
    assert_eq!(openai_tts_mime_type_for(Some("mp3")), "audio/mpeg");
    assert_eq!(openai_tts_mime_type_for(Some("wav")), "audio/wav");
    assert_eq!(openai_tts_mime_type_for(Some("opus")), "audio/opus");
    assert_eq!(openai_tts_mime_type_for(Some("aac")), "audio/aac");
    assert_eq!(openai_tts_mime_type_for(Some("flac")), "audio/flac");
    assert_eq!(openai_tts_mime_type_for(Some("pcm")), "audio/pcm");
    assert_eq!(openai_tts_mime_type_for(Some("unknown")), "audio/mpeg");
    assert_eq!(openai_tts_mime_type_for(None), "audio/mpeg");
  }

  #[test]
  fn build_request_body_includes_optional_fields() {
    let request = TtsRequest {
      model: "gpt-4o-mini-tts".into(),
      input: "hello world".into(),
      voice: "coral".into(),
      response_format: Some("wav".into()),
      speed: Some(1.25),
      volume: None,
      sample_rate: None,
    };
    let body = OpenAITtsProvider::build_request_body(&request);
    assert_eq!(body["model"], "gpt-4o-mini-tts");
    assert_eq!(body["input"], "hello world");
    assert_eq!(body["voice"], "coral");
    assert_eq!(body["response_format"], "wav");
    assert_eq!(body["speed"], 1.25);
  }

  #[test]
  fn build_request_body_omits_absent_optional_fields() {
    let request = TtsRequest {
      model: "gpt-4o-mini-tts".into(),
      input: "hi".into(),
      voice: "coral".into(),
      response_format: None,
      speed: None,
      volume: None,
      sample_rate: None,
    };
    let body = OpenAITtsProvider::build_request_body(&request);
    assert!(body.get("response_format").is_none());
    assert!(body.get("speed").is_none());
  }
}

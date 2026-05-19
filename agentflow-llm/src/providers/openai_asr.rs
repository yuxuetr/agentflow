//! OpenAI Whisper / GPT-4o-transcribe ASR provider.
//!
//! Implements [`AsrProvider`](super::modality::AsrProvider) by calling
//! `POST {base_url}/audio/transcriptions` with a multipart form. The
//! base URL is the same `https://api.openai.com/v1` the chat provider
//! uses, so the same `OPENAI_API_KEY` flows through.
//!
//! Supported models (from the OpenAI Whisper / transcription API):
//! - `whisper-1` — legacy, supports every `response_format` and
//!   `timestamp_granularities`.
//! - `gpt-4o-transcribe` — higher quality; only `json` / `text`
//!   response formats; no timestamps.
//! - `gpt-4o-mini-transcribe` — faster; same parameter constraints
//!   as `gpt-4o-transcribe`.
//!
//! All three accept the same multipart shape; this provider doesn't
//! gate parameters by model — the API returns a 400 if a parameter
//! is incompatible with the selected model, and we surface that
//! verbatim as `LLMError::HttpError`.

use crate::{
  LLMError, Result,
  providers::modality::{AsrProvider, AsrRequest, AsrResponse},
};
use async_trait::async_trait;
use reqwest::{
  Client,
  multipart::{Form, Part},
};

/// OpenAI-compatible ASR provider.
///
/// The `base_url` default points at OpenAI's production endpoint;
/// override it for self-hosted Whisper deployments (e.g. local
/// `faster-whisper` HTTP servers that mimic the OpenAI surface).
pub struct OpenAIAsrProvider {
  client: Client,
  api_key: String,
  base_url: String,
}

impl std::fmt::Debug for OpenAIAsrProvider {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    // Never print the API key — keep it out of trace logs and error
    // dumps even when callers `dbg!` the provider.
    f.debug_struct("OpenAIAsrProvider")
      .field("base_url", &self.base_url)
      .field("api_key", &"<redacted>")
      .finish()
  }
}

impl OpenAIAsrProvider {
  pub fn new(api_key: &str, base_url: Option<String>) -> Result<Self> {
    Self::with_client(super::default_http_client()?, api_key, base_url)
  }

  /// Construct with a caller-supplied [`reqwest::Client`]. Mirrors
  /// `OpenAIProvider::with_client` — used by tests that need
  /// `.no_proxy()` for localhost mocks, and by production deployments
  /// that share one HTTPS-pinned client across providers.
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

  /// Build the multipart form for an [`AsrRequest`].
  ///
  /// Public-in-crate so unit tests can capture the form before any
  /// HTTP call. We can't introspect a `reqwest::multipart::Form`
  /// directly, but we can at least exercise the construction path
  /// here so the test suite catches "field renamed" regressions.
  pub(crate) fn build_form(request: &AsrRequest) -> Form {
    let file_part = Part::bytes(request.audio_data.clone())
      .file_name(request.filename.clone())
      .mime_str(mime_for_filename(&request.filename))
      .unwrap_or_else(|_| {
        Part::bytes(request.audio_data.clone())
          .file_name(request.filename.clone())
          .mime_str("application/octet-stream")
          .expect("application/octet-stream is always a valid MIME")
      });

    let mut form = Form::new()
      .text("model", request.model.clone())
      .text("response_format", request.response_format.clone())
      .part("file", file_part);

    if let Some(language) = &request.language {
      form = form.text("language", language.clone());
    }
    if let Some(temperature) = request.temperature {
      form = form.text("temperature", temperature.to_string());
    }
    if let Some(prompt) = &request.prompt {
      form = form.text("prompt", prompt.clone());
    }
    form
  }
}

/// Map a filename's extension to an audio MIME type the OpenAI API
/// accepts. Falls back to `application/octet-stream` so the request
/// still flies even when the extension is unknown — the server reads
/// the codec from the bytes regardless.
fn mime_for_filename(filename: &str) -> &'static str {
  let lower = filename.to_lowercase();
  if lower.ends_with(".mp3") {
    "audio/mpeg"
  } else if lower.ends_with(".mp4") || lower.ends_with(".m4a") {
    "audio/mp4"
  } else if lower.ends_with(".mpeg") || lower.ends_with(".mpga") {
    "audio/mpeg"
  } else if lower.ends_with(".wav") {
    "audio/wav"
  } else if lower.ends_with(".webm") {
    "audio/webm"
  } else if lower.ends_with(".flac") {
    "audio/flac"
  } else if lower.ends_with(".ogg") || lower.ends_with(".opus") {
    "audio/ogg"
  } else {
    "application/octet-stream"
  }
}

/// Decode a transcription response body into the modality
/// [`AsrResponse`] envelope.
///
/// `response_format` drives the parsing strategy:
/// - `"json"` / `"verbose_json"` ⇒ parse body as JSON, pull `text`
///   field; carry the whole JSON in `metadata` so callers can opt
///   into segments / words / language detection.
/// - everything else (`"text"`, `"srt"`, `"vtt"`) ⇒ the body itself
///   IS the transcript / subtitle text.
pub(crate) fn parse_transcription_response(
  response_format: &str,
  body: &str,
) -> Result<AsrResponse> {
  match response_format {
    "json" | "verbose_json" => {
      let value: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        LLMError::ResponseParsingError {
          message: format!("OpenAI transcription JSON parse failed: {e}"),
        }
      })?;
      let text = value
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| LLMError::ResponseParsingError {
          message: format!(
            "OpenAI transcription response missing 'text' field. Body: {body}"
          ),
        })?
        .to_string();
      Ok(AsrResponse {
        text,
        metadata: Some(value),
      })
    }
    _ => Ok(AsrResponse {
      text: body.to_string(),
      metadata: None,
    }),
  }
}

#[async_trait]
impl AsrProvider for OpenAIAsrProvider {
  fn name(&self) -> &str {
    "openai"
  }

  async fn transcribe(&self, request: AsrRequest) -> Result<AsrResponse> {
    let url = format!("{}/audio/transcriptions", self.base_url);
    let response_format = request.response_format.clone();
    let form = Self::build_form(&request);

    let response = self
      .client
      .post(&url)
      .bearer_auth(&self.api_key)
      .multipart(form)
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

    let body = response.text().await?;
    parse_transcription_response(&response_format, &body)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn mime_for_filename_covers_documented_formats() {
    // OpenAI's docs enumerate: mp3, mp4, mpeg, mpga, m4a, wav, webm.
    // We also handle flac / ogg / opus since reqwest will happily send
    // them and the server reads codecs from bytes, not MIME.
    assert_eq!(mime_for_filename("clip.mp3"), "audio/mpeg");
    assert_eq!(mime_for_filename("clip.mp4"), "audio/mp4");
    assert_eq!(mime_for_filename("clip.m4a"), "audio/mp4");
    assert_eq!(mime_for_filename("clip.mpeg"), "audio/mpeg");
    assert_eq!(mime_for_filename("clip.mpga"), "audio/mpeg");
    assert_eq!(mime_for_filename("clip.wav"), "audio/wav");
    assert_eq!(mime_for_filename("clip.webm"), "audio/webm");
    assert_eq!(mime_for_filename("clip.flac"), "audio/flac");
    assert_eq!(mime_for_filename("clip.ogg"), "audio/ogg");
    assert_eq!(mime_for_filename("clip.opus"), "audio/ogg");
    // Unknown extension falls back to octet-stream so the request
    // still goes out — the server reads the codec from the bytes.
    assert_eq!(mime_for_filename("clip.unknown"), "application/octet-stream");
    // Case insensitive.
    assert_eq!(mime_for_filename("CLIP.MP3"), "audio/mpeg");
  }

  #[test]
  fn parse_json_response_pulls_text_and_preserves_metadata() {
    let body = r#"{"text":"hello world","language":"en","duration":1.23}"#;
    let response = parse_transcription_response("json", body).expect("parse ok");
    assert_eq!(response.text, "hello world");
    let metadata = response.metadata.expect("metadata preserved for json");
    assert_eq!(metadata.get("language").unwrap().as_str().unwrap(), "en");
    assert_eq!(metadata.get("duration").unwrap().as_f64().unwrap(), 1.23);
  }

  #[test]
  fn parse_verbose_json_response_pulls_text_and_preserves_segments() {
    // verbose_json carries segments + words; both must round-trip via metadata.
    let body = r#"{
      "text": "hello world",
      "segments": [{"id": 0, "start": 0.0, "end": 1.0, "text": "hello world"}],
      "language": "en"
    }"#;
    let response = parse_transcription_response("verbose_json", body).expect("parse ok");
    assert_eq!(response.text, "hello world");
    let metadata = response.metadata.expect("metadata preserved for verbose_json");
    assert_eq!(
      metadata
        .get("segments")
        .and_then(|s| s.as_array())
        .map(|a| a.len()),
      Some(1)
    );
  }

  #[test]
  fn parse_text_response_uses_body_verbatim_without_metadata() {
    // Plain text format: body IS the transcript.
    let response =
      parse_transcription_response("text", "transcript line one\ntranscript line two")
        .expect("parse ok");
    assert_eq!(response.text, "transcript line one\ntranscript line two");
    assert!(response.metadata.is_none());
  }

  #[test]
  fn parse_srt_response_uses_body_verbatim() {
    let srt = "1\n00:00:00,000 --> 00:00:01,000\nhello\n";
    let response = parse_transcription_response("srt", srt).expect("parse ok");
    assert_eq!(response.text, srt);
    assert!(response.metadata.is_none());
  }

  #[test]
  fn parse_json_with_missing_text_field_returns_typed_error() {
    let body = r#"{"language":"en"}"#;
    let err = parse_transcription_response("json", body).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("missing 'text' field"), "got: {msg}");
  }

  #[test]
  fn parse_json_with_invalid_payload_returns_typed_error() {
    let err = parse_transcription_response("json", "{not json").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("JSON parse failed"), "got: {msg}");
  }

  #[test]
  fn empty_api_key_is_rejected_at_construction() {
    let err = OpenAIAsrProvider::new("", None).unwrap_err();
    assert!(matches!(err, LLMError::MissingApiKey { ref provider } if provider == "openai"));
  }

  #[test]
  fn build_form_smoke_test() {
    // We can't introspect a reqwest::multipart::Form, but constructing
    // it must not panic on the documented optional-field combinations.
    let base_request = AsrRequest {
      model: "whisper-1".into(),
      audio_data: vec![1, 2, 3],
      filename: "clip.mp3".into(),
      response_format: "json".into(),
      language: None,
      temperature: None,
      prompt: None,
    };
    let _ = OpenAIAsrProvider::build_form(&base_request);

    let full_request = AsrRequest {
      model: "whisper-1".into(),
      audio_data: vec![1, 2, 3],
      filename: "clip.mp3".into(),
      response_format: "verbose_json".into(),
      language: Some("en".into()),
      temperature: Some(0.0),
      prompt: Some("AgentFlow, OpenAI, Whisper".into()),
    };
    let _ = OpenAIAsrProvider::build_form(&full_request);
  }
}

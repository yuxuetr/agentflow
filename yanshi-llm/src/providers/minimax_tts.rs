//! MiniMax text-to-speech provider (T2A v2).
//!
//! Implements [`TtsProvider`] via `POST {base_url}/t2a_v2` — a
//! synchronous JSON request/response, unlike DashScope's TTS (presigned
//! URL) or Google's (base64 inline data via `generateContent`): MiniMax
//! returns the audio as a **hex-encoded** string directly in the body.
//!
//! Regional host note: MiniMax splits traffic by region (international
//! `api.minimax.io` vs. mainland China `api.minimaxi.com`/
//! `api.minimaxi.chat`, sources disagree on the exact China T2A
//! subdomain) — a key only works against the host matching its own
//! account region. This crate's already-registered `minimax` chat
//! provider (`templates/default_models.yml`) uses `api.minimaxi.com`, so
//! this provider defaults to that same host for consistency with the
//! account/key setup the rest of the crate already assumes. If that
//! guess is wrong for a given deployment, `base_url` is overridable (per
//! the registry's per-model `base_url` field) and a mismatch fails as a
//! normal `HttpError` naming the real response, not a silent wrong
//! answer.

use crate::{
  LLMError, Result,
  providers::modality::{TtsProvider, TtsRequest, TtsResponse},
};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

pub struct MiniMaxTtsProvider {
  client: Client,
  api_key: String,
  base_url: String,
}

impl std::fmt::Debug for MiniMaxTtsProvider {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("MiniMaxTtsProvider")
      .field("base_url", &self.base_url)
      .field("api_key", &"<redacted>")
      .finish()
  }
}

impl MiniMaxTtsProvider {
  pub fn new(api_key: &str, base_url: Option<String>) -> Result<Self> {
    Self::with_client(super::default_http_client()?, api_key, base_url)
  }

  /// Construct with a caller-supplied [`reqwest::Client`]. Mirrors
  /// `OpenAITtsProvider::with_client`.
  pub fn with_client(client: Client, api_key: &str, base_url: Option<String>) -> Result<Self> {
    if api_key.is_empty() {
      return Err(LLMError::MissingApiKey {
        provider: "minimax".to_string(),
      });
    }
    let base_url = base_url.unwrap_or_else(|| "https://api.minimaxi.com/v1".to_string());
    Ok(Self {
      client,
      api_key: api_key.to_string(),
      base_url,
    })
  }
}

#[derive(Debug, Deserialize)]
struct T2aResponse {
  data: T2aData,
  #[serde(default)]
  extra_info: Option<T2aExtraInfo>,
  base_resp: T2aBaseResp,
}

#[derive(Debug, Deserialize)]
struct T2aData {
  audio: String,
}

#[derive(Debug, Deserialize)]
struct T2aExtraInfo {
  #[serde(default)]
  audio_format: Option<String>,
}

#[derive(Debug, Deserialize)]
struct T2aBaseResp {
  status_code: i64,
  #[serde(default)]
  status_msg: String,
}

/// Decode a hex string (as returned in `data.audio`) into raw bytes.
/// Hand-written rather than pulling in the `hex` crate for this single
/// call site — no other module in this workspace needs hex decoding.
fn decode_hex(input: &str) -> Result<Vec<u8>> {
  if !input.len().is_multiple_of(2) {
    return Err(LLMError::ResponseParsingError {
      message: "MiniMax TTS audio hex string has odd length".to_string(),
    });
  }
  let mut bytes = Vec::with_capacity(input.len() / 2);
  for chunk in input.as_bytes().chunks_exact(2) {
    let pair = std::str::from_utf8(chunk).map_err(|_| LLMError::ResponseParsingError {
      message: "MiniMax TTS audio hex string contains non-UTF8 bytes".to_string(),
    })?;
    let byte = u8::from_str_radix(pair, 16).map_err(|e| LLMError::ResponseParsingError {
      message: format!("MiniMax TTS audio hex string contains an invalid byte pair: {e}"),
    })?;
    bytes.push(byte);
  }
  Ok(bytes)
}

/// Map a MiniMax `extra_info.audio_format` value to a MIME type. Falls
/// back to `"audio/mpeg"` (MiniMax's own documented default output
/// format is mp3) for a missing or unrecognized format.
fn mime_for_audio_format(format: Option<&str>) -> &'static str {
  match format {
    Some("wav") => "audio/wav",
    Some("flac") => "audio/flac",
    Some("pcm") => "audio/pcm",
    Some("opus") => "audio/opus",
    Some("mp3") | None | Some(_) => "audio/mpeg",
  }
}

/// Parse a T2A v2 response body into raw audio bytes + a MIME type.
/// Pure function, independently unit-tested.
pub(crate) fn parse_t2a_response(body: &str) -> Result<(Vec<u8>, String)> {
  let parsed: T2aResponse =
    serde_json::from_str(body).map_err(|e| LLMError::ResponseParsingError {
      message: format!("MiniMax T2A response JSON parse failed: {e}"),
    })?;

  if parsed.base_resp.status_code != 0 {
    return Err(LLMError::ResponseParsingError {
      message: format!(
        "MiniMax T2A request failed: status_code={}, status_msg={}",
        parsed.base_resp.status_code, parsed.base_resp.status_msg
      ),
    });
  }

  let audio = decode_hex(&parsed.data.audio)?;
  let mime_type = mime_for_audio_format(
    parsed
      .extra_info
      .and_then(|info| info.audio_format)
      .as_deref(),
  )
  .to_string();

  Ok((audio, mime_type))
}

#[async_trait]
impl TtsProvider for MiniMaxTtsProvider {
  fn name(&self) -> &str {
    "minimax"
  }

  async fn synthesize(&self, request: TtsRequest) -> Result<TtsResponse> {
    let url = format!("{}/t2a_v2", self.base_url);
    let body = json!({
      "model": request.model,
      "text": request.input,
      "voice_setting": { "voice_id": request.voice },
    });

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

    let (audio, mime_type) = parse_t2a_response(&response.text().await?)?;
    Ok(TtsResponse { audio, mime_type })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn empty_api_key_is_rejected_at_construction() {
    let err = MiniMaxTtsProvider::new("", None).unwrap_err();
    assert!(matches!(err, LLMError::MissingApiKey { ref provider } if provider == "minimax"));
  }

  #[test]
  fn decode_hex_round_trips_known_bytes() {
    assert_eq!(decode_hex("68656c6c6f").unwrap(), b"hello");
    assert_eq!(decode_hex("").unwrap(), Vec::<u8>::new());
  }

  #[test]
  fn decode_hex_rejects_odd_length() {
    assert!(decode_hex("abc").is_err());
  }

  #[test]
  fn decode_hex_rejects_invalid_byte_pairs() {
    assert!(decode_hex("zz").is_err());
  }

  #[test]
  fn mime_for_audio_format_covers_documented_and_default() {
    assert_eq!(mime_for_audio_format(Some("mp3")), "audio/mpeg");
    assert_eq!(mime_for_audio_format(Some("wav")), "audio/wav");
    assert_eq!(mime_for_audio_format(Some("flac")), "audio/flac");
    assert_eq!(mime_for_audio_format(Some("pcm")), "audio/pcm");
    assert_eq!(mime_for_audio_format(Some("opus")), "audio/opus");
    assert_eq!(mime_for_audio_format(None), "audio/mpeg");
    assert_eq!(mime_for_audio_format(Some("unknown")), "audio/mpeg");
  }

  #[test]
  fn parse_t2a_response_decodes_audio_and_maps_format() {
    let body = json!({
      "data": { "audio": "68656c6c6f", "status": 2 },
      "extra_info": { "audio_format": "wav" },
      "base_resp": { "status_code": 0, "status_msg": "success" }
    })
    .to_string();
    let (audio, mime_type) = parse_t2a_response(&body).expect("parse ok");
    assert_eq!(audio, b"hello");
    assert_eq!(mime_type, "audio/wav");
  }

  #[test]
  fn parse_t2a_response_with_nonzero_status_code_returns_typed_error() {
    let body = json!({
      "data": { "audio": "", "status": 0 },
      "base_resp": { "status_code": 1004, "status_msg": "auth failed" }
    })
    .to_string();
    let err = parse_t2a_response(&body).unwrap_err();
    assert!(err.to_string().contains("auth failed"));
  }

  #[test]
  fn parse_t2a_response_with_invalid_json_returns_typed_error() {
    let err = parse_t2a_response("{not json").unwrap_err();
    assert!(err.to_string().contains("JSON parse failed"));
  }
}

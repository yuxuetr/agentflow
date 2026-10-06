//! Automatic Speech Recognition (audio → text) provider trait.

use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Request to transcribe an audio blob into text.
///
/// `audio_data` is raw bytes (mp3 / wav / flac / m4a / opus / etc.).
/// `filename` is what the provider attaches in multipart form data;
/// the extension is used by some vendors to infer codec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrRequest {
  /// Model identifier to dispatch to (e.g. `"step-asr"`, `"whisper-1"`).
  pub model: String,
  /// Raw audio bytes.
  pub audio_data: Vec<u8>,
  /// Original filename (mostly used for content-type detection).
  pub filename: String,
  /// Wire response format: `"json" | "text" | "srt" | "vtt"`.
  /// Provider-specific superset values are allowed but may not
  /// round-trip through every backend.
  pub response_format: String,
  /// Optional BCP-47 language hint (`"en"`, `"zh"`, ...). Improves
  /// recognition accuracy when known; ignored when unset.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub language: Option<String>,
  /// Optional sampling temperature (0.0..=1.0). Some providers expose
  /// it (e.g. Whisper); StepFun ignores it.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub temperature: Option<f32>,
  /// Optional context prompt — vendors like OpenAI Whisper use this to
  /// bias recognition toward domain vocabulary (acronyms, proper nouns,
  /// product names). StepFun ignores it. Capped at 224 tokens by
  /// Whisper; longer values are silently truncated by the API.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub prompt: Option<String>,
}

/// Transcript result for an [`AsrRequest`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrResponse {
  /// Recognised text.
  pub text: String,
  /// Vendor-specific extras (segments, timestamps, confidence, etc.)
  /// preserved verbatim so callers can opt-in to richer data without
  /// the trait having to enumerate every vendor's quirks.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub metadata: Option<serde_json::Value>,
}

/// Provider trait for ASR (audio → text) endpoints.
#[async_trait]
pub trait AsrProvider: Send + Sync {
  /// Short provider identifier, e.g. `"stepfun"` or `"openai"`.
  fn name(&self) -> &str;

  /// Transcribe `request` into text.
  async fn transcribe(&self, request: AsrRequest) -> Result<AsrResponse>;
}

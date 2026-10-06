//! DashScope text-to-image (Wan) + TTS (Qwen-TTS) provider.
//!
//! Unlike every other modality provider in this crate, DashScope's media
//! APIs are natively async-task-shaped for image generation: submit with
//! `X-DashScope-Async: enable` → `{task_id}`, then poll `GET
//! /api/v1/tasks/{task_id}` until done. `Text2ImageProvider::generate`
//! is a single-call trait method with no submit/poll surface (unlike
//! `Text2VideoProvider`, deliberately designed with one for Veo's
//! minutes-long jobs) — image generation is seconds-to-tens-of-seconds,
//! so this wraps submit+poll-until-done *inside* `generate()` rather
//! than exposing the async shape to callers.
//!
//! Uses the plain `https://dashscope.aliyuncs.com` host (native
//! DashScope API), NOT the `.../compatible-mode/v1` host the registered
//! `dashscope` provider's chat entries use — a different API surface
//! entirely, just the same vendor/API key.
//!
//! TTS targets Qwen-TTS (`qwen-tts-2025-05-22`), not CosyVoice: CosyVoice
//! is WebSocket-only (no REST variant documented) and this crate has no
//! WebSocket infrastructure. Qwen-TTS is a synchronous REST call that
//! returns a presigned URL to the audio (not inline bytes) — this module
//! fetches that URL with a follow-up GET to satisfy `TtsResponse.audio:
//! Vec<u8>`.
//!
//! No `AsrProvider` here: DashScope's ASR (Paraformer/Fun-ASR) requires
//! audio to already be at a publicly-accessible URL — its own docs say
//! raw bytes/binary streams aren't supported. `AsrRequest` carries raw
//! bytes with no object-storage step anywhere in this codebase to turn
//! them into a URL first, so this modality can't be implemented here.

use super::modality::{
  GeneratedImage, ImageGenerationResponse, Text2ImageProvider, Text2ImageRequest, TtsProvider,
  TtsRequest, TtsResponse,
};
use crate::{LLMError, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const MAX_WAIT: Duration = Duration::from_secs(60);

pub struct DashScopeMediaProvider {
  client: Client,
  api_key: String,
  base_url: String,
}

impl std::fmt::Debug for DashScopeMediaProvider {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("DashScopeMediaProvider")
      .field("base_url", &self.base_url)
      .field("api_key", &"<redacted>")
      .finish()
  }
}

impl DashScopeMediaProvider {
  pub fn new(api_key: &str, base_url: Option<String>) -> Result<Self> {
    Self::with_client(super::default_http_client()?, api_key, base_url)
  }

  /// Construct with a caller-supplied [`reqwest::Client`]. Mirrors
  /// `GoogleMediaProvider::with_client`.
  pub fn with_client(client: Client, api_key: &str, base_url: Option<String>) -> Result<Self> {
    if api_key.is_empty() {
      return Err(LLMError::MissingApiKey {
        provider: "dashscope".to_string(),
      });
    }
    let base_url = base_url.unwrap_or_else(|| "https://dashscope.aliyuncs.com".to_string());
    Ok(Self {
      client,
      api_key: api_key.to_string(),
      base_url,
    })
  }

  async fn poll_until_done(&self, task_id: &str) -> Result<Vec<TaskResult>> {
    let url = format!("{}/api/v1/tasks/{task_id}", self.base_url);
    let deadline = std::time::Instant::now() + MAX_WAIT;

    loop {
      let response = self
        .client
        .get(&url)
        .bearer_auth(&self.api_key)
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

      match parse_task_poll_response(&response.text().await?)? {
        TaskPollOutcome::Succeeded(results) => return Ok(results),
        TaskPollOutcome::Failed(message) => {
          return Err(LLMError::ResponseParsingError {
            message: format!("DashScope task {task_id} failed: {message}"),
          });
        }
        TaskPollOutcome::InProgress => {
          if std::time::Instant::now() >= deadline {
            return Err(LLMError::TimeoutError {
              timeout_ms: MAX_WAIT.as_millis() as u64,
            });
          }
          tokio::time::sleep(POLL_INTERVAL).await;
        }
      }
    }
  }
}

#[derive(Debug, Deserialize)]
struct TaskSubmissionResponse {
  output: TaskSubmissionOutput,
}

#[derive(Debug, Deserialize)]
struct TaskSubmissionOutput {
  task_id: String,
}

/// Parse a task-submission response, extracting `output.task_id`. Pure
/// function, independently unit-tested.
pub(crate) fn parse_task_submission_response(body: &str) -> Result<String> {
  let parsed: TaskSubmissionResponse =
    serde_json::from_str(body).map_err(|e| LLMError::ResponseParsingError {
      message: format!("DashScope task submission JSON parse failed: {e}"),
    })?;
  Ok(parsed.output.task_id)
}

#[derive(Debug, Deserialize)]
struct TaskPollResponse {
  output: TaskPollOutput,
}

#[derive(Debug, Deserialize)]
struct TaskPollOutput {
  task_status: String,
  #[serde(default)]
  results: Vec<TaskResult>,
  #[serde(default)]
  message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct TaskResult {
  pub(crate) url: String,
}

#[derive(Debug)]
pub(crate) enum TaskPollOutcome {
  Succeeded(Vec<TaskResult>),
  Failed(String),
  InProgress,
}

/// Parse a task-poll response into a [`TaskPollOutcome`]. Pure function,
/// independently unit-tested against fixture JSON for all 4
/// `task_status` values DashScope documents (PENDING/RUNNING/SUCCEEDED/FAILED).
pub(crate) fn parse_task_poll_response(body: &str) -> Result<TaskPollOutcome> {
  let parsed: TaskPollResponse =
    serde_json::from_str(body).map_err(|e| LLMError::ResponseParsingError {
      message: format!("DashScope task poll JSON parse failed: {e}"),
    })?;

  match parsed.output.task_status.as_str() {
    "SUCCEEDED" => Ok(TaskPollOutcome::Succeeded(parsed.output.results)),
    "FAILED" => Ok(TaskPollOutcome::Failed(
      parsed
        .output
        .message
        .unwrap_or_else(|| "no error message provided".to_string()),
    )),
    _ => Ok(TaskPollOutcome::InProgress),
  }
}

#[derive(Debug, Deserialize)]
struct TtsGenerationResponse {
  output: TtsGenerationOutput,
}

#[derive(Debug, Deserialize)]
struct TtsGenerationOutput {
  audio: TtsAudio,
}

#[derive(Debug, Deserialize)]
struct TtsAudio {
  url: String,
}

/// Parse a Qwen-TTS response, extracting `output.audio.url`. Pure
/// function, independently unit-tested.
pub(crate) fn parse_tts_response(body: &str) -> Result<String> {
  let parsed: TtsGenerationResponse =
    serde_json::from_str(body).map_err(|e| LLMError::ResponseParsingError {
      message: format!("DashScope TTS response JSON parse failed: {e}"),
    })?;
  Ok(parsed.output.audio.url)
}

/// Map a Qwen-TTS audio URL's extension to a MIME type. DashScope's
/// docs don't document an explicit output-format field for this
/// endpoint, so this is a best-effort inference from the URL, not a
/// documented guarantee — falls back to `"audio/wav"` (a commonly-cited
/// Qwen-TTS default in secondary sources) for an unrecognized or
/// missing extension.
fn mime_for_audio_url(url: &str) -> &'static str {
  let lower = url.to_lowercase();
  if lower.contains(".mp3") {
    "audio/mpeg"
  } else if lower.contains(".wav") {
    "audio/wav"
  } else if lower.contains(".flac") {
    "audio/flac"
  } else if lower.contains(".opus") {
    "audio/opus"
  } else {
    "audio/wav"
  }
}

#[async_trait]
impl Text2ImageProvider for DashScopeMediaProvider {
  fn name(&self) -> &str {
    "dashscope"
  }

  async fn generate(&self, request: Text2ImageRequest) -> Result<ImageGenerationResponse> {
    let url = format!(
      "{}/api/v1/services/aigc/text2image/image-synthesis",
      self.base_url
    );

    let mut parameters = serde_json::Map::new();
    if let Some(ref size) = request.size {
      parameters.insert("size".to_string(), Value::String(size.clone()));
    }
    if let Some(n) = request.n {
      parameters.insert("n".to_string(), Value::Number(n.into()));
    }
    if let Some(seed) = request.seed {
      parameters.insert("seed".to_string(), Value::Number(seed.into()));
    }

    let body = json!({
      "model": request.model,
      "input": { "prompt": request.prompt },
      "parameters": Value::Object(parameters),
    });

    let response = self
      .client
      .post(&url)
      .bearer_auth(&self.api_key)
      .header("X-DashScope-Async", "enable")
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

    let task_id = parse_task_submission_response(&response.text().await?)?;
    let results = self.poll_until_done(&task_id).await?;

    let images = results
      .into_iter()
      .map(|result| GeneratedImage {
        url: Some(result.url),
        b64_json: None,
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
impl TtsProvider for DashScopeMediaProvider {
  fn name(&self) -> &str {
    "dashscope"
  }

  async fn synthesize(&self, request: TtsRequest) -> Result<TtsResponse> {
    let url = format!(
      "{}/api/v1/services/aigc/multimodal-generation/generation",
      self.base_url
    );
    let body = json!({
      "model": request.model,
      "input": { "text": request.input, "voice": request.voice },
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

    let audio_url = parse_tts_response(&response.text().await?)?;
    let mime_type = mime_for_audio_url(&audio_url).to_string();

    let audio_response = self.client.get(&audio_url).send().await?;
    if !audio_response.status().is_success() {
      let status_code = audio_response.status().as_u16();
      return Err(LLMError::HttpError {
        status_code,
        message: "failed to fetch DashScope TTS audio from its presigned URL".to_string(),
      });
    }
    let audio = audio_response.bytes().await?.to_vec();

    Ok(TtsResponse { audio, mime_type })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn empty_api_key_is_rejected_at_construction() {
    let err = DashScopeMediaProvider::new("", None).unwrap_err();
    assert!(matches!(err, LLMError::MissingApiKey { ref provider } if provider == "dashscope"));
  }

  #[test]
  fn parse_task_submission_response_extracts_task_id() {
    let body = json!({
      "output": { "task_id": "abc123", "task_status": "PENDING" },
      "request_id": "req1"
    })
    .to_string();
    assert_eq!(parse_task_submission_response(&body).unwrap(), "abc123");
  }

  #[test]
  fn parse_task_submission_response_with_invalid_json_returns_typed_error() {
    let err = parse_task_submission_response("{not json").unwrap_err();
    assert!(err.to_string().contains("JSON parse failed"));
  }

  #[test]
  fn parse_task_submission_response_with_missing_task_id_returns_typed_error() {
    let body = json!({ "output": {}, "request_id": "req1" }).to_string();
    let err = parse_task_submission_response(&body).unwrap_err();
    assert!(err.to_string().contains("JSON parse failed"));
  }

  #[test]
  fn parse_task_poll_response_covers_all_four_states() {
    let succeeded = json!({
      "output": {
        "task_status": "SUCCEEDED",
        "results": [{"url": "https://example.com/img.png"}]
      }
    })
    .to_string();
    match parse_task_poll_response(&succeeded).unwrap() {
      TaskPollOutcome::Succeeded(results) => {
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].url, "https://example.com/img.png");
      }
      _ => panic!("expected Succeeded"),
    }

    let failed = json!({
      "output": { "task_status": "FAILED", "message": "bad prompt" }
    })
    .to_string();
    match parse_task_poll_response(&failed).unwrap() {
      TaskPollOutcome::Failed(message) => assert_eq!(message, "bad prompt"),
      _ => panic!("expected Failed"),
    }

    for status in ["PENDING", "RUNNING"] {
      let body = json!({ "output": { "task_status": status } }).to_string();
      match parse_task_poll_response(&body).unwrap() {
        TaskPollOutcome::InProgress => {}
        _ => panic!("expected InProgress for {status}"),
      }
    }
  }

  #[test]
  fn parse_task_poll_response_with_invalid_json_returns_typed_error() {
    let err = parse_task_poll_response("{not json").unwrap_err();
    assert!(err.to_string().contains("JSON parse failed"));
  }

  #[test]
  fn parse_tts_response_extracts_audio_url() {
    let body = json!({
      "output": { "audio": { "data": "", "url": "https://example.com/audio.mp3" } }
    })
    .to_string();
    assert_eq!(
      parse_tts_response(&body).unwrap(),
      "https://example.com/audio.mp3"
    );
  }

  #[test]
  fn parse_tts_response_with_missing_audio_returns_typed_error() {
    let body = json!({ "output": {} }).to_string();
    assert!(parse_tts_response(&body).is_err());
  }

  #[test]
  fn mime_for_audio_url_covers_documented_and_unknown_extensions() {
    assert_eq!(mime_for_audio_url("https://x/a.mp3?sig=1"), "audio/mpeg");
    assert_eq!(mime_for_audio_url("https://x/a.wav?sig=1"), "audio/wav");
    assert_eq!(mime_for_audio_url("https://x/a.flac"), "audio/flac");
    assert_eq!(mime_for_audio_url("https://x/a.opus"), "audio/opus");
    assert_eq!(mime_for_audio_url("https://x/a.unknown"), "audio/wav");
  }
}

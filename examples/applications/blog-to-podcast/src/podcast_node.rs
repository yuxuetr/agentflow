//! `PodcastNode` — custom `AsyncNode` wrapping the phonon-podcast pipeline.
//!
//! Plan A: thin wrapper. One AgentFlow node owns the full
//! `blog text → multi-speaker dialogue script → TTS → assembled audio +
//! SRT` chain by delegating to `phonon-podcast::OpenAiScriptGenerator`
//! and `phonon-podcast::PodcastPipeline`. Splitting the chain into
//! separate AgentFlow nodes (Plan B) is a follow-up driven by
//! dogfooding pain points.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use agentflow_core::async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult};
use agentflow_core::error::AgentFlowError;
use agentflow_core::value::FlowValue;
use async_trait::async_trait;
use phonon_ai::{EdgeTts, MiniMaxTts, OpenAiTts};
use phonon_podcast::{
  OpenAiScriptGenerator, PodcastPipeline, PodcastScript, ScriptGenerator, ScriptRequest, Segment,
  script_gen::SpeakerDef,
  subtitle::{SubtitleEntry, write_srt_file},
};
use serde_json::{Value, json};
use tracing::{info, instrument};

/// Which TTS provider to drive `phonon-podcast` with.
#[derive(Debug, Clone, Copy)]
pub enum TtsBackend {
  /// MiniMax T2A v2 (recommended; needs `MINIMAX_API_KEY`).
  MiniMax,
  /// Edge TTS (free; no key required).
  Edge,
  /// OpenAI TTS (needs `OPENAI_API_KEY`).
  OpenAi,
}

impl TtsBackend {
  /// Resolve from `PODCAST_TTS` env var; default `MiniMax`.
  pub fn from_env() -> Self {
    match std::env::var("PODCAST_TTS").ok().as_deref() {
      Some("edge") => Self::Edge,
      Some("openai") => Self::OpenAi,
      _ => Self::MiniMax,
    }
  }
}

/// Configuration for a single speaker in the podcast.
#[derive(Debug, Clone)]
pub struct SpeakerConfig {
  pub name: String,
  pub voice_id: String,
  pub role: String,
}

/// Static configuration for the `PodcastNode`. Per-run values
/// (`source_text`, `output_audio_path`) flow through `AsyncNodeInputs`.
#[derive(Debug, Clone)]
pub struct PodcastNodeConfig {
  pub host: SpeakerConfig,
  pub guest: SpeakerConfig,
  pub language: String,
  pub target_segments: usize,
  pub tts_backend: TtsBackend,
  /// Moonshot / OpenAI / DeepSeek / etc. — anything OpenAI-compatible.
  /// Reads `<env>_API_KEY` for the bearer token.
  pub llm_api_key_env: String,
  pub llm_base_url: String,
  pub llm_model: String,
}

impl PodcastNodeConfig {
  /// Default zero-OpenAI-key config: Moonshot for script, MiniMax for TTS,
  /// Mandarin Chinese voices.
  pub fn default_moonshot_minimax() -> Self {
    Self {
      host: SpeakerConfig {
        name: "小明".into(),
        // MiniMax voice — Mandarin male host.
        voice_id: "Chinese (Mandarin)_Lyrical_Voice".into(),
        role: "Podcast host: asks questions and guides conversation.".into(),
      },
      guest: SpeakerConfig {
        name: "小红".into(),
        // MiniMax voice — Mandarin female guest.
        voice_id: "Chinese (Mandarin)_HK_Flight_Attendant".into(),
        role: "Domain expert: gives insights and concrete examples.".into(),
      },
      language: "zh-CN".into(),
      target_segments: 10,
      tts_backend: TtsBackend::MiniMax,
      llm_api_key_env: "MOONSHOT_API_KEY".into(),
      llm_base_url: "https://api.moonshot.cn/v1".into(),
      // Stable canonical name on Moonshot; 128k context handles any
      // realistic blog without truncation. Override via --model.
      llm_model: "moonshot-v1-128k".into(),
    }
  }

  /// Switch to Edge TTS (free, no key) with zh-CN neural voices.
  pub fn with_edge_tts(mut self) -> Self {
    self.host.voice_id = "zh-CN-YunyangNeural".into();
    self.guest.voice_id = "zh-CN-XiaoxiaoNeural".into();
    self.tts_backend = TtsBackend::Edge;
    self
  }

  /// Override the LLM model name. Useful for swapping between Moonshot
  /// models (`moonshot-v1-128k` / `moonshot-v1-32k` / `kimi-k2.6` / …).
  pub fn with_llm_model(mut self, model: impl Into<String>) -> Self {
    self.llm_model = model.into();
    self
  }
}

/// Custom AgentFlow node — blog text → assembled podcast audio + SRT.
pub struct PodcastNode {
  config: PodcastNodeConfig,
}

impl PodcastNode {
  pub fn new(config: PodcastNodeConfig) -> Self {
    Self { config }
  }

  fn require_string_input<'a>(
    inputs: &'a AsyncNodeInputs,
    key: &str,
  ) -> Result<&'a str, AgentFlowError> {
    match inputs.get(key) {
      Some(FlowValue::Json(Value::String(s))) => Ok(s.as_str()),
      Some(_) => Err(input_error(format!("input `{key}` must be a JSON string"))),
      None => Err(input_error(format!("input `{key}` is required"))),
    }
  }

  fn build_script_request(&self, source_text: &str) -> ScriptRequest {
    ScriptRequest {
      topic: source_text.to_string(),
      speakers: vec![
        SpeakerDef {
          name: self.config.host.name.clone(),
          voice_id: self.config.host.voice_id.clone(),
          role: Some(self.config.host.role.clone()),
        },
        SpeakerDef {
          name: self.config.guest.name.clone(),
          voice_id: self.config.guest.voice_id.clone(),
          role: Some(self.config.guest.role.clone()),
        },
      ],
      system_prompt: None,
      target_segments: self.config.target_segments,
      language: self.config.language.clone(),
    }
  }

  async fn generate_script(&self, source_text: &str) -> Result<PodcastScript, AgentFlowError> {
    let api_key = std::env::var(&self.config.llm_api_key_env).map_err(|_| {
      AgentFlowError::ConfigurationError {
        message: format!(
          "env var `{}` is required for podcast script generation",
          self.config.llm_api_key_env
        ),
      }
    })?;
    let generator = OpenAiScriptGenerator::new(
      api_key,
      self.config.llm_base_url.clone(),
      self.config.llm_model.clone(),
    )
    .map_err(|err| AgentFlowError::AsyncExecutionError {
      message: format!("failed to construct OpenAiScriptGenerator: {err}"),
    })?;
    let request = self.build_script_request(source_text);
    generator
      .generate(&request)
      .await
      .map_err(|err| AgentFlowError::AsyncExecutionError {
        message: format!("podcast script generation failed: {err}"),
      })
  }

  fn override_script_voices_for_tts(&self, script: &mut PodcastScript) {
    // The script comes back with speaker `VoiceConfig`s seeded from the
    // ScriptRequest's voice_id strings. For Edge TTS we override to the
    // zh-CN Microsoft voices; for MiniMax / OpenAi the voice_ids in the
    // request already match the provider's namespace.
    for (name, voice) in script.speakers.iter_mut() {
      let cfg_voice_id = if name == &self.config.host.name {
        &self.config.host.voice_id
      } else if name == &self.config.guest.name {
        &self.config.guest.voice_id
      } else {
        continue;
      };
      voice.voice_id = cfg_voice_id.clone();
      voice.language = Some(self.config.language.clone());
    }
  }

  async fn render_audio(
    &self,
    script: &PodcastScript,
    output_audio: &PathBuf,
    output_srt: &PathBuf,
  ) -> Result<(f64, usize), AgentFlowError> {
    let buffer = match self.config.tts_backend {
      TtsBackend::MiniMax => {
        let tts = MiniMaxTts::new().map_err(|e| AgentFlowError::ConfigurationError {
          message: e.to_string(),
        })?;
        let pipeline = PodcastPipeline::new(tts);
        pipeline.generate(script).await
      }
      TtsBackend::Edge => {
        let tts = EdgeTts::new();
        let pipeline = PodcastPipeline::new(tts);
        pipeline.generate(script).await
      }
      TtsBackend::OpenAi => {
        let tts = OpenAiTts::new().map_err(|e| AgentFlowError::ConfigurationError {
          message: e.to_string(),
        })?;
        let pipeline = PodcastPipeline::new(tts);
        pipeline.generate(script).await
      }
    }
    .map_err(|err| AgentFlowError::AsyncExecutionError {
      message: format!("podcast pipeline failed: {err}"),
    })?;

    phonon_io::write_audio(output_audio, &buffer).map_err(|err| {
      AgentFlowError::AsyncExecutionError {
        message: format!("write audio to {}: {err}", output_audio.display()),
      }
    })?;

    let duration = buffer.duration_secs();
    // phonon's `segments_to_srt_file` expects STT TranscriptSegments
    // (already-timed transcript). Our script segments only carry
    // speaker + text, no timing — so we estimate timing by char-length
    // proportional split across the total rendered duration. This is
    // good enough for SRT v1; a future enhancement is to record real
    // per-segment durations from the TTS pipeline.
    let subtitle_entries = estimate_subtitle_timing(&script.segments, duration);
    write_srt_file(&subtitle_entries, output_srt).map_err(|err| {
      AgentFlowError::AsyncExecutionError {
        message: format!("write SRT to {}: {err}", output_srt.display()),
      }
    })?;

    Ok((duration, buffer.samples.len()))
  }
}

#[async_trait]
impl AsyncNode for PodcastNode {
  #[instrument(skip(self, inputs), fields(
    backend = ?self.config.tts_backend,
    target_segments = self.config.target_segments,
    language = %self.config.language,
  ))]
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let source_text = Self::require_string_input(inputs, "source_text")?.to_string();
    let output_audio_str = Self::require_string_input(inputs, "output_audio_path")?.to_string();
    let output_audio = PathBuf::from(&output_audio_str);
    let output_srt = output_audio.with_extension("srt");

    info!(
      blog_chars = source_text.chars().count(),
      "starting podcast generation"
    );

    let mut script = self.generate_script(&source_text).await?;
    self.override_script_voices_for_tts(&mut script);
    let segment_count = script.segment_count();
    info!(segments = segment_count, title = %script.title, "script ready");

    let (duration_secs, sample_count) = self
      .render_audio(&script, &output_audio, &output_srt)
      .await?;
    info!(
      duration_secs,
      sample_count,
      audio = %output_audio.display(),
      srt = %output_srt.display(),
      "podcast render complete"
    );

    let mut outputs = HashMap::new();
    outputs.insert(
      "audio_path".to_string(),
      FlowValue::File {
        path: output_audio.clone(),
        mime_type: Some(media_type_for(&output_audio).to_string()),
      },
    );
    outputs.insert(
      "srt_path".to_string(),
      FlowValue::File {
        path: output_srt,
        mime_type: Some("application/x-subrip".to_string()),
      },
    );
    outputs.insert(
      "summary".to_string(),
      FlowValue::Json(json!({
        "title": script.title,
        "segment_count": segment_count,
        "duration_secs": duration_secs,
        "sample_count": sample_count,
        "host": self.config.host.name,
        "guest": self.config.guest.name,
        "tts_backend": format!("{:?}", self.config.tts_backend),
      })),
    );
    Ok(outputs)
  }
}

fn input_error(message: impl Into<String>) -> AgentFlowError {
  AgentFlowError::NodeInputError {
    message: message.into(),
  }
}

/// Distribute total `duration_secs` across `segments` weighted by character
/// count (longer segments get more time). Falls back to uniform split if
/// every segment has zero chars or duration is non-positive.
fn estimate_subtitle_timing(segments: &[Segment], duration_secs: f64) -> Vec<SubtitleEntry> {
  if segments.is_empty() || duration_secs <= 0.0 {
    return Vec::new();
  }
  let total_chars: usize = segments.iter().map(|s| s.text.chars().count()).sum();
  let mut entries = Vec::with_capacity(segments.len());
  let mut cursor = 0.0f64;
  for segment in segments {
    let chars = segment.text.chars().count();
    let span = if total_chars == 0 {
      duration_secs / segments.len() as f64
    } else {
      duration_secs * (chars as f64 / total_chars as f64)
    };
    let start = cursor;
    let end = (cursor + span).min(duration_secs);
    entries.push(SubtitleEntry {
      start,
      end,
      text: format!("{}: {}", segment.speaker, segment.text.trim()),
    });
    cursor = end;
  }
  entries
}

fn media_type_for(path: &Path) -> &'static str {
  match path.extension().and_then(|e| e.to_str()) {
    Some("wav") => "audio/wav",
    Some("mp3") => "audio/mpeg",
    Some("flac") => "audio/flac",
    Some("ogg") => "audio/ogg",
    _ => "application/octet-stream",
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_config_uses_moonshot_minimax_with_zh_cn() {
    let cfg = PodcastNodeConfig::default_moonshot_minimax();
    assert_eq!(cfg.language, "zh-CN");
    assert!(matches!(cfg.tts_backend, TtsBackend::MiniMax));
    assert_eq!(cfg.llm_api_key_env, "MOONSHOT_API_KEY");
    assert_eq!(cfg.llm_base_url, "https://api.moonshot.cn/v1");
    assert_eq!(cfg.host.name, "小明");
    assert_eq!(cfg.guest.name, "小红");
  }

  #[test]
  fn with_edge_tts_swaps_voices_and_backend() {
    let cfg = PodcastNodeConfig::default_moonshot_minimax().with_edge_tts();
    assert!(matches!(cfg.tts_backend, TtsBackend::Edge));
    assert_eq!(cfg.host.voice_id, "zh-CN-YunyangNeural");
    assert_eq!(cfg.guest.voice_id, "zh-CN-XiaoxiaoNeural");
    // LLM stays on Moonshot — only TTS changes.
    assert_eq!(cfg.llm_api_key_env, "MOONSHOT_API_KEY");
  }

  #[test]
  fn tts_backend_from_env_defaults_to_minimax() {
    // SAFETY: tests run single-threaded by default within a binary.
    unsafe { std::env::remove_var("PODCAST_TTS") };
    assert!(matches!(TtsBackend::from_env(), TtsBackend::MiniMax));
  }

  #[test]
  fn tts_backend_from_env_honours_explicit_choices() {
    unsafe { std::env::set_var("PODCAST_TTS", "edge") };
    assert!(matches!(TtsBackend::from_env(), TtsBackend::Edge));
    unsafe { std::env::set_var("PODCAST_TTS", "openai") };
    assert!(matches!(TtsBackend::from_env(), TtsBackend::OpenAi));
    unsafe { std::env::set_var("PODCAST_TTS", "minimax") };
    assert!(matches!(TtsBackend::from_env(), TtsBackend::MiniMax));
    unsafe { std::env::remove_var("PODCAST_TTS") };
  }

  #[test]
  fn build_script_request_carries_speakers_and_language() {
    let cfg = PodcastNodeConfig::default_moonshot_minimax();
    let node = PodcastNode::new(cfg);
    let req = node.build_script_request("blog about Rust");
    assert_eq!(req.topic, "blog about Rust");
    assert_eq!(req.language, "zh-CN");
    assert_eq!(req.target_segments, 10);
    assert_eq!(req.speakers.len(), 2);
    assert_eq!(req.speakers[0].name, "小明");
    assert_eq!(req.speakers[1].name, "小红");
    assert!(req.speakers[0].role.is_some());
  }

  #[test]
  fn estimate_subtitle_timing_distributes_duration_by_char_length() {
    let segments = vec![
      Segment {
        speaker: "host".into(),
        text: "Hi.".into(), // 3 chars
        language: None,
      },
      Segment {
        speaker: "guest".into(),
        text: "Hello there friend!".into(), // 19 chars
        language: None,
      },
    ];
    let entries = estimate_subtitle_timing(&segments, 22.0);
    assert_eq!(entries.len(), 2);
    // Total chars = 22, so 1 char ≈ 1 second.
    assert!((entries[0].end - 3.0).abs() < 0.5, "got {:?}", entries[0]);
    assert!(entries[0].text.starts_with("host: "));
    assert!((entries[1].end - 22.0).abs() < 0.5, "got {:?}", entries[1]);
  }

  #[test]
  fn estimate_subtitle_timing_handles_empty_and_zero_duration() {
    assert!(estimate_subtitle_timing(&[], 5.0).is_empty());
    let segs = vec![Segment {
      speaker: "a".into(),
      text: "x".into(),
      language: None,
    }];
    assert!(estimate_subtitle_timing(&segs, 0.0).is_empty());
  }

  #[test]
  fn media_type_for_recognises_audio_extensions() {
    assert_eq!(media_type_for(&PathBuf::from("a.wav")), "audio/wav");
    assert_eq!(media_type_for(&PathBuf::from("a.mp3")), "audio/mpeg");
    assert_eq!(media_type_for(&PathBuf::from("a.flac")), "audio/flac");
    assert_eq!(media_type_for(&PathBuf::from("a.ogg")), "audio/ogg");
    assert_eq!(
      media_type_for(&PathBuf::from("a.unknown")),
      "application/octet-stream"
    );
  }
}

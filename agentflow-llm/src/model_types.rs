//! # Model Type System
//!
//! Defines the closed set of model types AgentFlow routes through. Each
//! variant maps to a distinct API shape (chat completion, embedding,
//! image generation, etc.) — what input modalities a chat model
//! actually accepts is carried separately on `ModelConfig::accepts`.
//!
//! P-LLM.0 Slice 3 collapsed `Text`, `ImageUnderstand`, `VideoUnderstand`,
//! `DocUnderstand`, `CodeGen`, and `FunctionCalling` into a single
//! `Chat` variant. Those labels were misleading: most models tagged
//! "vision" or "multimodal" in the registry are general chat models
//! that happen to accept image input (GPT-4o, Claude, Qwen-VL, Step-1v,
//! GLM-4.5V), not dedicated vision-specialist APIs. Input-modality
//! detail moved to the explicit `accepts: [...]` field per model entry.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Closed set of model types AgentFlow can route to.
///
/// One variant per distinct API shape. Input-modality detail
/// (text only, +image, +audio, +video, +document) lives on
/// `ModelConfig::accepts`, not here — see the module docs for the
/// rationale behind the P-LLM.0 collapse.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelType {
  /// Chat-shaped text-reasoning model.
  ///
  /// Output is text. Input is text plus whatever extra modalities are
  /// listed in `ModelConfig::accepts` (image / audio / video / doc).
  /// This is the canonical category for GPT / Claude / Gemini / Qwen /
  /// Moonshot / DeepSeek / Step-1 / GLM and every other chat-completion
  /// endpoint, regardless of whether the model accepts image input.
  Chat,
  /// Text-to-image generation.
  Text2Image,
  /// Image-to-image transformation.
  Image2Image,
  /// Image editing with text instructions.
  ImageEdit,
  /// Text-to-speech synthesis.
  Tts,
  /// Automatic speech recognition (audio → text).
  Asr,
  /// Text-to-video generation.
  Text2Video,
  /// Text embedding generation.
  Embedding,
}

impl ModelType {
  /// Get human-readable description of the model type
  pub fn description(&self) -> &'static str {
    match self {
      ModelType::Chat => "Chat-shaped text reasoning model",
      ModelType::Text2Image => "Text-to-image generation",
      ModelType::Image2Image => "Image-to-image transformation",
      ModelType::ImageEdit => "Image editing with text instructions",
      ModelType::Tts => "Text-to-speech synthesis",
      ModelType::Asr => "Automatic speech recognition",
      ModelType::Text2Video => "Text-to-video generation",
      ModelType::Embedding => "Text embedding generation",
    }
  }

  /// Default input modalities for this model type when an entry has no
  /// explicit `accepts:` field.
  ///
  /// `Chat` defaults to `[Text]` only — a chat model that accepts image
  /// (or audio / video) input must declare it via `ModelConfig::accepts`.
  /// This is the post-P-LLM.0 contract; the previous behaviour where
  /// `ImageUnderstand` implicitly meant `[Text, Image]` no longer
  /// applies because the variant itself is gone.
  pub fn supported_inputs(&self) -> HashSet<InputType> {
    let mut inputs = HashSet::new();
    match self {
      ModelType::Chat => {
        inputs.insert(InputType::Text);
      }
      ModelType::Text2Image | ModelType::Text2Video => {
        inputs.insert(InputType::Text);
      }
      ModelType::Image2Image | ModelType::ImageEdit => {
        inputs.insert(InputType::Text);
        inputs.insert(InputType::Image);
      }
      ModelType::Tts => {
        inputs.insert(InputType::Text);
      }
      ModelType::Asr => {
        inputs.insert(InputType::Audio);
      }
      ModelType::Embedding => {
        inputs.insert(InputType::Text);
      }
    }
    inputs
  }

  /// Get the primary output type for this model
  pub fn primary_output(&self) -> OutputType {
    match self {
      ModelType::Chat | ModelType::Asr => OutputType::Text,
      ModelType::Text2Image | ModelType::Image2Image | ModelType::ImageEdit => OutputType::Image,
      ModelType::Tts => OutputType::Audio,
      ModelType::Text2Video => OutputType::Video,
      ModelType::Embedding => OutputType::Vector,
    }
  }

  /// Whether this model type supports streaming responses by default.
  pub fn supports_streaming(&self) -> bool {
    match self {
      ModelType::Chat => true,
      ModelType::Text2Image
      | ModelType::Image2Image
      | ModelType::ImageEdit
      | ModelType::Text2Video => false,
      ModelType::Tts => true,
      ModelType::Asr | ModelType::Embedding => false,
    }
  }

  /// Whether this model requires streaming mode (no non-streaming path).
  pub fn requires_streaming(&self) -> bool {
    false
  }

  /// Whether this model type supports tool / function calling.
  ///
  /// Only `Chat` does today. Generation, TTS, ASR, and Embedding
  /// endpoints don't have a tool-call surface.
  pub fn supports_tools(&self) -> bool {
    matches!(self, ModelType::Chat)
  }

  /// Whether this model type's default `supported_inputs()` covers more
  /// than text. **Note**: this is the model-type-default view only.
  /// For the authoritative per-model answer (including explicit
  /// `accepts: [text, image]` on chat models), use
  /// `ModelConfig::is_multimodal()` or `ModelCapabilities::is_multimodal()`.
  pub fn is_multimodal(&self) -> bool {
    self.supported_inputs().len() > 1
  }

  /// Get typical use cases for this model type
  pub fn use_cases(&self) -> Vec<&'static str> {
    match self {
      ModelType::Chat => vec![
        "Conversation",
        "Q&A",
        "Reasoning",
        "Code generation",
        "Tool use",
        "Vision Q&A (when accepts includes image)",
        "Document understanding (when accepts includes document)",
      ],
      ModelType::Text2Image => vec!["Art generation", "Concept visualization", "Design mockups"],
      ModelType::Image2Image => vec!["Style transfer", "Image enhancement", "Format conversion"],
      ModelType::ImageEdit => vec!["Photo editing", "Object removal", "Style modification"],
      ModelType::Tts => vec!["Voice assistants", "Audio books", "Accessibility"],
      ModelType::Asr => vec!["Transcription", "Voice commands", "Meeting notes"],
      ModelType::Text2Video => vec!["Animation", "Video content creation", "Demonstrations"],
      ModelType::Embedding => vec!["Semantic search", "Similarity matching", "Classification"],
    }
  }
}

/// Input data types that models can accept
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputType {
  /// Plain text input
  Text,
  /// Image data (base64 or URL)
  Image,
  /// Audio data (various formats)
  Audio,
  /// Video data
  Video,
  /// Document data (PDF, etc.)
  Document,
}

impl InputType {
  /// Get supported file formats for this input type
  pub fn supported_formats(&self) -> Vec<&'static str> {
    match self {
      InputType::Text => vec!["plain/text", "utf-8"],
      InputType::Image => vec!["image/jpeg", "image/png", "image/webp", "image/gif"],
      InputType::Audio => vec![
        "audio/flac",
        "audio/mp3",
        "audio/mp4",
        "audio/mpeg",
        "audio/mpga",
        "audio/m4a",
        "audio/ogg",
        "audio/wav",
        "audio/webm",
        "audio/aac",
        "audio/opus",
      ],
      InputType::Video => vec!["video/mp4", "video/mpeg", "video/quicktime", "video/webm"],
      InputType::Document => vec!["application/pdf", "text/plain", "application/msword"],
    }
  }

  /// Check if a MIME type is supported for this input type
  pub fn supports_mime_type(&self, mime_type: &str) -> bool {
    self.supported_formats().contains(&mime_type)
  }
}

/// Output data types that models can produce
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputType {
  /// Text response
  Text,
  /// Image data
  Image,
  /// Audio data
  Audio,
  /// Video data
  Video,
  /// Numeric vector (embeddings)
  Vector,
  /// Function call with parameters
  FunctionCall,
}

impl OutputType {
  /// Get typical output formats for this type
  pub fn output_formats(&self) -> Vec<&'static str> {
    match self {
      OutputType::Text => vec!["text/plain", "application/json", "text/markdown"],
      OutputType::Image => vec!["image/png", "image/jpeg", "image/webp"],
      OutputType::Audio => vec!["audio/wav", "audio/mp3", "audio/flac", "audio/opus"],
      OutputType::Video => vec!["video/mp4", "video/webm"],
      OutputType::Vector => vec!["application/json", "application/x-numpy"],
      OutputType::FunctionCall => vec!["application/json"],
    }
  }

  /// Check if this output type can be streamed
  pub fn can_stream(&self) -> bool {
    match self {
      OutputType::Text | OutputType::Audio => true,
      OutputType::Image | OutputType::Video | OutputType::Vector | OutputType::FunctionCall => {
        false
      }
    }
  }
}

/// Model capability flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
  /// Model type with specific input/output requirements
  pub model_type: ModelType,
  /// Input modalities this model actually accepts.
  ///
  /// This is the source of truth post-P-LLM.0 — it is populated from
  /// `ModelConfig::accepts` (explicit) when present, otherwise from
  /// `ModelType::supported_inputs()` (model-type default). Callers
  /// asking "can this model take image input?" should check
  /// `accepts.contains(&InputType::Image)`, not match on `model_type`.
  #[serde(default)]
  pub accepts: HashSet<InputType>,
  /// Whether the model supports streaming responses
  pub supports_streaming: bool,
  /// Whether the model requires streaming (no non-streaming mode)
  pub requires_streaming: bool,
  /// Whether the model supports tool/function calling at all (any path).
  pub supports_tools: bool,
  /// Whether the model supports provider-native tool calling
  /// (OpenAI `tool_calls`, Anthropic `tool_use`, Google `functionCall`).
  ///
  /// When `false`, callers (e.g. ReAct) should fall back to prompt-based
  /// tool-call protocols. Defaults are derived from the model type, but
  /// individual models can override via `ModelConfig::supports_native_tool_calling`.
  #[serde(default = "default_native_tool_calling")]
  pub native_tool_calling: bool,
  /// Maximum context window size in tokens
  pub max_context_tokens: Option<u32>,
  /// Maximum output tokens per request
  pub max_output_tokens: Option<u32>,
  /// Whether the model supports system messages
  pub supports_system_messages: bool,
  /// Custom capabilities specific to the model
  pub custom_capabilities: std::collections::HashMap<String, serde_json::Value>,
}

fn default_native_tool_calling() -> bool {
  false
}

impl ModelCapabilities {
  /// Create capabilities from a model type with defaults.
  ///
  /// `native_tool_calling` is left `false` here; callers configure it
  /// from the model registry (most modern OpenAI / Anthropic / Google
  /// models opt in via the YAML `native_tool_calling: true` field).
  /// `accepts` is initialised from the model-type default — to override
  /// it (e.g., a chat model that accepts image input), set the
  /// `accepts` field on `ModelConfig` and rebuild via
  /// `ModelConfig::get_capabilities()`.
  pub fn from_model_type(model_type: ModelType) -> Self {
    let accepts = model_type.supported_inputs();
    let supports_system_messages = matches!(model_type, ModelType::Chat);
    Self {
      supports_streaming: model_type.supports_streaming(),
      requires_streaming: model_type.requires_streaming(),
      supports_tools: model_type.supports_tools(),
      native_tool_calling: false,
      supports_system_messages,
      max_context_tokens: None,
      max_output_tokens: None,
      custom_capabilities: std::collections::HashMap::new(),
      accepts,
      model_type,
    }
  }

  /// Validate if an input type is supported.
  ///
  /// Checks against the authoritative `accepts` set, so a chat model
  /// with `accepts: [text, image]` correctly returns `true` for
  /// `Image`.
  pub fn supports_input(&self, input_type: &InputType) -> bool {
    self.accepts.contains(input_type)
  }

  /// Get the expected output type
  pub fn expected_output(&self) -> OutputType {
    self.model_type.primary_output()
  }

  /// Whether the model accepts more than one input modality. Authoritative.
  pub fn is_multimodal(&self) -> bool {
    self.accepts.len() > 1
  }

  /// Validate a request against model capabilities
  pub fn validate_request(
    &self,
    has_text: bool,
    has_images: bool,
    has_audio: bool,
    has_video: bool,
    requires_streaming: bool,
    uses_tools: bool,
  ) -> Result<(), String> {
    // Check input types against the authoritative `accepts` set.
    if has_text && !self.accepts.contains(&InputType::Text) {
      return Err("Model does not support text input".to_string());
    }
    if has_images && !self.accepts.contains(&InputType::Image) {
      return Err("Model does not support image input".to_string());
    }
    if has_audio && !self.accepts.contains(&InputType::Audio) {
      return Err("Model does not support audio input".to_string());
    }
    if has_video && !self.accepts.contains(&InputType::Video) {
      return Err("Model does not support video input".to_string());
    }

    // Check streaming requirements
    if requires_streaming && !self.supports_streaming {
      return Err("Model does not support streaming".to_string());
    }
    if self.requires_streaming && !requires_streaming {
      return Err("Model requires streaming mode".to_string());
    }

    // Check tool usage
    if uses_tools && !self.supports_tools {
      return Err("Model does not support tools/function calling".to_string());
    }

    Ok(())
  }
}

/// Parse a YAML / config type string into a `ModelType`.
///
/// Accepts:
///   - Canonical post-P-LLM.0 names: `chat`, `embedding`, `tts`, `asr`,
///     `text_to_image`, `image_to_image`, `image_edit`, `text_to_video`.
///   - Pre-P-LLM.0 chat-shaped aliases (`text`, `multimodal`,
///     `imageunderstand`, `videounderstand`, `docunderstand`,
///     `codegen`, `functioncalling`) all collapse to `Chat` — that's
///     the entire point of the P-LLM.0 cleanup.
///   - Pre-P-LLM.0 image-generation aliases (`generateimage` /
///     `text2image` → `Text2Image`; `image` → `Text2Image` per the
///     pre-P-LLM.0 contract that `type: image` was used by Imagen
///     entries; `image2image` → `Image2Image`; `editimage` /
///     `imageedit` → `ImageEdit`; `text2video` → `Text2Video`).
///   - Unknown values default to `Chat` (safest fallback — preserves
///     historical default-fallback behaviour).
impl From<&str> for ModelType {
  fn from(legacy_type: &str) -> Self {
    match legacy_type {
      // Canonical post-P-LLM.0 names.
      "chat" => ModelType::Chat,
      "embedding" => ModelType::Embedding,
      "tts" => ModelType::Tts,
      "asr" => ModelType::Asr,
      "text_to_image" | "text2image" => ModelType::Text2Image,
      "image_to_image" | "image2image" => ModelType::Image2Image,
      "image_edit" | "imageedit" => ModelType::ImageEdit,
      "text_to_video" | "text2video" => ModelType::Text2Video,
      // Pre-P-LLM.0 chat-shaped aliases (all collapse to Chat).
      "text" | "multimodal" | "imageunderstand" | "videounderstand" | "docunderstand"
      | "codegen" | "functioncalling" => ModelType::Chat,
      // Pre-P-LLM.0 image-generation aliases.
      "generateimage" => ModelType::Text2Image,
      "image" => ModelType::Text2Image, // Historical Imagen mapping.
      "editimage" => ModelType::ImageEdit,
      // Pre-P-LLM.0 audio alias.
      "audio" => ModelType::Tts,
      // Unknown → safest default.
      _ => ModelType::Chat,
    }
  }
}

impl ModelType {
  /// Canonical post-P-LLM.0 YAML string for this variant.
  ///
  /// Used when serialising back to disk (`agentflow llm models
  /// --refresh-from-api`) and for diagnostic output. Naming matches
  /// `From<&str>` — round-tripping through the string form is stable.
  pub fn to_legacy_string(&self) -> &'static str {
    match self {
      ModelType::Chat => "chat",
      ModelType::Text2Image => "text_to_image",
      ModelType::Image2Image => "image_to_image",
      ModelType::ImageEdit => "image_edit",
      ModelType::Text2Video => "text_to_video",
      ModelType::Tts => "tts",
      ModelType::Asr => "asr",
      ModelType::Embedding => "embedding",
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn chat_type_capabilities_default_to_text_only() {
    let chat = ModelType::Chat;
    assert_eq!(chat.supported_inputs(), {
      let mut s = HashSet::new();
      s.insert(InputType::Text);
      s
    });
    // is_multimodal at the type level is false — vision capability
    // is expressed via ModelConfig::accepts, not the variant.
    assert!(!chat.is_multimodal());
    assert_eq!(chat.primary_output(), OutputType::Text);
    assert!(chat.supports_tools());
    assert!(chat.supports_streaming());
  }

  #[test]
  fn capabilities_with_explicit_accepts_report_multimodal() {
    // ModelCapabilities::accepts is the authoritative set. Building
    // it from a chat ModelType gives `[Text]` only; setting the
    // explicit accepts field to `[Text, Image]` makes `is_multimodal`
    // return true and `supports_input(Image)` return true — without
    // changing the variant.
    let mut caps = ModelCapabilities::from_model_type(ModelType::Chat);
    caps.accepts.insert(InputType::Image);
    assert!(caps.is_multimodal());
    assert!(caps.supports_input(&InputType::Image));
    assert!(caps.supports_input(&InputType::Text));
    assert!(!caps.supports_input(&InputType::Audio));
  }

  #[test]
  fn capabilities_validate_request_uses_accepts_set() {
    // Default chat caps reject image input...
    let chat = ModelCapabilities::from_model_type(ModelType::Chat);
    assert!(
      chat
        .validate_request(true, true, false, false, false, false)
        .is_err()
    );

    // ...but adding Image to accepts unlocks the path. Same variant,
    // different accepts → different behaviour.
    let mut vision_chat = chat.clone();
    vision_chat.accepts.insert(InputType::Image);
    assert!(
      vision_chat
        .validate_request(true, true, false, false, false, false)
        .is_ok()
    );

    // Audio is still rejected since accepts doesn't include it.
    assert!(
      vision_chat
        .validate_request(true, false, true, false, false, false)
        .is_err()
    );
  }

  #[test]
  fn input_type_mime_format_recognition() {
    let image_input = InputType::Image;
    assert!(image_input.supports_mime_type("image/jpeg"));
    assert!(image_input.supports_mime_type("image/png"));
    assert!(!image_input.supports_mime_type("text/plain"));
  }

  #[test]
  fn from_str_recognises_post_pllm0_canonical_names() {
    assert_eq!(ModelType::from("chat"), ModelType::Chat);
    assert_eq!(ModelType::from("text_to_image"), ModelType::Text2Image);
    assert_eq!(ModelType::from("image_to_image"), ModelType::Image2Image);
    assert_eq!(ModelType::from("image_edit"), ModelType::ImageEdit);
    assert_eq!(ModelType::from("text_to_video"), ModelType::Text2Video);
    assert_eq!(ModelType::from("tts"), ModelType::Tts);
    assert_eq!(ModelType::from("asr"), ModelType::Asr);
    assert_eq!(ModelType::from("embedding"), ModelType::Embedding);
  }

  #[test]
  fn from_str_collapses_legacy_chat_aliases_onto_chat() {
    for legacy in [
      "text",
      "multimodal",
      "imageunderstand",
      "videounderstand",
      "docunderstand",
      "codegen",
      "functioncalling",
    ] {
      assert_eq!(
        ModelType::from(legacy),
        ModelType::Chat,
        "legacy alias '{legacy}' should collapse to Chat"
      );
    }
  }

  #[test]
  fn from_str_maps_legacy_image_and_audio_aliases() {
    assert_eq!(ModelType::from("generateimage"), ModelType::Text2Image);
    assert_eq!(ModelType::from("image"), ModelType::Text2Image);
    assert_eq!(ModelType::from("editimage"), ModelType::ImageEdit);
    assert_eq!(ModelType::from("text2image"), ModelType::Text2Image);
    assert_eq!(ModelType::from("image2image"), ModelType::Image2Image);
    assert_eq!(ModelType::from("imageedit"), ModelType::ImageEdit);
    assert_eq!(ModelType::from("audio"), ModelType::Tts);
  }

  #[test]
  fn legacy_string_round_trip_is_stable() {
    for variant in [
      ModelType::Chat,
      ModelType::Text2Image,
      ModelType::Image2Image,
      ModelType::ImageEdit,
      ModelType::Text2Video,
      ModelType::Tts,
      ModelType::Asr,
      ModelType::Embedding,
    ] {
      let s = variant.to_legacy_string();
      assert_eq!(ModelType::from(s), variant, "round-trip failed for '{s}'");
    }
  }
}

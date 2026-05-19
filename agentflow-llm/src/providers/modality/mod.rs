//! Per-modality provider traits for non-chat APIs.
//!
//! Chat-shaped models route through `LLMProvider` (in
//! [`crate::providers`]). The five trait surfaces here cover the
//! non-chat modalities AgentFlow ships nodes for:
//!
//! - [`asr::AsrProvider`] — audio → text (Whisper, StepFun ASR)
//! - [`tts::TtsProvider`] — text → audio (StepFun TTS, ElevenLabs)
//! - [`text_to_image::Text2ImageProvider`] — text → image
//!   (DALL-E, Imagen, StepFun T2I)
//! - [`image_to_image::Image2ImageProvider`] — image+text → image
//! - [`image_edit::ImageEditProvider`] — image+text → image
//!   (DALL-E edit, StepFun image edit)
//!
//! Each trait is intentionally narrow — modality shapes diverge enough
//! that a single "MultimodalProvider" abstraction would leak. Common
//! response shape (URL or base64 image bytes) is shared via
//! [`ImageGenerationResponse`] / [`GeneratedImage`].
//!
//! ## P-LLM.1 contract
//!
//! These traits are the seam that lets `agentflow-nodes::{asr, tts,
//! text_to_image, image_to_image, image_edit}` drop their direct
//! StepFun coupling (current code calls `AgentFlow::stepfun_client(...)`
//! and `providers::stepfun::*Request` types directly). Until P-LLM.3
//! lands, the nodes still go through StepFun directly — this module
//! is what they'll route through instead.

pub mod asr;
pub mod image_edit;
pub mod image_to_image;
pub mod text_to_image;
pub mod tts;

pub use asr::{AsrProvider, AsrRequest, AsrResponse};
pub use image_edit::{ImageEditProvider, ImageEditRequest};
pub use image_to_image::{Image2ImageProvider, Image2ImageRequest};
pub use text_to_image::{Text2ImageProvider, Text2ImageRequest};
pub use tts::{TtsProvider, TtsRequest, TtsResponse};

use serde::{Deserialize, Serialize};

/// Common response envelope for image-generating endpoints.
///
/// Used by `Text2ImageProvider`, `Image2ImageProvider`, and
/// `ImageEditProvider` since vendor responses for these three share a
/// shape (a list of images, each either a URL or a base64 blob, plus
/// a creation timestamp). Vendor-specific bookkeeping that doesn't fit
/// here belongs in `metadata`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageGenerationResponse {
  /// Unix timestamp the provider says the image was created.
  pub created: u64,
  /// Generated images. Length depends on the request's `n` field.
  pub images: Vec<GeneratedImage>,
  /// Vendor-specific extras (request id, billing info, etc.) preserved
  /// for caller inspection without forcing every consumer to know
  /// about them.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub metadata: Option<serde_json::Value>,
}

/// A single image entry in an [`ImageGenerationResponse`].
///
/// Exactly one of `url` / `b64_json` is populated, mirroring the
/// `response_format: "url" | "b64_json"` choice in the request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedImage {
  /// HTTP URL the vendor will serve the image from (time-limited on
  /// most providers).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub url: Option<String>,
  /// Base64-encoded image bytes (no `data:` prefix).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub b64_json: Option<String>,
  /// Seed used to generate this image, when known.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub seed: Option<i32>,
}

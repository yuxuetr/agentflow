//! Image-to-Image (image+text → image) provider trait.

use super::ImageGenerationResponse;
use crate::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Request to transform a source image into a new image guided by a
/// text prompt.
///
/// `source_url` accepts either a public HTTP URL or a `data:` URI;
/// vendors that need raw bytes can decode the data URI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image2ImageRequest {
  /// Model identifier.
  pub model: String,
  /// Text prompt for the transformation.
  pub prompt: String,
  /// Source image — URL or `data:` URI.
  pub source_url: String,
  /// Weight of the source image in the (0.0, 1.0] range. Higher
  /// values keep the output closer to the source.
  pub source_weight: f32,
  /// Output image size, vendor-specific format.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub size: Option<String>,
  /// Number of images to generate.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub n: Option<u32>,
  /// Wire response format: `"url"` or `"b64_json"`.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub response_format: Option<String>,
  /// Random seed.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub seed: Option<i32>,
  /// Sampling steps.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub steps: Option<u32>,
  /// Classifier-free guidance scale.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub cfg_scale: Option<f32>,
}

/// Provider trait for image-to-image endpoints.
#[async_trait]
pub trait Image2ImageProvider: Send + Sync {
  /// Short provider identifier.
  fn name(&self) -> &str;

  /// Transform the source image per `request`.
  async fn transform(&self, request: Image2ImageRequest) -> Result<ImageGenerationResponse>;
}

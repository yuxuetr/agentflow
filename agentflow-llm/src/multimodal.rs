//! # Multimodal Support for AgentFlow LLM
//!
//! This module provides support for multimodal inputs (text + images) to LLMs.
//!
//! ## Example Usage
//!
//! ```ignore
//! use agentflow_llm::{AgentFlow, multimodal::{MultimodalMessage, MessageContent}};
//!
//! let message = MultimodalMessage::new("user")
//!   .add_text("Describe this image in elegant language")
//!   .add_image_url("https://example.com/image.jpg")
//!   .build();
//!
//! let response = AgentFlow::model("step-1o-turbo-vision")
//!   .multimodal_prompt(message)
//!   .temperature(0.7)
//!   .execute().await?;
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Content types that can be included in multimodal messages.
///
/// `Document*` and `Video*` are provider-restricted today: only Anthropic
/// translates `Document*` (PDF) content, and only Google translates `Video*`
/// content (see `providers::anthropic::openai_content_to_anthropic_content`
/// and `providers::google::openai_content_to_gemini_parts`). Sending either
/// to a model whose registry `accepts:` doesn't declare that modality fails
/// at `validate_request` time with `LLMError::InvalidModelConfig`, before a
/// request is ever built.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageContent {
  /// Plain text content
  Text { text: String },
  /// Image from URL
  ImageUrl { image_url: ImageUrl },
  /// Base64 encoded image
  ImageData { image_data: ImageData },
  /// PDF/document from a URL (Anthropic only)
  DocumentUrl { document_url: DocumentUrl },
  /// Base64 encoded PDF/document (Anthropic only)
  DocumentData { document_data: DocumentData },
  /// Video from a URL, including YouTube links (Google only)
  VideoUrl { video_url: VideoUrl },
  /// Base64 encoded video (Google only)
  VideoData { video_data: VideoData },
}

/// Image URL configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
  pub url: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub detail: Option<String>, // "low", "high", "auto"
}

/// Base64 image data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageData {
  pub data: String,       // base64 encoded data
  pub media_type: String, // "image/jpeg", "image/png", etc.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub detail: Option<String>, // "low", "high", "auto"
}

/// PDF/document URL configuration. No `media_type` field — Anthropic's
/// `document` content block infers it server-side for a `url` source (see
/// the `source: { type: "url", url: ... }` shape in the Anthropic API
/// docs); it's only required alongside base64 payloads ([`DocumentData`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentUrl {
  pub url: String,
}

/// Base64 PDF/document data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentData {
  pub data: String,       // base64 encoded data
  pub media_type: String, // "application/pdf"
}

/// Video URL configuration. `media_type` is an optional hint for a remote
/// reference whose MIME type can't be inferred from the URL — Google's
/// `file_data` accepts an explicit `mime_type` alongside `file_uri`, but
/// omits it entirely for YouTube links (`youtube.com/watch` /
/// `youtu.be/`), which the adapter detects and handles regardless of
/// whether this field is set.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoUrl {
  pub url: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub media_type: Option<String>, // "video/mp4", etc.
}

/// Base64 video data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoData {
  pub data: String,       // base64 encoded data
  pub media_type: String, // "video/mp4", etc.
}

impl MessageContent {
  /// Create text content
  pub fn text<S: Into<String>>(text: S) -> Self {
    Self::Text { text: text.into() }
  }

  /// Create image URL content
  pub fn image_url<S: Into<String>>(url: S) -> Self {
    Self::ImageUrl {
      image_url: ImageUrl {
        url: url.into(),
        detail: None,
      },
    }
  }

  /// Create image URL content with detail level
  pub fn image_url_with_detail<S: Into<String>>(url: S, detail: S) -> Self {
    Self::ImageUrl {
      image_url: ImageUrl {
        url: url.into(),
        detail: Some(detail.into()),
      },
    }
  }

  /// Create base64 image content
  pub fn image_data<S: Into<String>>(data: S, media_type: S) -> Self {
    Self::ImageData {
      image_data: ImageData {
        data: data.into(),
        media_type: media_type.into(),
        detail: None,
      },
    }
  }

  /// Create base64 image content with detail level
  pub fn image_data_with_detail<S: Into<String>>(data: S, media_type: S, detail: S) -> Self {
    Self::ImageData {
      image_data: ImageData {
        data: data.into(),
        media_type: media_type.into(),
        detail: Some(detail.into()),
      },
    }
  }

  /// Create PDF/document URL content (Anthropic only — see [`MessageContent`] docs)
  pub fn document_url<S: Into<String>>(url: S) -> Self {
    Self::DocumentUrl {
      document_url: DocumentUrl { url: url.into() },
    }
  }

  /// Create base64 PDF/document content (Anthropic only)
  pub fn document_data<S: Into<String>>(data: S, media_type: S) -> Self {
    Self::DocumentData {
      document_data: DocumentData {
        data: data.into(),
        media_type: media_type.into(),
      },
    }
  }

  /// Create video URL content (Google only — see [`MessageContent`] docs)
  pub fn video_url<S: Into<String>>(url: S) -> Self {
    Self::VideoUrl {
      video_url: VideoUrl {
        url: url.into(),
        media_type: None,
      },
    }
  }

  /// Create video URL content with an explicit MIME-type hint (Google only)
  pub fn video_url_with_media_type<S: Into<String>>(url: S, media_type: S) -> Self {
    Self::VideoUrl {
      video_url: VideoUrl {
        url: url.into(),
        media_type: Some(media_type.into()),
      },
    }
  }

  /// Create base64 video content (Google only)
  pub fn video_data<S: Into<String>>(data: S, media_type: S) -> Self {
    Self::VideoData {
      video_data: VideoData {
        data: data.into(),
        media_type: media_type.into(),
      },
    }
  }

  /// Check if this content is text
  pub fn is_text(&self) -> bool {
    matches!(self, MessageContent::Text { .. })
  }

  /// Check if this content is an image
  pub fn is_image(&self) -> bool {
    matches!(
      self,
      MessageContent::ImageUrl { .. } | MessageContent::ImageData { .. }
    )
  }

  /// Check if this content is a PDF/document
  pub fn is_document(&self) -> bool {
    matches!(
      self,
      MessageContent::DocumentUrl { .. } | MessageContent::DocumentData { .. }
    )
  }

  /// Check if this content is a video
  pub fn is_video(&self) -> bool {
    matches!(
      self,
      MessageContent::VideoUrl { .. } | MessageContent::VideoData { .. }
    )
  }

  /// Get text content if this is text
  pub fn as_text(&self) -> Option<&String> {
    match self {
      MessageContent::Text { text } => Some(text),
      _ => None,
    }
  }
}

/// A multimodal message that can contain text and images
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultimodalMessage {
  pub role: String,
  pub content: Vec<MessageContent>,
  #[serde(skip_serializing_if = "HashMap::is_empty")]
  pub metadata: HashMap<String, Value>,
}

impl MultimodalMessage {
  /// Create a new multimodal message
  #[allow(clippy::new_ret_no_self)]
  pub fn new<S: Into<String>>(role: S) -> MultimodalMessageBuilder {
    MultimodalMessageBuilder {
      role: role.into(),
      content: Vec::new(),
      metadata: HashMap::new(),
    }
  }

  /// Create a user message
  pub fn user() -> MultimodalMessageBuilder {
    Self::new("user")
  }

  /// Create a system message
  pub fn system() -> MultimodalMessageBuilder {
    Self::new("system")
  }

  /// Create an assistant message
  pub fn assistant() -> MultimodalMessageBuilder {
    Self::new("assistant")
  }

  /// Check if message contains only text
  pub fn is_text_only(&self) -> bool {
    self.content.iter().all(|c| c.is_text())
  }

  /// Check if message contains images
  pub fn has_images(&self) -> bool {
    self.content.iter().any(|c| c.is_image())
  }

  /// Check if message contains a PDF/document (Anthropic only — see
  /// [`MessageContent`] docs)
  pub fn has_document(&self) -> bool {
    self.content.iter().any(|c| c.is_document())
  }

  /// Check if message contains a video (Google only — see [`MessageContent`] docs)
  pub fn has_video(&self) -> bool {
    self.content.iter().any(|c| c.is_video())
  }

  /// Get all text content concatenated
  pub fn get_text(&self) -> String {
    self
      .content
      .iter()
      .filter_map(|c| c.as_text())
      .cloned()
      .collect::<Vec<_>>()
      .join(" ")
  }

  /// Convert to OpenAI-compatible JSON format.
  ///
  /// This is the one boundary every provider adapter's `build_request_body`
  /// consumes (`ProviderRequest::messages: Vec<Value>`), so each
  /// [`MessageContent`] variant must serialize into the shape OpenAI's real
  /// Chat Completions API — and every OpenAI-compatible vendor riding the
  /// same wire format (Moonshot, StepFun, DashScope, GLM, DeepSeek, MiniMax)
  /// — actually accepts. Crucially, `ImageData` (base64) is *not* its own
  /// content-block type on that wire format; OpenAI only recognizes
  /// `image_url`, with a `data:<mime>;base64,<payload>` URI standing in for
  /// an inline image. Serializing `ImageData` under its own derived
  /// `"image_data"` tag (P-LLM2.4 finding) produced a block no real provider
  /// understands, silently breaking every base64-image call.
  pub fn to_openai_format(&self) -> Value {
    let content: Vec<Value> = self
      .content
      .iter()
      .map(Self::content_to_openai_value)
      .collect();
    serde_json::json!({
      "role": self.role,
      "content": content
    })
  }

  fn content_to_openai_value(content: &MessageContent) -> Value {
    match content {
      MessageContent::Text { text } => serde_json::json!({
        "type": "text",
        "text": text
      }),
      MessageContent::ImageUrl { image_url } => serde_json::json!({
        "type": "image_url",
        "image_url": image_url
      }),
      MessageContent::ImageData { image_data } => {
        let image_url = ImageUrl {
          url: format!("data:{};base64,{}", image_data.media_type, image_data.data),
          detail: image_data.detail.clone(),
        };
        serde_json::json!({
          "type": "image_url",
          "image_url": image_url
        })
      }
      MessageContent::DocumentUrl { document_url } => serde_json::json!({
        "type": "document_url",
        "document_url": document_url
      }),
      MessageContent::DocumentData { document_data } => {
        let document_url = DocumentUrl {
          url: format!(
            "data:{};base64,{}",
            document_data.media_type, document_data.data
          ),
        };
        serde_json::json!({
          "type": "document_url",
          "document_url": document_url
        })
      }
      MessageContent::VideoUrl { video_url } => serde_json::json!({
        "type": "video_url",
        "video_url": video_url
      }),
      MessageContent::VideoData { video_data } => {
        let video_url = VideoUrl {
          url: format!("data:{};base64,{}", video_data.media_type, video_data.data),
          media_type: None,
        };
        serde_json::json!({
          "type": "video_url",
          "video_url": video_url
        })
      }
    }
  }

  /// Convert to simple text format (for text-only models)
  pub fn to_text_format(&self) -> String {
    if self.is_text_only() {
      self.get_text()
    } else {
      // For mixed content, include placeholders for images
      self
        .content
        .iter()
        .map(|content| match content {
          MessageContent::Text { text } => text.clone(),
          MessageContent::ImageUrl { .. } => "[Image from URL]".to_string(),
          MessageContent::ImageData { .. } => "[Image Data]".to_string(),
          MessageContent::DocumentUrl { .. } => "[Document from URL]".to_string(),
          MessageContent::DocumentData { .. } => "[Document Data]".to_string(),
          MessageContent::VideoUrl { .. } => "[Video from URL]".to_string(),
          MessageContent::VideoData { .. } => "[Video Data]".to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
    }
  }
}

/// Builder for creating multimodal messages
pub struct MultimodalMessageBuilder {
  role: String,
  content: Vec<MessageContent>,
  metadata: HashMap<String, Value>,
}

impl MultimodalMessageBuilder {
  /// Add text content
  pub fn add_text<S: Into<String>>(mut self, text: S) -> Self {
    self.content.push(MessageContent::text(text));
    self
  }

  /// Add image from URL
  pub fn add_image_url<S: Into<String>>(mut self, url: S) -> Self {
    self.content.push(MessageContent::image_url(url));
    self
  }

  /// Add image from URL with detail level
  pub fn add_image_url_with_detail<S: Into<String>>(mut self, url: S, detail: S) -> Self {
    self
      .content
      .push(MessageContent::image_url_with_detail(url, detail));
    self
  }

  /// Add base64 image data
  pub fn add_image_data<S: Into<String>>(mut self, data: S, media_type: S) -> Self {
    self
      .content
      .push(MessageContent::image_data(data, media_type));
    self
  }

  /// Add base64 image data with detail level
  pub fn add_image_data_with_detail<S: Into<String>>(
    mut self,
    data: S,
    media_type: S,
    detail: S,
  ) -> Self {
    self.content.push(MessageContent::image_data_with_detail(
      data, media_type, detail,
    ));
    self
  }

  /// Add PDF/document from a URL (Anthropic only — see [`MessageContent`] docs)
  pub fn add_document_url<S: Into<String>>(mut self, url: S) -> Self {
    self.content.push(MessageContent::document_url(url));
    self
  }

  /// Add base64 PDF/document data (Anthropic only)
  pub fn add_document_data<S: Into<String>>(mut self, data: S, media_type: S) -> Self {
    self
      .content
      .push(MessageContent::document_data(data, media_type));
    self
  }

  /// Add video from a URL, including YouTube links (Google only — see
  /// [`MessageContent`] docs)
  pub fn add_video_url<S: Into<String>>(mut self, url: S) -> Self {
    self.content.push(MessageContent::video_url(url));
    self
  }

  /// Add video from a URL with an explicit MIME-type hint (Google only)
  pub fn add_video_url_with_media_type<S: Into<String>>(mut self, url: S, media_type: S) -> Self {
    self
      .content
      .push(MessageContent::video_url_with_media_type(url, media_type));
    self
  }

  /// Add base64 video data (Google only)
  pub fn add_video_data<S: Into<String>>(mut self, data: S, media_type: S) -> Self {
    self
      .content
      .push(MessageContent::video_data(data, media_type));
    self
  }

  /// Add arbitrary content
  pub fn add_content(mut self, content: MessageContent) -> Self {
    self.content.push(content);
    self
  }

  /// Add metadata
  pub fn add_metadata<K: Into<String>, V: Into<Value>>(mut self, key: K, value: V) -> Self {
    self.metadata.insert(key.into(), value.into());
    self
  }

  /// Build the multimodal message
  pub fn build(self) -> MultimodalMessage {
    MultimodalMessage {
      role: self.role,
      content: self.content,
      metadata: self.metadata,
    }
  }
}

/// Helper functions for creating common multimodal patterns
impl MultimodalMessage {
  /// Create a text-only message (shortcut)
  pub fn text<R: Into<String>, T: Into<String>>(role: R, text: T) -> Self {
    Self::new(role).add_text(text).build()
  }

  /// Create a text + image URL message (common pattern)
  pub fn text_and_image<R: Into<String>, T: Into<String>, U: Into<String>>(
    role: R,
    text: T,
    image_url: U,
  ) -> Self {
    Self::new(role)
      .add_text(text)
      .add_image_url(image_url)
      .build()
  }

  /// Create a message with multiple images and text
  pub fn text_and_images<R: Into<String>, T: Into<String>, U: Into<String>>(
    role: R,
    text: T,
    image_urls: Vec<U>,
  ) -> Self {
    let mut builder = Self::new(role).add_text(text);
    for url in image_urls {
      builder = builder.add_image_url(url);
    }
    builder.build()
  }
}

/// Conversion from simple string to text-only multimodal message
impl From<String> for MultimodalMessage {
  fn from(text: String) -> Self {
    Self::text("user", text)
  }
}

impl From<&str> for MultimodalMessage {
  fn from(text: &str) -> Self {
    Self::text("user", text)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_create_text_message() {
    let msg = MultimodalMessage::text("user", "Hello world");
    assert_eq!(msg.role, "user");
    assert_eq!(msg.content.len(), 1);
    assert!(msg.is_text_only());
    assert!(!msg.has_images());
  }

  #[test]
  fn test_create_multimodal_message() {
    let msg = MultimodalMessage::text_and_image(
      "user",
      "Describe this image",
      "https://example.com/image.jpg",
    );
    assert_eq!(msg.role, "user");
    assert_eq!(msg.content.len(), 2);
    assert!(!msg.is_text_only());
    assert!(msg.has_images());
  }

  #[test]
  fn test_builder_pattern() {
    let msg = MultimodalMessage::user()
      .add_text("Here are some images:")
      .add_image_url("https://example.com/1.jpg")
      .add_image_url("https://example.com/2.jpg")
      .add_metadata("source", "test")
      .build();

    assert_eq!(msg.content.len(), 3);
    assert!(msg.has_images());
    assert_eq!(msg.metadata.get("source").unwrap(), "test");
  }

  #[test]
  fn test_openai_format_conversion() {
    let msg = MultimodalMessage::text_and_image(
      "user",
      "What's in this image?",
      "https://example.com/test.jpg",
    );

    let json = msg.to_openai_format();
    assert_eq!(json["role"], "user");
    assert!(json["content"].is_array());
    assert_eq!(json["content"].as_array().unwrap().len(), 2);
  }

  /// P-LLM2.4 regression: base64 image content must serialize as an
  /// `image_url` block with a `data:` URI — the real OpenAI-compatible wire
  /// shape — not the non-standard derived `"image_data"` tag no provider
  /// (OpenAI-compatible or otherwise) actually understands.
  #[test]
  fn to_openai_format_encodes_image_data_as_data_uri_image_url() {
    let msg = MultimodalMessage::user()
      .add_text("what is this")
      .add_image_data("aGVsbG8=", "image/png")
      .build();

    let json = msg.to_openai_format();
    let parts = json["content"].as_array().unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[1]["type"], "image_url");
    assert_eq!(
      parts[1]["image_url"]["url"],
      "data:image/png;base64,aGVsbG8="
    );
    assert!(parts[1].get("image_data").is_none());
  }

  #[test]
  fn to_openai_format_image_data_omits_detail_when_unset() {
    let msg = MultimodalMessage::user()
      .add_image_data("aGVsbG8=", "image/png")
      .build();

    let json = msg.to_openai_format();
    let parts = json["content"].as_array().unwrap();
    assert!(parts[0]["image_url"].get("detail").is_none());
  }

  #[test]
  fn has_document_and_has_video_reflect_content() {
    let text_only = MultimodalMessage::text("user", "hi");
    assert!(!text_only.has_document());
    assert!(!text_only.has_video());

    let with_document = MultimodalMessage::user()
      .add_document_url("https://example.com/report.pdf")
      .build();
    assert!(with_document.has_document());
    assert!(!with_document.has_video());
    assert!(!with_document.has_images());

    let with_video = MultimodalMessage::user()
      .add_video_url("https://example.com/clip.mp4")
      .build();
    assert!(with_video.has_video());
    assert!(!with_video.has_document());
  }

  #[test]
  fn to_openai_format_document_url_round_trips() {
    let msg = MultimodalMessage::user()
      .add_document_url("https://example.com/report.pdf")
      .build();

    let json = msg.to_openai_format();
    let parts = json["content"].as_array().unwrap();
    assert_eq!(parts[0]["type"], "document_url");
    assert_eq!(
      parts[0]["document_url"]["url"],
      "https://example.com/report.pdf"
    );
  }

  /// Base64 documents collapse into the same `document_url` kind as remote
  /// URLs, with the payload embedded as a `data:` URI — mirroring how
  /// `ImageData` collapses into `image_url`.
  #[test]
  fn to_openai_format_encodes_document_data_as_data_uri_document_url() {
    let msg = MultimodalMessage::user()
      .add_document_data("aGVsbG8=", "application/pdf")
      .build();

    let json = msg.to_openai_format();
    let parts = json["content"].as_array().unwrap();
    assert_eq!(parts[0]["type"], "document_url");
    assert_eq!(
      parts[0]["document_url"]["url"],
      "data:application/pdf;base64,aGVsbG8="
    );
    assert!(parts[0].get("document_data").is_none());
  }

  #[test]
  fn to_openai_format_video_url_carries_optional_media_type() {
    let msg = MultimodalMessage::user()
      .add_video_url("https://example.com/clip.mp4")
      .build();
    let json = msg.to_openai_format();
    let parts = json["content"].as_array().unwrap();
    assert_eq!(parts[0]["type"], "video_url");
    assert_eq!(parts[0]["video_url"]["url"], "https://example.com/clip.mp4");
    assert!(parts[0]["video_url"].get("media_type").is_none());

    let msg = MultimodalMessage::user()
      .add_video_url_with_media_type("https://example.com/clip.webm", "video/webm")
      .build();
    let json = msg.to_openai_format();
    let parts = json["content"].as_array().unwrap();
    assert_eq!(parts[0]["video_url"]["media_type"], "video/webm");
  }

  /// Base64 video collapses into the same `video_url` kind as remote URLs.
  #[test]
  fn to_openai_format_encodes_video_data_as_data_uri_video_url() {
    let msg = MultimodalMessage::user()
      .add_video_data("AAAA", "video/mp4")
      .build();

    let json = msg.to_openai_format();
    let parts = json["content"].as_array().unwrap();
    assert_eq!(parts[0]["type"], "video_url");
    assert_eq!(parts[0]["video_url"]["url"], "data:video/mp4;base64,AAAA");
    assert!(parts[0].get("video_data").is_none());
  }

  #[test]
  fn to_text_format_placeholders_cover_all_variants() {
    let msg = MultimodalMessage::user()
      .add_text("hi")
      .add_image_url("https://example.com/a.jpg")
      .add_document_url("https://example.com/a.pdf")
      .add_video_url("https://example.com/a.mp4")
      .build();
    let text = msg.to_text_format();
    assert!(text.contains("hi"));
    assert!(text.contains("[Image from URL]"));
    assert!(text.contains("[Document from URL]"));
    assert!(text.contains("[Video from URL]"));
  }
}

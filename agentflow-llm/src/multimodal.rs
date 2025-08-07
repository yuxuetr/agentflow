//! # Multimodal Support for AgentFlow LLM
//! 
//! This module provides support for multimodal inputs (text + images) to LLMs.
//! 
//! ## Example Usage
//! 
//! ```rust
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

/// Content types that can be included in multimodal messages
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageContent {
  /// Plain text content
  Text { 
    text: String 
  },
  /// Image from URL
  ImageUrl { 
    image_url: ImageUrl 
  },
  /// Base64 encoded image
  ImageData {
    image_data: ImageData
  },
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
  pub data: String, // base64 encoded data
  pub media_type: String, // "image/jpeg", "image/png", etc.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub detail: Option<String>, // "low", "high", "auto"
}

impl MessageContent {
  /// Create text content
  pub fn text<S: Into<String>>(text: S) -> Self {
    Self::Text { 
      text: text.into() 
    }
  }

  /// Create image URL content
  pub fn image_url<S: Into<String>>(url: S) -> Self {
    Self::ImageUrl {
      image_url: ImageUrl {
        url: url.into(),
        detail: None,
      }
    }
  }

  /// Create image URL content with detail level
  pub fn image_url_with_detail<S: Into<String>>(url: S, detail: S) -> Self {
    Self::ImageUrl {
      image_url: ImageUrl {
        url: url.into(),
        detail: Some(detail.into()),
      }
    }
  }

  /// Create base64 image content
  pub fn image_data<S: Into<String>>(data: S, media_type: S) -> Self {
    Self::ImageData {
      image_data: ImageData {
        data: data.into(),
        media_type: media_type.into(),
        detail: None,
      }
    }
  }

  /// Create base64 image content with detail level
  pub fn image_data_with_detail<S: Into<String>>(data: S, media_type: S, detail: S) -> Self {
    Self::ImageData {
      image_data: ImageData {
        data: data.into(),
        media_type: media_type.into(),
        detail: Some(detail.into()),
      }
    }
  }

  /// Check if this content is text
  pub fn is_text(&self) -> bool {
    matches!(self, MessageContent::Text { .. })
  }

  /// Check if this content is an image
  pub fn is_image(&self) -> bool {
    matches!(self, MessageContent::ImageUrl { .. } | MessageContent::ImageData { .. })
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

  /// Get all text content concatenated
  pub fn get_text(&self) -> String {
    self.content
      .iter()
      .filter_map(|c| c.as_text())
      .cloned()
      .collect::<Vec<_>>()
      .join(" ")
  }

  /// Convert to OpenAI-compatible JSON format
  pub fn to_openai_format(&self) -> Value {
    serde_json::json!({
      "role": self.role,
      "content": self.content
    })
  }

  /// Convert to simple text format (for text-only models)
  pub fn to_text_format(&self) -> String {
    if self.is_text_only() {
      self.get_text()
    } else {
      // For mixed content, include placeholders for images
      self.content
        .iter()
        .map(|content| match content {
          MessageContent::Text { text } => text.clone(),
          MessageContent::ImageUrl { .. } => "[Image from URL]".to_string(),
          MessageContent::ImageData { .. } => "[Image Data]".to_string(),
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
    self.content.push(MessageContent::image_url_with_detail(url, detail));
    self
  }

  /// Add base64 image data
  pub fn add_image_data<S: Into<String>>(mut self, data: S, media_type: S) -> Self {
    self.content.push(MessageContent::image_data(data, media_type));
    self
  }

  /// Add base64 image data with detail level
  pub fn add_image_data_with_detail<S: Into<String>>(mut self, data: S, media_type: S, detail: S) -> Self {
    self.content.push(MessageContent::image_data_with_detail(data, media_type, detail));
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
    Self::new(role)
      .add_text(text)
      .build()
  }

  /// Create a text + image URL message (common pattern)
  pub fn text_and_image<R: Into<String>, T: Into<String>, U: Into<String>>(
    role: R, 
    text: T, 
    image_url: U
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
    image_urls: Vec<U>
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
      "https://example.com/image.jpg"
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
      "https://example.com/test.jpg"
    );

    let json = msg.to_openai_format();
    assert_eq!(json["role"], "user");
    assert!(json["content"].is_array());
    assert_eq!(json["content"].as_array().unwrap().len(), 2);
  }
}
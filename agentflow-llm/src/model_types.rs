//! # Model Type System
//! 
//! This module defines granular model types with specific input/output capabilities.
//! This enables automatic validation of requests and proper handling of different
//! model capabilities.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Granular model types with specific input/output requirements
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelType {
  /// Text-based large model - input: text, output: text
  Text,
  /// Image understanding - input: text + image, output: text  
  ImageUnderstand,
  /// Text-to-image - input: text, output: image
  Text2Image,
  /// Image-to-image - input: image, output: image
  Image2Image,
  /// Image editing - input: image + text, output: image
  ImageEdit,
  /// Text-to-speech - input: text, output: audio
  Tts,
  /// Automatic speech recognition - input: audio, output: text
  Asr,
  /// Video understanding - input: text + video, output: text
  VideoUnderstand,
  /// Text-to-video - input: text, output: video
  Text2Video,
  /// Code generation - input: text, output: text (specialized for code)
  CodeGen,
  /// Document understanding - input: text + document, output: text
  DocUnderstand,
  /// Embedding generation - input: text, output: vector
  Embedding,
  /// Function calling - input: text + function schemas, output: function calls
  FunctionCalling,
}

impl ModelType {
  /// Get human-readable description of the model type
  pub fn description(&self) -> &'static str {
    match self {
      ModelType::Text => "Text-based language model",
      ModelType::ImageUnderstand => "Image understanding and analysis",
      ModelType::Text2Image => "Text-to-image generation",
      ModelType::Image2Image => "Image-to-image transformation",
      ModelType::ImageEdit => "Image editing with text instructions",
      ModelType::Tts => "Text-to-speech synthesis",
      ModelType::Asr => "Automatic speech recognition",
      ModelType::VideoUnderstand => "Video understanding and analysis",
      ModelType::Text2Video => "Text-to-video generation",
      ModelType::CodeGen => "Code generation and completion",
      ModelType::DocUnderstand => "Document understanding and analysis",
      ModelType::Embedding => "Text embedding generation",
      ModelType::FunctionCalling => "Function calling and tool usage",
    }
  }

  /// Get supported input types for this model
  pub fn supported_inputs(&self) -> HashSet<InputType> {
    let mut inputs = HashSet::new();
    
    match self {
      ModelType::Text | ModelType::CodeGen | ModelType::FunctionCalling => {
        inputs.insert(InputType::Text);
      },
      ModelType::ImageUnderstand | ModelType::ImageEdit => {
        inputs.insert(InputType::Text);
        inputs.insert(InputType::Image);
      },
      ModelType::Text2Image => {
        inputs.insert(InputType::Text);
      },
      ModelType::Image2Image => {
        inputs.insert(InputType::Image);
      },
      ModelType::Tts => {
        inputs.insert(InputType::Text);
      },
      ModelType::Asr => {
        inputs.insert(InputType::Audio);
      },
      ModelType::VideoUnderstand => {
        inputs.insert(InputType::Text);
        inputs.insert(InputType::Video);
      },
      ModelType::Text2Video => {
        inputs.insert(InputType::Text);
      },
      ModelType::DocUnderstand => {
        inputs.insert(InputType::Text);
        inputs.insert(InputType::Document);
      },
      ModelType::Embedding => {
        inputs.insert(InputType::Text);
      },
    }
    
    inputs
  }

  /// Get the primary output type for this model
  pub fn primary_output(&self) -> OutputType {
    match self {
      ModelType::Text | ModelType::ImageUnderstand | ModelType::VideoUnderstand 
      | ModelType::DocUnderstand | ModelType::CodeGen | ModelType::Asr => OutputType::Text,
      ModelType::Text2Image | ModelType::Image2Image | ModelType::ImageEdit => OutputType::Image,
      ModelType::Tts => OutputType::Audio,
      ModelType::Text2Video => OutputType::Video,
      ModelType::Embedding => OutputType::Vector,
      ModelType::FunctionCalling => OutputType::FunctionCall,
    }
  }

  /// Check if this model type supports streaming
  pub fn supports_streaming(&self) -> bool {
    match self {
      ModelType::Text | ModelType::ImageUnderstand | ModelType::VideoUnderstand 
      | ModelType::DocUnderstand | ModelType::CodeGen | ModelType::FunctionCalling => true,
      ModelType::Text2Image | ModelType::Image2Image | ModelType::ImageEdit 
      | ModelType::Text2Video => false, // Generation models typically don't stream
      ModelType::Tts => true, // TTS can stream audio
      ModelType::Asr => false, // ASR processes complete audio files
      ModelType::Embedding => false, // Embeddings are single vectors
    }
  }

  /// Check if this model requires streaming (no non-streaming mode)
  pub fn requires_streaming(&self) -> bool {
    match self {
      // Most models have both streaming and non-streaming modes
      _ => false,
    }
  }

  /// Check if this model supports tools/function calling
  pub fn supports_tools(&self) -> bool {
    match self {
      ModelType::Text | ModelType::ImageUnderstand | ModelType::VideoUnderstand 
      | ModelType::DocUnderstand | ModelType::CodeGen | ModelType::FunctionCalling => true,
      _ => false,
    }
  }

  /// Check if this is a multimodal model (accepts multiple input types)
  pub fn is_multimodal(&self) -> bool {
    self.supported_inputs().len() > 1
  }

  /// Get typical use cases for this model type
  pub fn use_cases(&self) -> Vec<&'static str> {
    match self {
      ModelType::Text => vec!["Conversation", "Q&A", "Text generation", "Summarization"],
      ModelType::ImageUnderstand => vec!["Image description", "Visual Q&A", "Object detection", "Scene analysis"],
      ModelType::Text2Image => vec!["Art generation", "Concept visualization", "Design mockups"],
      ModelType::Image2Image => vec!["Style transfer", "Image enhancement", "Format conversion"],
      ModelType::ImageEdit => vec!["Photo editing", "Object removal", "Style modification"],
      ModelType::Tts => vec!["Voice assistants", "Audio books", "Accessibility"],
      ModelType::Asr => vec!["Transcription", "Voice commands", "Meeting notes"],
      ModelType::VideoUnderstand => vec!["Video analysis", "Content moderation", "Scene detection"],
      ModelType::Text2Video => vec!["Animation", "Video content creation", "Demonstrations"],
      ModelType::CodeGen => vec!["Code completion", "Bug fixing", "Code explanation"],
      ModelType::DocUnderstand => vec!["Document analysis", "Information extraction", "Summarization"],
      ModelType::Embedding => vec!["Semantic search", "Similarity matching", "Classification"],
      ModelType::FunctionCalling => vec!["API integration", "Tool usage", "Workflow automation"],
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
      InputType::Audio => vec!["audio/flac", "audio/mp3", "audio/mp4", "audio/mpeg", 
                               "audio/mpga", "audio/m4a", "audio/ogg", "audio/wav", 
                               "audio/webm", "audio/aac", "audio/opus"],
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
      OutputType::Image | OutputType::Video | OutputType::Vector | OutputType::FunctionCall => false,
    }
  }
}

/// Model capability flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
  /// Model type with specific input/output requirements
  pub model_type: ModelType,
  /// Whether the model supports streaming responses
  pub supports_streaming: bool,
  /// Whether the model requires streaming (no non-streaming mode)
  pub requires_streaming: bool,
  /// Whether the model supports tool/function calling
  pub supports_tools: bool,
  /// Maximum context window size in tokens
  pub max_context_tokens: Option<u32>,
  /// Maximum output tokens per request
  pub max_output_tokens: Option<u32>,
  /// Whether the model supports system messages
  pub supports_system_messages: bool,
  /// Custom capabilities specific to the model
  pub custom_capabilities: std::collections::HashMap<String, serde_json::Value>,
}

impl ModelCapabilities {
  /// Create capabilities from a model type with defaults
  pub fn from_model_type(model_type: ModelType) -> Self {
    Self {
      supports_streaming: model_type.supports_streaming(),
      requires_streaming: model_type.requires_streaming(),
      supports_tools: model_type.supports_tools(),
      supports_system_messages: match model_type {
        ModelType::Text | ModelType::ImageUnderstand | ModelType::VideoUnderstand 
        | ModelType::DocUnderstand | ModelType::CodeGen | ModelType::FunctionCalling => true,
        _ => false,
      },
      max_context_tokens: None,
      max_output_tokens: None,
      custom_capabilities: std::collections::HashMap::new(),
      model_type,
    }
  }

  /// Validate if an input type is supported
  pub fn supports_input(&self, input_type: &InputType) -> bool {
    self.model_type.supported_inputs().contains(input_type)
  }

  /// Get the expected output type
  pub fn expected_output(&self) -> OutputType {
    self.model_type.primary_output()
  }

  /// Check if the model can handle multimodal input
  pub fn is_multimodal(&self) -> bool {
    self.model_type.is_multimodal()
  }

  /// Validate a request against model capabilities
  pub fn validate_request(&self, has_text: bool, has_images: bool, has_audio: bool, 
                         has_video: bool, requires_streaming: bool, uses_tools: bool) -> Result<(), String> {
    let supported_inputs = self.model_type.supported_inputs();
    
    // Check input types
    if has_text && !supported_inputs.contains(&InputType::Text) {
      return Err("Model does not support text input".to_string());
    }
    if has_images && !supported_inputs.contains(&InputType::Image) {
      return Err("Model does not support image input".to_string());
    }
    if has_audio && !supported_inputs.contains(&InputType::Audio) {
      return Err("Model does not support audio input".to_string());
    }
    if has_video && !supported_inputs.contains(&InputType::Video) {
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

/// Legacy model type mapping for backward compatibility
impl From<&str> for ModelType {
  fn from(legacy_type: &str) -> Self {
    match legacy_type {
      "text" => ModelType::Text,
      "multimodal" => ModelType::ImageUnderstand, // Default multimodal to image understanding
      "image" => ModelType::Text2Image,
      "audio" => ModelType::Tts,
      "tts" => ModelType::Tts,
      "asr" => ModelType::Asr,
      "embedding" => ModelType::Embedding,
      _ => ModelType::Text, // Default fallback
    }
  }
}

/// Convert to legacy string format for backward compatibility
impl ModelType {
  pub fn to_legacy_string(&self) -> &'static str {
    match self {
      ModelType::Text | ModelType::CodeGen => "text",
      ModelType::ImageUnderstand | ModelType::ImageEdit | ModelType::VideoUnderstand | ModelType::DocUnderstand => "multimodal",
      ModelType::Text2Image | ModelType::Image2Image => "image",
      ModelType::Text2Video => "video",
      ModelType::Tts => "tts", 
      ModelType::Asr => "asr",
      ModelType::Embedding => "embedding",
      ModelType::FunctionCalling => "text", // Function calling is still primarily text-based
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_model_type_capabilities() {
    let text_model = ModelType::Text;
    assert!(text_model.supported_inputs().contains(&InputType::Text));
    assert!(!text_model.is_multimodal());
    assert_eq!(text_model.primary_output(), OutputType::Text);

    let vision_model = ModelType::ImageUnderstand;
    assert!(vision_model.supported_inputs().contains(&InputType::Text));
    assert!(vision_model.supported_inputs().contains(&InputType::Image));
    assert!(vision_model.is_multimodal());
    assert_eq!(vision_model.primary_output(), OutputType::Text);
  }

  #[test]
  fn test_input_type_formats() {
    let image_input = InputType::Image;
    assert!(image_input.supports_mime_type("image/jpeg"));
    assert!(image_input.supports_mime_type("image/png"));
    assert!(!image_input.supports_mime_type("text/plain"));
  }

  #[test]
  fn test_capabilities_validation() {
    let capabilities = ModelCapabilities::from_model_type(ModelType::ImageUnderstand);
    
    // Should accept text + image
    assert!(capabilities.validate_request(true, true, false, false, false, false).is_ok());
    
    // Should reject audio for image understanding model
    assert!(capabilities.validate_request(true, false, true, false, false, false).is_err());
  }

  #[test]
  fn test_legacy_compatibility() {
    assert_eq!(ModelType::from("text"), ModelType::Text);
    assert_eq!(ModelType::from("multimodal"), ModelType::ImageUnderstand);
    assert_eq!(ModelType::Text.to_legacy_string(), "text");
  }
}
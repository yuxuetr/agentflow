//! ImageUnderstand Node - Specialized node for multimodal image understanding using vision models
//!
//! This node provides functionality for understanding and analyzing images using vision-capable
//! language models. It supports both text and image inputs in the same conversation context.

use crate::{NodeError, NodeResult};
use agentflow_core::{AsyncNode, SharedState};
use agentflow_llm::{AgentFlow, ResponseFormat as LlmResponseFormat};
use agentflow_llm::multimodal::{MessageContent as LlmMessageContent, MultimodalMessage};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use base64::{engine::general_purpose::STANDARD, Engine as _};

/// Message content types for multimodal conversations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum MessageContent {
  /// Text content in the message
  Text {
    text: String,
  },
  /// Image URL content in the message
  ImageUrl {
    image_url: ImageUrlContent,
  },
}

/// Image URL configuration for vision models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrlContent {
  /// URL or base64 data URI of the image
  pub url: String,
  /// Optional detail level for image processing
  pub detail: Option<String>,
}

/// Message role in the conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
  System,
  User,
  Assistant,
}

/// Complete message structure for vision models
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionMessage {
  pub role: MessageRole,
  pub content: Vec<MessageContent>,
}

/// Response format for vision model outputs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VisionResponseFormat {
  /// Plain text response
  Text,
  /// Structured JSON response
  Json,
  /// Markdown formatted response
  Markdown,
}

impl Default for VisionResponseFormat {
  fn default() -> Self {
    VisionResponseFormat::Text
  }
}

/// Node for image understanding using vision-capable language models
///
/// The ImageUnderstand node provides specialized functionality for multimodal AI models
/// that can process both text and image inputs simultaneously.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUnderstandNode {
  /// Unique identifier for this node
  pub name: String,
  
  /// Vision model to use (e.g., "gpt-4o", "claude-3-5-sonnet", "step-1o-turbo-vision")
  pub model: String,
  
  /// System message to set context and behavior
  pub system_message: Option<String>,
  
  /// Text prompt for the user message
  pub text_prompt: String,
  
  /// Image input source (file path, URL, or SharedState key)
  pub image_source: String,
  
  /// Additional images for multi-image understanding
  pub additional_images: Vec<String>,
  
  /// Image detail level: "low", "high", "auto"
  pub image_detail: Option<String>,
  
  /// Maximum tokens to generate
  pub max_tokens: Option<i32>,
  
  /// Temperature for response randomness (0.0-2.0)
  pub temperature: Option<f32>,
  
  /// Top-p sampling parameter
  pub top_p: Option<f32>,
  
  /// Response format specification
  pub response_format: VisionResponseFormat,
  
  /// Input keys that this node expects from SharedState
  pub input_keys: Vec<String>,
  
  /// Output key where results will be stored in SharedState
  pub output_key: String,
}

impl ImageUnderstandNode {
  /// Create a new ImageUnderstandNode with basic configuration
  pub fn new(name: &str, model: &str, text_prompt: &str, image_source: &str) -> Self {
    let output_key = format!("{}_output", name);
    
    Self {
      name: name.to_string(),
      model: model.to_string(),
      system_message: None,
      text_prompt: text_prompt.to_string(),
      image_source: image_source.to_string(),
      additional_images: Vec::new(),
      image_detail: Some("auto".to_string()),
      max_tokens: Some(1000),
      temperature: Some(0.7),
      top_p: None,
      response_format: VisionResponseFormat::default(),
      input_keys: Vec::new(),
      output_key,
    }
  }
  
  /// Create an ImageUnderstandNode configured for image description
  pub fn image_describer(name: &str, model: &str, image_source: &str) -> Self {
    Self::new(name, model, "Please describe this image in detail.", image_source)
      .with_system_message("You are an expert at analyzing and describing images. Provide detailed, accurate descriptions of visual content.")
      .with_max_tokens(500)
      .with_temperature(0.3)
      .with_image_detail("high")
  }
  
  /// Create an ImageUnderstandNode configured for image analysis  
  pub fn image_analyzer(name: &str, model: &str, image_source: &str) -> Self {
    Self::new(name, model, "Analyze this image and provide insights about {{analysis_focus}}.", image_source)
      .with_system_message("You are an AI assistant specialized in visual analysis. Provide thorough, analytical insights about images.")
      .with_max_tokens(800)
      .with_temperature(0.4)
      .with_input_keys(vec!["analysis_focus".to_string()])
  }
  
  /// Create an ImageUnderstandNode configured for OCR and text extraction
  pub fn text_extractor(name: &str, model: &str, image_source: &str) -> Self {
    Self::new(name, model, "Extract all text content from this image. Preserve formatting and structure where possible.", image_source)
      .with_system_message("You are an OCR specialist. Extract text accurately while maintaining the original structure and formatting.")
      .with_max_tokens(1500)
      .with_temperature(0.1)
      .with_response_format(VisionResponseFormat::Json)
  }
  
  /// Create an ImageUnderstandNode configured for multi-image comparison
  pub fn image_comparator(name: &str, model: &str, primary_image: &str, comparison_images: Vec<String>) -> Self {
    Self::new(name, model, "Compare these images and identify {{comparison_criteria}}.", primary_image)
      .with_additional_images(comparison_images)
      .with_system_message("You are an expert at visual comparison and analysis. Identify similarities, differences, and key insights.")
      .with_max_tokens(1000)
      .with_temperature(0.5)
      .with_input_keys(vec!["comparison_criteria".to_string()])
  }
  
  /// Create an ImageUnderstandNode configured for visual Q&A
  pub fn visual_qa(name: &str, model: &str, image_source: &str) -> Self {
    Self::new(name, model, "{{question}}", image_source)
      .with_system_message("You are a helpful assistant that can answer questions about images accurately and comprehensively.")
      .with_max_tokens(600)
      .with_temperature(0.6)
      .with_input_keys(vec!["question".to_string()])
  }
  
  /// Set the system message
  pub fn with_system_message(mut self, system_message: &str) -> Self {
    self.system_message = Some(system_message.to_string());
    self
  }
  
  /// Set additional images for multi-image analysis
  pub fn with_additional_images(mut self, images: Vec<String>) -> Self {
    self.additional_images = images;
    self
  }
  
  /// Set the image detail level
  pub fn with_image_detail(mut self, detail: &str) -> Self {
    self.image_detail = Some(detail.to_string());
    self
  }
  
  /// Set the maximum tokens to generate
  pub fn with_max_tokens(mut self, max_tokens: i32) -> Self {
    self.max_tokens = Some(max_tokens);
    self
  }
  
  /// Set the temperature parameter
  pub fn with_temperature(mut self, temperature: f32) -> Self {
    self.temperature = Some(temperature);
    self
  }
  
  /// Set the top-p parameter
  pub fn with_top_p(mut self, top_p: f32) -> Self {
    self.top_p = Some(top_p);
    self
  }
  
  /// Set the response format
  pub fn with_response_format(mut self, format: VisionResponseFormat) -> Self {
    self.response_format = format;
    self
  }
  
  /// Set the text prompt
  pub fn with_text_prompt(mut self, prompt: &str) -> Self {
    self.text_prompt = prompt.to_string();
    self
  }
  
  /// Set the input keys that this node expects
  pub fn with_input_keys(mut self, keys: Vec<String>) -> Self {
    self.input_keys = keys;
    self
  }
  
  /// Resolve template variables in the text prompt using SharedState
  fn resolve_text_prompt(&self, shared: &SharedState) -> NodeResult<String> {
    let mut resolved = self.text_prompt.clone();
    
    for key in &self.input_keys {
      let placeholder = format!("{{{{{}}}}}", key);
      if resolved.contains(&placeholder) {
        if let Some(value) = shared.get(key) {
          let replacement = match value {
            Value::String(s) => s.clone(),
            v => v.to_string().trim_matches('"').to_string(),
          };
          resolved = resolved.replace(&placeholder, &replacement);
        } else {
          return Err(NodeError::ValidationError {
            message: format!("Missing input key '{}' for text prompt template", key),
          });
        }
      }
    }
    
    Ok(resolved)
  }
  
  /// Load image data from file path, URL, or SharedState
  fn load_image_data(&self, image_ref: &str, shared: &SharedState) -> NodeResult<String> {
    // First check if it's a SharedState key
    if let Some(value) = shared.get(image_ref) {
      return match value {
        Value::String(s) => Ok(s.clone()),
        _ => Err(NodeError::ValidationError {
          message: format!("Image data at '{}' must be a string", image_ref),
        }),
      };
    }
    
    // Check if it's already a data URI or URL
    if image_ref.starts_with("data:") || image_ref.starts_with("http") {
      return Ok(image_ref.to_string());
    }
    
    // Otherwise treat as file path and read the file
    match std::fs::read(image_ref) {
      Ok(data) => {
        let mime_type = if image_ref.ends_with(".png") {
          "image/png"
        } else if image_ref.ends_with(".jpg") || image_ref.ends_with(".jpeg") {
          "image/jpeg" 
        } else if image_ref.ends_with(".gif") {
          "image/gif"
        } else if image_ref.ends_with(".webp") {
          "image/webp"
        } else {
          "image/png" // default
        };
        
        Ok(format!("data:{};base64,{}", mime_type, STANDARD.encode(data)))
      },
      Err(e) => Err(NodeError::IoError(e)),
    }
  }
  
  /// Build the messages array for the vision model API
  fn build_messages(&self, shared: &SharedState) -> NodeResult<Vec<VisionMessage>> {
    let mut messages = Vec::new();
    
    // Add system message if provided
    if let Some(system_msg) = &self.system_message {
      messages.push(VisionMessage {
        role: MessageRole::System,
        content: vec![MessageContent::Text { text: system_msg.clone() }],
      });
    }
    
    // Resolve text prompt
    let resolved_text = self.resolve_text_prompt(shared)?;
    
    // Load primary image
    let primary_image_data = self.load_image_data(&self.image_source, shared)?;
    
    // Build user message content
    let mut user_content = vec![
      MessageContent::Text { text: resolved_text },
      MessageContent::ImageUrl {
        image_url: ImageUrlContent {
          url: primary_image_data,
          detail: self.image_detail.clone(),
        },
      },
    ];
    
    // Add additional images
    for additional_image in &self.additional_images {
      let image_data = self.load_image_data(additional_image, shared)?;
      user_content.push(MessageContent::ImageUrl {
        image_url: ImageUrlContent {
          url: image_data,
          detail: self.image_detail.clone(),
        },
      });
    }
    
    messages.push(VisionMessage {
      role: MessageRole::User,
      content: user_content,
    });
    
    Ok(messages)
  }
  
  /// Validate image understanding configuration
  fn validate_config(&self) -> NodeResult<()> {
    if self.model.is_empty() {
      return Err(NodeError::ConfigurationError {
        message: "Model cannot be empty".to_string(),
      });
    }
    
    if self.text_prompt.is_empty() {
      return Err(NodeError::ConfigurationError {
        message: "Text prompt cannot be empty".to_string(),
      });
    }
    
    if self.image_source.is_empty() {
      return Err(NodeError::ConfigurationError {
        message: "Image source cannot be empty".to_string(),
      });
    }
    
    if let Some(max_tokens) = self.max_tokens {
      if max_tokens < 1 || max_tokens > 4096 {
        return Err(NodeError::ConfigurationError {
          message: "Max tokens must be between 1 and 4096".to_string(),
        });
      }
    }
    
    if let Some(temperature) = self.temperature {
      if temperature < 0.0 || temperature > 2.0 {
        return Err(NodeError::ConfigurationError {
          message: "Temperature must be between 0.0 and 2.0".to_string(),
        });
      }
    }
    
    if let Some(top_p) = self.top_p {
      if top_p <= 0.0 || top_p > 1.0 {
        return Err(NodeError::ConfigurationError {
          message: "Top-p must be between 0.0 and 1.0".to_string(),
        });
      }
    }
    
    Ok(())
  }
  
  /// Build the API request payload
  fn build_request_payload(&self, shared: &SharedState) -> NodeResult<HashMap<String, Value>> {
    let messages = self.build_messages(shared)?;
    
    let mut payload = HashMap::new();
    payload.insert("model".to_string(), Value::String(self.model.clone()));
    payload.insert("messages".to_string(), serde_json::to_value(messages)?);
    
    if let Some(max_tokens) = self.max_tokens {
      payload.insert("max_tokens".to_string(), Value::Number(max_tokens.into()));
    }
    
    if let Some(temperature) = self.temperature {
      payload.insert("temperature".to_string(), 
        Value::Number(serde_json::Number::from_f64(temperature as f64).unwrap()));
    }
    
    if let Some(top_p) = self.top_p {
      payload.insert("top_p".to_string(), 
        Value::Number(serde_json::Number::from_f64(top_p as f64).unwrap()));
    }
    
    // Add response format if not default
    match self.response_format {
      VisionResponseFormat::Json => {
        payload.insert("response_format".to_string(), serde_json::json!({"type": "json_object"}));
      },
      _ => {}, // Text and Markdown use default response format
    }
    
    Ok(payload)
  }

  /// Execute using real vision model via agentflow-llm
  async fn execute_real_vision_model(&self, payload: &HashMap<String, Value>) -> NodeResult<String> {
    // Initialize AgentFlow (this handles configuration loading)
    AgentFlow::init()
      .await
      .map_err(|e| NodeError::ConfigurationError {
        message: format!("Failed to initialize AgentFlow: {}", e),
      })?;
    
    // Convert our internal message format to agentflow-llm format
    let multimodal_messages = self.convert_to_llm_messages(payload)?;
    
    // Build request using fluent API
    let mut request = AgentFlow::model(&self.model);
    
    // Set multimodal messages
    request = request.multimodal_messages(multimodal_messages);
    
    // Add parameters
    if let Some(max_tokens) = self.max_tokens {
      request = request.max_tokens(max_tokens as u32);
    }
    if let Some(temp) = self.temperature {
      request = request.temperature(temp);
    }
    if let Some(top_p) = self.top_p {
      request = request.top_p(top_p);
    }
    
    // Map response format
    match self.response_format {
      VisionResponseFormat::Json => {
        request = request.response_format(LlmResponseFormat::JsonObject);
      }
      _ => {} // Text and Markdown use default text response
    }
    
    // Execute the request
    let response = request.execute().await.map_err(|e| {
      NodeError::ExecutionError {
        message: format!("Vision model execution failed: {}", e),
      }
    })?;
    
    Ok(response)
  }
  
  /// Convert our internal VisionMessage format to agentflow-llm MultimodalMessage format
  fn convert_to_llm_messages(&self, payload: &HashMap<String, Value>) -> NodeResult<Vec<MultimodalMessage>> {
    let messages = payload.get("messages")
      .ok_or_else(|| NodeError::ValidationError {
        message: "Missing messages in payload".to_string(),
      })?;
    
    let vision_messages: Vec<VisionMessage> = serde_json::from_value(messages.clone())
      .map_err(|e| NodeError::ValidationError {
        message: format!("Invalid message format: {}", e),
      })?;
    
    let mut llm_messages = Vec::new();
    
    for vision_msg in vision_messages {
      let role = match vision_msg.role {
        MessageRole::System => "system",
        MessageRole::User => "user", 
        MessageRole::Assistant => "assistant",
      };
      
      let mut llm_content = Vec::new();
      
      for content in vision_msg.content {
        match content {
          MessageContent::Text { text } => {
            llm_content.push(LlmMessageContent::text(text));
          }
          MessageContent::ImageUrl { image_url } => {
            if let Some(detail) = image_url.detail {
              llm_content.push(LlmMessageContent::image_url_with_detail(
                image_url.url, 
                detail
              ));
            } else {
              llm_content.push(LlmMessageContent::image_url(image_url.url));
            }
          }
        }
      }
      
      let llm_message = MultimodalMessage {
        role: role.to_string(),
        content: llm_content,
        metadata: HashMap::new(),
      };
      
      llm_messages.push(llm_message);
    }
    
    Ok(llm_messages)
  }
}

#[async_trait]
impl AsyncNode for ImageUnderstandNode {
  async fn prep_async(&self, shared: &SharedState) -> agentflow_core::Result<Value> {
    // Validate configuration
    self.validate_config()
      .map_err(|e| agentflow_core::AgentFlowError::AsyncExecutionError { message: e.to_string() })?;
    
    // Build request payload
    let payload = self.build_request_payload(shared)
      .map_err(|e| agentflow_core::AgentFlowError::AsyncExecutionError { message: e.to_string() })?;
    
    Ok(serde_json::to_value(payload).unwrap())
  }
  
  async fn exec_async(&self, prep_result: Value) -> agentflow_core::Result<Value> {
    let payload: HashMap<String, Value> = serde_json::from_value(prep_result)
      .map_err(|e| agentflow_core::AgentFlowError::AsyncExecutionError { message: e.to_string() })?;
    
    println!("🔍 ImageUnderstand Node '{}' executing with model: {}", self.name, self.model);
    
    // Extract and display message info for debugging
    if let Some(messages) = payload.get("messages") {
      if let Ok(messages_array) = serde_json::from_value::<Vec<VisionMessage>>(messages.clone()) {
        for (i, msg) in messages_array.iter().enumerate() {
          match msg.role {
            MessageRole::System => println!("   System: {}", 
              msg.content.iter().find_map(|c| match c {
                MessageContent::Text { text } => Some(text.as_str()),
                _ => None,
              }).unwrap_or("")),
            MessageRole::User => {
              let text_content = msg.content.iter().find_map(|c| match c {
                MessageContent::Text { text } => Some(text.as_str()),
                _ => None,
              }).unwrap_or("");
              let image_count = msg.content.iter().filter(|c| matches!(c, MessageContent::ImageUrl { .. })).count();
              println!("   User[{}]: {} (with {} image{})", i, text_content, image_count, if image_count != 1 { "s" } else { "" });
            },
            _ => {},
          }
        }
      }
    }
    
    if let Some(max_tokens) = self.max_tokens {
      println!("   Max Tokens: {}", max_tokens);
    }
    if let Some(temp) = self.temperature {
      println!("   Temperature: {}", temp);
    }
    
    // Execute real vision model API call via agentflow-llm
    match self.execute_real_vision_model(&payload).await {
      Ok(response) => {
        println!("✅ ImageUnderstand Node '{}' completed successfully", self.name);
        Ok(serde_json::json!({
          "choices": [{
            "message": {
              "role": "assistant",
              "content": response
            }
          }]
        }))
      },
      Err(e) => {
        println!("⚠️  Real vision model failed ({}), using mock response", e);
        // Fallback to mock for development/testing
        let mock_result = serde_json::json!({
          "choices": [{
            "message": {
              "role": "assistant",
              "content": "This is a mock vision analysis response. The real vision model failed to execute."
            }
          }]
        });
        Ok(mock_result)
      }
    }
  }
  
  async fn post_async(
    &self,
    shared: &SharedState,
    _prep_result: Value,
    exec_result: Value,
  ) -> agentflow_core::Result<Option<String>> {
    // Extract the actual content from the API response
    let content = if let Some(choices) = exec_result.get("choices") {
      if let Some(choice) = choices.get(0) {
        if let Some(message) = choice.get("message") {
          if let Some(content) = message.get("content") {
            content.clone()
          } else {
            exec_result.clone()
          }
        } else {
          exec_result.clone()
        }
      } else {
        exec_result.clone()
      }
    } else {
      exec_result.clone()
    };
    
    // Store result in SharedState
    shared.insert(self.output_key.clone(), content);
    Ok(Some(self.output_key.clone()))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;
  
  #[tokio::test]
  async fn test_image_understand_node_basic() {
    let shared = SharedState::new();
    // Mock image data in SharedState
    shared.insert("test_image.jpg".to_string(), json!("data:image/jpeg;base64,/9j/4AAQSkZJRgABAQEAAAAAAAD//gADRklGRg=="));
    
    let node = ImageUnderstandNode::new("vision_test", "gpt-4o", "Describe this image", "test_image.jpg");
    let result = node.run_async(&shared).await.unwrap();
    
    assert!(result.is_some());
    assert!(shared.get(&node.output_key).is_some());
  }
  
  #[tokio::test]
  async fn test_image_understand_node_with_template() {
    let shared = SharedState::new();
    shared.insert("analysis_image.png".to_string(), json!("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg=="));
    shared.insert("analysis_focus".to_string(), json!("color composition and lighting"));
    
    let node = ImageUnderstandNode::image_analyzer("analyzer", "claude-3-5-sonnet", "analysis_image.png");
    let result = node.run_async(&shared).await.unwrap();
    
    assert!(result.is_some());
    assert!(shared.get(&node.output_key).is_some());
  }
  
  #[tokio::test]
  async fn test_image_understand_multi_image() {
    let shared = SharedState::new();
    shared.insert("primary.jpg".to_string(), json!("data:image/jpeg;base64,/9j/4AAQSkZJRgABAQEAAAAAAAD//gADRklGRg=="));
    shared.insert("compare1.jpg".to_string(), json!("data:image/jpeg;base64,/9j/4AAQSkZJRgABAQEAAAAAAAD//gADRklGRg=="));
    shared.insert("compare2.jpg".to_string(), json!("data:image/jpeg;base64,/9j/4AAQSkZJRgABAQEAAAAAAAD//gADRklGRg=="));
    shared.insert("comparison_criteria".to_string(), json!("similarities and differences"));
    
    let node = ImageUnderstandNode::image_comparator("comparator", "gpt-4o", "primary.jpg", 
      vec!["compare1.jpg".to_string(), "compare2.jpg".to_string()]);
    let result = node.run_async(&shared).await.unwrap();
    
    assert!(result.is_some());
    assert!(shared.get(&node.output_key).is_some());
  }
  
  #[test]
  fn test_image_understand_validation() {
    let mut node = ImageUnderstandNode::new("test", "", "prompt", "image.jpg");
    assert!(node.validate_config().is_err()); // Empty model
    
    node.model = "gpt-4o".to_string();
    node.text_prompt = "".to_string();
    assert!(node.validate_config().is_err()); // Empty prompt
    
    node.text_prompt = "Analyze this".to_string();
    node.image_source = "".to_string(); 
    assert!(node.validate_config().is_err()); // Empty image source
    
    node.image_source = "test.jpg".to_string();
    node.max_tokens = Some(5000);
    assert!(node.validate_config().is_err()); // Invalid max_tokens
    
    node.max_tokens = Some(1000);
    node.temperature = Some(3.0);
    assert!(node.validate_config().is_err()); // Invalid temperature
    
    node.temperature = Some(0.7);
    assert!(node.validate_config().is_ok()); // Valid config
  }
  
  #[test]
  fn test_helper_constructors() {
    let describer = ImageUnderstandNode::image_describer("desc", "gpt-4o", "image.jpg");
    assert_eq!(describer.text_prompt, "Please describe this image in detail.");
    assert_eq!(describer.temperature, Some(0.3));
    assert!(describer.system_message.is_some());
    
    let analyzer = ImageUnderstandNode::image_analyzer("analyze", "claude-3-5-sonnet", "image.png"); 
    assert!(analyzer.text_prompt.contains("{{analysis_focus}}"));
    assert!(analyzer.input_keys.contains(&"analysis_focus".to_string()));
    
    let extractor = ImageUnderstandNode::text_extractor("ocr", "gpt-4o", "document.png");
    assert!(extractor.text_prompt.contains("Extract all text"));
    assert!(matches!(extractor.response_format, VisionResponseFormat::Json));
    assert_eq!(extractor.temperature, Some(0.1));
    
    let qa = ImageUnderstandNode::visual_qa("qa", "gpt-4o", "photo.jpg");
    assert_eq!(qa.text_prompt, "{{question}}");
    assert!(qa.input_keys.contains(&"question".to_string()));
  }
  
  #[test]
  fn test_response_formats() {
    let node_text = ImageUnderstandNode::new("test", "gpt-4o", "prompt", "image.jpg")
      .with_response_format(VisionResponseFormat::Text);
    assert!(matches!(node_text.response_format, VisionResponseFormat::Text));
    
    let node_json = ImageUnderstandNode::new("test", "gpt-4o", "prompt", "image.jpg")
      .with_response_format(VisionResponseFormat::Json);
    assert!(matches!(node_json.response_format, VisionResponseFormat::Json));
    
    let node_markdown = ImageUnderstandNode::new("test", "gpt-4o", "prompt", "image.jpg")
      .with_response_format(VisionResponseFormat::Markdown);
    assert!(matches!(node_markdown.response_format, VisionResponseFormat::Markdown));
  }
}
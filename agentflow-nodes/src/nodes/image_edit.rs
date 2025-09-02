//! ImageEditNode - Specialized node for image editing using AI models
//!
//! This node provides functionality for editing existing images using AI models like DALL-E.
//! It handles file input, prompt-based modifications, and various output formats.

use crate::{NodeError, NodeResult};
use agentflow_core::{AsyncNode, SharedState};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use base64::{engine::general_purpose::STANDARD, Engine as _};

/// Response format for image editing operations
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageEditResponseFormat {
    /// Returns a URL to the generated image
    Url,
    /// Returns the image as base64-encoded JSON
    B64Json,
}

impl Default for ImageEditResponseFormat {
  fn default() -> Self {
    ImageEditResponseFormat::Url
  }
}

/// Node for editing images using AI models
///
/// The ImageEditNode provides specialized functionality for image editing operations,
/// supporting various AI models with parameters specific to image modification tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageEditNode {
  /// Unique identifier for this node
  pub name: String,
  
  /// AI model to use for image editing (e.g., "dall-e-2", "stable-diffusion-edit")
  pub model: String,
  
  /// File path or SharedState key containing the source image to edit
  pub image: String,
  
  /// Text prompt describing the desired edits
  pub prompt: String,
  
  /// Optional mask image for selective editing (file path or SharedState key)
  pub mask: Option<String>,
  
  /// Output image dimensions (e.g., "1024x1024", "512x512")
  pub size: Option<String>,
  
  /// Number of image variations to generate
  pub n: Option<i32>,
  
  /// Response format for the generated image
  pub response_format: ImageEditResponseFormat,
  
  /// Number of denoising steps (model-specific)
  pub steps: Option<i32>,
  
  /// CFG scale for controlling adherence to prompt
  pub cfg_scale: Option<f32>,
  
  /// Input keys that this node expects from SharedState
  pub input_keys: Vec<String>,
  
  /// Output key where results will be stored in SharedState
  pub output_key: String,
}

impl ImageEditNode {
  /// Create a new ImageEditNode with basic configuration
  pub fn new(name: &str, model: &str, image: &str, prompt: &str) -> Self {
    let output_key = format!("{}_output", name);
    
    Self {
      name: name.to_string(),
      model: model.to_string(),
      image: image.to_string(),
      prompt: prompt.to_string(),
      mask: None,
      size: None,
      n: Some(1),
      response_format: ImageEditResponseFormat::default(),
      steps: None,
      cfg_scale: None,
      input_keys: Vec::new(),
      output_key,
    }
  }
  
  /// Create an ImageEditNode configured for photo retouching
  pub fn photo_retoucher(name: &str, model: &str, image: &str) -> Self {
    Self::new(name, model, image, "Enhance and retouch this photo")
      .with_size("1024x1024")
      .with_steps(50)
      .with_cfg_scale(7.5)
  }
  
  /// Create an ImageEditNode configured for object removal
  pub fn object_remover(name: &str, model: &str, image: &str, mask: &str) -> Self {
    Self::new(name, model, image, "Remove the masked object seamlessly")
      .with_mask(mask)
      .with_size("1024x1024")
      .with_steps(40)
      .with_cfg_scale(8.0)
  }
  
  /// Create an ImageEditNode configured for style modification
  pub fn style_modifier(name: &str, model: &str, image: &str) -> Self {
    Self::new(name, model, image, "Apply {{style}} to this image")
      .with_size("1024x1024")
      .with_steps(30)
      .with_cfg_scale(7.0)
      .with_input_keys(vec!["style".to_string()])
  }
  
  /// Create an ImageEditNode configured for background replacement
  pub fn background_replacer(name: &str, model: &str, image: &str, mask: &str) -> Self {
    Self::new(name, model, image, "Replace background with {{new_background}}")
      .with_mask(mask)
      .with_size("1024x1024")
      .with_steps(45)
      .with_cfg_scale(7.5)
      .with_input_keys(vec!["new_background".to_string()])
  }
  
  /// Set the mask image for selective editing
  pub fn with_mask(mut self, mask: &str) -> Self {
    self.mask = Some(mask.to_string());
    self
  }
  
  /// Set the output image size
  pub fn with_size(mut self, size: &str) -> Self {
    self.size = Some(size.to_string());
    self
  }
  
  /// Set the number of image variations to generate
  pub fn with_n(mut self, n: i32) -> Self {
    self.n = Some(n);
    self
  }
  
  /// Set the response format
  pub fn with_response_format(mut self, format: ImageEditResponseFormat) -> Self {
    self.response_format = format;
    self
  }
  
  /// Set the number of denoising steps
  pub fn with_steps(mut self, steps: i32) -> Self {
    self.steps = Some(steps);
    self
  }
  
  /// Set the CFG scale
  pub fn with_cfg_scale(mut self, cfg_scale: f32) -> Self {
    self.cfg_scale = Some(cfg_scale);
    self
  }
  
  /// Set the prompt text
  pub fn with_prompt(mut self, prompt: &str) -> Self {
    self.prompt = prompt.to_string();
    self
  }
  
  /// Set the input keys that this node expects
  pub fn with_input_keys(mut self, keys: Vec<String>) -> Self {
    self.input_keys = keys;
    self
  }
  
  /// Resolve template variables in the prompt using SharedState
  fn resolve_prompt(&self, shared: &SharedState) -> NodeResult<String> {
    let mut resolved = self.prompt.clone();
    
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
            message: format!("Missing input key '{}' for prompt template", key),
          });
        }
      }
    }
    
    Ok(resolved)
  }
  
  /// Load image data from file path or SharedState
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
    
    // Otherwise treat as file path and read the file
    match std::fs::read(image_ref) {
      Ok(data) => Ok(STANDARD.encode(data)),
      Err(e) => Err(NodeError::IoError(e)),
    }
  }
  
  /// Validate image editing configuration
  fn validate_config(&self) -> NodeResult<()> {
    if self.model.is_empty() {
      return Err(NodeError::ConfigurationError {
        message: "Model cannot be empty".to_string(),
      });
    }
    
    if self.image.is_empty() {
      return Err(NodeError::ConfigurationError {
        message: "Image source cannot be empty".to_string(),
      });
    }
    
    if self.prompt.is_empty() {
      return Err(NodeError::ConfigurationError {
        message: "Prompt cannot be empty".to_string(),
      });
    }
    
    if let Some(n) = self.n {
      if n < 1 || n > 10 {
        return Err(NodeError::ConfigurationError {
          message: "Number of images (n) must be between 1 and 10".to_string(),
        });
      }
    }
    
    if let Some(steps) = self.steps {
      if steps < 1 || steps > 150 {
        return Err(NodeError::ConfigurationError {
          message: "Steps must be between 1 and 150".to_string(),
        });
      }
    }
    
    if let Some(cfg_scale) = self.cfg_scale {
      if cfg_scale < 1.0 || cfg_scale > 20.0 {
        return Err(NodeError::ConfigurationError {
          message: "CFG scale must be between 1.0 and 20.0".to_string(),
        });
      }
    }
    
    Ok(())
  }
  
  /// Build the API request payload
  fn build_request_payload(&self, shared: &SharedState) -> NodeResult<HashMap<String, Value>> {
    let resolved_prompt = self.resolve_prompt(shared)?;
    let image_data = self.load_image_data(&self.image, shared)?;
    
    let mut payload = HashMap::new();
    payload.insert("model".to_string(), Value::String(self.model.clone()));
    payload.insert("prompt".to_string(), Value::String(resolved_prompt));
    payload.insert("image".to_string(), Value::String(image_data));
    payload.insert("response_format".to_string(), 
      Value::String(serde_json::to_string(&self.response_format)?.trim_matches('"').to_string()));
    
    if let Some(mask_ref) = &self.mask {
      let mask_data = self.load_image_data(mask_ref, shared)?;
      payload.insert("mask".to_string(), Value::String(mask_data));
    }
    
    if let Some(size) = &self.size {
      payload.insert("size".to_string(), Value::String(size.clone()));
    }
    
    if let Some(n) = self.n {
      payload.insert("n".to_string(), Value::Number(n.into()));
    }
    
    if let Some(steps) = self.steps {
      payload.insert("steps".to_string(), Value::Number(steps.into()));
    }
    
    if let Some(cfg_scale) = self.cfg_scale {
      payload.insert("cfg_scale".to_string(), 
        Value::Number(serde_json::Number::from_f64(cfg_scale as f64).unwrap()));
    }
    
    Ok(payload)
  }
}

#[async_trait]
impl AsyncNode for ImageEditNode {
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
    
    println!("🔄 ImageEditNode '{}' executing with model: {}", self.name, self.model);
    println!("   Prompt: {}", payload.get("prompt").unwrap().as_str().unwrap_or(""));
    if payload.contains_key("mask") {
      println!("   Using mask for selective editing");
    }
    if let Some(size) = &self.size {
      println!("   Size: {}", size);
    }
    
    // Simulate API call (in real implementation, this would call the actual API)
    let mock_result = match self.response_format {
      ImageEditResponseFormat::Url => {
        serde_json::json!({
          "data": [{
            "url": format!("https://api.example.com/edited_image_{}.png", self.name)
          }]
        })
      },
      ImageEditResponseFormat::B64Json => {
        serde_json::json!({
          "data": [{
            "b64_json": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg=="
          }]
        })
      }
    };
    
    println!("✅ ImageEditNode '{}' completed successfully", self.name);
    Ok(mock_result)
  }
  
  async fn post_async(
    &self,
    shared: &SharedState,
    _prep_result: Value,
    exec_result: Value,
  ) -> agentflow_core::Result<Option<String>> {
    // Store result in SharedState
    shared.insert(self.output_key.clone(), exec_result);
    Ok(Some(self.output_key.clone()))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;
  
  #[tokio::test]
  async fn test_image_edit_node_basic() {
    let shared = SharedState::new();
    // Mock image data in SharedState instead of using a file
    shared.insert("test_image.png".to_string(), json!("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg=="));
    
    let node = ImageEditNode::new("test_editor", "dall-e-2", "test_image.png", "Remove background");
    let result = node.run_async(&shared).await.unwrap();
    
    assert!(result.is_some());
    assert!(shared.get(&node.output_key).is_some());
  }
  
  #[tokio::test]
  async fn test_image_edit_node_with_mask() {
    let shared = SharedState::new();
    // Mock image and mask data in SharedState
    shared.insert("photo.png".to_string(), json!("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg=="));
    shared.insert("mask.png".to_string(), json!("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg=="));
    
    let node = ImageEditNode::object_remover("remover", "dall-e-2", "photo.png", "mask.png");
    let result = node.run_async(&shared).await.unwrap();
    
    assert!(result.is_some());
    assert_eq!(node.mask, Some("mask.png".to_string()));
  }
  
  #[tokio::test]
  async fn test_image_edit_node_with_template() {
    let shared = SharedState::new();
    // Mock image data and template variable
    shared.insert("portrait.jpg".to_string(), json!("data:image/jpeg;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg=="));
    shared.insert("style".to_string(), json!("watercolor painting"));
    
    let node = ImageEditNode::style_modifier("styler", "stable-diffusion-edit", "portrait.jpg");
    let result = node.run_async(&shared).await.unwrap();
    
    assert!(result.is_some());
  }
  
  #[test]
  fn test_image_edit_node_validation() {
    let mut node = ImageEditNode::new("test", "", "image.png", "prompt");
    assert!(node.validate_config().is_err()); // Empty model
    
    node.model = "dall-e-2".to_string();
    node.image = "".to_string();
    assert!(node.validate_config().is_err()); // Empty image
    
    node.image = "test.png".to_string();
    node.prompt = "".to_string();
    assert!(node.validate_config().is_err()); // Empty prompt
    
    node.prompt = "Edit this image".to_string();
    node.n = Some(15);
    assert!(node.validate_config().is_err()); // Invalid n value
    
    node.n = Some(1);
    node.cfg_scale = Some(25.0);
    assert!(node.validate_config().is_err()); // Invalid CFG scale
    
    node.cfg_scale = Some(7.5);
    assert!(node.validate_config().is_ok()); // Valid config
  }
  
  #[test]
  fn test_image_edit_response_formats() {
    let node_url = ImageEditNode::new("test", "dall-e-2", "test.png", "edit")
      .with_response_format(ImageEditResponseFormat::Url);
    assert!(matches!(node_url.response_format, ImageEditResponseFormat::Url));
    
    let node_b64 = ImageEditNode::new("test", "dall-e-2", "test.png", "edit")
      .with_response_format(ImageEditResponseFormat::B64Json);
    assert!(matches!(node_b64.response_format, ImageEditResponseFormat::B64Json));
  }
  
  #[test]
  fn test_helper_constructors() {
    let retoucher = ImageEditNode::photo_retoucher("retoucher", "dall-e-2", "photo.jpg");
    assert_eq!(retoucher.size, Some("1024x1024".to_string()));
    assert_eq!(retoucher.steps, Some(50));
    assert_eq!(retoucher.cfg_scale, Some(7.5));
    
    let remover = ImageEditNode::object_remover("remover", "dall-e-2", "image.png", "mask.png");
    assert_eq!(remover.mask, Some("mask.png".to_string()));
    assert!(remover.prompt.contains("Remove"));
    
    let styler = ImageEditNode::style_modifier("styler", "stable-diffusion-edit", "portrait.jpg");
    assert!(styler.input_keys.contains(&"style".to_string()));
    assert!(styler.prompt.contains("{{style}}"));
  }
}
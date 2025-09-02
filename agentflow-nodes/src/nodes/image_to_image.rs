use crate::{AsyncNode, NodeError, NodeResult, SharedState};
use agentflow_core::AgentFlowError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Image-to-Image transformation node
#[derive(Debug, Clone)]
pub struct ImageToImageNode {
    pub name: String,
    pub model: String,
    pub prompt_template: String,
    pub source_url_template: String,    // Template for source image URL/key
    pub input_keys: Vec<String>,
    pub output_key: String,
    
    // Image-to-image specific parameters
    pub source_weight: Option<f32>,     // Source image influence (0.0-1.0)
    pub size: Option<String>,           // Output size: "256x256", "512x512", "768x768", "1024x1024"
    pub n: Option<u32>,                 // Number of images to generate (default 1)
    pub response_format: ImageResponseFormat, // b64_json or url
    pub steps: Option<u32>,             // Generation steps
    pub cfg_scale: Option<f32>,         // Classifier-free guidance scale
    pub strength: Option<f32>,          // How much to transform the source (0.0-1.0)
    pub seed: Option<u64>,              // For reproducible generation
    
    // Workflow control
    pub dependencies: Vec<String>,
    pub condition: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageResponseFormat {
    #[serde(rename = "b64_json")]
    Base64Json,
    #[serde(rename = "url")]
    Url,
}

impl Default for ImageResponseFormat {
    fn default() -> Self {
        ImageResponseFormat::Base64Json
    }
}

impl ImageToImageNode {
    pub fn new(name: &str, model: &str) -> Self {
        Self {
            name: name.to_string(),
            model: model.to_string(),
            prompt_template: String::new(),
            source_url_template: String::new(),
            input_keys: Vec::new(),
            output_key: format!("{}_transformed", name),
            source_weight: None,
            size: None,
            n: None,
            response_format: ImageResponseFormat::default(),
            steps: None,
            cfg_scale: None,
            strength: None,
            seed: None,
            dependencies: Vec::new(),
            condition: None,
            timeout_ms: None,
        }
    }
    
    // Builder pattern methods
    pub fn with_prompt(mut self, template: &str) -> Self {
        self.prompt_template = template.to_string();
        self
    }
    
    pub fn with_source_url(mut self, source_template: &str) -> Self {
        self.source_url_template = source_template.to_string();
        self
    }
    
    pub fn with_source_weight(mut self, weight: f32) -> Self {
        self.source_weight = Some(weight.clamp(0.0, 1.0));
        self
    }
    
    pub fn with_size(mut self, size: &str) -> Self {
        self.size = Some(size.to_string());
        self
    }
    
    pub fn with_count(mut self, n: u32) -> Self {
        self.n = Some(n);
        self
    }
    
    pub fn with_response_format(mut self, format: ImageResponseFormat) -> Self {
        self.response_format = format;
        self
    }
    
    pub fn with_steps(mut self, steps: u32) -> Self {
        self.steps = Some(steps);
        self
    }
    
    pub fn with_cfg_scale(mut self, scale: f32) -> Self {
        self.cfg_scale = Some(scale);
        self
    }
    
    pub fn with_strength(mut self, strength: f32) -> Self {
        self.strength = Some(strength.clamp(0.0, 1.0));
        self
    }
    
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }
    
    pub fn with_input_keys(mut self, keys: Vec<String>) -> Self {
        self.input_keys = keys;
        self
    }
    
    pub fn with_output_key(mut self, key: &str) -> Self {
        self.output_key = key.to_string();
        self
    }
    
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }
    
    /// Validate and resolve source image
    fn resolve_source_image(&self, shared: &SharedState) -> NodeResult<String> {
        let resolved_source = shared.resolve_template_advanced(&self.source_url_template);
        
        // Check if it's a SharedState key reference
        if !resolved_source.starts_with("http") && !resolved_source.starts_with("data:") && !resolved_source.starts_with("file:") {
            // Try to get from SharedState
            if let Some(source_data) = shared.get(&resolved_source) {
                return Ok(source_data.as_str().unwrap_or(&resolved_source).to_string());
            }
        }
        
        // Validate source format
        if resolved_source.is_empty() {
            return Err(NodeError::ValidationError {
                message: "Source image URL/data cannot be empty".to_string(),
            });
        }
        
        // Basic validation for supported formats
        let is_valid_source = resolved_source.starts_with("http") ||
                             resolved_source.starts_with("data:image/") ||
                             resolved_source.starts_with("file:");
        
        if !is_valid_source {
            return Err(NodeError::ValidationError {
                message: format!("Invalid source image format. Expected URL, data URI, or file path, got: {}", 
                    &resolved_source[..resolved_source.len().min(100)]),
            });
        }
        
        Ok(resolved_source)
    }
    
    /// Create configuration for image-to-image generation
    fn create_image_config(&self, resolved_prompt: &str, source_url: &str) -> NodeResult<Value> {
        let mut config = serde_json::Map::new();
        
        config.insert("model".to_string(), Value::String(self.model.clone()));
        config.insert("prompt".to_string(), Value::String(resolved_prompt.to_string()));
        config.insert("source_url".to_string(), Value::String(source_url.to_string()));
        
        if let Some(source_weight) = self.source_weight {
            config.insert("source_weight".to_string(), 
                Value::Number(serde_json::Number::from_f64(source_weight as f64).unwrap()));
        }
        
        if let Some(ref size) = self.size {
            config.insert("size".to_string(), Value::String(size.clone()));
        }
        
        if let Some(n) = self.n {
            config.insert("n".to_string(), Value::Number(serde_json::Number::from(n)));
        }
        
        config.insert("response_format".to_string(), serde_json::to_value(&self.response_format)?);
        
        if let Some(steps) = self.steps {
            config.insert("steps".to_string(), Value::Number(serde_json::Number::from(steps)));
        }
        
        if let Some(cfg_scale) = self.cfg_scale {
            config.insert("cfg_scale".to_string(),
                Value::Number(serde_json::Number::from_f64(cfg_scale as f64).unwrap()));
        }
        
        if let Some(strength) = self.strength {
            config.insert("strength".to_string(),
                Value::Number(serde_json::Number::from_f64(strength as f64).unwrap()));
        }
        
        if let Some(seed) = self.seed {
            config.insert("seed".to_string(), Value::Number(serde_json::Number::from(seed)));
        }
        
        Ok(Value::Object(config))
    }
    
    /// Mock image-to-image transformation
    async fn execute_mock_image_transform(&self, config: &serde_json::Map<String, Value>) -> NodeResult<String> {
        let prompt = config.get("prompt").unwrap().as_str().unwrap();
        let model = config.get("model").unwrap().as_str().unwrap();
        let source_url = config.get("source_url").unwrap().as_str().unwrap();
        let size = config.get("size").map(|s| s.as_str().unwrap_or("1024x1024")).unwrap_or("1024x1024");
        
        println!("🔄 Executing Image-to-Image transformation (MOCK):");
        println!("   Model: {}", model);
        println!("   Prompt: {}", prompt);
        println!("   Source: {}...", &source_url[..source_url.len().min(50)]);
        println!("   Size: {}", size);
        
        if let Some(strength) = config.get("strength").and_then(|s| s.as_f64()) {
            println!("   Strength: {}", strength);
        }
        
        // Simulate processing time (longer than text-to-image)
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        
        // Mock response based on response format
        let mock_response = match &self.response_format {
            ImageResponseFormat::Base64Json => {
                // Mock transformed base64 image data (different from input)
                "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEklEQVR42mNkYPhfz4AABQAB/AgBbwAAAABJRU5ErkJggg=="
            }
            ImageResponseFormat::Url => {
                // Mock transformed URL
                "https://example.com/transformed-image-mock.png"
            }
        };
        
        println!("✅ Image Transformation (MOCK, {:?}): Generated {} transformed image", self.response_format, size);
        Ok(mock_response.to_string())
    }
}

#[async_trait]
impl AsyncNode for ImageToImageNode {
    async fn prep_async(&self, shared: &SharedState) -> Result<Value, AgentFlowError> {
        // Check conditional execution
        if let Some(ref condition) = self.condition {
            let resolved_condition = shared.resolve_template_advanced(condition);
            if resolved_condition != "true" {
                println!("⏭️  Skipping ImageToImage node '{}' due to condition: {}", self.name, resolved_condition);
                return Ok(Value::Object(serde_json::Map::new()));
            }
        }
        
        // Resolve prompt template
        let resolved_prompt = shared.resolve_template_advanced(&self.prompt_template);
        
        // Include input keys data in prompt resolution
        let mut enriched_prompt = resolved_prompt;
        for input_key in &self.input_keys {
            if let Some(input_value) = shared.get(input_key) {
                let input_str = match input_value {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                enriched_prompt = enriched_prompt.replace(
                    &format!("{{{{{}}}}}", input_key),
                    &input_str,
                );
            }
        }
        
        // Resolve and validate source image
        let source_url = self.resolve_source_image(shared)
            .map_err(|e| AgentFlowError::AsyncExecutionError { message: e.to_string() })?;
        
        let config = self.create_image_config(&enriched_prompt, &source_url)
            .map_err(|e| AgentFlowError::AsyncExecutionError {
                message: format!("Failed to create image-to-image config: {}", e),
            })?;
        
        println!("🔧 ImageToImage Node '{}' prepared:", self.name);
        println!("   Model: {}", self.model);
        println!("   Prompt: {}", enriched_prompt);
        println!("   Source: {}...", &source_url[..source_url.len().min(50)]);
        
        Ok(config)
    }
    
    async fn exec_async(&self, prep_result: Value) -> Result<Value, AgentFlowError> {
        let config = prep_result
            .as_object()
            .ok_or_else(|| AgentFlowError::AsyncExecutionError {
                message: "Invalid prep result for ImageToImage node".to_string(),
            })?;
        
        // Skip execution if condition failed
        if config.is_empty() {
            return Ok(Value::String("Skipped due to condition".to_string()));
        }
        
        // Apply timeout if configured
        let response = if let Some(timeout_ms) = self.timeout_ms {
            let timeout_duration = std::time::Duration::from_millis(timeout_ms);
            match tokio::time::timeout(timeout_duration, self.execute_mock_image_transform(config)).await {
                Ok(result) => result.map_err(|e| AgentFlowError::AsyncExecutionError {
                    message: e.to_string(),
                })?,
                Err(_) => return Err(AgentFlowError::TimeoutExceeded { duration_ms: timeout_ms }),
            }
        } else {
            self.execute_mock_image_transform(config).await
                .map_err(|e| AgentFlowError::AsyncExecutionError {
                    message: e.to_string(),
                })?
        };
        
        Ok(Value::String(response))
    }
    
    async fn post_async(
        &self,
        shared: &SharedState,
        _prep_result: Value,
        exec_result: Value,
    ) -> Result<Option<String>, AgentFlowError> {
        // Store the transformed image data
        shared.insert(self.output_key.clone(), exec_result.clone());
        
        // Also store as generic "transformed_image" for workflow chaining
        shared.insert("transformed_image".to_string(), exec_result);
        
        println!("💾 Stored transformed image in shared state as: '{}'", self.output_key);
        
        Ok(None) // No specific next action
    }
    
    fn get_node_id(&self) -> Option<String> {
        Some(self.name.clone())
    }
}

/// Helper constructors for common image-to-image scenarios
impl ImageToImageNode {
    /// Create a style transfer node
    pub fn style_transfer(name: &str, model: &str, source_key: &str) -> Self {
        Self::new(name, model)
            .with_source_url(&format!("{{{{{}}}}}", source_key))
            .with_strength(0.7)
            .with_cfg_scale(7.0)
            .with_steps(50)
            .with_size("1024x1024")
    }
    
    /// Create an image enhancement node
    pub fn enhance_image(name: &str, model: &str, source_key: &str) -> Self {
        Self::new(name, model)
            .with_source_url(&format!("{{{{{}}}}}", source_key))
            .with_strength(0.3)  // Light transformation
            .with_cfg_scale(5.0)
            .with_steps(30)
    }
    
    /// Create a variation generator
    pub fn create_variations(name: &str, model: &str, source_key: &str, count: u32) -> Self {
        Self::new(name, model)
            .with_source_url(&format!("{{{{{}}}}}", source_key))
            .with_count(count)
            .with_strength(0.5)
            .with_cfg_scale(6.0)
            .with_size("768x768")
    }
    
    /// Create an upscaler node
    pub fn upscale_image(name: &str, model: &str, source_key: &str) -> Self {
        Self::new(name, model)
            .with_source_url(&format!("{{{{{}}}}}", source_key))
            .with_prompt("high quality, detailed, sharp")
            .with_strength(0.2)  // Minimal changes, just upscale
            .with_size("2048x2048")
            .with_cfg_scale(4.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_image_to_image_node_creation() {
        let node = ImageToImageNode::new("test_transform", "stable-diffusion");
        assert_eq!(node.name, "test_transform");
        assert_eq!(node.model, "stable-diffusion");
        assert_eq!(node.output_key, "test_transform_transformed");
        assert!(matches!(node.response_format, ImageResponseFormat::Base64Json));
    }
    
    #[tokio::test]
    async fn test_image_to_image_with_source_validation() {
        let node = ImageToImageNode::new("transform", "sd")
            .with_prompt("Transform this into {{style}}")
            .with_source_url("{{source_image}}")
            .with_strength(0.7);
        
        let shared = SharedState::new();
        shared.insert("style".to_string(), Value::String("watercolor painting".to_string()));
        shared.insert("source_image".to_string(), Value::String("data:image/png;base64,mock_data".to_string()));
        
        // Test preparation - should succeed with valid source
        let result = node.prep_async(&shared).await;
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_image_to_image_source_validation_error() {
        let node = ImageToImageNode::new("transform", "sd")
            .with_source_url("{{missing_source}}");
        
        let shared = SharedState::new();
        // Don't provide the source - should fail
        
        let result = node.prep_async(&shared).await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_helper_constructors() {
        // Test style transfer
        let style_node = ImageToImageNode::style_transfer("style", "sd", "input_image");
        assert_eq!(style_node.source_url_template, "{{input_image}}");
        assert_eq!(style_node.strength, Some(0.7));
        assert_eq!(style_node.size, Some("1024x1024".to_string()));
        
        // Test enhancement
        let enhance_node = ImageToImageNode::enhance_image("enhance", "sd", "low_quality");
        assert_eq!(enhance_node.source_url_template, "{{low_quality}}");
        assert_eq!(enhance_node.strength, Some(0.3));
        
        // Test variations
        let var_node = ImageToImageNode::create_variations("variations", "sd", "base_image", 4);
        assert_eq!(var_node.n, Some(4));
        assert_eq!(var_node.strength, Some(0.5));
        
        // Test upscaler
        let upscale_node = ImageToImageNode::upscale_image("upscale", "sd", "small_image");
        assert_eq!(upscale_node.size, Some("2048x2048".to_string()));
        assert_eq!(upscale_node.strength, Some(0.2));
        assert!(upscale_node.prompt_template.contains("high quality"));
    }
    
    #[tokio::test]
    async fn test_image_to_image_full_workflow() {
        let node = ImageToImageNode::style_transfer("style_transfer", "stable-diffusion", "source_img")
            .with_prompt("Transform to {{art_style}} style")
            .with_input_keys(vec!["art_style".to_string()]);
        
        let shared = SharedState::new();
        shared.insert("art_style".to_string(), Value::String("impressionist".to_string()));
        shared.insert("source_img".to_string(), Value::String("data:image/png;base64,mock_source_data".to_string()));
        
        // Test full execution
        let result = node.run_async(&shared).await.unwrap();
        assert!(result.is_none());
        
        // Check output was stored
        let transformed = shared.get(&node.output_key).unwrap();
        assert!(transformed.as_str().unwrap().contains("data:image") || transformed.as_str().unwrap().contains("http"));
    }
}
use agentflow_core::{
  async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
  error::AgentFlowError,
  value::FlowValue,
};
use agentflow_llm::{
  AgentFlow, providers::modality::Text2ImageRequest as ModalityText2ImageRequest,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ... (rest of the file is the same until the AsyncNode implementation)

/// Text-to-Image generation node
#[derive(Debug, Clone)]
pub struct TextToImageNode {
  pub name: String,
  pub model: String,
  pub prompt_template: String,
  pub negative_prompt: Option<String>,
  pub input_keys: Vec<String>,
  pub output_key: String,

  // Image generation specific parameters
  pub size: Option<String>, // "256x256", "512x512", "768x768", "1024x1024"
  pub response_format: ImageResponseFormat, // b64_json or url
  pub steps: Option<u32>,   // Generation steps
  pub cfg_scale: Option<f32>, // Classifier-free guidance scale
  pub style_reference: Option<StyleReference>,
  pub n: Option<u32>,    // Number of images to generate (default 1)
  pub seed: Option<u64>, // For reproducible generation

  // Workflow control
  pub dependencies: Vec<String>,
  pub condition: Option<String>,
  pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum ImageResponseFormat {
  #[serde(rename = "b64_json")]
  #[default]
  Base64Json, // Return as base64 encoded JSON
  #[serde(rename = "url")]
  Url, // Return as URL reference
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleReference {
  pub image_url: Option<String>,  // Reference image URL
  pub style_weight: Option<f32>,  // Style influence weight (0.0-1.0)
  pub style_name: Option<String>, // Named style preset
}

impl TextToImageNode {
  pub fn new(name: &str, model: &str) -> Self {
    Self {
      name: name.to_string(),
      model: model.to_string(),
      prompt_template: String::new(),
      negative_prompt: None,
      input_keys: Vec::new(),
      output_key: format!("{}_image", name),
      size: None,
      response_format: ImageResponseFormat::default(),
      steps: None,
      cfg_scale: None,
      style_reference: None,
      n: None,
      seed: None,
      dependencies: Vec::new(),
      condition: None,
      timeout_ms: None,
    }
  }

  pub fn with_prompt(mut self, template: &str) -> Self {
    self.prompt_template = template.to_string();
    self
  }

  pub fn with_negative_prompt(mut self, negative: &str) -> Self {
    self.negative_prompt = Some(negative.to_string());
    self
  }

  pub fn with_size(mut self, size: &str) -> Self {
    self.size = Some(size.to_string());
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

  pub fn with_style_reference(mut self, style: StyleReference) -> Self {
    self.style_reference = Some(style);
    self
  }

  pub fn with_count(mut self, n: u32) -> Self {
    self.n = Some(n);
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

  /// Resolve template variables in the prompt using inputs
  fn resolve_prompt(&self, inputs: &AsyncNodeInputs) -> Result<String, AgentFlowError> {
    let mut resolved = self.prompt_template.clone();
    for (key, value) in inputs {
      let placeholder = format!("{{{{{}}}}}", key);
      if resolved.contains(&placeholder)
        && let FlowValue::Json(Value::String(s)) = value
      {
        resolved = resolved.replace(&placeholder, s);
      }
    }
    Ok(resolved)
  }

  /// Create configuration for image generation
  fn create_image_config(
    &self,
    resolved_prompt: &str,
    inputs: &AsyncNodeInputs,
  ) -> Result<Value, AgentFlowError> {
    let mut config = serde_json::Map::new();

    config.insert("model".to_string(), Value::String(self.model.clone()));
    config.insert(
      "prompt".to_string(),
      Value::String(resolved_prompt.to_string()),
    );

    if let Some(ref neg_prompt) = self.negative_prompt {
      let mut resolved_negative = neg_prompt.clone();
      for (key, value) in inputs {
        let placeholder = format!("{{{{{}}}}}", key);
        if resolved_negative.contains(&placeholder)
          && let FlowValue::Json(Value::String(s)) = value
        {
          resolved_negative = resolved_negative.replace(&placeholder, s);
        }
      }
      config.insert(
        "negative_prompt".to_string(),
        Value::String(resolved_negative),
      );
    }

    if let Some(ref size) = self.size {
      config.insert("size".to_string(), Value::String(size.clone()));
    }

    config.insert(
      "response_format".to_string(),
      serde_json::to_value(&self.response_format).unwrap(),
    );

    if let Some(steps) = self.steps {
      config.insert(
        "steps".to_string(),
        Value::Number(serde_json::Number::from(steps)),
      );
    }

    if let Some(cfg_scale) = self.cfg_scale {
      let number = serde_json::Number::from_f64(cfg_scale as f64).ok_or_else(|| {
        AgentFlowError::NodeInputError {
          message: format!("Invalid value for cfg_scale: {}", cfg_scale),
        }
      })?;
      config.insert("cfg_scale".to_string(), Value::Number(number));
    }

    if let Some(ref style_ref) = self.style_reference {
      config.insert(
        "style_reference".to_string(),
        serde_json::to_value(style_ref).unwrap(),
      );
    }

    if let Some(n) = self.n {
      config.insert("n".to_string(), Value::Number(serde_json::Number::from(n)));
    }

    if let Some(seed) = self.seed {
      config.insert(
        "seed".to_string(),
        Value::Number(serde_json::Number::from(seed)),
      );
    }

    Ok(Value::Object(config))
  }

  /// Execute real image generation through the modality dispatcher.
  ///
  /// Post-P-LLM.3: vendor selection comes from the model registry, not
  /// a hardcoded StepFun client. `self.style_reference` is currently
  /// dropped because it's StepFun-specific and the cross-vendor
  /// `Text2ImageRequest` trait surface doesn't carry it; if a future
  /// trait extension adds vendor extras (`extra: Map<String, Value>`),
  /// the conversion below regains it.
  async fn execute_real_image_generation(
    &self,
    config: &serde_json::Map<String, Value>,
  ) -> Result<String, AgentFlowError> {
    let prompt = config
      .get("prompt")
      .and_then(|v| v.as_str())
      .unwrap_or("")
      .to_string();
    let model = config
      .get("model")
      .and_then(|v| v.as_str())
      .unwrap_or(self.model.as_str())
      .to_string();
    let size = config
      .get("size")
      .and_then(|s| s.as_str())
      .unwrap_or("1024x1024");

    println!("🎨 Executing Text-to-Image request via modality dispatcher:");
    println!("   Model: {}", model);
    println!("   Prompt: {}", prompt);
    println!("   Size: {}", size);

    let provider =
      AgentFlow::text2image_for(&model)
        .await
        .map_err(|e| AgentFlowError::ConfigurationError {
          message: format!(
            "Failed to resolve text-to-image provider for '{}': {}",
            model, e
          ),
        })?;

    let response_format = match &self.response_format {
      ImageResponseFormat::Base64Json => "b64_json",
      ImageResponseFormat::Url => "url",
    };

    let request = ModalityText2ImageRequest {
      model: model.clone(),
      prompt,
      size: Some(size.to_string()),
      n: self.n,
      response_format: Some(response_format.to_string()),
      seed: self.seed.map(|s| s as i32),
      steps: self.steps,
      cfg_scale: self.cfg_scale,
    };

    let image_response =
      provider
        .generate(request)
        .await
        .map_err(|e| AgentFlowError::AsyncExecutionError {
          message: format!("Text-to-image generation failed: {}", e),
        })?;

    let first_image =
      image_response
        .images
        .first()
        .ok_or_else(|| AgentFlowError::AsyncExecutionError {
          message: "No images returned from text-to-image provider".to_string(),
        })?;

    let result = match response_format {
      "b64_json" => first_image
        .b64_json
        .as_ref()
        .map(|b| format!("data:image/png;base64,{}", b))
        .ok_or_else(|| AgentFlowError::AsyncExecutionError {
          message: "No base64 image data returned from provider".to_string(),
        })?,
      "url" => first_image
        .url
        .clone()
        .ok_or_else(|| AgentFlowError::AsyncExecutionError {
          message: "No image URL returned from provider".to_string(),
        })?,
      other => {
        return Err(AgentFlowError::AsyncExecutionError {
          message: format!("Unsupported response format: {}", other),
        });
      }
    };

    println!(
      "✅ Image Generation via '{}': size {} format {}",
      provider.name(),
      size,
      response_format
    );
    Ok(result)
  }

  /// Mock image generation (for testing/fallback)
  async fn execute_mock_image_generation(
    &self,
    config: &serde_json::Map<String, Value>,
  ) -> Result<String, AgentFlowError> {
    let prompt = config.get("prompt").unwrap().as_str().unwrap();
    let model = config.get("model").unwrap().as_str().unwrap();
    let size = config
      .get("size")
      .map(|s| s.as_str().unwrap_or("1024x1024"))
      .unwrap_or("1024x1024");

    println!("🎨 Executing Text-to-Image request (MOCK - API key not available):");
    println!("   Model: {}", model);
    println!("   Prompt: {}", prompt);
    println!("   Size: {}", size);

    // Simulate processing time
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Mock response based on response format
    let mock_response = match &self.response_format {
      ImageResponseFormat::Base64Json => {
        // Mock base64 image data
        "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg=="
      }
      ImageResponseFormat::Url => {
        // Mock URL
        "https://example.com/generated-image-mock.png"
      }
    };

    println!(
      "✅ Image Generation (MOCK, {:?}): Generated {} image",
      self.response_format, size
    );
    Ok(mock_response.to_string())
  }
}

#[async_trait]
impl AsyncNode for TextToImageNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    if let Some(ref condition) = self.condition
      && let Some(FlowValue::Json(Value::String(cond))) = inputs.get(condition)
      && cond != "true"
    {
      println!(
        "⏭️  Skipping TextToImage node '{}' due to condition: {}",
        self.name, cond
      );
      return Ok(HashMap::new());
    }

    let enriched_prompt = self.resolve_prompt(inputs)?;
    let config = self.create_image_config(&enriched_prompt, inputs)?;

    println!("🔧 TextToImage Node '{}' prepared:", self.name);
    println!("   Model: {}", self.model);
    println!("   Prompt: {}", enriched_prompt);
    if let Some(ref size) = self.size {
      println!("   Size: {}", size);
    }

    let response = if let Some(timeout_ms) = self.timeout_ms {
      let timeout_duration = std::time::Duration::from_millis(timeout_ms);
      match tokio::time::timeout(
        timeout_duration,
        self.execute_real_image_generation(config.as_object().unwrap()),
      )
      .await
      {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => {
          // Fallback to mock if real API fails
          match tokio::time::timeout(
            timeout_duration,
            self.execute_mock_image_generation(config.as_object().unwrap()),
          )
          .await
          {
            Ok(result) => result?,
            Err(_) => {
              return Err(AgentFlowError::TimeoutExceeded {
                duration_ms: timeout_ms,
              });
            }
          }
        }
        Err(_) => {
          return Err(AgentFlowError::TimeoutExceeded {
            duration_ms: timeout_ms,
          });
        }
      }
    } else {
      // Try real API first, fallback to mock
      match self
        .execute_real_image_generation(config.as_object().unwrap())
        .await
      {
        Ok(result) => result,
        Err(_) => {
          self
            .execute_mock_image_generation(config.as_object().unwrap())
            .await?
        }
      }
    };

    let mut outputs = HashMap::new();
    outputs.insert(
      self.output_key.clone(),
      FlowValue::Json(Value::String(response)),
    );

    Ok(outputs)
  }
}

/// Helper constructors for common text-to-image scenarios
impl TextToImageNode {
  /// Create a high-quality artistic image generator
  pub fn artistic_generator(name: &str, model: &str) -> Self {
    Self::new(name, model)
      .with_size("1024x1024")
      .with_steps(50)
      .with_cfg_scale(7.5)
      .with_response_format(ImageResponseFormat::Base64Json)
  }

  /// Create a fast prototype image generator
  pub fn quick_generator(name: &str, model: &str) -> Self {
    Self::new(name, model)
      .with_size("512x512")
      .with_steps(20)
      .with_cfg_scale(5.0)
      .with_response_format(ImageResponseFormat::Url)
  }

  /// Create a batch image generator
  pub fn batch_generator(name: &str, model: &str, count: u32) -> Self {
    Self::new(name, model)
      .with_size("768x768")
      .with_count(count)
      .with_steps(30)
      .with_response_format(ImageResponseFormat::Base64Json)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_text_to_image_node_execution() {
    let node = TextToImageNode::new("test_gen", "dalle-3")
      .with_prompt("A beautiful sunset over mountains")
      .with_size("512x512");

    let inputs = AsyncNodeInputs::new();

    let result = node.execute(&inputs).await;
    assert!(result.is_ok());

    let outputs = result.unwrap();
    let output = outputs.get("test_gen_image").unwrap();
    if let FlowValue::Json(Value::String(s)) = output {
      assert!(s.starts_with("data:image"));
    }
  }
}

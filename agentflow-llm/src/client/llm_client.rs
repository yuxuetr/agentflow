use crate::{
  config::ModelConfig,
  providers::{ProviderRequest},
  registry::ModelRegistry,
  multimodal::MultimodalMessage,
  StreamingResponse, Result,
};
use agentflow_core::observability::{ExecutionEvent, MetricsCollector};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

#[cfg(feature = "logging")]
use tracing::{debug, info, warn, error};

/// Response format options for model output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseFormat {
  /// Default text response
  Text,
  /// JSON object response (enforces valid JSON)
  JsonObject,
  /// JSON schema response with specific structure
  JsonSchema {
    /// Name of the response schema
    name: String,
    /// JSON schema definition
    schema: Value,
    /// Whether the schema is strict
    strict: Option<bool>,
  },
}

/// Main LLM client for executing model requests
pub struct LLMClient {
  pub model_name: String,
  pub prompt: String,
  pub multimodal_messages: Option<Vec<MultimodalMessage>>,
  pub temperature: Option<f32>,
  pub max_tokens: Option<u32>,
  pub top_p: Option<f32>,
  pub frequency_penalty: Option<f32>,
  pub stop: Option<Vec<String>>,
  pub tools: Option<Vec<Value>>,
  pub response_format: Option<ResponseFormat>,
  pub enable_logging: bool,
  pub additional_params: HashMap<String, Value>,
  pub metrics_collector: Option<Arc<MetricsCollector>>,
}

impl LLMClient {
  /// Create a new LLM client
  pub fn new(model_name: &str) -> Self {
    Self {
      model_name: model_name.to_string(),
      prompt: String::new(),
      multimodal_messages: None,
      temperature: None,
      max_tokens: None,
      top_p: None,
      frequency_penalty: None,
      stop: None,
      tools: None,
      response_format: None,
      enable_logging: true,
      additional_params: HashMap::new(),
      metrics_collector: None,
    }
  }

  /// Execute the request and return a non-streaming response
  pub async fn execute(&self) -> Result<String> {
    let start_time = Instant::now();
    
    if self.enable_logging {
      self.log_request_start();
    }
    
    // Record start event
    if let Some(ref collector) = self.metrics_collector {
      let event = ExecutionEvent {
        node_id: format!("llm.{}", self.model_name),
        event_type: "llm_request_start".to_string(),
        timestamp: start_time,
        duration_ms: None,
        metadata: {
          let mut meta = HashMap::new();
          meta.insert("model".to_string(), self.model_name.clone());
          meta.insert("prompt_length".to_string(), self.prompt.len().to_string());
          meta
        },
      };
      collector.record_event(event);
      collector.increment_counter(&format!("llm.{}.requests", self.model_name), 1.0);
    }

    let registry = ModelRegistry::global();
    let model_config = registry.get_model(&self.model_name)?;
    let provider = registry.get_provider(&model_config.vendor)?;

    let request = self.build_request(&model_config, false)?;
    let result = provider.execute(&request).await;
    let duration = start_time.elapsed();

    // Record completion event
    if let Some(ref collector) = self.metrics_collector {
      let (event_type, is_success) = match &result {
        Ok(_) => ("llm_request_success", true),
        Err(_) => ("llm_request_error", false),
      };

      let event = ExecutionEvent {
        node_id: format!("llm.{}", self.model_name),
        event_type: event_type.to_string(),
        timestamp: start_time,
        duration_ms: Some(duration.as_millis() as u64),
        metadata: {
          let mut meta = HashMap::new();
          meta.insert("model".to_string(), self.model_name.clone());
          meta.insert("duration_ms".to_string(), duration.as_millis().to_string());
          if let Ok(ref response) = result {
            meta.insert("response_length".to_string(), response.content.len().to_string());
            if let Some(ref usage) = response.usage {
              if let Some(prompt_tokens) = usage.prompt_tokens {
                meta.insert("prompt_tokens".to_string(), prompt_tokens.to_string());
              }
              if let Some(completion_tokens) = usage.completion_tokens {
                meta.insert("completion_tokens".to_string(), completion_tokens.to_string());
              }
              if let Some(total_tokens) = usage.total_tokens {
                meta.insert("total_tokens".to_string(), total_tokens.to_string());
              }
            }
          }
          meta
        },
      };
      collector.record_event(event);

      if is_success {
        collector.increment_counter(&format!("llm.{}.success", self.model_name), 1.0);
        collector.increment_counter(&format!("llm.{}.duration_ms", self.model_name), duration.as_millis() as f64);
        
        if let Ok(ref response) = result {
          if let Some(ref usage) = response.usage {
            if let Some(tokens) = usage.total_tokens {
              collector.increment_counter(&format!("llm.{}.total_tokens", self.model_name), tokens as f64);
            }
          }
        }
      } else {
        collector.increment_counter(&format!("llm.{}.errors", self.model_name), 1.0);
      }
    }

    let final_result = result.map(|response| response.content.to_string());
    
    if self.enable_logging {
      self.log_request_complete(&final_result, duration);
    }
    
    final_result
  }
  
  /// Log request start information
  fn log_request_start(&self) {
    #[cfg(feature = "logging")]
    {
      info!(
        "Starting LLM request: model={}, prompt_len={}, temp={:?}, format={:?}",
        self.model_name,
        self.prompt.len(),
        self.temperature,
        self.response_format
      );
      
      debug!("Full prompt: {}", self.prompt);
      
      if let Some(tools) = &self.tools {
        debug!("Tools enabled: {} functions", tools.len());
      }
    }
    
    #[cfg(not(feature = "logging"))]
    {
      println!("[AgentFlow] Request: {} ({})", self.model_name, self.prompt.len());
    }
  }
  
  /// Log request completion information
  fn log_request_complete(&self, result: &Result<String>, duration: std::time::Duration) {
    #[cfg(feature = "logging")]
    {
      match result {
        Ok(response) => {
          info!(
            "LLM request completed: model={}, duration={}ms, response_len={}",
            self.model_name,
            duration.as_millis(),
            response.len()
          );
          
          debug!("Response content: {}", response);
          
          // Validate JSON if JSON format was requested
          if let Some(ResponseFormat::JsonObject) | Some(ResponseFormat::JsonSchema { .. }) = &self.response_format {
            match serde_json::from_str::<Value>(response) {
              Ok(_) => debug!("✅ Response is valid JSON"),
              Err(e) => warn!("⚠️ Invalid JSON response: {}", e),
            }
          }
        }
        Err(e) => {
          error!(
            "LLM request failed: model={}, duration={}ms, error={}",
            self.model_name,
            duration.as_millis(),
            e
          );
        }
      }
    }
    
    #[cfg(not(feature = "logging"))]
    {
      match result {
        Ok(response) => println!("[AgentFlow] ✅ Response: {} chars in {}ms", response.len(), duration.as_millis()),
        Err(e) => println!("[AgentFlow] ❌ Error: {} ({}ms)", e, duration.as_millis()),
      }
    }
  }

  /// Execute the request and return a streaming response
  pub async fn execute_streaming(&self) -> Result<Box<dyn StreamingResponse>> {
    if self.enable_logging {
      self.log_request_start();
    }
    
    let registry = ModelRegistry::global();
    let model_config = registry.get_model(&self.model_name)?;
    let provider = registry.get_provider(&model_config.vendor)?;

    let request = self.build_request(&model_config, true)?;
    
    let result = provider.execute_streaming(&request).await;
    
    if self.enable_logging {
      #[cfg(feature = "logging")]
      match &result {
        Ok(_) => info!("Streaming request started successfully: {}", self.model_name),
        Err(e) => error!("Streaming request failed: {}", e),
      }
      
      #[cfg(not(feature = "logging"))]
      match &result {
        Ok(_) => println!("[AgentFlow] 📡 Streaming started: {}", self.model_name),
        Err(e) => println!("[AgentFlow] ❌ Streaming failed: {}", e),
      }
    }
    
    result
  }

  fn build_request(&self, model_config: &ModelConfig, streaming: bool) -> Result<ProviderRequest> {
    let mut params = HashMap::new();
    
    // Apply model defaults
    if let Some(temp) = model_config.temperature.or(self.temperature) {
      params.insert("temperature".to_string(), Value::Number(serde_json::Number::from_f64(temp as f64).unwrap()));
    }
    
    if let Some(tokens) = model_config.max_tokens.or(self.max_tokens) {
      params.insert("max_tokens".to_string(), Value::Number(serde_json::Number::from(tokens)));
    }

    if let Some(top_p) = model_config.top_p.or(self.top_p) {
      params.insert("top_p".to_string(), Value::Number(serde_json::Number::from_f64(top_p as f64).unwrap()));
    }

    if let Some(freq_penalty) = self.frequency_penalty {
      params.insert("frequency_penalty".to_string(), Value::Number(serde_json::Number::from_f64(freq_penalty as f64).unwrap()));
    }

    if let Some(stop_sequences) = &self.stop {
      if stop_sequences.len() == 1 {
        params.insert("stop".to_string(), Value::String(stop_sequences[0].clone()));
      } else {
        params.insert("stop".to_string(), Value::Array(stop_sequences.iter().map(|s| Value::String(s.clone())).collect()));
      }
    }

    if let Some(tools) = &self.tools {
      params.insert("tools".to_string(), Value::Array(tools.clone()));
    }
    
    // Add response format
    if let Some(format) = &self.response_format {
      match format {
        ResponseFormat::Text => {
          // Default, no parameter needed
        }
        ResponseFormat::JsonObject => {
          params.insert("response_format".to_string(), serde_json::json!({
            "type": "json_object"
          }));
        }
        ResponseFormat::JsonSchema { name, schema, strict } => {
          let mut format_obj = serde_json::json!({
            "type": "json_schema",
            "json_schema": {
              "name": name,
              "schema": schema
            }
          });
          if let Some(strict_val) = strict {
            format_obj["json_schema"]["strict"] = Value::Bool(*strict_val);
          }
          params.insert("response_format".to_string(), format_obj);
        }
      }
    }

    // Override with user-provided params
    for (key, value) in &self.additional_params {
      params.insert(key.clone(), value.clone());
    }

    // Build messages based on input type
    let messages = if let Some(ref multimodal_messages) = self.multimodal_messages {
      // Use multimodal messages directly
      self.build_multimodal_messages(multimodal_messages, model_config)?
    } else {
      // Use traditional prompt
      vec![self.build_message_content(model_config)?]
    };

    Ok(ProviderRequest {
      model: model_config.model_id.clone().unwrap_or_else(|| self.model_name.clone()),
      messages,
      stream: streaming,
      parameters: params,
    })
  }

  /// Build multimodal messages for the request
  fn build_multimodal_messages(
    &self,
    multimodal_messages: &[MultimodalMessage],
    model_config: &ModelConfig,
  ) -> Result<Vec<serde_json::Value>> {
    let mut messages = Vec::new();

    for msg in multimodal_messages {
      match model_config.model_type() {
        "multimodal" => {
          // Use full multimodal format for multimodal models
          messages.push(msg.to_openai_format());
        },
        _ => {
          // Convert to text-only for non-multimodal models
          messages.push(serde_json::json!({
            "role": msg.role,
            "content": msg.to_text_format()
          }));
        }
      }
    }

    Ok(messages)
  }

  /// Build message content based on model type and capabilities
  fn build_message_content(&self, model_config: &ModelConfig) -> Result<serde_json::Value> {
    // For now, we only support text content input
    // Future enhancement: detect and handle image/audio inputs based on model type
    match model_config.model_type() {
      "multimodal" => {
        // Check if prompt contains image markers or base64 data
        if self.prompt.contains("data:image/") || self.prompt.contains("![image]") {
          // TODO: Parse multimodal content - for now, pass as text
          Ok(serde_json::json!({"role": "user", "content": self.prompt}))
        } else {
          Ok(serde_json::json!({"role": "user", "content": self.prompt}))
        }
      },
      "image" => {
        // Image generation models might expect different prompt structure
        Ok(serde_json::json!({"role": "user", "content": self.prompt}))
      },
      "audio" | "tts" => {
        // Audio models might expect different prompt structure
        Ok(serde_json::json!({"role": "user", "content": self.prompt}))
      },
      _ => {
        // Default text-only message
        Ok(serde_json::json!({"role": "user", "content": self.prompt}))
      }
    }
  }
}

/// Builder pattern for LLM client
pub struct LLMClientBuilder {
  client: LLMClient,
}

impl LLMClientBuilder {
  pub fn new(model_name: &str) -> Self {
    Self {
      client: LLMClient::new(model_name),
    }
  }

  pub fn prompt(mut self, prompt: &str) -> Self {
    self.client.prompt = prompt.to_string();
    self.client.multimodal_messages = None; // Clear multimodal if using prompt
    self
  }

  /// Set multimodal messages (replaces any existing prompt or messages)
  pub fn multimodal_messages(mut self, messages: Vec<MultimodalMessage>) -> Self {
    self.client.multimodal_messages = Some(messages);
    self.client.prompt = String::new(); // Clear prompt if using multimodal
    self
  }

  /// Add a single multimodal message
  pub fn add_multimodal_message(mut self, message: MultimodalMessage) -> Self {
    if let Some(ref mut messages) = self.client.multimodal_messages {
      messages.push(message);
    } else {
      self.client.multimodal_messages = Some(vec![message]);
      self.client.prompt = String::new(); // Clear prompt if using multimodal
    }
    self
  }

  /// Shortcut for creating a multimodal prompt with text and image
  pub fn multimodal_prompt(mut self, message: MultimodalMessage) -> Self {
    self.client.multimodal_messages = Some(vec![message]);
    self.client.prompt = String::new(); // Clear prompt if using multimodal
    self
  }

  /// Helper method to create text + image message quickly
  pub fn text_and_image<T: Into<String>, U: Into<String>>(self, text: T, image_url: U) -> Self {
    let message = MultimodalMessage::text_and_image("user", text, image_url);
    self.multimodal_prompt(message)
  }


  pub fn temperature(mut self, temperature: f32) -> Self {
    self.client.temperature = Some(temperature);
    self
  }

  pub fn max_tokens(mut self, max_tokens: u32) -> Self {
    self.client.max_tokens = Some(max_tokens);
    self
  }

  pub fn top_p(mut self, top_p: f32) -> Self {
    self.client.top_p = Some(top_p);
    self
  }

  pub fn frequency_penalty(mut self, penalty: f32) -> Self {
    self.client.frequency_penalty = Some(penalty);
    self
  }

  pub fn stop<S: Into<String>>(mut self, stop_sequences: Vec<S>) -> Self {
    self.client.stop = Some(stop_sequences.into_iter().map(|s| s.into()).collect());
    self
  }

  pub fn tools(mut self, tools: Vec<Value>) -> Self {
    self.client.tools = Some(tools);
    self
  }
  
  pub fn response_format(mut self, format: ResponseFormat) -> Self {
    self.client.response_format = Some(format);
    self
  }
  
  pub fn json_mode(mut self) -> Self {
    self.client.response_format = Some(ResponseFormat::JsonObject);
    self
  }
  
  pub fn json_schema<S: Into<String>>(mut self, name: S, schema: Value) -> Self {
    self.client.response_format = Some(ResponseFormat::JsonSchema {
      name: name.into(),
      schema,
      strict: Some(true),
    });
    self
  }
  
  pub fn enable_logging(mut self, enabled: bool) -> Self {
    self.client.enable_logging = enabled;
    self
  }

  pub fn param<T: Into<Value>>(mut self, key: &str, value: T) -> Self {
    self.client.additional_params.insert(key.to_string(), value.into());
    self
  }

  pub fn with_metrics(mut self, collector: Arc<MetricsCollector>) -> Self {
    self.client.metrics_collector = Some(collector);
    self
  }

  pub async fn execute(self) -> Result<String> {
    self.client.execute().await
  }

  pub async fn execute_streaming(self) -> Result<Box<dyn StreamingResponse>> {
    self.client.execute_streaming().await
  }
}
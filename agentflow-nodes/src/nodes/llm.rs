use crate::{AsyncNode, SharedState, NodeError, NodeResult};
use agentflow_core::AgentFlowError;
use async_trait::async_trait;
use serde_json::Value;

/// LLM Node with optional agentflow-llm integration
#[derive(Debug, Clone)]
pub struct LlmNode {
  pub name: String,
  pub model: String,
  pub prompt_template: String,
  pub system_template: Option<String>,
  pub temperature: Option<f32>,
  pub max_tokens: Option<u32>,
  #[cfg(feature = "llm")]
  pub use_real_llm: bool,
}

impl LlmNode {
  pub fn new(name: &str, model: &str) -> Self {
    Self {
      name: name.to_string(),
      model: model.to_string(),
      prompt_template: String::new(),
      system_template: None,
      temperature: None,
      max_tokens: None,
      #[cfg(feature = "llm")]
      use_real_llm: false,
    }
  }

  pub fn with_prompt(mut self, template: &str) -> Self {
    self.prompt_template = template.to_string();
    self
  }

  pub fn with_system(mut self, template: &str) -> Self {
    self.system_template = Some(template.to_string());
    self
  }

  pub fn with_temperature(mut self, temperature: f32) -> Self {
    self.temperature = Some(temperature);
    self
  }

  pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
    self.max_tokens = Some(max_tokens);
    self
  }

  #[cfg(feature = "llm")]
  pub fn with_real_llm(mut self, enabled: bool) -> Self {
    self.use_real_llm = enabled;
    self
  }

  /// Create LLM configuration from resolved parameters
  fn create_llm_config(&self, resolved_prompt: &str, resolved_system: Option<&str>, resolved_model: &str) -> Value {
    let mut config = serde_json::Map::new();
    config.insert("model".to_string(), Value::String(resolved_model.to_string()));
    config.insert("prompt".to_string(), Value::String(resolved_prompt.to_string()));

    if let Some(system) = resolved_system {
      config.insert("system".to_string(), Value::String(system.to_string()));
    }

    if let Some(temp) = self.temperature {
      config.insert(
        "temperature".to_string(),
        Value::Number(serde_json::Number::from_f64(temp as f64).unwrap()),
      );
    }

    if let Some(tokens) = self.max_tokens {
      config.insert(
        "max_tokens".to_string(),
        Value::Number(serde_json::Number::from(tokens)),
      );
    }

    Value::Object(config)
  }

  /// Execute using real LLM (when agentflow-llm feature is enabled)
  #[cfg(feature = "llm")]
  async fn execute_real_llm(&self, config: &serde_json::Map<String, Value>) -> NodeResult<String> {
    use agentflow_llm::{AgentFlow, MultimodalMessage};
    
    let prompt = config.get("prompt").unwrap().as_str().unwrap();
    let model = config.get("model").unwrap().as_str().unwrap();
    let system = config.get("system").map(|s| s.as_str().unwrap_or(""));
    
    // Initialize AgentFlow (this handles configuration loading)
    AgentFlow::init().await.map_err(|e| {
      NodeError::ExecutionError {
        message: format!("Failed to initialize AgentFlow: {}", e),
      }
    })?;

    // Build request using fluent API
    let mut request = AgentFlow::model(model);
    
    // If system message is provided, use multimodal messages approach
    if let Some(sys) = system {
      if !sys.is_empty() {
        let system_msg = MultimodalMessage::system().add_text(sys).build();
        let user_msg = MultimodalMessage::user().add_text(prompt).build();
        request = request.multimodal_messages(vec![system_msg, user_msg]);
      } else {
        request = request.prompt(prompt);
      }
    } else {
      request = request.prompt(prompt);
    }
    
    if let Some(temp) = config.get("temperature").and_then(|t| t.as_f64()) {
      request = request.temperature(temp as f32);
    }
    
    if let Some(tokens) = config.get("max_tokens").and_then(|t| t.as_u64()) {
      request = request.max_tokens(tokens as u32);
    }

    // Execute the request
    let response = request.execute().await.map_err(|e| {
      NodeError::ExecutionError {
        message: format!("LLM request failed: {}", e),
      }
    })?;

    Ok(response)
  }

  /// Execute using mock LLM (fallback implementation)
  async fn execute_mock_llm(&self, config: &serde_json::Map<String, Value>) -> NodeResult<String> {
    let prompt = config.get("prompt").unwrap().as_str().unwrap();
    let model = config.get("model").unwrap().as_str().unwrap();

    println!("🤖 Executing LLM request (MOCK):");
    println!("   Model: {}", model);
    println!("   Prompt: {}", prompt);

    // Simulate processing time
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Mock response based on prompt content
    let mock_response = if prompt.contains("2+2") || prompt.contains("2 + 2") {
      "2 + 2 equals 4. This is a basic arithmetic operation."
    } else if prompt.contains("capital") && prompt.contains("France") {
      "The capital of France is Paris."
    } else if prompt.contains("quantum") {
      "Quantum computing uses quantum mechanical phenomena like superposition and entanglement to process information in ways that classical computers cannot."
    } else {
      "I understand your question. This is a mock response from the LLM node."
    };

    println!("✅ LLM Response (MOCK): {}", mock_response);
    Ok(mock_response.to_string())
  }
}

#[async_trait]
impl AsyncNode for LlmNode {
  async fn prep_async(&self, shared: &SharedState) -> Result<Value, AgentFlowError> {
    // Resolve templates using SharedState
    let resolved_prompt = shared.resolve_template_advanced(&self.prompt_template);
    let resolved_system = self
      .system_template
      .as_ref()
      .map(|s| shared.resolve_template_advanced(s));
    let resolved_model = shared.resolve_template_advanced(&self.model);

    let config = self.create_llm_config(&resolved_prompt, resolved_system.as_deref(), &resolved_model);

    println!("🔧 LLM Node '{}' prepared:", self.name);
    println!("   Model: {}", resolved_model);
    println!("   Prompt: {}", resolved_prompt);
    if let Some(system) = resolved_system {
      println!("   System: {}", system);
    }

    Ok(config)
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value, AgentFlowError> {
    let config = prep_result.as_object()
      .ok_or_else(|| AgentFlowError::AsyncExecutionError {
        message: "Invalid prep result for LLM node".to_string(),
      })?;

    let response = {
      #[cfg(feature = "llm")]
      if self.use_real_llm {
        match self.execute_real_llm(config).await {
          Ok(response) => response,
          Err(_) => {
            println!("⚠️  Real LLM failed, falling back to mock");
            self.execute_mock_llm(config).await.map_err(|e| AgentFlowError::AsyncExecutionError {
              message: e.to_string(),
            })?
          }
        }
      } else {
        self.execute_mock_llm(config).await.map_err(|e| AgentFlowError::AsyncExecutionError {
          message: e.to_string(),
        })?
      }

      #[cfg(not(feature = "llm"))]
      {
        self.execute_mock_llm(config).await.map_err(|e| AgentFlowError::AsyncExecutionError {
          message: e.to_string(),
        })?
      }
    };

    Ok(Value::String(response))
  }

  async fn post_async(
    &self,
    shared: &SharedState,
    _prep_result: Value,
    exec_result: Value,
  ) -> Result<Option<String>, AgentFlowError> {
    // Store the result in shared state for other nodes to use
    let output_key = format!("{}_output", self.name);
    shared.insert(output_key.clone(), exec_result.clone());

    // Also store as "answer" for common access
    shared.insert("answer".to_string(), exec_result);

    println!("💾 Stored LLM output in shared state as: {}", output_key);

    Ok(None) // No specific next action
  }

  fn get_node_id(&self) -> Option<String> {
    Some(self.name.clone())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_llm_node_creation() {
    let node = LlmNode::new("test_llm", "gpt-4");
    assert_eq!(node.name, "test_llm");
    assert_eq!(node.model, "gpt-4");
    assert!(node.prompt_template.is_empty());
    assert!(node.system_template.is_none());
    assert!(node.temperature.is_none());
    assert!(node.max_tokens.is_none());
  }

  #[tokio::test]
  async fn test_llm_node_builder_pattern() {
    let node = LlmNode::new("test_llm", "gpt-4")
      .with_prompt("What is 2+2?")
      .with_system("You are a helpful math assistant")
      .with_temperature(0.7)
      .with_max_tokens(100);

    assert_eq!(node.prompt_template, "What is 2+2?");
    assert_eq!(node.system_template, Some("You are a helpful math assistant".to_string()));
    assert_eq!(node.temperature, Some(0.7));
    assert_eq!(node.max_tokens, Some(100));
  }

  #[tokio::test]
  async fn test_llm_node_prep_async() {
    let node = LlmNode::new("math_llm", "gpt-4")
      .with_prompt("What is {{question}}?")
      .with_system("You are a {{role}} assistant");

    let shared = SharedState::new();
    shared.insert("question".to_string(), Value::String("2+2".to_string()));
    shared.insert("role".to_string(), Value::String("math".to_string()));

    let config = node.prep_async(&shared).await.unwrap();
    let config_obj = config.as_object().unwrap();

    assert_eq!(config_obj.get("model").unwrap().as_str().unwrap(), "gpt-4");
    assert_eq!(config_obj.get("prompt").unwrap().as_str().unwrap(), "What is 2+2?");
    assert_eq!(config_obj.get("system").unwrap().as_str().unwrap(), "You are a math assistant");
  }

  #[tokio::test]
  async fn test_llm_node_exec_async_mock() {
    let node = LlmNode::new("math_llm", "gpt-4")
      .with_prompt("What is 2+2?");

    let shared = SharedState::new();
    let prep_result = node.prep_async(&shared).await.unwrap();
    let exec_result = node.exec_async(prep_result).await.unwrap();

    assert_eq!(exec_result.as_str().unwrap(), "2 + 2 equals 4. This is a basic arithmetic operation.");
  }

  #[tokio::test]
  async fn test_llm_node_different_mock_responses() {
    let test_cases = vec![
      ("What is 2+2?", "2 + 2 equals 4. This is a basic arithmetic operation."),
      ("What is the capital of France?", "The capital of France is Paris."),
      ("Explain quantum computing", "Quantum computing uses quantum mechanical phenomena like superposition and entanglement to process information in ways that classical computers cannot."),
      ("Random question", "I understand your question. This is a mock response from the LLM node."),
    ];

    for (prompt, expected_response) in test_cases {
      let node = LlmNode::new("test_llm", "gpt-4").with_prompt(prompt);
      let shared = SharedState::new();
      let prep_result = node.prep_async(&shared).await.unwrap();
      let exec_result = node.exec_async(prep_result).await.unwrap();
      assert_eq!(exec_result.as_str().unwrap(), expected_response);
    }
  }

  #[tokio::test]
  async fn test_llm_node_post_async() {
    let node = LlmNode::new("test_llm", "gpt-4");
    let shared = SharedState::new();
    
    let response = Value::String("Test response".to_string());
    let prep_result = Value::Object(serde_json::Map::new());
    
    let result = node.post_async(&shared, prep_result, response.clone()).await.unwrap();
    assert!(result.is_none());

    // Verify shared state was updated
    assert_eq!(shared.get("test_llm_output").unwrap(), response);
    assert_eq!(shared.get("answer").unwrap(), response);
  }

  #[tokio::test]
  async fn test_llm_node_full_lifecycle() {
    let node = LlmNode::new("full_test", "gpt-4")
      .with_prompt("What is the capital of {{country}}?")
      .with_temperature(0.5);

    let shared = SharedState::new();
    shared.insert("country".to_string(), Value::String("France".to_string()));

    // Test full workflow
    let result = node.run_async(&shared).await.unwrap();
    assert!(result.is_none());

    // Verify output was stored
    let output = shared.get("full_test_output").unwrap();
    assert_eq!(output.as_str().unwrap(), "The capital of France is Paris.");
    
    let answer = shared.get("answer").unwrap();
    assert_eq!(answer.as_str().unwrap(), "The capital of France is Paris.");
  }

  #[tokio::test]
  async fn test_llm_node_template_resolution() {
    let node = LlmNode::new("template_test", "{{model_name}}")
      .with_prompt("Solve: {{problem}}")
      .with_system("You are a {{subject}} tutor");

    let shared = SharedState::new();
    shared.insert("model_name".to_string(), Value::String("gpt-3.5-turbo".to_string()));
    shared.insert("problem".to_string(), Value::String("x + 5 = 10".to_string()));
    shared.insert("subject".to_string(), Value::String("math".to_string()));

    let config = node.prep_async(&shared).await.unwrap();
    let config_obj = config.as_object().unwrap();

    assert_eq!(config_obj.get("model").unwrap().as_str().unwrap(), "gpt-3.5-turbo");
    assert_eq!(config_obj.get("prompt").unwrap().as_str().unwrap(), "Solve: x + 5 = 10");
    assert_eq!(config_obj.get("system").unwrap().as_str().unwrap(), "You are a math tutor");
  }

  #[tokio::test]
  async fn test_llm_node_get_node_id() {
    let node = LlmNode::new("unique_id", "gpt-4");
    assert_eq!(node.get_node_id().unwrap(), "unique_id");
  }

  #[tokio::test]
  async fn test_llm_node_with_numeric_parameters() {
    let node = LlmNode::new("numeric_test", "gpt-4")
      .with_temperature(0.8)
      .with_max_tokens(256);

    let shared = SharedState::new();
    let config = node.prep_async(&shared).await.unwrap();
    let config_obj = config.as_object().unwrap();

    let temp = config_obj.get("temperature").unwrap().as_f64().unwrap();
    assert!((temp - 0.8).abs() < 0.01, "Temperature {} should be approximately 0.8", temp);
    assert_eq!(config_obj.get("max_tokens").unwrap().as_u64().unwrap(), 256);
  }

  #[cfg(feature = "llm")]
  #[tokio::test]
  async fn test_llm_node_real_llm_fallback() {
    let node = LlmNode::new("real_test", "gpt-4").with_real_llm(true);
    let shared = SharedState::new();
    
    // This should succeed by falling back to mock when real LLM fails
    let result = node.run_async(&shared).await;
    assert!(result.is_ok());
    
    // Should have the fallback message in output
    let output = shared.get("real_test_output").unwrap();
    assert!(output.as_str().unwrap().contains("mock response"));
  }
}
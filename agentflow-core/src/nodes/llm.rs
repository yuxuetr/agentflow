use crate::{AsyncNode, SharedState};
use async_trait::async_trait;
use serde_json::Value;

/// Simple LLM Node that demonstrates template resolution and agentflow-llm integration
pub struct LlmNode {
  name: String,
  model: String,
  prompt_template: String,
  system_template: Option<String>,
  temperature: Option<f32>,
  max_tokens: Option<u32>,
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
}

#[async_trait]
impl AsyncNode for LlmNode {
  async fn prep_async(&self, shared: &SharedState) -> Result<Value, crate::AgentFlowError> {
    // Resolve templates using SharedState
    let resolved_prompt = shared.resolve_template_advanced(&self.prompt_template);
    let resolved_system = self
      .system_template
      .as_ref()
      .map(|s| shared.resolve_template_advanced(s));
    let resolved_model = shared.resolve_template_advanced(&self.model);

    // Create the configuration for the LLM request
    let mut config = serde_json::Map::new();
    config.insert("model".to_string(), Value::String(resolved_model.clone()));
    config.insert("prompt".to_string(), Value::String(resolved_prompt));

    if let Some(system) = resolved_system {
      config.insert("system".to_string(), Value::String(system));
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

    println!("🔧 LLM Node '{}' prepared:", self.name);
    println!("   Model: {}", resolved_model);
    println!(
      "   Prompt: {}",
      config.get("prompt").unwrap().as_str().unwrap_or("")
    );
    if let Some(system) = config.get("system") {
      println!("   System: {}", system.as_str().unwrap_or(""));
    }

    Ok(Value::Object(config))
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value, crate::AgentFlowError> {
    // In a real implementation, this would call agentflow-llm
    // For now, simulate the LLM response

    let config = prep_result.as_object().unwrap();
    let prompt = config.get("prompt").unwrap().as_str().unwrap();
    let model = config.get("model").unwrap().as_str().unwrap();

    println!("🤖 Executing LLM request:");
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

    println!("✅ LLM Response: {}", mock_response);

    Ok(Value::String(mock_response.to_string()))
  }

  async fn post_async(
    &self,
    shared: &SharedState,
    _prep_result: Value,
    exec_result: Value,
  ) -> Result<Option<String>, crate::AgentFlowError> {
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

use crate::{NodeError, NodeResult};
use agentflow_core::{AgentFlowError, AsyncNode, SharedState};
use agentflow_llm::AgentFlow;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Response format specification for LLM outputs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ResponseFormat {
  Text,
  Markdown,
  JSON {
    schema: Option<serde_json::Value>,
    strict: bool,
  },
  YAML,
  XML,
  CSV,
  Code { language: String },
  KeyValue,
  List,
  Table,
  Image { format: String },
  Audio { format: String },
  File { mime_type: String },
}

impl Default for ResponseFormat {
  fn default() -> Self {
    ResponseFormat::Text
  }
}

impl ResponseFormat {
  pub fn json_schema(schema: serde_json::Value) -> Self {
    ResponseFormat::JSON {
      schema: Some(schema),
      strict: true,
    }
  }

  pub fn loose_json() -> Self {
    ResponseFormat::JSON {
      schema: None,
      strict: false,
    }
  }
}

/// MCP tool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
  pub mcp_server: Option<MCPServerConfig>,
  pub available_tools: Vec<ToolDefinition>,
  pub auto_discover: bool,
  pub tool_filter: Option<Vec<String>>,
  pub max_tools: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MCPServerConfig {
  pub server_type: MCPServerType,
  pub connection_string: String,
  pub timeout_ms: Option<u64>,
  pub retry_attempts: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MCPServerType {
  Stdio { command: Vec<String> },
  HTTP { base_url: String },
  Unix { socket_path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
  pub name: String,
  pub description: String,
  pub parameters: serde_json::Value,
  pub source: ToolSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolSource {
  MCP { server: String },
  Builtin,
  Custom { handler: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToolChoice {
  None,
  Auto,
  Required,
  Specific { name: String },
  Any { names: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
  pub max_attempts: u32,
  pub initial_delay_ms: u64,
  pub backoff_multiplier: f32,
}

impl Default for RetryConfig {
  fn default() -> Self {
    Self {
      max_attempts: 3,
      initial_delay_ms: 1000,
      backoff_multiplier: 2.0,
    }
  }
}

/// Enhanced LLM Node with comprehensive parameter support
#[derive(Debug, Clone)]
pub struct LlmNode {
  // Basic identification
  pub name: String,
  pub model: String,
  
  // Core prompt configuration
  pub prompt_template: String,
  pub system_template: Option<String>,
  pub input_keys: Vec<String>,
  pub output_key: String,
  
  // Standard LLM parameters
  pub temperature: Option<f32>,
  pub max_tokens: Option<u32>,
  pub top_p: Option<f32>,
  pub top_k: Option<u32>,
  pub frequency_penalty: Option<f32>,
  pub presence_penalty: Option<f32>,
  pub stop: Option<Vec<String>>,
  pub seed: Option<u64>,
  
  // Response format specification
  pub response_format: ResponseFormat,
  
  // MCP tool integration
  pub tools: Option<ToolConfig>,
  pub tool_choice: Option<ToolChoice>,
  
  // Note: Multimodal support moved to ImageUnderstandNode for better separation of concerns
  
  // Workflow control
  pub dependencies: Vec<String>,
  pub condition: Option<String>,
  pub retry_config: Option<RetryConfig>,
  pub timeout_ms: Option<u64>,
  
  // Always use real LLM now that agentflow-llm is a required dependency
  pub use_real_llm: bool,
}

impl LlmNode {
  // Note: Image conversion functionality moved to shared utilities and ImageUnderstandNode

  pub fn new(name: &str, model: &str) -> Self {
    Self {
      name: name.to_string(),
      model: model.to_string(),
      prompt_template: String::new(),
      system_template: None,
      input_keys: Vec::new(),
      output_key: format!("{}_output", name),
      temperature: None,
      max_tokens: None,
      top_p: None,
      top_k: None,
      frequency_penalty: None,
      presence_penalty: None,
      stop: None,
      seed: None,
      response_format: ResponseFormat::default(),
      tools: None,
      tool_choice: None,
      dependencies: Vec::new(),
      condition: None,
      retry_config: None,
      timeout_ms: None,
      use_real_llm: true, // Always use real LLM by default
    }
  }

  // Builder pattern methods for core configuration
  pub fn with_prompt(mut self, template: &str) -> Self {
    self.prompt_template = template.to_string();
    self
  }

  pub fn with_system(mut self, template: &str) -> Self {
    self.system_template = Some(template.to_string());
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

  // Standard LLM parameter setters
  pub fn with_temperature(mut self, temperature: f32) -> Self {
    self.temperature = Some(temperature);
    self
  }

  pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
    self.max_tokens = Some(max_tokens);
    self
  }

  pub fn with_top_p(mut self, top_p: f32) -> Self {
    self.top_p = Some(top_p);
    self
  }

  pub fn with_top_k(mut self, top_k: u32) -> Self {
    self.top_k = Some(top_k);
    self
  }

  pub fn with_frequency_penalty(mut self, penalty: f32) -> Self {
    self.frequency_penalty = Some(penalty);
    self
  }

  pub fn with_presence_penalty(mut self, penalty: f32) -> Self {
    self.presence_penalty = Some(penalty);
    self
  }

  pub fn with_stop_sequences(mut self, stop: Vec<String>) -> Self {
    self.stop = Some(stop);
    self
  }

  pub fn with_seed(mut self, seed: u64) -> Self {
    self.seed = Some(seed);
    self
  }

  // Response format configuration
  pub fn with_response_format(mut self, format: ResponseFormat) -> Self {
    self.response_format = format;
    self
  }

  pub fn with_json_response(mut self, schema: Option<serde_json::Value>) -> Self {
    self.response_format = ResponseFormat::JSON {
      schema,
      strict: true,
    };
    self
  }

  pub fn with_markdown_response(mut self) -> Self {
    self.response_format = ResponseFormat::Markdown;
    self
  }

  // MCP tool configuration
  pub fn with_tools(mut self, tools: ToolConfig) -> Self {
    self.tools = Some(tools);
    self
  }

  pub fn with_tool_choice(mut self, choice: ToolChoice) -> Self {
    self.tool_choice = Some(choice);
    self
  }

  // Note: Multimodal support (images, audio) has been moved to ImageUnderstandNode 
  // for better separation of concerns. Use ImageUnderstandNode for vision tasks.

  // Workflow control
  pub fn with_dependencies(mut self, deps: Vec<String>) -> Self {
    self.dependencies = deps;
    self
  }

  pub fn with_condition(mut self, condition: &str) -> Self {
    self.condition = Some(condition.to_string());
    self
  }

  pub fn with_retry_config(mut self, config: RetryConfig) -> Self {
    self.retry_config = Some(config);
    self
  }

  pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
    self.timeout_ms = Some(timeout_ms);
    self
  }

  // Legacy compatibility - deprecated, always uses real LLM now
  pub fn with_real_llm(mut self, enabled: bool) -> Self {
    self.use_real_llm = enabled;
    self
  }
  
  // New method to explicitly enable mock mode for testing
  pub fn with_mock_mode(mut self) -> Self {
    self.use_real_llm = false;
    self
  }

  /// Create a text analysis node
  pub fn text_analyzer(name: &str, model: &str) -> Self {
    Self::new(name, model)
      .with_response_format(ResponseFormat::JSON {
        schema: Some(serde_json::json!({
          "type": "object",
          "properties": {
            "summary": {"type": "string"},
            "key_points": {
              "type": "array",
              "items": {"type": "string"}
            },
            "sentiment": {"type": "string", "enum": ["positive", "negative", "neutral"]},
            "confidence": {"type": "number", "minimum": 0, "maximum": 1}
          },
          "required": ["summary", "key_points", "sentiment", "confidence"]
        })),
        strict: true,
      })
      .with_temperature(0.3)
  }

  /// Create a creative writing node
  pub fn creative_writer(name: &str, model: &str) -> Self {
    Self::new(name, model)
      .with_response_format(ResponseFormat::Markdown)
      .with_temperature(0.8)
      .with_max_tokens(2000)
  }

  /// Create a code generation node
  pub fn code_generator(name: &str, model: &str, language: &str) -> Self {
    Self::new(name, model)
      .with_response_format(ResponseFormat::Code {
        language: language.to_string(),
      })
      .with_temperature(0.2)
      .with_max_tokens(1500)
  }

  /// Create a research node with web search tools
  pub fn web_researcher(name: &str, model: &str) -> Self {
    let tools = ToolConfig {
      mcp_server: Some(MCPServerConfig {
        server_type: MCPServerType::Stdio {
          command: vec!["python".to_string(), "web_search_mcp_server.py".to_string()],
        },
        connection_string: "stdio://web_search".to_string(),
        timeout_ms: Some(30000),
        retry_attempts: Some(3),
      }),
      available_tools: vec![
        ToolDefinition {
          name: "search_web".to_string(),
          description: "Search the web for information".to_string(),
          parameters: serde_json::json!({
            "type": "object",
            "properties": {
              "query": {"type": "string", "description": "Search query"},
              "max_results": {"type": "number", "default": 5}
            },
            "required": ["query"]
          }),
          source: ToolSource::MCP {
            server: "web_search".to_string(),
          },
        }
      ],
      auto_discover: true,
      tool_filter: Some(vec!["search_web".to_string(), "fetch_url".to_string()]),
      max_tools: Some(5),
    };

    Self::new(name, model)
      .with_tools(tools)
      .with_tool_choice(ToolChoice::Auto)
      .with_json_response(None)
      .with_temperature(0.4)
      .with_max_tokens(3000)
  }

  /// Create LLM configuration from resolved parameters
  fn create_llm_config(
    &self,
    resolved_prompt: &str,
    resolved_system: Option<&str>,
    resolved_model: &str,
  ) -> Value {
    let mut config = serde_json::Map::new();
    config.insert(
      "model".to_string(),
      Value::String(resolved_model.to_string()),
    );
    config.insert(
      "prompt".to_string(),
      Value::String(resolved_prompt.to_string()),
    );

    if let Some(system) = resolved_system {
      config.insert("system".to_string(), Value::String(system.to_string()));
    }

    // Standard LLM parameters
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

    if let Some(top_p) = self.top_p {
      config.insert(
        "top_p".to_string(),
        Value::Number(serde_json::Number::from_f64(top_p as f64).unwrap()),
      );
    }

    if let Some(top_k) = self.top_k {
      config.insert(
        "top_k".to_string(),
        Value::Number(serde_json::Number::from(top_k)),
      );
    }

    if let Some(freq_penalty) = self.frequency_penalty {
      config.insert(
        "frequency_penalty".to_string(),
        Value::Number(serde_json::Number::from_f64(freq_penalty as f64).unwrap()),
      );
    }

    if let Some(pres_penalty) = self.presence_penalty {
      config.insert(
        "presence_penalty".to_string(),
        Value::Number(serde_json::Number::from_f64(pres_penalty as f64).unwrap()),
      );
    }

    if let Some(stop_sequences) = &self.stop {
      config.insert(
        "stop".to_string(),
        Value::Array(stop_sequences.iter().map(|s| Value::String(s.clone())).collect()),
      );
    }

    if let Some(seed) = self.seed {
      config.insert(
        "seed".to_string(),
        Value::Number(serde_json::Number::from(seed)),
      );
    }

    // Response format
    config.insert(
      "response_format".to_string(),
      serde_json::to_value(&self.response_format).unwrap_or(Value::String("text".to_string())),
    );

    // Tools configuration
    if let Some(tools_config) = &self.tools {
      config.insert(
        "tools_config".to_string(),
        serde_json::to_value(tools_config).unwrap_or(Value::Null),
      );
    }

    if let Some(tool_choice) = &self.tool_choice {
      config.insert(
        "tool_choice".to_string(),
        serde_json::to_value(tool_choice).unwrap_or(Value::Null),
      );
    }

    // Note: Multimodal support removed - use ImageUnderstandNode for vision tasks

    // Workflow control parameters
    if let Some(timeout) = self.timeout_ms {
      config.insert(
        "timeout_ms".to_string(),
        Value::Number(serde_json::Number::from(timeout)),
      );
    }

    Value::Object(config)
  }

  /// Execute using real LLM via agentflow-llm
  async fn execute_real_llm(&self, config: &serde_json::Map<String, Value>) -> NodeResult<String> {
    use agentflow_llm::ResponseFormat as LlmResponseFormat;
    
    let mut prompt = config.get("prompt").unwrap().as_str().unwrap().to_string();
    let model = config.get("model").unwrap().as_str().unwrap();
    let system = config.get("system").map(|s| s.as_str().unwrap_or(""));
    
    // Initialize AgentFlow (this handles configuration loading)
    AgentFlow::init()
      .await
      .map_err(|e| NodeError::ConfigurationError {
        message: format!("Failed to initialize AgentFlow: {}", e),
      })?;
    
    println!("🤖 Executing LLM request:");
    println!("   Model: {}", model);
    println!("   Prompt: {}", prompt);
    if let Some(sys) = system {
      if !sys.is_empty() {
        println!("   System: {}", sys);
      }
    }
    
    // Build request using fluent API - LLMNode is text-only
    let mut request = AgentFlow::model(model).prompt(&prompt);
    
    // Add system message if present
    if let Some(sys) = system {
      if !sys.is_empty() {
        request = request.system(sys);
      }
    }
    
    // Apply all standard LLM parameters
    if let Some(temp) = config.get("temperature").and_then(|t| t.as_f64()) {
      request = request.temperature(temp as f32);
    }
    if let Some(max_tokens) = config.get("max_tokens").and_then(|t| t.as_i64()) {
      request = request.max_tokens(max_tokens as u32);
    }
    if let Some(top_p) = config.get("top_p").and_then(|t| t.as_f64()) {
      request = request.top_p(top_p as f32);
    }
    if let Some(freq_penalty) = config.get("frequency_penalty").and_then(|t| t.as_f64()) {
      request = request.frequency_penalty(freq_penalty as f32);
    }
    if let Some(stop_sequences) = config.get("stop").and_then(|v| v.as_array()) {
      let stops: Vec<String> = stop_sequences
        .iter()
        .filter_map(|s| s.as_str().map(|s| s.to_string()))
        .collect();
      if !stops.is_empty() {
        request = request.stop(stops);
      }
    }
    
    // Handle response format
    if let Some(resp_format) = config.get("response_format") {
      match serde_json::from_value::<ResponseFormat>(resp_format.clone()) {
        Ok(format) => match format {
          ResponseFormat::JSON { schema, strict } => {
            // Some models (like Qwen) require 'json' to be in the prompt when using JSON response format
            if !prompt.to_lowercase().contains("json") {
              prompt.push_str("\n\nPlease respond in JSON format.");
              // Update the request with the modified prompt
              request = request.prompt(&prompt);
            }
            
            if let Some(schema) = schema {
              request = request.response_format(LlmResponseFormat::JsonSchema {
                name: "response".to_string(),
                schema,
                strict: Some(strict),
              });
            } else {
              request = request.response_format(LlmResponseFormat::JsonObject);
            }
          }
          _ => {} // Other formats use default text response
        },
        Err(_) => {} // Invalid format, use default
      }
    }
    
    // Handle tools (MCP integration placeholder)
    if let Some(tools_config) = config.get("tools_config") {
      println!("⚠️  MCP tools support planned for future agentflow-mcp integration");
      println!("    Tools config: {}", serde_json::to_string_pretty(tools_config).unwrap_or_default());
    }
    
    // Add any additional parameters using the param() method
    if let Some(top_k) = config.get("top_k").and_then(|t| t.as_i64()) {
      request = request.param("top_k", top_k);
    }
    if let Some(seed) = config.get("seed").and_then(|t| t.as_i64()) {
      request = request.param("seed", seed);
    }
    if let Some(pres_penalty) = config.get("presence_penalty").and_then(|t| t.as_f64()) {
      request = request.param("presence_penalty", pres_penalty);
    }
    
    // Execute the request
    let response = request.execute().await.map_err(|e| {
      NodeError::ExecutionError {
        message: format!("LLM execution failed: {}", e),
      }
    })?;
    
    println!("✅ LLM Response: {}...", &response[..100.min(response.len())]);
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

    // Create base config
    let config = self.create_llm_config(
      &resolved_prompt,
      resolved_system.as_deref(),
      &resolved_model,
    );

    // Note: Image and audio processing removed - use ImageUnderstandNode for multimodal tasks

    println!("🔧 LLM Node '{}' prepared:", self.name);
    println!("   Model: {}", resolved_model);
    println!("   Prompt: {}", resolved_prompt);
    if let Some(system) = resolved_system {
      println!("   System: {}", system);
    }
    
    // Log additional features being used (text-only)
    if let Some(config_obj) = config.as_object() {
      if config_obj.contains_key("tools_config") {
        println!("   🔧 Tools: enabled");
      }
      if let Some(temp) = config_obj.get("temperature") {
        println!("   🌡️  Temperature: {}", temp);
      }
      if let Some(format) = config_obj.get("response_format") {
        println!("   📋 Response format: {}", serde_json::to_string(format).unwrap_or_default());
      }
    }

    Ok(config)
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value, AgentFlowError> {
    let config = prep_result
      .as_object()
      .ok_or_else(|| AgentFlowError::AsyncExecutionError {
        message: "Invalid prep result for LLM node".to_string(),
      })?;

    let response = if self.use_real_llm {
      match self.execute_real_llm(config).await {
        Ok(response) => response,
        Err(e) => {
          println!("⚠️  Real LLM failed ({}), falling back to mock", e);
          self.execute_mock_llm(config).await.map_err(|e| {
            AgentFlowError::AsyncExecutionError {
              message: e.to_string(),
            }
          })?
        }
      }
    } else {
      self
        .execute_mock_llm(config)
        .await
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
    assert_eq!(
      node.system_template,
      Some("You are a helpful math assistant".to_string())
    );
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
    assert_eq!(
      config_obj.get("prompt").unwrap().as_str().unwrap(),
      "What is 2+2?"
    );
    assert_eq!(
      config_obj.get("system").unwrap().as_str().unwrap(),
      "You are a math assistant"
    );
  }

  #[tokio::test]
  async fn test_llm_node_exec_async_mock() {
    let node = LlmNode::new("math_llm", "gpt-4").with_prompt("What is 2+2?");

    let shared = SharedState::new();
    let prep_result = node.prep_async(&shared).await.unwrap();
    let exec_result = node.exec_async(prep_result).await.unwrap();

    assert_eq!(
      exec_result.as_str().unwrap(),
      "2 + 2 equals 4. This is a basic arithmetic operation."
    );
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

    let result = node
      .post_async(&shared, prep_result, response.clone())
      .await
      .unwrap();
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
    shared.insert(
      "model_name".to_string(),
      Value::String("gpt-3.5-turbo".to_string()),
    );
    shared.insert(
      "problem".to_string(),
      Value::String("x + 5 = 10".to_string()),
    );
    shared.insert("subject".to_string(), Value::String("math".to_string()));

    let config = node.prep_async(&shared).await.unwrap();
    let config_obj = config.as_object().unwrap();

    assert_eq!(
      config_obj.get("model").unwrap().as_str().unwrap(),
      "gpt-3.5-turbo"
    );
    assert_eq!(
      config_obj.get("prompt").unwrap().as_str().unwrap(),
      "Solve: x + 5 = 10"
    );
    assert_eq!(
      config_obj.get("system").unwrap().as_str().unwrap(),
      "You are a math tutor"
    );
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
    assert!(
      (temp - 0.8).abs() < 0.01,
      "Temperature {} should be approximately 0.8",
      temp
    );
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

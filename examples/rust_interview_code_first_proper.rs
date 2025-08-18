// Proper Code-First Rust Interview Questions Workflow
// This demonstrates the correct use of agentflow-core workflow orchestration
// with LLM nodes, state management, and template resolution

use agentflow_core::{AsyncNode, SharedState};
use agentflow_llm::AgentFlow;
use async_trait::async_trait;
use serde_json::Value;

/// LLM Node that integrates agentflow-core with agentflow-llm
/// This is the proper way to use both crates together
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
  async fn prep_async(&self, shared: &SharedState) -> Result<Value, agentflow_core::AgentFlowError> {
    // Resolve templates using SharedState (this is the core workflow orchestration benefit!)
    let resolved_prompt = shared.resolve_template_advanced(&self.prompt_template);
    let resolved_system = self
      .system_template
      .as_ref()
      .map(|s| shared.resolve_template_advanced(s));

    // Create the configuration for the LLM request
    let mut config = serde_json::Map::new();
    config.insert("model".to_string(), Value::String(self.model.clone()));
    config.insert("prompt".to_string(), Value::String(resolved_prompt.clone()));

    if let Some(system) = resolved_system.clone() {
      config.insert("system".to_string(), Value::String(system.clone()));
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
    println!("   Model: {}", self.model);
    println!("   Prompt: {}", resolved_prompt);
    if let Some(system) = resolved_system {
      println!("   System: {}", system);
    }

    Ok(Value::Object(config))
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value, agentflow_core::AgentFlowError> {
    let config = prep_result.as_object().unwrap();
    let prompt = config.get("prompt").unwrap().as_str().unwrap();
    let model = config.get("model").unwrap().as_str().unwrap();
    let system = config.get("system").map(|s| s.as_str().unwrap_or(""));

    println!("🤖 Executing LLM request via agentflow-llm:");
    println!("   Model: {}", model);
    println!("   Prompt: {}", prompt.chars().take(80).collect::<String>() + "...");

    // Now we properly use agentflow-llm instead of bypassing it!
    let mut builder = AgentFlow::model(model).prompt(prompt);

    if let Some(system_prompt) = system {
      if !system_prompt.is_empty() {
        // Note: Current AgentFlow doesn't have system message support in the public API
        // This would be added to the AgentFlow builder pattern
        // For now, we'll append system to prompt as a workaround
        let full_prompt = format!("System: {}\n\nUser: {}", system_prompt, prompt);
        builder = AgentFlow::model(model).prompt(&full_prompt);
      }
    }

    if let Some(temp) = config.get("temperature") {
      if let Some(temp_f64) = temp.as_f64() {
        builder = builder.temperature(temp_f64 as f32);
      }
    }

    if let Some(tokens) = config.get("max_tokens") {
      if let Some(tokens_u64) = tokens.as_u64() {
        builder = builder.max_tokens(tokens_u64 as u32);
      }
    }

    // Execute the real LLM request
    match builder.execute().await {
      Ok(response) => {
        println!("✅ LLM Response received ({} chars)", response.len());
        println!("🤖 Using REAL AI-generated content from agentflow-llm");
        Ok(Value::String(response))
      }
      Err(e) => {
        println!("⚠️  LLM API call failed: {}", e);
        println!("🎭 Falling back to mock response");
        
        // Graceful degradation with context-aware mock responses
        let mock_response = if prompt.to_lowercase().contains("interview") && prompt.to_lowercase().contains("rust") {
          if prompt.to_lowercase().contains("evaluate") || prompt.to_lowercase().contains("assessment") {
            "[MOCK] These Rust interview questions demonstrate good coverage of fundamental concepts including ownership, async programming, and error handling. They are well-suited for 3-5 years experience level with appropriate technical depth and practical relevance."
          } else {
            "[MOCK] Here are 5 Rust backend interview questions:\n1. Explain ownership and borrowing\n2. Describe async/await patterns\n3. Error handling with Result<T,E>\n4. Trait design and generics\n5. Performance optimization techniques"
          }
        } else {
          "[MOCK] I understand your question. This is a context-aware mock response from the LLM node."
        };
        
        Ok(Value::String(mock_response.to_string()))
      }
    }
  }

  async fn post_async(
    &self,
    shared: &SharedState,
    _prep_result: Value,
    exec_result: Value,
  ) -> Result<Option<String>, agentflow_core::AgentFlowError> {
    // Store the result in shared state for other nodes to use (core workflow orchestration!)
    let output_key = format!("{}_output", self.name);
    shared.insert(output_key.clone(), exec_result.clone());

    println!("💾 Stored result in SharedState as: {}", output_key);

    Ok(None) // No specific next action - let workflow orchestrator decide
  }

  fn get_node_id(&self) -> Option<String> {
    Some(self.name.clone())
  }
}

/// Code-First Workflow Orchestrator
/// This demonstrates proper agentflow-core workflow construction
pub struct InterviewWorkflow {
  shared_state: SharedState,
  question_generator: LlmNode,
  question_evaluator: LlmNode,
}

impl InterviewWorkflow {
  pub fn new() -> Self {
    // Initialize shared state with workflow inputs
    let shared_state = SharedState::new();
    
    // Set up the workflow context
    shared_state.insert(
      "model".to_string(), 
      Value::String("step-2-mini".to_string())
    );
    shared_state.insert(
      "experience_level".to_string(),
      Value::String("3-5 years".to_string())
    );

    // Node 1: Question Generator
    let question_generator = LlmNode::new("question_generator", "step-2-mini")
      .with_system("A senior Rust engineer with extensive backend development experience")
      .with_prompt("Please help me create 5 Rust backend interview questions")
      .with_temperature(0.7)
      .with_max_tokens(800);

    // Node 2: Question Evaluator (depends on Node 1 output via template)
    let question_evaluator = LlmNode::new("question_evaluator", "step-2-mini")
      .with_system("You are a senior Rust backend interviewer, help me evaluate whether the following interview questions meet the standards for {{ experience_level }} of Rust backend development")
      .with_prompt("{{ question_generator_output }}")  // Template dependency!
      .with_temperature(0.6)
      .with_max_tokens(600);

    Self {
      shared_state,
      question_generator,
      question_evaluator,
    }
  }

  /// Execute the workflow using proper agentflow-core orchestration
  pub async fn execute(&self) -> Result<WorkflowResults, Box<dyn std::error::Error>> {
    println!("🚀 Executing Code-First Workflow with agentflow-core Orchestration");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Initialize AgentFlow system
    println!("\n🔧 Initializing AgentFlow LLM system...");
    match AgentFlow::init().await {
      Ok(_) => println!("✅ AgentFlow initialized successfully"),
      Err(e) => {
        println!("⚠️  AgentFlow initialization failed: {}", e);
        println!("🔄 Continuing with workflow (will use mock responses)");
      }
    }

    println!("\n📝 Step 1: Question Generation Node");
    println!("   🔗 Dependencies: None (entry point)");
    println!("   📊 State Variables: model, experience_level");
    
    // Execute Node 1 using agentflow-core orchestration
    self.question_generator.run_async(&self.shared_state).await?;
    
    println!("\n🔍 Step 2: Question Evaluation Node");
    println!("   🔗 Dependencies: question_generator_output");
    println!("   📊 Template Resolution: {{ question_generator_output }}");
    
    // Execute Node 2 - it will automatically resolve the template dependency!
    self.question_evaluator.run_async(&self.shared_state).await?;

    // Extract results from shared state (this is the workflow orchestration benefit!)
    let questions = self.shared_state
      .get("question_generator_output")
      .map(|v| v.as_str().unwrap_or("No questions generated").to_string())
      .unwrap_or_else(|| "No questions generated".to_string());

    let evaluation = self.shared_state
      .get("question_evaluator_output")
      .map(|v| v.as_str().unwrap_or("No evaluation performed").to_string())
      .unwrap_or_else(|| "No evaluation performed".to_string());

    Ok(WorkflowResults { questions, evaluation })
  }

  /// Demonstrate advanced workflow features
  pub async fn execute_with_advanced_features(&self) -> Result<WorkflowResults, Box<dyn std::error::Error>> {
    println!("\n🚀 Advanced Workflow Features Demo");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Demonstrate robustness features from agentflow-core
    println!("\n🛡️  Executing with robustness features:");
    
    // Execute with timeout
    println!("   ⏱️  Node 1 with timeout protection...");
    let timeout_duration = std::time::Duration::from_secs(30);
    self.question_generator
      .run_async_with_timeout(&self.shared_state, timeout_duration)
      .await?;

    // Execute with retries  
    println!("   🔄 Node 2 with retry protection...");
    let retry_wait = std::time::Duration::from_millis(1000);
    self.question_evaluator
      .run_async_with_retries(&self.shared_state, 3, retry_wait)
      .await?;

    // Show observability
    println!("\n📊 Workflow State After Execution:");
    for (key, value) in self.shared_state.iter() {
      let preview = match value.as_str() {
        Some(s) => format!("{}...", s.chars().take(50).collect::<String>()),
        None => format!("{:?}", value),
      };
      println!("   {}: {}", key, preview);
    }

    let questions = self.shared_state
      .get("question_generator_output")
      .map(|v| v.as_str().unwrap_or("No questions generated").to_string())
      .unwrap_or_else(|| "No questions generated".to_string());

    let evaluation = self.shared_state
      .get("question_evaluator_output")
      .map(|v| v.as_str().unwrap_or("No evaluation performed").to_string())
      .unwrap_or_else(|| "No evaluation performed".to_string());

    Ok(WorkflowResults { questions, evaluation })
  }
}

pub struct WorkflowResults {
  pub questions: String,
  pub evaluation: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("🎯 AgentFlow Code-First Workflow with Proper Core Integration");
  println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

  // Create and execute the workflow
  let workflow = InterviewWorkflow::new();
  
  // Execute basic workflow
  let results = workflow.execute().await?;

  // Display results
  println!("\n📋 Generated Questions:");
  println!("{}", results.questions);
  
  println!("\n📊 Quality Evaluation:");
  println!("{}", results.evaluation);

  // Demonstrate advanced features
  println!("\n\n🔧 Advanced Features Demonstration:");
  let _advanced_results = workflow.execute_with_advanced_features().await?;

  // Show the architecture benefits
  println!("\n\n🎯 Code-First + agentflow-core Architecture Benefits:");
  println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
  
  println!("\n✅ **Proper Workflow Orchestration**:");
  println!("   🔗 Template dependency resolution: {{ question_generator_output }}");
  println!("   📊 Shared state management across nodes");
  println!("   🔄 Sequential execution with data flow");
  
  println!("\n✅ **agentflow-core Integration**:");
  println!("   🛡️  Robustness: Timeouts, retries, error handling");
  println!("   📈 Observability: Metrics collection, state tracking");
  println!("   🎭 Graceful degradation: Mock fallbacks");
  
  println!("\n✅ **agentflow-llm Integration**:");
  println!("   🤖 Real LLM API calls when available");
  println!("   🔑 Automatic API key resolution");
  println!("   ⚙️  Model-agnostic interface");

  println!("\n✅ **Code-First Benefits**:");
  println!("   🧠 Complex logic and conditional flows");
  println!("   🔧 Custom node implementations");
  println!("   🎯 Type safety and IDE support");
  
  println!("\n❌ **What Was Wrong Before**:");
  println!("   🚫 Bypassed agentflow-core orchestration");
  println!("   🚫 No template resolution or state sharing");
  println!("   🚫 Manual dependency management");
  println!("   🚫 Lost robustness and observability features");

  println!("\n✨ Workflow completed with proper architecture!");

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_workflow_state_management() {
    let workflow = InterviewWorkflow::new();
    
    // Test that initial state is set correctly
    assert!(workflow.shared_state.contains_key("model"));
    assert!(workflow.shared_state.contains_key("experience_level"));
  }

  #[tokio::test]
  async fn test_node_template_resolution() {
    let shared_state = SharedState::new();
    shared_state.insert("test_var".to_string(), Value::String("test_value".to_string()));
    
    let node = LlmNode::new("test", "test_model")
      .with_prompt("Test: {{ test_var }}");
    
    let prep_result = node.prep_async(&shared_state).await.unwrap();
    let config = prep_result.as_object().unwrap();
    let resolved_prompt = config.get("prompt").unwrap().as_str().unwrap();
    
    assert_eq!(resolved_prompt, "Test: test_value");
  }

  #[tokio::test]
  async fn test_workflow_execution() {
    let workflow = InterviewWorkflow::new();
    
    // This might fail if no API key, but should handle gracefully
    let result = workflow.execute().await;
    
    // Should succeed even with mock responses
    assert!(result.is_ok());
    
    let results = result.unwrap();
    assert!(!results.questions.is_empty());
    assert!(!results.evaluation.is_empty());
  }
}
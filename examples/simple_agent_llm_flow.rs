// Simple Agent Flow with LLM Integration
// This example demonstrates how to create a flow where an LLM (moonshot demo)
// invokes and processes responses within an agent flow

use agentflow_core::{AgentFlowError, AsyncFlow, AsyncNode, Result, SharedState};
use agentflow_llm::AgentFlow as LLMAgentFlow;
use async_trait::async_trait;
use serde_json::Value;

// LLM-powered agent node that uses the moonshot demo pattern
pub struct LLMAgentNode {
  node_id: String,
  prompt_template: String,
  model_name: String,
  next_node: Option<String>,
}

impl LLMAgentNode {
  pub fn new(node_id: &str, prompt_template: &str, model_name: &str) -> Self {
    Self {
      node_id: node_id.to_string(),
      prompt_template: prompt_template.to_string(),
      model_name: model_name.to_string(),
      next_node: None,
    }
  }

  pub fn with_next_node(mut self, next_node: &str) -> Self {
    self.next_node = Some(next_node.to_string());
    self
  }

  fn build_prompt(&self, shared_state: &SharedState) -> String {
    let mut prompt = self.prompt_template.clone();

    // Replace placeholders in prompt template with values from shared state
    if let Some(Value::String(user_input)) = shared_state.get("user_input") {
      prompt = prompt.replace("{user_input}", &user_input);
    }

    if let Some(Value::String(context)) = shared_state.get("context") {
      prompt = prompt.replace("{context}", &context);
    }

    prompt
  }
}

#[async_trait]
impl AsyncNode for LLMAgentNode {
  async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
    // Prepare the prompt from shared state
    let prompt = self.build_prompt(shared);
    println!(
      "🤖 [{}] Preparing LLM request with prompt: {}",
      self.node_id, prompt
    );

    Ok(Value::Object({
      let mut map = serde_json::Map::new();
      map.insert("prompt".to_string(), Value::String(prompt));
      map.insert("model".to_string(), Value::String(self.model_name.clone()));
      map
    }))
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value> {
    let prompt = prep_result
      .get("prompt")
      .and_then(|v| v.as_str())
      .ok_or_else(|| AgentFlowError::AsyncExecutionError {
        message: "Invalid prompt in prep result".to_string(),
      })?;

    let model = prep_result
      .get("model")
      .and_then(|v| v.as_str())
      .unwrap_or(&self.model_name);

    println!(
      "🚀 [{}] Executing LLM request to model: {}",
      self.node_id, model
    );

    // Initialize the LLM system (following moonshot demo pattern)
    match LLMAgentFlow::init().await {
      Ok(()) => println!(
        "✅ [{}] LLM configuration loaded successfully",
        self.node_id
      ),
      Err(e) => {
        println!(
          "❌ [{}] Failed to load LLM configuration: {}",
          self.node_id, e
        );
        return Err(AgentFlowError::AsyncExecutionError {
          message: format!("LLM initialization failed: {}", e),
        });
      }
    }

    // Execute LLM request (following moonshot demo pattern)
    match LLMAgentFlow::model(model).prompt(prompt).execute().await {
      Ok(response) => {
        println!("✅ [{}] LLM Response received: {}", self.node_id, response);
        Ok(Value::Object({
          let mut map = serde_json::Map::new();
          map.insert("llm_response".to_string(), Value::String(response));
          map.insert("model_used".to_string(), Value::String(model.to_string()));
          map.insert("node_id".to_string(), Value::String(self.node_id.clone()));
          map
        }))
      }
      Err(e) => {
        println!("❌ [{}] LLM request failed: {}", self.node_id, e);
        Err(AgentFlowError::AsyncExecutionError {
          message: format!("LLM execution failed: {}", e),
        })
      }
    }
  }

  async fn post_async(
    &self,
    shared: &SharedState,
    _prep_result: Value,
    exec_result: Value,
  ) -> Result<Option<String>> {
    // Store the LLM response in shared state for other nodes
    if let Some(response) = exec_result.get("llm_response") {
      shared.insert(format!("{}_response", self.node_id), response.clone());
      println!("💾 [{}] Stored LLM response in shared state", self.node_id);
    }

    // Store execution metadata
    shared.insert(format!("{}_executed", self.node_id), Value::Bool(true));
    shared.insert("last_llm_result".to_string(), exec_result);

    println!("✨ [{}] Post-processing completed", self.node_id);
    Ok(self.next_node.clone())
  }
}

// Response processor node that analyzes LLM responses
pub struct ResponseProcessorNode {
  node_id: String,
  next_node: Option<String>,
}

impl ResponseProcessorNode {
  pub fn new(node_id: &str) -> Self {
    Self {
      node_id: node_id.to_string(),
      next_node: None,
    }
  }

  pub fn with_next_node(mut self, next_node: &str) -> Self {
    self.next_node = Some(next_node.to_string());
    self
  }
}

#[async_trait]
impl AsyncNode for ResponseProcessorNode {
  async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
    // Get the last LLM result from shared state
    let llm_result =
      shared
        .get("last_llm_result")
        .ok_or_else(|| AgentFlowError::AsyncExecutionError {
          message: "No LLM result found in shared state".to_string(),
        })?;

    Ok(llm_result.clone())
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value> {
    let llm_response = prep_result
      .get("llm_response")
      .and_then(|v| v.as_str())
      .unwrap_or("");

    println!(
      "🔍 [{}] Processing LLM response: {}",
      self.node_id, llm_response
    );

    // Simple response analysis
    let word_count = llm_response.split_whitespace().count();
    let sentiment = if llm_response.contains("good")
      || llm_response.contains("great")
      || llm_response.contains("excellent")
    {
      "positive"
    } else if llm_response.contains("bad")
      || llm_response.contains("terrible")
      || llm_response.contains("awful")
    {
      "negative"
    } else {
      "neutral"
    };

    let analysis = Value::Object({
      let mut map = serde_json::Map::new();
      map.insert(
        "word_count".to_string(),
        Value::Number(serde_json::Number::from(word_count)),
      );
      map.insert(
        "sentiment".to_string(),
        Value::String(sentiment.to_string()),
      );
      map.insert(
        "original_response".to_string(),
        Value::String(llm_response.to_string()),
      );
      map
    });

    println!(
      "📊 [{}] Analysis complete: {} words, {} sentiment",
      self.node_id, word_count, sentiment
    );

    Ok(analysis)
  }

  async fn post_async(
    &self,
    shared: &SharedState,
    _prep_result: Value,
    exec_result: Value,
  ) -> Result<Option<String>> {
    // Store analysis results
    shared.insert("response_analysis".to_string(), exec_result);
    shared.insert(format!("{}_executed", self.node_id), Value::Bool(true));

    println!("📋 [{}] Analysis stored in shared state", self.node_id);
    Ok(self.next_node.clone())
  }
}

// Simple decision node that routes based on analysis
pub struct DecisionNode {
  node_id: String,
}

impl DecisionNode {
  pub fn new(node_id: &str) -> Self {
    Self {
      node_id: node_id.to_string(),
    }
  }
}

#[async_trait]
impl AsyncNode for DecisionNode {
  async fn prep_async(&self, shared: &SharedState) -> Result<Value> {
    let analysis =
      shared
        .get("response_analysis")
        .ok_or_else(|| AgentFlowError::AsyncExecutionError {
          message: "No response analysis found".to_string(),
        })?;

    Ok(analysis.clone())
  }

  async fn exec_async(&self, prep_result: Value) -> Result<Value> {
    let sentiment = prep_result
      .get("sentiment")
      .and_then(|v| v.as_str())
      .unwrap_or("neutral");

    let word_count = prep_result
      .get("word_count")
      .and_then(|v| v.as_u64())
      .unwrap_or(0);

    println!(
      "🤔 [{}] Making decision based on sentiment: {}, words: {}",
      self.node_id, sentiment, word_count
    );

    let decision = match sentiment {
      "positive" => "success_node",
      "negative" => "retry_node",
      _ => {
        if word_count > 10 {
          "detailed_node"
        } else {
          "simple_node"
        }
      }
    };

    println!("✅ [{}] Decision made: route to {}", self.node_id, decision);

    Ok(Value::String(decision.to_string()))
  }

  async fn post_async(
    &self,
    shared: &SharedState,
    _prep_result: Value,
    exec_result: Value,
  ) -> Result<Option<String>> {
    let next_route = exec_result.as_str().map(|s| s.to_string());
    shared.insert("decision_made".to_string(), exec_result);
    shared.insert(format!("{}_executed", self.node_id), Value::Bool(true));

    Ok(next_route)
  }
}

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
  println!("🌟 Simple Agent Flow with LLM Integration Demo");
  println!("This demonstrates how LLM (moonshot demo) can be invoked within an agent flow\n");

  // Create the flow nodes
  let llm_node = LLMAgentNode::new(
    "initial_llm",
    "You are a helpful assistant. Please respond to: {user_input}. Consider this context: {context}",
    "moonshot-v1-8k"
  ).with_next_node("processor");

  let processor_node = ResponseProcessorNode::new("processor").with_next_node("decision");

  let decision_node = DecisionNode::new("decision");

  // Simple terminal nodes for different outcomes
  let success_node = LLMAgentNode::new(
    "success",
    "Great! The previous response was positive. Please provide a celebratory follow-up message.",
    "moonshot-v1-8k",
  );

  let retry_node = LLMAgentNode::new(
    "retry",
    "The previous response was negative. Please provide a more encouraging response to: {user_input}",
    "moonshot-v1-8k"
  );

  let detailed_node = LLMAgentNode::new(
    "detailed",
    "The previous response was detailed. Please summarize it in one sentence.",
    "moonshot-v1-8k",
  );

  let simple_node = LLMAgentNode::new(
    "simple",
    "The previous response was brief. Please expand on it with more details.",
    "moonshot-v1-8k",
  );

  // Create the async flow
  let mut flow = AsyncFlow::new(Box::new(llm_node));
  flow.add_node("processor".to_string(), Box::new(processor_node));
  flow.add_node("decision".to_string(), Box::new(decision_node));
  flow.add_node("success_node".to_string(), Box::new(success_node));
  flow.add_node("retry_node".to_string(), Box::new(retry_node));
  flow.add_node("detailed_node".to_string(), Box::new(detailed_node));
  flow.add_node("simple_node".to_string(), Box::new(simple_node));

  // Enable observability
  flow.enable_tracing("simple_agent_llm_flow".to_string());

  // Set up shared state with user input and context
  let shared = SharedState::new();
  shared.insert(
    "user_input".to_string(),
    Value::String("What is the Deep Learning?".to_string()),
  );
  shared.insert(
    "context".to_string(),
    Value::String("The user is planning outdoor activities.".to_string()),
  );

  println!("🚀 Starting the agent flow...\n");

  // Execute the flow
  match flow.run_async(&shared).await {
    Ok(final_result) => {
      println!("\n🎉 Flow completed successfully!");
      println!("Final result: {}", final_result);

      // Display the journey through the flow
      println!("\n📋 Flow execution summary:");
      if shared.contains_key("initial_llm_executed") {
        println!("✅ Initial LLM node executed");
        if let Some(response) = shared.get("initial_llm_response") {
          println!("   Response: {}", response);
        }
      }

      if shared.contains_key("processor_executed") {
        println!("✅ Response processor executed");
        if let Some(analysis) = shared.get("response_analysis") {
          println!("   Analysis: {}", analysis);
        }
      }

      if shared.contains_key("decision_executed") {
        println!("✅ Decision node executed");
        if let Some(decision) = shared.get("decision_made") {
          println!("   Decision: {}", decision);
        }
      }

      // Show which final node was executed
      for node in &["success", "retry", "detailed", "simple"] {
        if shared.contains_key(&format!("{}_executed", node)) {
          println!("✅ Final node '{}' executed", node);
          if let Some(response) = shared.get(&format!("{}_response", node)) {
            println!("   Final response: {}", response);
          }
        }
      }
    }
    Err(e) => {
      println!("❌ Flow execution failed: {}", e);
      return Err(e.into());
    }
  }

  println!("\n🏁 Demo completed!");
  Ok(())
}

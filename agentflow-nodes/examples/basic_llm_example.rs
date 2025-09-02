//! Basic LLM Node Example
//! 
//! This example demonstrates how to use agentflow-nodes to call real LLM models
//! with basic parameters like temperature and max_tokens.

use agentflow_core::{AsyncNode, SharedState};
use agentflow_nodes::LlmNode;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("🚀 Basic LLM Node Example");
  println!("========================\n");

  // Create shared state for context sharing between nodes
  let shared = SharedState::new();
  shared.insert("user_question".to_string(), Value::String("What is 2+2? Explain your reasoning.".to_string()));

  // Create a basic LLM node with real model calling enabled
  let llm_node = LlmNode::new("math_assistant", "step-2-mini")
    .with_prompt("Question: {{user_question}}\n\nPlease provide a clear and concise answer.")
    .with_system("You are a helpful math tutor. Explain your reasoning step by step.")
    .with_temperature(0.3) // Lower temperature for more focused responses
    .with_max_tokens(150)  // Limit response length
    .with_real_llm(true);  // Enable real LLM calling (this is the default now)

  println!("📋 Node Configuration:");
  println!("   Name: {}", llm_node.name);
  println!("   Model: {}", llm_node.model);
  println!("   Temperature: {:?}", llm_node.temperature);
  println!("   Max Tokens: {:?}", llm_node.max_tokens);
  println!("   Using Real LLM: {}\n", llm_node.use_real_llm);

  // Execute the LLM node
  println!("🔄 Executing LLM node...");
  match llm_node.run_async(&shared).await {
    Ok(_) => {
      // Retrieve the result from shared state
      if let Some(result) = shared.get("math_assistant_output") {
        println!("✅ LLM Response:");
        println!("   {}\n", result.as_str().unwrap_or("Could not parse response"));
        
        // Also check the generic "answer" key
        if let Some(answer) = shared.get("answer") {
          println!("📝 Answer (generic key): {}", answer.as_str().unwrap_or("N/A"));
        }
      } else {
        println!("❌ No response found in shared state");
      }
    }
    Err(e) => {
      println!("❌ Error executing LLM node: {}", e);
      println!("💡 Make sure you have:");
      println!("   1. Set up API keys in ~/.agentflow/.env");
      println!("   2. Run 'AgentFlow::generate_config().await?' to create config files");
      println!("   3. Configured the model 'gpt-4o-mini' in your models.yml");
    }
  }

  println!("\n🏁 Example completed!");
  Ok(())
}
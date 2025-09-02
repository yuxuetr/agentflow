//! Hello World LLM Example
//! 
//! The simplest possible example using agentflow-nodes to call a real LLM
//! with the prompt "Who are you"

use agentflow_core::{AsyncNode, SharedState};
use agentflow_nodes::LlmNode;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("🚀 Hello World LLM Example");
  println!("==========================\n");

  // Create shared state (not needed for this simple example, but good practice)
  let shared = SharedState::new();

  // Create the simplest possible LLM node
  let hello_node = LlmNode::new("hello", "qwen-plus")
    .with_prompt("Who are you?")
    .with_temperature(0.7)
    .with_max_tokens(100);

  println!("🤖 Asking the LLM: 'Who are you?'");
  println!("📡 Model: qwen-plus");
  println!("🌡️  Temperature: 0.7");
  println!("📏 Max tokens: 100\n");

  // Execute the LLM node
  println!("🔄 Calling the model...\n");

  match hello_node.run_async(&shared).await {
    Ok(_) => {
      // Get the response from shared state
      if let Some(response) = shared.get("hello_output") {
        println!("✅ LLM Response:");
        println!("┌─────────────────────────────────────────────────────────────┐");
        println!("│ {:<59} │", response.as_str().unwrap_or("Could not parse response"));
        println!("└─────────────────────────────────────────────────────────────┘");
      } else {
        println!("❌ No response found");
      }
    }
    Err(e) => {
      println!("❌ Error: {}", e);
      println!("\n💡 Quick Setup Guide:");
      println!("   1. Create ~/.agentflow/.env with your API keys:");
      println!("      STEPFUN_API_KEY=sk-your-stepfun-key-here");
      println!("   2. Make sure 'step-2-mini' is configured in ~/.agentflow/models.yml");
      println!("   3. Check your internet connection");
    }
  }

  println!("\n🏁 Hello World example completed!");
  Ok(())
}
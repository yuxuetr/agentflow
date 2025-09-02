use agentflow_core::{AsyncNode, SharedState};
use agentflow_nodes::LlmNode;
use agentflow_nodes::nodes::llm::ResponseFormat;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  // Initialize AgentFlow
  agentflow_llm::AgentFlow::init().await.expect("Failed to initialize AgentFlow");

  println!("🔍 Debug Test for OpenAI Models");
  println!("================================\n");

  let shared = SharedState::new();
  
  // Test specific models that were showing as unavailable
  let test_models = vec![
    "gpt-4o-2024-08-06",
    "gpt-audio",
    "gpt-4o-search-preview",
    "gpt-3.5-turbo-0125"
  ];

  for model_id in test_models {
    println!("Testing model: {}", model_id);
    println!("{}", "-".repeat(40));
    
    let node_name = format!("{}_test", model_id.replace("-", "_").replace(".", "_"));
    let test_node = LlmNode::new(&node_name, model_id)
      .with_prompt("Say 'test successful' in exactly 3 words.")
      .with_system("You are a helpful assistant.")
      .with_temperature(0.1)
      .with_max_tokens(10)
      .with_response_format(ResponseFormat::Markdown);

    match test_node.run_async(&shared).await {
      Ok(_) => {
        let output_key = format!("{}_output", node_name);
        if let Some(result) = shared.get(&output_key) {
          println!("✅ Success!");
          
          if let Some(text) = result.as_str() {
            println!("Response: {}", text);
            
            // Check for various mock indicators
            if text.contains("mock") || text.contains("Mock") || text.contains("MOCK") {
              println!("⚠️  WARNING: Response contains 'mock' - may be fallback!");
            }
            if text.is_empty() {
              println!("⚠️  WARNING: Empty response!");
            }
          } else {
            println!("⚠️  Response is not a string: {:?}", result);
          }
        } else {
          println!("❌ No output found in shared state");
          // Print all available keys
          println!("Available keys:");
          for (key, _) in shared.iter() {
            println!("  - {}", key);
          }
        }
      }
      Err(e) => {
        println!("❌ Error: {:?}", e);
      }
    }
    println!();
  }
  
  Ok(())
}

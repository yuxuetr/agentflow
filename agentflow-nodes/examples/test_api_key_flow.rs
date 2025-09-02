//! Test API Key Flow Through AgentFlow-Nodes
//! 
//! This tests the environment variable flow through the layers

use agentflow_core::{AsyncNode, SharedState};
use agentflow_nodes::LlmNode;
use agentflow_nodes::nodes::llm::ResponseFormat;
use serde_json::Value;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing API Key Flow Through AgentFlow-Nodes");
    println!("================================================\n");

    // Initialize AgentFlow once at the start (like in our working examples)
    println!("🔧 Initializing AgentFlow...");
    agentflow_llm::AgentFlow::init().await.expect("Failed to initialize AgentFlow");
    
    // Verify environment is loaded
    println!("✅ Environment check after init:");
    match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => println!("   ANTHROPIC_API_KEY: Set ({}...{})", &key[..4], &key[key.len()-4..]),
        Err(_) => println!("   ANTHROPIC_API_KEY: ❌ NOT SET"),
    }

    // Create shared state
    let shared = SharedState::new();
    shared.insert("test_prompt".to_string(), 
        Value::String("What is 2+2? Answer briefly.".to_string()));

    // Test a simple model directly through the provider (should work)
    println!("\n🔬 Testing direct provider call...");
    match agentflow_llm::AgentFlow::model("claude-3-haiku-20240307")
        .prompt("What is 2+2? Answer in one word.")
        .execute()
        .await 
    {
        Ok(response) => {
            println!("✅ Direct provider: SUCCESS");
            println!("   Response: {}", response.chars().take(50).collect::<String>());
        }
        Err(e) => {
            println!("❌ Direct provider: FAILED - {}", e);
        }
    }

    // Test the same model through LlmNode (this is failing)
    println!("\n🔬 Testing through LlmNode...");
    
    let llm_node = LlmNode::new("api_key_test", "claude-3-haiku-20240307")
        .with_prompt("What is {{test_prompt}}")
        .with_temperature(0.1)
        .with_max_tokens(10)
        .with_response_format(ResponseFormat::Text);

    match llm_node.run_async(&shared).await {
        Ok(_) => {
            if let Some(result) = shared.get("api_key_test_output") {
                let response = result.as_str().unwrap_or("No response");
                if response.contains("mock response") {
                    println!("❌ LlmNode: Fell back to mock (API key not passed through)");
                } else {
                    println!("✅ LlmNode: SUCCESS");
                    println!("   Response: {}", response.chars().take(50).collect::<String>());
                }
            }
        }
        Err(e) => {
            println!("❌ LlmNode: FAILED - {}", e);
        }
    }

    println!("\n🎯 Analysis:");
    println!("- If direct provider works but LlmNode fails, there's an environment isolation issue");
    println!("- If both fail, there's a broader API key issue");
    println!("- If both succeed, the issue is in our test setup");

    Ok(())
}

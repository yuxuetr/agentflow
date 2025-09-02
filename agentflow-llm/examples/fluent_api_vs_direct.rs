//! Compare Fluent API vs Direct Provider
//! 
//! This compares the AgentFlow fluent API (failing) with direct provider calls (working)

use agentflow_llm::config::LLMConfig;
use agentflow_llm::providers::{AnthropicProvider, LLMProvider, ProviderRequest};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Fluent API vs Direct Provider Comparison");
    println!("============================================\n");

    // Initialize AgentFlow
    agentflow_llm::AgentFlow::init().await?;
    
    // Get API key and create provider directly
    let config = LLMConfig::from_yaml("models: {}\nproviders: {}\ndefaults: {}")?;
    let api_key = config.get_api_key("anthropic")?;
    let provider = AnthropicProvider::new(&api_key, None)?;
    
    // Test 1: Direct provider call (we know this works)
    println!("🔧 Test 1: Direct Provider Call");
    println!("------------------------------");
    
    let mut params = std::collections::HashMap::new();
    params.insert("max_tokens".to_string(), json!(15));
    params.insert("temperature".to_string(), json!(0.1));
    
    let direct_request = ProviderRequest {
        model: "claude-3-haiku-20240307".to_string(),
        messages: vec![
            json!({"role": "user", "content": "What is 2+2? Answer briefly."})
        ],
        stream: false,
        parameters: params,
    };
    
    match provider.execute(&direct_request).await {
        Ok(response) => {
            println!("✅ Direct provider: SUCCESS");
            println!("   Response: {:?}", response.content);
        }
        Err(e) => {
            println!("❌ Direct provider: FAILED - {}", e);
        }
    }
    
    // Test 2: AgentFlow fluent API (this is what's failing)
    println!("\n🔧 Test 2: AgentFlow Fluent API");
    println!("-------------------------------");
    
    match agentflow_llm::AgentFlow::model("claude-3-haiku-20240307")
        .prompt("What is 2+2? Answer briefly.")
        .max_tokens(15)
        .temperature(0.1)
        .execute()
        .await 
    {
        Ok(response) => {
            println!("✅ Fluent API: SUCCESS");
            println!("   Response: {}", response.chars().take(100).collect::<String>());
        }
        Err(e) => {
            println!("❌ Fluent API: FAILED - {}", e);
            
            // Analyze the error
            match &e {
                agentflow_llm::LLMError::HttpError { status_code, message } => {
                    println!("   HTTP Status: {}", status_code);
                    println!("   HTTP Body: {}", message);
                }
                _ => {
                    println!("   Other error: {:?}", e);
                }
            }
        }
    }
    
    println!("\n🎯 Diagnosis:");
    println!("- If direct provider succeeds but fluent API fails:");
    println!("  → Issue is in the AgentFlow fluent API layer");
    println!("- If both succeed:");
    println!("  → Issue is specifically in agentflow-nodes integration");
    println!("- If both fail:");
    println!("  → Issue is in the core provider (but we know this works)");

    Ok(())
}

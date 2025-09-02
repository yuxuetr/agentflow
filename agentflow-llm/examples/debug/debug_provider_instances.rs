//! Debug Provider Instances
//! 
//! Compare registry provider vs direct provider creation

use agentflow_llm::registry::ModelRegistry;
use agentflow_llm::config::LLMConfig;
use agentflow_llm::providers::{ProviderRequest, LLMProvider, AnthropicProvider};
use std::collections::HashMap;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Debug Provider Instances");
    println!("============================\n");

    // Initialize AgentFlow 
    agentflow_llm::AgentFlow::init().await?;
    
    // Create the test request
    let test_request = ProviderRequest {
        model: "claude-3-haiku-20240307".to_string(),
        messages: vec![json!({
            "role": "user", 
            "content": "What is 2+2? Answer briefly."
        })],
        stream: false,
        parameters: {
            let mut p = HashMap::new();
            p.insert("max_tokens".to_string(), json!(15));
            p.insert("temperature".to_string(), json!(0.1));
            p
        },
    };
    
    // Test 1: Direct provider creation (we know this works)
    println!("🔧 Test 1: Direct Provider Creation");
    
    let config = LLMConfig::from_yaml("models: {}\nproviders: {}\ndefaults: {}")?;
    let api_key = config.get_api_key("anthropic")?;
    let direct_provider = AnthropicProvider::new(&api_key, None)?;
    
    match direct_provider.execute(&test_request).await {
        Ok(response) => {
            println!("✅ Direct provider: SUCCESS");
            println!("   Response: {:?}", response.content);
        }
        Err(e) => {
            println!("❌ Direct provider: FAILED - {}", e);
        }
    }
    
    // Test 2: Registry provider (this is what fluent API uses)
    println!("\n🔧 Test 2: Registry Provider");
    
    let registry = ModelRegistry::global();
    let model_config = registry.get_model("claude-3-haiku-20240307")?;
    let registry_provider = registry.get_provider(&model_config.vendor)?;
    
    match registry_provider.execute(&test_request).await {
        Ok(response) => {
            println!("✅ Registry provider: SUCCESS");
            println!("   Response: {:?}", response.content);
        }
        Err(e) => {
            println!("❌ Registry provider: FAILED - {}", e);
            
            // Let's inspect the provider more
            println!("   Provider name: {}", registry_provider.name());
            println!("   Provider base_url: {}", registry_provider.base_url());
        }
    }
    
    // Test 3: Check if the API key is being passed correctly to registry provider
    println!("\n🔧 Test 3: API Key Check");
    
    // Let's check what the registry provider gets as an API key
    // We can't directly access the API key, but we can test with a validation call
    match registry_provider.validate_config().await {
        Ok(()) => {
            println!("✅ Registry provider validate_config: SUCCESS");
        }
        Err(e) => {
            println!("❌ Registry provider validate_config: FAILED - {}", e);
        }
    }
    
    match direct_provider.validate_config().await {
        Ok(()) => {
            println!("✅ Direct provider validate_config: SUCCESS");
        }
        Err(e) => {
            println!("❌ Direct provider validate_config: FAILED - {}", e);
        }
    }

    println!("\n🎯 Analysis:");
    println!("- If direct provider works but registry provider fails:");
    println!("  → Issue is in how registry creates/configures providers");
    println!("- If both fail now but direct worked before:");
    println!("  → Something changed in the environment or API key handling");

    Ok(())
}

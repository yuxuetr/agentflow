//! Debug HTTP Request Details
//! 
//! This captures and compares the exact HTTP request being made

use agentflow_llm::config::LLMConfig;
use agentflow_llm::providers::{AnthropicProvider, LLMProvider, ProviderRequest};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Debug HTTP Request Details");
    println!("=============================\n");

    // Initialize AgentFlow
    agentflow_llm::AgentFlow::init().await?;
    
    // Get API key
    let config = LLMConfig::from_yaml("models: {}\nproviders: {}\ndefaults: {}")?;
    let api_key = config.get_api_key("anthropic")?;
    
    println!("✅ API Key: {}...{} (length: {})", &api_key[..4], &api_key[api_key.len()-4..], api_key.len());
    
    // Create provider
    let provider = AnthropicProvider::new(&api_key, None)?;
    println!("✅ Provider created successfully");
    
    // Test the validate_config method (this makes a real HTTP request)
    println!("\n🔗 Testing validate_config (makes real HTTP request)...");
    match provider.validate_config().await {
        Ok(()) => {
            println!("✅ validate_config(): SUCCESS - API key and connection working");
        }
        Err(e) => {
            println!("❌ validate_config(): FAILED - {}", e);
            
            // This tells us exactly what the HTTP error is
            match &e {
                agentflow_llm::LLMError::HttpError { status_code, message } => {
                    println!("   HTTP Status: {}", status_code);
                    println!("   HTTP Body: {}", message);
                }
                agentflow_llm::LLMError::AuthenticationError { provider, message } => {
                    println!("   Auth Error for {}: {}", provider, message);
                }
                _ => {
                    println!("   Other error: {:?}", e);
                }
            }
        }
    }

    // Let's also test with a real execute request
    println!("\n🔗 Testing real execute request...");
    
    // Create a sample ProviderRequest
    let mut params = std::collections::HashMap::new();
    params.insert("max_tokens".to_string(), json!(10));
    params.insert("temperature".to_string(), json!(0.1));
    
    let test_request = ProviderRequest {
        model: "claude-3-haiku-20240307".to_string(),
        messages: vec![
            json!({"role": "user", "content": "What is 2+2?"})
        ],
        stream: false,
        parameters: params,
    };
    
    println!("📋 Request details:");
    println!("   Model: {}", test_request.model);
    println!("   Messages: {:?}", test_request.messages);
    println!("   Stream: {}", test_request.stream);
    println!("   Parameters: {:?}", test_request.parameters);
    
    // Try the actual execute method
    match provider.execute(&test_request).await {
        Ok(response) => {
            println!("✅ execute(): SUCCESS");
            println!("   Response: {}", format!("{:?}", response.content).chars().take(100).collect::<String>());
        }
        Err(e) => {
            println!("❌ execute(): FAILED - {}", e);
            
            match &e {
                agentflow_llm::LLMError::HttpError { status_code, message } => {
                    println!("   HTTP Status: {}", status_code);
                    println!("   HTTP Body: {}", message);
                    
                    // Parse the error message to understand the issue
                    if message.contains("not_found") {
                        println!("   🔍 Analysis: Model not found - check model name or API access");
                    } else if message.contains("authentication") || message.contains("unauthorized") {
                        println!("   🔍 Analysis: Authentication issue - check API key");
                    }
                }
                _ => {
                    println!("   Other error type: {:?}", e);
                }
            }
        }
    }
    
    println!("\n💡 Analysis:");
    println!("- If validate_config() succeeds, the basic connection works");
    println!("- If execute() fails with 404, check the specific error message");
    println!("- Compare the request format with working curl commands");

    Ok(())
}

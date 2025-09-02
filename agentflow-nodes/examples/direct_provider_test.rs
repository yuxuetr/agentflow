//! Direct Provider Test
//! Tests the agentflow-llm Anthropic provider directly to isolate the issue

use agentflow_llm::providers::{AnthropicProvider, LLMProvider, ProviderRequest};
use serde_json::json;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("🔍 Direct Provider Test");
  println!("======================\n");

  // Initialize AgentFlow to load environment variables
  agentflow_llm::AgentFlow::init().await.expect("Failed to initialize AgentFlow");

  // Get API key from environment
  let api_key = std::env::var("ANTHROPIC_API_KEY")
    .expect("ANTHROPIC_API_KEY must be set");
  
  println!("✅ Found API key: {}...", &api_key[..20]);

  // Create provider directly
  let provider = AnthropicProvider::new(&api_key, None)?;
  println!("✅ Created Anthropic provider");

  // Test the provider's supported models
  println!("\n📋 Provider supported models:");
  for model in provider.supported_models() {
    println!("   • {}", model);
  }

  // Test validate_config
  println!("\n🔧 Testing provider config validation...");
  match provider.validate_config().await {
    Ok(_) => println!("   ✅ Provider configuration is valid"),
    Err(e) => println!("   ❌ Provider config validation failed: {}", e),
  }

  // Test a direct request using the provider
  println!("\n🧪 Testing direct provider request...");
  
  let request = ProviderRequest {
    model: "claude-3-haiku-20240307".to_string(),
    messages: vec![
      json!({"role": "user", "content": "Hello! This is a direct provider test."})
    ],
    stream: false,
    parameters: {
      let mut params = HashMap::new();
      params.insert("max_tokens".to_string(), json!(20));
      params.insert("temperature".to_string(), json!(0.3));
      params
    },
  };

  match provider.execute(&request).await {
    Ok(response) => {
      println!("   ✅ Direct provider request SUCCESSFUL!");
      println!("   📝 Response: {:?}", response.content);
      if let Some(usage) = &response.usage {
        println!("   📊 Usage: {:?}", usage);
      }
    }
    Err(e) => {
      println!("   ❌ Direct provider request FAILED: {}", e);
    }
  }

  println!("\n💡 This test isolates whether the issue is in:");
  println!("   • AgentFlow-LLM provider implementation");
  println!("   • Or in the nodes/registry layer");
  
  Ok(())
}

//! Debug API Key Retrieval
//! 
//! This tests the exact API key retrieval process used by AgentFlow

use agentflow_llm::config::LLMConfig;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Debug API Key Retrieval Test");
    println!("===============================\n");

    // Initialize AgentFlow to load environment
    println!("🔧 Calling AgentFlow::init()...");
    agentflow_llm::AgentFlow::init().await?;
    
    // Check raw environment variable
    println!("\n📋 Direct Environment Variable Check:");
    match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => {
            let masked = if key.len() > 8 {
                format!("{}...{}", &key[..4], &key[key.len()-4..])
            } else {
                "***MASKED***".to_string()
            };
            println!("✅ ANTHROPIC_API_KEY: {} (length: {})", masked, key.len());
            
            // Check if it starts with expected prefix
            if key.starts_with("sk-ant-") {
                println!("✅ API key format: Valid Anthropic format");
            } else {
                println!("⚠️  API key format: Unexpected format (expected sk-ant-*)");
            }
        }
        Err(e) => println!("❌ ANTHROPIC_API_KEY: Not found - {}", e),
    }

    // Test the config's get_api_key method
    println!("\n🔧 Testing LLMConfig API Key Retrieval:");
    
    // Create a minimal config (using from_yaml with empty config)
    let minimal_yaml = r#"
models: {}
providers: {}
defaults: {}
"#;
    
    match LLMConfig::from_yaml(minimal_yaml) {
        Ok(config) => {
            println!("✅ LLMConfig created successfully");
            
            match config.get_api_key("anthropic") {
                Ok(key) => {
                    let masked = if key.len() > 8 {
                        format!("{}...{}", &key[..4], &key[key.len()-4..])
                    } else {
                        "***MASKED***".to_string()
                    };
                    println!("✅ Config.get_api_key(\"anthropic\"): {} (length: {})", masked, key.len());
                    
                    // Check if the retrieved key matches the env var
                    match env::var("ANTHROPIC_API_KEY") {
                        Ok(env_key) => {
                            if key == env_key {
                                println!("✅ Key consistency: Config matches environment");
                            } else {
                                println!("❌ Key consistency: Config differs from environment!");
                            }
                        }
                        Err(_) => println!("❌ Key consistency: Cannot compare - env var missing"),
                    }
                    
                    // Test creating the provider directly with the retrieved key
                    println!("\n🏭 Testing Provider Creation:");
                    
                    match agentflow_llm::providers::AnthropicProvider::new(&key, None) {
                        Ok(_provider) => {
                            println!("✅ AnthropicProvider::new(): SUCCESS");
                        }
                        Err(e) => {
                            println!("❌ AnthropicProvider::new(): FAILED - {}", e);
                        }
                    }
                }
                Err(e) => println!("❌ Config.get_api_key(\"anthropic\"): Failed - {}", e),
            }
        }
        Err(e) => {
            println!("❌ Failed to create LLMConfig: {}", e);
        }
    }

    println!("\n🎯 Summary:");
    println!("- Environment variable loading: Check above");  
    println!("- Config API key retrieval: Check above");
    println!("- Provider instantiation: Check above");
    println!("- If all succeed but HTTP fails, the issue is in the HTTP request itself");

    Ok(())
}

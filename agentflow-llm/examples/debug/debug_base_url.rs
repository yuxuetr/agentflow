//! Debug Base URL Configuration
//! 
//! Check what base_url the registry is using vs direct creation

use agentflow_llm::registry::ModelRegistry;
use agentflow_llm::providers::LLMProvider;
use agentflow_llm::config::LLMConfig;
use agentflow_llm::providers::AnthropicProvider;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Debug Base URL Configuration");
    println!("================================\n");

    // Initialize AgentFlow 
    agentflow_llm::AgentFlow::init().await?;
    
    let registry = ModelRegistry::global();
    
    // Check the configuration that the registry uses
    println!("🔧 Checking Registry Configuration:");
    
    let config = LLMConfig::from_yaml("models: {}\nproviders: {}\ndefaults: {}")?;
    
    // Check what the config returns for the anthropic provider
    match config.get_provider("anthropic") {
        Some(provider_config) => {
            println!("✅ Anthropic provider config found");
            match &provider_config.base_url {
                Some(base_url) => {
                    println!("   ⚠️  Base URL from config: '{}'", base_url);
                    println!("   🔍 This might be different from the default!");
                }
                None => {
                    println!("   ✅ Base URL: None (will use default)");
                }
            }
        }
        None => {
            println!("❌ No anthropic provider config found");
        }
    }
    
    // Get the actual provider from registry to see what base_url it has
    println!("\n🔧 Checking Registry Provider:");
    let model_config = registry.get_model("claude-3-haiku-20240307")?;
    let registry_provider = registry.get_provider(&model_config.vendor)?;
    
    println!("   Registry provider base_url: '{}'", registry_provider.base_url());
    
    // Compare with direct provider
    println!("\n🔧 Checking Direct Provider:");
    let api_key = config.get_api_key("anthropic")?;
    let direct_provider = AnthropicProvider::new(&api_key, None)?;
    
    println!("   Direct provider base_url: '{}'", direct_provider.base_url());
    
    // Test with the same base_url as registry
    println!("\n🔧 Testing Direct Provider with Registry Base URL:");
    
    // Extract the base URL without the /v1 suffix that might be added
    let registry_base_url = registry_provider.base_url();
    let corrected_base_url = if registry_base_url.ends_with("/v1") {
        &registry_base_url[..registry_base_url.len() - 3]
    } else {
        registry_base_url
    };
    
    println!("   Trying with base_url: '{}'", corrected_base_url);
    
    match AnthropicProvider::new(&api_key, Some(corrected_base_url.to_string())) {
        Ok(test_provider) => {
            println!("   ✅ Provider created with registry base_url");
            println!("   Test provider base_url: '{}'", test_provider.base_url());
        }
        Err(e) => {
            println!("   ❌ Failed to create provider: {}", e);
        }
    }

    println!("\n🎯 Analysis:");
    println!("- If base URLs differ, that's likely the issue");
    println!("- The registry might be using a misconfigured base URL");
    println!("- Compare registry vs direct provider URLs");

    Ok(())
}

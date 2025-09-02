//! Debug Registry Model Lookup
//! 
//! This tests the registry model lookup that the fluent API uses

use agentflow_llm::registry::ModelRegistry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Debug Registry Model Lookup");
    println!("===============================\n");

    // Initialize AgentFlow (this loads the registry)
    agentflow_llm::AgentFlow::init().await?;
    
    let registry = ModelRegistry::global();
    
    // Test 1: List all models in registry
    println!("📋 All models in registry:");
    let models = registry.list_models();
    println!("   Total models: {}", models.len());
    
    for model in &models {
        if model.contains("claude") {
            println!("   ✅ Claude model: {}", model);
        }
    }
    
    // Test 2: Try to get the specific model that's failing
    println!("\n🔍 Testing model lookup for 'claude-3-haiku-20240307':");
    
    match registry.get_model("claude-3-haiku-20240307") {
        Ok(model_config) => {
            println!("✅ Model found!");
            println!("   Vendor: {}", model_config.vendor);
            println!("   Type: {:?}", model_config.model_type());
            if let Some(model_id) = &model_config.model_id {
                println!("   Model ID override: {}", model_id);
            }
            
            // Test 3: Try to get the provider for this vendor
            println!("\n🔍 Testing provider lookup for vendor '{}':", model_config.vendor);
            
            match registry.get_provider(&model_config.vendor) {
                Ok(_provider) => {
                    println!("✅ Provider found!");
                }
                Err(e) => {
                    println!("❌ Provider not found: {}", e);
                }
            }
        }
        Err(e) => {
            println!("❌ Model not found: {}", e);
            
            // Let's see if there are any similar models
            println!("\n🔍 Looking for similar Claude models:");
            for model in &models {
                if model.to_lowercase().contains("haiku") {
                    println!("   📋 Found Haiku model: {}", model);
                }
            }
        }
    }
    
    // Test 4: Check if the registry has providers loaded
    println!("\n📋 All providers in registry:");
    let providers = registry.list_providers();
    println!("   Total providers: {}", providers.len());
    
    for provider in &providers {
        println!("   ✅ Provider: {}", provider);
    }

    Ok(())
}

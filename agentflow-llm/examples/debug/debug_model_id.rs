//! Debug Model ID Field
//! 
//! This checks the model_id field that might be causing the issue

use agentflow_llm::registry::ModelRegistry;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Debug Model ID Field");
    println!("========================\n");

    // Initialize AgentFlow (this loads the registry)
    agentflow_llm::AgentFlow::init().await?;
    
    let registry = ModelRegistry::global();
    
    // Check the claude-3-haiku-20240307 model specifically
    match registry.get_model("claude-3-haiku-20240307") {
        Ok(model_config) => {
            println!("✅ Model found: claude-3-haiku-20240307");
            println!("   Vendor: {}", model_config.vendor);
            println!("   Type: {:?}", model_config.model_type());
            
            // This is the key field - what does model_id contain?
            match &model_config.model_id {
                Some(model_id) => {
                    println!("   ⚠️  model_id OVERRIDE: '{}'", model_id);
                    println!("   🔍 This means the provider will get '{}' instead of 'claude-3-haiku-20240307'", model_id);
                }
                None => {
                    println!("   ✅ model_id: None (will use original name)");
                }
            }
            
            // Let's also check a few other Claude models
            println!("\n🔍 Checking other Claude models:");
            
            let test_models = vec![
                "claude-3-5-sonnet-20241022",
                "claude-sonnet-4-20250514", 
                "claude-3-haiku",
                "claude-3-5-sonnet"
            ];
            
            for model_name in test_models {
                if let Ok(config) = registry.get_model(model_name) {
                    match &config.model_id {
                        Some(id) => println!("   {} -> model_id: '{}'", model_name, id),
                        None => println!("   {} -> model_id: None", model_name),
                    }
                }
            }
        }
        Err(e) => {
            println!("❌ Model not found: {}", e);
        }
    }

    Ok(())
}

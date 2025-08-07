//! Update Models Configuration Example
//! 
//! This example demonstrates how to update the default_models.yml file
//! with models fetched from vendor APIs.

use agentflow_llm::ConfigUpdater;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  // Initialize logging
  env_logger::init();

  println!("🔄 Updating models configuration...");
  
  // Check for API keys
  let api_keys = vec![
    ("MOONSHOT_API_KEY", "MoonShot"),
    ("DASHSCOPE_API_KEY", "DashScope"),
    ("ANTHROPIC_API_KEY", "Anthropic"),
    ("GEMINI_API_KEY", "Google Gemini"),
  ];
  
  let mut available_keys = Vec::new();
  for (env_var, vendor) in &api_keys {
    if env::var(env_var).is_ok() {
      available_keys.push(*vendor);
    }
  }
  
  if available_keys.is_empty() {
    println!("⚠️  No API keys found. Models will be fetched only from vendors with available API keys.");
    println!("   Set environment variables to fetch more models:");
    for (env_var, vendor) in &api_keys {
      println!("   export {}=your_api_key_here  # for {}", env_var, vendor);
    }
  } else {
    println!("✅ Found API keys for: {}", available_keys.join(", "));
  }
  
  // Update the configuration
  let updater = ConfigUpdater::new()?;
  let config_path = "templates/default_models.yml";
  
  println!("\n📥 Fetching models from all supported vendors...");
  
  match updater.update_default_models(config_path).await {
    Ok(result) => {
      println!("✅ Configuration updated successfully!");
      println!("\n{}", result.create_report());
      
      println!("📝 Updated configuration file: {}", config_path);
      println!("🎉 You can now use the newly discovered models in your AgentFlow applications!");
    }
    Err(e) => {
      eprintln!("❌ Failed to update configuration: {}", e);
      std::process::exit(1);
    }
  }
  
  Ok(())
}
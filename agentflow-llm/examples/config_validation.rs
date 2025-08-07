use agentflow_llm::config::validate_config;

#[tokio::main]
async fn main() {
  println!("=== AgentFlow LLM Configuration Validation ===\n");

  // Load environment variables
  dotenvy::from_filename(".env").ok();
  dotenvy::from_filename("examples/.env").ok();

  // Try to validate the main config first, fall back to demo
  let config_file = if std::path::Path::new("examples/models.yml").exists() && 
                       std::env::var("OPENAI_API_KEY").is_ok() &&
                       std::env::var("ANTHROPIC_API_KEY").is_ok() &&
                       std::env::var("GOOGLE_API_KEY").is_ok() {
    println!("Using production configuration (all API keys present)...");
    "examples/models.yml"
  } else {
    println!("Using demo configuration (no real API keys required)...");
    dotenvy::from_filename("examples/demo.env").ok();
    "examples/models-demo.yml"
  };

  // Validate the configuration
  match validate_config(config_file).await {
    Ok(report) => {
      println!("✅ Configuration validation completed successfully!\n");
      println!("{}", report.summary());
    }
    Err(e) => {
      println!("❌ Configuration validation failed:\n");
      println!("{}", e);
      std::process::exit(1);
    }
  }

  // Also demonstrate registry validation
  println!("\n=== Registry Provider Validation ===");

  use agentflow_llm::{AgentFlow, registry::ModelRegistry};

  match AgentFlow::init_with_config(config_file).await {
    Ok(()) => {
      let registry = ModelRegistry::global();
      
      println!("Available models:");
      for model in registry.list_models() {
        println!("  - {}", model);
      }

      println!("\nValidating providers...");
      match registry.validate_all_providers().await {
        Ok(validation_report) => {
          println!("{}", validation_report.summary());
          
          if !validation_report.is_all_valid() {
            println!("⚠️ Some providers failed validation. Check your API keys and network connectivity.");
            std::process::exit(1);
          } else {
            println!("🎉 All providers are working correctly!");
          }
        }
        Err(e) => {
          println!("❌ Provider validation failed: {}", e);
          std::process::exit(1);
        }
      }
    }
    Err(e) => {
      println!("❌ Failed to initialize registry: {}", e);
      std::process::exit(1);
    }
  }
}
// use agentflow_llm::{registry::ModelRegistry, AgentFlow, LLMError};
use agentflow_llm::{AgentFlow, LLMError};

#[tokio::main]
async fn main() -> Result<(), LLMError> {
  // println!("=== AgentFlow LLM Working Demo ===\n");

  // Force use of demo environment by loading demo env last
  // std::env::remove_var("OPENAI_API_KEY");
  // std::env::remove_var("ANTHROPIC_API_KEY");
  // std::env::remove_var("GOOGLE_API_KEY");

  // dotenvy::from_filename("examples/dashscope.env").ok();

  // println!("🔧 Using demo configuration with safe API keys");

  // // Initialize the LLM system with demo configuration
  // match AgentFlow::init_with_config("examples/models-demo.yml").await {
  //   Ok(()) => println!("✅ Configuration loaded successfully"),
  //   Err(e) => {
  //     println!("❌ Failed to load configuration: {}", e);
  //     return Err(e);
  //   }
  // }

  // Show available models
  // let registry = ModelRegistry::global();
  // let models = registry.list_models();
  // let providers = registry.list_providers();

  // println!("\n📋 Available models:");
  // for model in &models {
  //   if let Ok(info) = registry.get_model_info(model) {
  //     println!("  • {} ({})", model, info.vendor);
  //   }
  // }

  // println!("\n🔌 Available providers:");
  // for provider in &providers {
  //   println!("  • {}", provider);
  // }

  // println!("\n🧪 Testing API interface (demonstrating non-streaming vs streaming):");

  // Test 1: Non-streaming execution (returns String directly)
  println!("  ✅ Testing NON-STREAMING execution with .execute()...");
  AgentFlow::init().await?;
  let non_streaming_client = AgentFlow::model("moonshot-v1-8k").prompt("Who are you?");

  let response = non_streaming_client.execute().await?;
  println!("Response: {}", response);

  Ok(())
}

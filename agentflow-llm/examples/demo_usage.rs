use agentflow_llm::{AgentFlow, LLMError, registry::ModelRegistry};

#[tokio::main]
async fn main() -> Result<(), LLMError> {
  println!("=== AgentFlow LLM Demo ===\n");

  // Load demo environment variables
  dotenvy::from_filename("examples/demo.env").ok();

  // Initialize the LLM system with demo configuration
  match AgentFlow::init_with_config("examples/models-demo.yml").await {
    Ok(()) => println!("✅ Configuration loaded successfully"),
    Err(e) => {
      println!("❌ Failed to load configuration: {}", e);
      return Err(e);
    }
  }

  // Show available models
  let registry = ModelRegistry::global();
  let models = registry.list_models();
  let providers = registry.list_providers();
  
  println!("\n📋 Available models:");
  if models.is_empty() {
    println!("  (No models loaded - this might indicate a configuration issue)");
  } else {
    for model in models {
      if let Ok(info) = registry.get_model_info(&model) {
        println!("  • {} ({})", model, info.vendor);
      }
    }
  }

  println!("\n🔌 Available providers:");
  if providers.is_empty() {
    println!("  (No providers loaded - this might indicate a configuration issue)");
  } else {
    for provider in providers {
      println!("  • {}", provider);
    }
  }

  println!("\n🏗️  API Example:");
  println!("   let response = AgentFlow::model(\"gpt-4o-mini\")");
  println!("       .prompt(\"Hello, world!\")");
  println!("       .temperature(0.7)");
  println!("       .execute()");
  println!("       .await?;");

  println!("\n📡 Streaming Example:");
  println!("   let mut stream = AgentFlow::model(\"claude-3-haiku\")");
  println!("       .prompt(\"Tell me a story\")");
  println!("       .streaming(true)");
  println!("       .execute_streaming()");
  println!("       .await?;");
  println!("   ");
  println!("   while let Some(chunk) = stream.next_chunk().await? {{");
  println!("       print!(\"{{}}\", chunk.content);");
  println!("       if chunk.is_final {{ break; }}");
  println!("   }}");

  println!("\n💡 To make actual API calls:");
  println!("   1. Copy examples/.env.example to .env");
  println!("   2. Add your real API keys");
  println!("   3. Use examples/models.yml instead");
  println!("   4. Run: cargo run --example basic_usage");

  println!("\n✨ Demo completed successfully!");
  Ok(())
}
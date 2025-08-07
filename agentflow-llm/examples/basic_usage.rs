use agentflow_llm::{AgentFlow, LLMError};

#[tokio::main]
async fn main() -> Result<(), LLMError> {
  // Set up environment variables
  dotenvy::from_filename(".env").ok();
  dotenvy::from_filename("examples/.env").ok(); // Also try examples directory

  // Check if we have real API keys first
  let has_real_keys = std::env::var("OPENAI_API_KEY")
    .map(|key| !key.is_empty() && !key.starts_with("demo-key"))
    .unwrap_or(false);
  
  let config_file = if has_real_keys {
    "examples/models.yml"
  } else {
    dotenvy::from_filename("examples/demo.env").ok();
    "examples/models-demo.yml"
  };

  // Initialize the LLM system with configuration
  match AgentFlow::init_with_config(config_file).await {
    Ok(()) => println!("✅ Configuration loaded successfully"),
    Err(e) => {
      println!("❌ Failed to load configuration: {}", e);
      println!("💡 Make sure to:");
      println!("  1. Copy examples/.env.example to .env");
      println!("  2. Add your actual API keys");
      println!("  3. Ensure examples/models.yml exists");
      return Err(e);
    }
  }

  // Basic non-streaming usage
  println!("=== Basic Usage ===");
  
  if !has_real_keys {
    println!("⚠️  Using demo configuration - API calls will fail with real requests");
    println!("💡 To test real API calls:");
    println!("   1. Copy examples/.env.example to .env");
    println!("   2. Add your real API keys");
    println!("   3. Re-run this example");
    println!("\n✨ Demo completed successfully - configuration system is working!");
    return Ok(());
  }
  
  let response = AgentFlow::model("gpt-4o-mini")
    .prompt("What is the capital of France?")
    .temperature(0.7)
    .max_tokens(100)
    .execute()
    .await?;

  println!("Response: {}", response);

  // Streaming usage
  println!("\n=== Streaming Usage ===");
  let mut stream = AgentFlow::model("gpt-4o-mini")
    .prompt("Tell me a short story about a robot.")
    .temperature(0.8)
    .execute_streaming()
    .await?;

  print!("Streaming response: ");
  while let Some(chunk) = stream.next_chunk().await? {
    print!("{}", chunk.content);
    if chunk.is_final {
      println!("\n[Stream complete]");
      break;
    }
  }

  // Different models
  println!("\n=== Multiple Models ===");
  let models = ["gpt-4o-mini", "claude-3-haiku"];
  
  for model in &models {
    match AgentFlow::model(model)
      .prompt("Say hello in one word")
      .execute()
      .await
    {
      Ok(response) => println!("{}: {}", model, response.trim()),
      Err(e) => println!("{}: Error - {}", model, e),
    }
  }

  Ok(())
}
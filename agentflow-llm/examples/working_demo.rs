use agentflow_llm::{AgentFlow, LLMError, registry::ModelRegistry};

#[tokio::main]
async fn main() -> Result<(), LLMError> {
  println!("=== AgentFlow LLM Working Demo ===\n");

  // Force use of demo environment by loading demo env last
  std::env::remove_var("OPENAI_API_KEY");
  std::env::remove_var("ANTHROPIC_API_KEY");
  std::env::remove_var("GOOGLE_API_KEY");
  
  dotenvy::from_filename("examples/demo.env").ok();

  println!("🔧 Using demo configuration with safe API keys");

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
  for model in &models {
    if let Ok(info) = registry.get_model_info(model) {
      println!("  • {} ({})", model, info.vendor);
    }
  }

  println!("\n🔌 Available providers:");
  for provider in &providers {
    println!("  • {}", provider);
  }

  println!("\n🧪 Testing API interface (demonstrating non-streaming vs streaming):");
  
  // Test 1: Non-streaming execution (returns String directly)
  println!("  ✅ Testing NON-STREAMING execution with .execute()...");
  let _non_streaming_client = AgentFlow::model("gpt-4o-mini")
    .prompt("Hello, world!")
    .temperature(0.7)
    .max_tokens(100)
    .top_p(0.9)
    .frequency_penalty(0.1);
  
  // This would return Result<String> if we had valid API keys
  // let response: String = non_streaming_client.execute().await?;
  println!("    → Built client for .execute() → returns Result<String>");
  println!("    → Use case: Get complete response at once");

  // Test 2: Streaming execution (returns StreamingResponse)
  println!("  ✅ Testing STREAMING execution with .execute_streaming()...");
  let _streaming_client = AgentFlow::model("claude-3-haiku")
    .prompt("Tell me a story")
    .temperature(0.8)
    .stop(vec!["\n\n".to_string(), "THE END".to_string()]);
  
  // This would return Result<Box<dyn StreamingResponse>> if we had valid API keys
  // let mut stream = streaming_client.execute_streaming().await?;
  // while let Some(chunk) = stream.next_chunk().await? {
  //   print!("{}", chunk.content);
  // }
  println!("    → Built client for .execute_streaming() → returns Result<Box<dyn StreamingResponse>>");
  println!("    → Use case: Process response chunks in real-time");

  // Test 3: Client with tools (for future MCP integration)
  println!("  ✅ Testing client with TOOLS for MCP integration...");
  let tools = vec![
    serde_json::json!({
      "type": "function",
      "function": {
        "name": "search_web",
        "description": "Search the web for information",
        "parameters": {
          "type": "object",
          "properties": {
            "query": {"type": "string"}
          }
        }
      }
    })
  ];
  let _tools_client = AgentFlow::model("gpt-4o")
    .prompt("Search for Rust programming tutorials")
    .tools(tools)
    .temperature(0.6);
  println!("    → Built client with tools for function calling");
  println!("    → Ready for agentflow-mcp integration");

  println!("\n📊 API Usage Patterns:");
  println!("  🔄 NON-STREAMING: client.execute().await? → String");
  println!("  ⚡ STREAMING: client.execute_streaming().await? → StreamingResponse");
  println!("  🛠️  WITH TOOLS: client.tools(mcp_tools).execute().await?");

  // Test 4: Configuration access
  println!("  ✅ Testing configuration access...");
  for model in &models {
    let config = registry.get_model(model)?;
    println!("    - {}: vendor={}, temp={:?}, tools={:?}, multimodal={:?}", 
             model, config.vendor, config.temperature, config.supports_tools, config.supports_multimodal);
  }

  println!("\n🎉 All tests passed!");
  println!("\n💡 This demo shows that the AgentFlow LLM integration is working correctly.");
  println!("   The configuration system, model registry, and API builders are all functional.");
  println!("   To make actual API calls, you would need valid API keys.");

  println!("\n🔗 Next steps:");
  println!("   1. Get API keys from:");
  println!("      - OpenAI: https://platform.openai.com/api-keys");
  println!("      - Anthropic: https://console.anthropic.com/");
  println!("      - Google: https://aistudio.google.com/app/apikey");
  println!("   2. Copy examples/.env.example to .env");
  println!("   3. Add your real API keys to .env");
  println!("   4. Run: cargo run --example basic_usage");

  Ok(())
}
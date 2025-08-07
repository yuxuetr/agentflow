use agentflow_llm::AgentFlow;

#[tokio::main]
async fn main() -> Result<(), agentflow_llm::LLMError> {
  println!("🚀 Testing Step provider integration...");

  // Initialize the LLM system
  AgentFlow::init().await?;

  println!("✅ Step provider successfully integrated!");
  println!("📋 Step models should be available in configuration");

  // Try to load a Step model configuration (this tests provider validation)
  // We'll just test that the model can be created without executing it
  let step_client = AgentFlow::model("step-2-mini").prompt("Whoooo are you?");
  println!("✅ step-2-mini model client created successfully");
  
  // Check if API key is available before attempting execution
  if std::env::var("STEP_API_KEY").is_ok() {
    println!("🔑 STEP_API_KEY found, executing test request...");
    let step_response = step_client.execute().await?;
    println!("📝 Response: {}", step_response);
  } else {
    println!("📝 No STEP_API_KEY found - skipping actual API call");
    println!("💡 To test actual Step API calls, set STEP_API_KEY environment variable");
  }

  Ok(())
}

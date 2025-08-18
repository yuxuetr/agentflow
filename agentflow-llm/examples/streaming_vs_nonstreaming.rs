use agentflow_llm::{AgentFlow, LLMError};

#[tokio::main]
async fn main() -> Result<(), LLMError> {
  println!("=== Streaming vs Non-Streaming Examples ===\n");

  // Load demo configuration (with placeholder API keys)
  dotenvy::from_filename("examples/demo.env").ok();
  AgentFlow::init_with_config("examples/models-demo.yml").await?;

  println!("🔧 Note: This demo shows API patterns with placeholder keys\n");

  // Example 1: Non-Streaming Request
  println!("📄 NON-STREAMING Example:");
  println!("   Purpose: Get complete response at once");
  println!("   Returns: Result<String>");
  println!("   Code pattern:");
  println!();
  println!("   let response = AgentFlow::model(\"gpt-4o\")");
  println!("     .prompt(\"Explain Rust ownership\")");
  println!("     .temperature(0.7)");
  println!("     .max_tokens(200)");
  println!("     .execute().await?;  // Returns full String");
  println!("   println!(\"Complete response: {{}}\", response);");
  println!();

  // Example 2: Streaming Request
  println!("⚡ STREAMING Example:");
  println!("   Purpose: Process response chunks in real-time");
  println!("   Returns: Result<Box<dyn StreamingResponse>>");
  println!("   Code pattern:");
  println!();
  println!("   let mut stream = AgentFlow::model(\"claude-3-5-sonnet\")");
  println!("     .prompt(\"Write a short story\")");
  println!("     .temperature(0.8)");
  println!("     .execute_streaming().await?;  // Returns stream");
  println!();
  println!("   while let Some(chunk) = stream.next_chunk().await? {{");
  println!("     print!(\"{{}}\", chunk.content);  // Print each chunk");
  println!("     if chunk.is_final {{");
  println!("       println!(\"\\n[Stream completed]\");");
  println!("       break;");
  println!("     }}");
  println!("   }}");
  println!();

  // Example 3: With Tools
  println!("🛠️  WITH TOOLS Example (for MCP integration):");
  println!("   Purpose: Function calling with tools from agentflow-mcp");
  println!("   Code pattern:");
  println!();
  println!("   // Tools would come from agentflow-mcp crate");
  println!("   let mcp_tools = get_available_tools().await?;");
  println!();
  println!("   let response = AgentFlow::model(\"gpt-4o\")");
  println!("     .prompt(\"Search for weather in Tokyo\")");
  println!("     .tools(mcp_tools)  // Vec<Value> from MCP");
  println!("     .temperature(0.6)");
  println!("     .execute().await?;");
  println!();

  // Example 4: All Parameters Combined
  println!("🎛️  ALL PARAMETERS Example:");
  println!("   let response = AgentFlow::model(\"gpt-4o\")");
  println!("     .prompt(\"Write a haiku about programming\")");
  println!("     .temperature(0.8)");
  println!("     .max_tokens(150)");
  println!("     .top_p(0.9)");
  println!("     .frequency_penalty(0.1)");
  println!("     .stop(vec![\"END\".to_string()])");
  println!("     .tools(function_tools)");
  println!("     .param(\"seed\", 42)  // Custom parameter");
  println!("     .execute().await?;");
  println!();

  println!("📋 Key Differences:");
  println!("   • .execute()          → Result<String> (complete response)");
  println!("   • .execute_streaming() → Result<StreamingResponse> (chunk by chunk)");
  println!("   • No .streaming(bool) needed - method choice determines behavior");
  println!("   • YAML config provides defaults, function params override");
  println!("   • Tools ready for agentflow-mcp integration");

  Ok(())
}

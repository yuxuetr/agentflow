use agentflow_llm::{AgentFlow, LLMError};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), LLMError> {
  println!("=== AgentFlow LLM Logging & JSON Demo ===\n");

  // Initialize logging
  println!("🔧 Initializing logging system...");
  AgentFlow::init_logging().ok();
  
  // Initialize with built-in config (no API keys required for demo)
  println!("📋 Loading built-in configuration...");
  AgentFlow::init_with_builtin_config().await?;

  println!("\n✅ Setup complete! Demonstrating capabilities:\n");

  // Demo 1: Logging Features
  println!("📝 Demo 1: Comprehensive Logging");
  println!("   ✅ Request metadata (model, prompt length, temperature)");
  println!("   ✅ Response timing and statistics");  
  println!("   ✅ JSON validation when using JSON mode");
  println!("   ✅ Error details with context");
  println!("   ✅ Debug-level content logging (with RUST_LOG=debug)");
  println!();

  // Demo 2: JSON Response Formats
  println!("🔧 Demo 2: JSON Response Format Options");
  println!();

  // JSON Object Mode
  println!("   📄 JSON Object Mode:");
  println!("   AgentFlow::model(\"gpt-4o\")");
  println!("     .prompt(\"Return user data as JSON\")");
  println!("     .json_mode()  // Enforces valid JSON");
  println!("     .execute().await?;");
  println!();

  // JSON Schema Mode
  let _user_schema = json!({
    "type": "object",
    "properties": {
      "name": {"type": "string"},
      "age": {"type": "number", "minimum": 0},
      "email": {"type": "string", "format": "email"}
    },
    "required": ["name", "age", "email"]
  });

  println!("   📋 JSON Schema Mode:");
  println!("   AgentFlow::model(\"gpt-4o\")");
  println!("     .prompt(\"Generate a user profile\")");
  println!("     .json_schema(\"user_profile\", schema)  // Enforces structure");
  println!("     .execute().await?;");
  println!();

  // Demo 3: Logging Configuration
  println!("⚙️  Demo 3: Logging Configuration");
  println!();
  println!("   Environment Variables:");
  println!("   • RUST_LOG=debug           - Full request/response content");
  println!("   • RUST_LOG=info            - Request summaries and timing");
  println!("   • RUST_LOG=warn            - Warnings and validation issues");
  println!("   • RUST_LOG=error           - Only errors and failures");
  println!();
  println!("   Per-Request Control:");
  println!("   • .enable_logging(true)    - Enable for this request");
  println!("   • .enable_logging(false)   - Disable for this request");
  println!();

  // Demo 4: Structured Output Examples
  println!("📊 Demo 4: Structured Output Use Cases");
  println!();
  println!("   🔄 API Integration:");
  println!("     - Parse LLM responses into typed structures");
  println!("     - Validate response format automatically");
  println!("     - Handle structured data reliably");
  println!();
  println!("   🧪 Testing & Debugging:");
  println!("     - Log full request/response chains");
  println!("     - Trace performance bottlenecks");
  println!("     - Validate model behavior");
  println!();
  println!("   ⚡ Production Monitoring:");
  println!("     - Track request patterns and timing");
  println!("     - Monitor API usage and costs");
  println!("     - Alert on validation failures");
  println!();

  // Demo 5: Real-world Example Structure
  println!("🌍 Demo 5: Real-world Example");
  println!();
  println!("   ```rust");
  println!("   // Initialize with logging");
  println!("   AgentFlow::init_logging()?;");
  println!("   AgentFlow::init_with_env().await?;");
  println!();
  println!("   // Structured data extraction");
  println!("   let analysis = AgentFlow::model(\"gpt-4o\")");
  println!("     .prompt(\"Analyze this customer feedback...\")");
  println!("     .json_schema(\"feedback_analysis\", analysis_schema)");
  println!("     .temperature(0.3)  // Lower temp for structured output");
  println!("     .enable_logging(true)");
  println!("     .execute().await?;");
  println!(); 
  println!("   // Parse the JSON response");
  println!("   let parsed: FeedbackAnalysis = serde_json::from_str(&analysis)?;");
  println!("   ```");
  println!();

  // Summary
  println!("📋 Summary of Capabilities:");
  println!("   ✅ Comprehensive request/response logging");
  println!("   ✅ JSON mode enforcement and validation");
  println!("   ✅ JSON Schema-based structured output");
  println!("   ✅ Configurable logging levels (RUST_LOG)");
  println!("   ✅ Per-request logging control");
  println!("   ✅ Error context and debugging info");
  println!("   ✅ Performance timing and metrics");
  println!("   ✅ API key security (masking in logs)");

  println!("\n🚀 Ready for production use with comprehensive observability!");

  Ok(())
}
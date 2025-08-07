use agentflow_llm::{AgentFlow, ResponseFormat, LLMError};
use serde_json::{json, Value};

#[tokio::main]
async fn main() -> Result<(), LLMError> {
  println!("=== AgentFlow LLM Logging and JSON Output Demo ===\n");

  // Initialize logging (if feature is enabled)
  AgentFlow::init_logging().ok();
  
  // Load configuration and environment
  AgentFlow::init_with_env().await?;

  println!("🧪 Demonstrating logging and structured output capabilities\n");

  // Demo 1: Basic logging
  println!("📝 Demo 1: Request Logging");
  println!("   - Shows request/response metadata");
  println!("   - Includes timing information");
  println!("   - Validates response format");
  println!();

  let _basic_response = AgentFlow::model("gpt-4o-mini")
    .prompt("Explain what JSON is in one sentence.")
    .temperature(0.7)
    .enable_logging(true)
    .execute().await;

  match _basic_response {
    Ok(response) => println!("✅ Basic request completed: {} chars\\n", response.len()),
    Err(e) => println!("❌ Basic request failed: {}\\n", e),
  }

  // Demo 2: JSON Mode
  println!("🔧 Demo 2: JSON Object Mode");
  println!("   - Forces model to return valid JSON");
  println!("   - Automatically validates JSON structure");
  println!("   - Useful for API integrations");
  println!();

  let json_prompt = r#"Return information about Rust programming language with the following structure:
{
  "name": "programming language name",
  "year_created": year_as_number,
  "paradigms": ["list", "of", "paradigms"],
  "popular_use_cases": ["list", "of", "use", "cases"],
  "difficulty_level": "beginner/intermediate/advanced"
}"#;

  let _json_response = AgentFlow::model("gpt-4o")
    .prompt(json_prompt)
    .json_mode()
    .temperature(0.3)
    .enable_logging(true)
    .execute().await;

  match _json_response {
    Ok(response) => {
      println!("✅ JSON response received:");
      
      // Parse and pretty-print the JSON
      match serde_json::from_str::<Value>(&response) {
        Ok(parsed) => {
          println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
          println!();
        }
        Err(e) => println!("⚠️  Failed to parse as JSON: {}", e),
      }
    }
    Err(e) => println!("❌ JSON request failed: {}", e),
  }

  // Demo 3: JSON Schema Mode
  println!("📋 Demo 3: JSON Schema Mode");
  println!("   - Enforces specific JSON structure");
  println!("   - Validates against provided schema");
  println!("   - Ensures consistent output format");
  println!();

  let user_schema = json!({
    "type": "object",
    "properties": {
      "name": {"type": "string"},
      "age": {"type": "number", "minimum": 0, "maximum": 150},
      "email": {"type": "string", "format": "email"},
      "skills": {
        "type": "array",
        "items": {"type": "string"}
      },
      "experience_years": {"type": "number", "minimum": 0}
    },
    "required": ["name", "age", "email", "skills"],
    "additionalProperties": false
  });

  let _schema_response = AgentFlow::model("gpt-4o")
    .prompt("Generate a realistic software developer profile")
    .response_format(ResponseFormat::JsonSchema {
      name: "developer_profile".to_string(),
      schema: user_schema,
      strict: Some(true),
    })
    .temperature(0.5)
    .enable_logging(true)
    .execute().await;

  match _schema_response {
    Ok(response) => {
      println!("✅ Schema-validated response received:");
      
      match serde_json::from_str::<Value>(&response) {
        Ok(parsed) => {
          println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
          println!();
        }
        Err(e) => println!("⚠️  Failed to parse as JSON: {}", e),
      }
    }
    Err(e) => println!("❌ Schema request failed: {}", e),
  }

  // Demo 4: Streaming with Logging
  println!("📡 Demo 4: Streaming with Logging");
  println!("   - Shows real-time chunk processing");
  println!("   - Logs streaming events");
  println!("   - Demonstrates response building");
  println!();

  let _streaming_result = AgentFlow::model("claude-3-haiku")
    .prompt("Write a short JSON object describing a cat, stream the response")
    .json_mode()
    .enable_logging(true)
    .execute_streaming().await;

  match _streaming_result {
    Ok(mut stream) => {
      println!("📡 Streaming response:");
      let mut full_response = String::new();
      
      while let Ok(Some(chunk)) = stream.next_chunk().await {
        print!("{}", chunk.content);
        full_response.push_str(&chunk.content);
        
        if chunk.is_final {
          println!("\\n\\n✅ Stream completed");
          
          // Validate the complete JSON
          if let Ok(parsed) = serde_json::from_str::<Value>(&full_response) {
            println!("🔍 Complete JSON validation: ✅");
            println!("📄 Formatted output:");
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
          } else {
            println!("⚠️  Complete response is not valid JSON");
          }
          break;
        }
      }
      println!();
    }
    Err(e) => println!("❌ Streaming failed: {}", e),
  }

  // Demo 5: Error Handling and Logging
  println!("🚨 Demo 5: Error Handling");
  println!("   - Demonstrates comprehensive error logging");
  println!("   - Shows different error types");
  println!();

  let _error_response = AgentFlow::model("nonexistent-model")
    .prompt("This should fail")
    .enable_logging(true)
    .execute().await;

  match _error_response {
    Ok(_) => println!("❌ Expected error but got success"),
    Err(e) => println!("✅ Error properly logged: {}", e),
  }

  println!("\\n📊 Summary:");
  println!("   ✅ Request/response logging with timing");
  println!("   ✅ JSON mode for structured output");
  println!("   ✅ JSON schema validation");
  println!("   ✅ Streaming with real-time logging");
  println!("   ✅ Comprehensive error handling");
  
  println!("\\n💡 Logging Configuration:");
  println!("   - Set RUST_LOG=debug for detailed logs");
  println!("   - Set RUST_LOG=agentflow_llm=info for request summaries");
  println!("   - Use .enable_logging(false) to disable per-request");

  Ok(())
}
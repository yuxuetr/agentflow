//! Model Compatibility Example
//! 
//! This example demonstrates how different models handle different response formats
//! and provides compatibility patterns for various LLM providers.

use agentflow_core::{AsyncNode, SharedState};
use agentflow_nodes::{LlmNode, nodes::llm::ResponseFormat};
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("🚀 Model Compatibility Example");
  println!("===============================\n");

  let shared = SharedState::new();
  shared.insert("user_input".to_string(), 
    Value::String("I love this product! Great features and excellent performance.".to_string()));

  // 1. Text Response (Compatible with ALL models)
  println!("📝 Example 1: Text Response (Universal Compatibility)");
  
  let text_node = LlmNode::new("text_analyzer", "qwen-plus")
    .with_prompt("Analyze the sentiment of: {{user_input}}")
    .with_system("You are a sentiment analysis expert. Provide clear analysis.")
    .with_temperature(0.3)
    .with_max_tokens(200)
    .with_response_format(ResponseFormat::Text); // Always supported

  match text_node.run_async(&shared).await {
    Ok(_) => {
      if let Some(result) = shared.get("text_analyzer_output") {
        println!("✅ Text Analysis:");
        println!("{}\n", result.as_str().unwrap_or("No response"));
      }
    }
    Err(e) => println!("❌ Text analysis failed: {}\n", e),
  }

  // 2. Loose JSON for models that support json_object but not strict schema
  println!("📊 Example 2: Loose JSON (qwen-plus, step-2-mini compatible)");
  
  let loose_json_node = LlmNode::new("loose_json_analyzer", "qwen-plus")
    .with_prompt("Analyze the sentiment of: {{user_input}}\n\nPlease respond in JSON format with fields: sentiment, confidence, summary")
    .with_system("You are a sentiment analysis expert. Always respond in valid JSON format.")
    .with_temperature(0.2)
    .with_max_tokens(300)
    .with_response_format(ResponseFormat::loose_json()); // No strict schema

  match loose_json_node.run_async(&shared).await {
    Ok(_) => {
      if let Some(result) = shared.get("loose_json_analyzer_output") {
        println!("✅ Loose JSON Analysis:");
        if let Ok(parsed) = serde_json::from_str::<Value>(result.as_str().unwrap_or("{}")) {
          println!("{}\n", serde_json::to_string_pretty(&parsed)?);
        } else {
          println!("{}\n", result.as_str().unwrap_or("Invalid JSON"));
        }
      }
    }
    Err(e) => println!("❌ Loose JSON analysis failed: {}\n", e),
  }

  // 3. Model-specific optimized approaches
  println!("🎯 Example 3: Model-Specific Optimizations");

  // For step-2-mini: Simple text with structured prompt
  let stepfun_node = LlmNode::new("stepfun_analysis", "step-2-mini")
    .with_prompt(r#"
Analyze this feedback: {{user_input}}

Please structure your response as:
Sentiment: [positive/negative/neutral]
Confidence: [0-100]%
Summary: [brief summary]
Key Points: [bullet points]
"#)
    .with_system("You are a helpful analysis assistant. Follow the format exactly.")
    .with_temperature(0.3)
    .with_max_tokens(250);

  match stepfun_node.run_async(&shared).await {
    Ok(_) => {
      if let Some(result) = shared.get("stepfun_analysis_output") {
        println!("✅ StepFun Model Analysis:");
        println!("{}\n", result.as_str().unwrap_or("No response"));
      }
    }
    Err(e) => println!("❌ StepFun analysis failed: {}\n", e),
  }

  // For qwen-plus: Enhanced with explicit JSON mention
  let qwen_node = LlmNode::new("qwen_analysis", "qwen-plus")
    .with_prompt("Analyze sentiment of: {{user_input}}\n\nProvide analysis in JSON format.")
    .with_system("You are an expert analyst. Always respond with valid JSON containing sentiment, confidence, and insights.")
    .with_temperature(0.2)
    .with_max_tokens(400)
    .with_response_format(ResponseFormat::loose_json());

  match qwen_node.run_async(&shared).await {
    Ok(_) => {
      if let Some(result) = shared.get("qwen_analysis_output") {
        println!("✅ Qwen Model JSON Analysis:");
        if let Ok(parsed) = serde_json::from_str::<Value>(result.as_str().unwrap_or("{}")) {
          println!("{}\n", serde_json::to_string_pretty(&parsed)?);
        } else {
          println!("{}\n", result.as_str().unwrap_or("Invalid JSON"));
        }
      }
    }
    Err(e) => println!("❌ Qwen analysis failed: {}\n", e),
  }

  // 4. Creative tasks (Markdown works well with most models)
  println!("✨ Example 4: Creative Content (Markdown Format)");
  
  let creative_node = LlmNode::new("creative_writer", "qwen-plus")
    .with_prompt("Write a short product review based on: {{user_input}}")
    .with_system("You are a creative writer. Write engaging, well-formatted content.")
    .with_temperature(0.7)
    .with_max_tokens(300)
    .with_response_format(ResponseFormat::Markdown);

  match creative_node.run_async(&shared).await {
    Ok(_) => {
      if let Some(result) = shared.get("creative_writer_output") {
        println!("✅ Creative Content (Markdown):");
        println!("{}\n", result.as_str().unwrap_or("No response"));
      }
    }
    Err(e) => println!("❌ Creative writing failed: {}\n", e),
  }

  // 5. Error-resistant approach with fallbacks
  println!("🛡️  Example 5: Error-Resistant Multi-Model Approach");
  
  let models_to_try = vec!["qwen-plus", "step-2-mini"];
  let mut success = false;

  for model in models_to_try {
    println!("   Trying model: {}", model);
    
    let fallback_node = LlmNode::new("fallback_analyzer", model)
      .with_prompt("Analyze: {{user_input}}")
      .with_system("Provide helpful analysis.")
      .with_temperature(0.4)
      .with_max_tokens(150)
      .with_response_format(ResponseFormat::Text); // Most compatible format

    match fallback_node.run_async(&shared).await {
      Ok(_) => {
        if let Some(result) = shared.get("fallback_analyzer_output") {
          println!("   ✅ Success with {}: {}", model, 
            &result.as_str().unwrap_or("No response")[..50.min(result.as_str().unwrap_or("").len())]);
          success = true;
          break;
        }
      }
      Err(e) => {
        println!("   ❌ {} failed: {}", model, e);
        continue;
      }
    }
  }

  if !success {
    println!("   ⚠️  All models failed, but mock mode should work");
  }

  println!("\n🏁 Model compatibility example completed!");
  
  println!("\n💡 Key Compatibility Insights:");
  println!("   📝 Text format: Supported by ALL models");
  println!("   📊 Loose JSON: Works with qwen-plus, step-2-mini (with prompt hints)");
  println!("   📋 Strict JSON Schema: Limited model support");
  println!("   ✨ Markdown: Good general support");
  println!("   🔧 Model-specific prompting improves success rates");
  println!("   🛡️  Always have text-based fallbacks");

  println!("\n🎯 Recommendations:");
  println!("   1. Start with text format for maximum compatibility");
  println!("   2. Use loose JSON when you need structure"); 
  println!("   3. Include format hints in prompts (e.g., 'respond in JSON')");
  println!("   4. Test with multiple models for critical workflows");
  println!("   5. Implement graceful fallbacks");

  Ok(())
}
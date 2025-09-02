//! Fixed Enhanced LLM Example
//! 
//! This is an improved version that works better with qwen-plus and step-2-mini models
//! by using compatible response formats and proper prompt structuring.

use agentflow_core::{AsyncNode, SharedState};
use agentflow_nodes::{LlmNode, nodes::llm::ResponseFormat};
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("🚀 Fixed Enhanced LLM Node Example");
  println!("===================================\n");

  let shared = SharedState::new();

  // 1. Text Analysis with Compatible JSON (Fixed)
  println!("📊 Example 1: Text Analysis with Compatible JSON Output");
  shared.insert("sample_text".to_string(), 
    Value::String("I love the new features in this product! The interface is intuitive and the performance is excellent.".to_string()));

  let sentiment_node = LlmNode::new("sentiment_analyzer", "qwen-plus")
    .with_prompt("Analyze the sentiment and key themes of: {{sample_text}}\n\nRespond in JSON format with: summary, key_points, sentiment, confidence")
    .with_system("You are a sentiment analysis expert. Always respond in valid JSON format.")
    .with_temperature(0.3)
    .with_max_tokens(400)
    .with_response_format(ResponseFormat::loose_json()); // Use loose JSON instead of strict schema

  match sentiment_node.run_async(&shared).await {
    Ok(_) => {
      if let Some(result) = shared.get("sentiment_analyzer_output") {
        println!("✅ Analysis complete");
        println!("📋 Structured Analysis Result:");
        if let Ok(parsed) = serde_json::from_str::<Value>(result.as_str().unwrap_or("{}")) {
          println!("{}\n", serde_json::to_string_pretty(&parsed)?);
        } else {
          println!("{}\n", result.as_str().unwrap_or("Could not parse JSON"));
        }
      }
    }
    Err(e) => println!("❌ Sentiment analysis failed: {}\n", e),
  }

  // 2. Creative Writing with Markdown (This should work well)
  println!("✍️  Example 2: Creative Writing with Markdown Formatting");
  
  let story_node = LlmNode::new("story_writer", "qwen-plus")
    .with_prompt("Write a short story about artificial intelligence in science fiction style")
    .with_system("You are a creative writer specializing in engaging narratives")
    .with_temperature(0.8)
    .with_max_tokens(600)
    .with_response_format(ResponseFormat::Markdown);

  match story_node.run_async(&shared).await {
    Ok(_) => {
      if let Some(result) = shared.get("story_writer_output") {
        println!("✅ Story generation complete");
        println!("📖 Generated Story (Markdown):");
        let story_preview = result.as_str().unwrap_or("No story generated");
        if story_preview.len() > 500 {
          println!("{}...\n[Story truncated for display]\n", &story_preview[..500]);
        } else {
          println!("{}\n", story_preview);
        }
      }
    }
    Err(e) => println!("❌ Story generation failed: {}\n", e),
  }

  // 3. Code Generation (Should work well with qwen-plus)
  println!("💻 Example 3: Code Generation with Language-Specific Output");
  
  let rust_node = LlmNode::new("rust_coder", "qwen-plus")
    .with_prompt("Implement a binary search function that finds an element in a sorted vector and returns its index")
    .with_system("You are an expert Rust programmer. Write clean, idiomatic Rust code with proper error handling.")
    .with_temperature(0.2)
    .with_max_tokens(800)
    .with_response_format(ResponseFormat::Code { language: "rust".to_string() });

  match rust_node.run_async(&shared).await {
    Ok(_) => {
      if let Some(result) = shared.get("rust_coder_output") {
        println!("✅ Code generation complete");
        println!("🦀 Generated Rust Code:");
        let code_preview = result.as_str().unwrap_or("No code generated");
        if code_preview.len() > 800 {
          println!("{}...\n[Code truncated for display]\n", &code_preview[..800]);
        } else {
          println!("{}\n", code_preview);
        }
      }
    }
    Err(e) => println!("❌ Code generation failed: {}\n", e),
  }

  // 4. Simple Analysis with step-2-mini (Text format for maximum compatibility)
  println!("🔧 Example 4: Simple Analysis with step-2-mini (Text Format)");
  
  shared.insert("business_data".to_string(),
    Value::String("Q1 sales data: Revenue $2.1M (+15%), Customers 1,847 (+8%), Churn 3.2% (-1.1%)".to_string()));

  let simple_node = LlmNode::new("simple_analyzer", "step-2-mini")
    .with_prompt("Analyze this business data and provide insights: {{business_data}}")
    .with_system("You are a business analyst. Provide clear, actionable insights.")
    .with_temperature(0.4)
    .with_max_tokens(300)
    .with_response_format(ResponseFormat::Text); // Most compatible format

  match simple_node.run_async(&shared).await {
    Ok(_) => {
      if let Some(result) = shared.get("simple_analyzer_output") {
        println!("✅ Business analysis complete");
        println!("📈 Analysis Results:");
        println!("{}\n", result.as_str().unwrap_or("No analysis generated"));
      }
    }
    Err(e) => println!("❌ Business analysis failed: {}\n", e),
  }

  // 5. Structured prompt approach (Works with any model)
  println!("📋 Example 5: Structured Prompt Approach (Universal Compatibility)");
  
  let structured_node = LlmNode::new("structured_analyzer", "qwen-plus")
    .with_prompt(r#"
Analyze the sentiment of: {{sample_text}}

Please provide your analysis in this format:
- Sentiment: [positive/negative/neutral] 
- Confidence: [0-100]%
- Key Themes: [list main themes]
- Summary: [brief summary]
- Recommendations: [if applicable]
"#)
    .with_system("You are an expert analyst. Follow the requested format exactly.")
    .with_temperature(0.3)
    .with_max_tokens(400);

  match structured_node.run_async(&shared).await {
    Ok(_) => {
      if let Some(result) = shared.get("structured_analyzer_output") {
        println!("✅ Structured analysis complete");
        println!("📊 Formatted Analysis:");
        println!("{}\n", result.as_str().unwrap_or("No analysis generated"));
      }
    }
    Err(e) => println!("❌ Structured analysis failed: {}\n", e),
  }

  println!("🎉 Fixed Enhanced Example Complete!");
  println!("\n💡 Key Fixes Applied:");
  println!("   ✅ Used loose_json() instead of strict JSON schema");
  println!("   ✅ Added 'JSON' keyword to prompts when using JSON format");
  println!("   ✅ Used Text format for maximum model compatibility");
  println!("   ✅ Structured prompts for consistent output without strict schemas");
  println!("   ✅ Model-specific optimizations");
  
  println!("\n🏆 Results Summary:");
  let results = [
    ("Sentiment Analysis", "sentiment_analyzer_output"),
    ("Story Generation", "story_writer_output"), 
    ("Code Generation", "rust_coder_output"),
    ("Business Analysis", "simple_analyzer_output"),
    ("Structured Analysis", "structured_analyzer_output")
  ];

  for (name, key) in results {
    if shared.get(key).is_some() {
      println!("   ✅ {}: Success", name);
    } else {
      println!("   ❌ {}: Failed", name);
    }
  }

  Ok(())
}
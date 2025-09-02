//! Advanced LLM Node Example
//! 
//! This example demonstrates advanced features of agentflow-nodes LLM integration:
//! - Multiple LLM parameters (temperature, top_p, frequency_penalty, etc.)
//! - Stop sequences for controlled generation
//! - Response format configuration
//! - Chaining multiple LLM nodes
//! - Template variable resolution

use agentflow_core::{AsyncNode, SharedState};
use agentflow_nodes::LlmNode;
use agentflow_nodes::nodes::llm::ResponseFormat;
use serde_json::{json, Value};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("🚀 Advanced LLM Node Example");
  println!("============================\n");

  // Create shared state with complex data
  let shared = SharedState::new();
  shared.insert("topic".to_string(), Value::String("artificial intelligence".to_string()));
  shared.insert("audience".to_string(), Value::String("high school students".to_string()));
  shared.insert("style".to_string(), Value::String("engaging and fun".to_string()));

  // 1. Creative Writing Node with Advanced Parameters
  println!("📝 Step 1: Creative Content Generation");
  let creative_node = LlmNode::new("content_creator", "step-2-mini")
    .with_prompt("Write a {{style}} introduction to {{topic}} for {{audience}}. Make it exactly 2 paragraphs.")
    .with_system("You are an expert educator who makes complex topics accessible and interesting.")
    .with_temperature(0.8)         // High creativity
    .with_max_tokens(300)          // Moderate length
    .with_top_p(0.9)              // Nucleus sampling
    .with_frequency_penalty(0.3)   // Reduce repetition
    .with_presence_penalty(0.2)    // Encourage topic diversity
    .with_stop_sequences(vec!["---".to_string(), "###".to_string()]) // Stop at these markers
    .with_response_format(ResponseFormat::Markdown); // Request markdown format

  match creative_node.run_async(&shared).await {
    Ok(_) => {
      if let Some(intro) = shared.get("content_creator_output") {
        println!("✅ Creative Introduction Generated:");
        println!("{}\n", intro.as_str().unwrap_or("Could not parse"));
      }
    }
    Err(e) => {
      println!("❌ Creative node failed: {}", e);
    }
  }

  // 2. Analysis Node with JSON Response Format
  println!("📊 Step 2: Content Analysis with Structured Output");
  let analysis_node = LlmNode::new("content_analyzer", "qwen-plus")
    .with_prompt("Analyze the following content and provide a structured assessment:\n\n{{content_creator_output}}")
    .with_system("You are a content quality analyst. Provide objective, structured feedback.")
    .with_temperature(0.2)         // Low temperature for analytical precision
    .with_max_tokens(400)
    .with_json_response(Some(json!({
      "type": "object",
      "properties": {
        "readability_score": {
          "type": "number",
          "minimum": 1,
          "maximum": 10,
          "description": "Readability score from 1-10"
        },
        "engagement_level": {
          "type": "string",
          "enum": ["low", "medium", "high"],
          "description": "Expected engagement level"
        },
        "key_strengths": {
          "type": "array",
          "items": {"type": "string"},
          "description": "List of content strengths"
        },
        "suggestions": {
          "type": "array", 
          "items": {"type": "string"},
          "description": "Improvement suggestions"
        },
        "appropriate_for_audience": {
          "type": "boolean",
          "description": "Whether content suits the target audience"
        }
      },
      "required": ["readability_score", "engagement_level", "key_strengths", "appropriate_for_audience"]
    })));

  match analysis_node.run_async(&shared).await {
    Ok(_) => {
      if let Some(analysis) = shared.get("content_analyzer_output") {
        println!("✅ Structured Analysis:");
        if let Ok(parsed) = serde_json::from_str::<Value>(analysis.as_str().unwrap_or("{}")) {
          println!("{}\n", serde_json::to_string_pretty(&parsed)?);
        } else {
          println!("{}\n", analysis.as_str().unwrap_or("Could not parse"));
        }
      }
    }
    Err(e) => {
      println!("❌ Analysis node failed: {}", e);
    }
  }

  // 3. Revision Node with Seed for Reproducible Results
  println!("🔄 Step 3: Content Revision with Reproducible Generation");
  let revision_node = LlmNode::new("content_reviser", "gpt-4o-mini")
    .with_prompt("Based on this analysis: {{content_analyzer_output}}\n\nRevise this content: {{content_creator_output}}")
    .with_system("You are an expert editor. Apply the analytical feedback to improve the content while maintaining its core message.")
    .with_temperature(0.5)         // Balanced creativity and consistency
    .with_max_tokens(350)
    .with_top_k(40)               // Limit vocabulary diversity
    .with_seed(12345)             // Reproducible results
    .with_response_format(ResponseFormat::Markdown);

  match revision_node.run_async(&shared).await {
    Ok(_) => {
      if let Some(revised) = shared.get("content_reviser_output") {
        println!("✅ Revised Content:");
        println!("{}\n", revised.as_str().unwrap_or("Could not parse"));
      }
    }
    Err(e) => {
      println!("❌ Revision node failed: {}", e);
    }
  }

  // 4. Summary Node with Custom Stop Sequences
  println!("📋 Step 4: Final Summary Generation");
  // Using deepseek-chat as an alternative since Claude API has insufficient credits
  let summary_node = LlmNode::new("summarizer", "deepseek-chat")
    .with_prompt("Create a brief summary of this content improvement process. Include:\n1. Original topic: {{topic}}\n2. Target audience: {{audience}}\n3. Key improvements made\n\nSTOP")
    .with_system("You are a process documentation expert. Be concise and clear.")
    .with_temperature(0.1)         // Very low for consistency
    .with_max_tokens(200)
    .with_stop_sequences(vec!["STOP".to_string(), "END".to_string()]); // Will stop at "STOP" in prompt

  match summary_node.run_async(&shared).await {
    Ok(_) => {
      if let Some(summary) = shared.get("summarizer_output") {
        println!("✅ Process Summary:");
        println!("{}\n", summary.as_str().unwrap_or("Could not parse"));
      }
    }
    Err(e) => {
      println!("❌ Summary node failed: {}", e);
    }
  }

  // Display all shared state keys for debugging
  println!("🔍 Shared State Keys:");
  println!("   Available keys in shared state:");
  let keys = ["content_creator_output", "content_analyzer_output", "content_reviser_output", "summarizer_output"];
  for key in keys {
    if shared.get(key).is_some() {
      println!("   ✅ {}", key);
    } else {
      println!("   ❌ {} (missing)", key);
    }
  }

  println!("\n🏁 Advanced example completed!");
  println!("💡 This example demonstrated:");
  println!("   • Multiple LLM parameter configurations");
  println!("   • Template variable resolution from shared state");
  println!("   • Structured JSON response format");
  println!("   • Stop sequences for controlled generation");
  println!("   • Chaining multiple LLM nodes");
  println!("   • Reproducible generation with seed parameter");

  Ok(())
}
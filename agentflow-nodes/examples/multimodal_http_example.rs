//! Multimodal LLM with HTTP Images Example
//! 
//! This example demonstrates multimodal capabilities using HTTP/HTTPS image URLs,
//! which is the most straightforward approach for most LLM APIs.

use agentflow_core::{AsyncNode, SharedState};
use agentflow_nodes::{LlmNode, nodes::llm::ResponseFormat};
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("🚀 Multimodal LLM with HTTP Images Example");
  println!("==========================================\n");

  // Create shared state with HTTP image URLs
  let shared = SharedState::new();
  
  // Use publicly accessible images for demonstration
  // These are simple test images that should work with most multimodal APIs
  shared.insert("architecture_diagram".to_string(), 
    Value::String("https://yuxuetr.com/assets/images/lancedb-get-started-4588d196f793040e30459060ab55f331.png".to_string()));
  shared.insert("flow_chart".to_string(), 
    Value::String("https://yuxuetr.com/assets/images/rust-future-04085fd94c907c71b832da47e7abdf74.png".to_string()));
  
  // Add context variables
  shared.insert("analysis_type".to_string(), Value::String("technical analysis".to_string()));
  shared.insert("project_name".to_string(), Value::String("AgentFlow".to_string()));

  println!("🔗 Using HTTP image URLs for multimodal analysis");
  println!("   Architecture: {}", shared.get("architecture_diagram").unwrap().as_str().unwrap());
  println!("   Flow Chart: {}\n", shared.get("flow_chart").unwrap().as_str().unwrap());

  // 1. Single Image Analysis
  println!("🖼️ Step 1: Architecture Diagram Analysis");
  let arch_analyzer = LlmNode::new("arch_analyzer", "step-1o-turbo-vision")
    .with_prompt("Analyze this system architecture diagram. What components and relationships do you see?")
    .with_system("You are a software architect. Provide detailed analysis of technical diagrams.")
    .with_images(vec!["architecture_diagram".to_string()])
    .with_temperature(0.3)
    .with_max_tokens(400)
    .with_response_format(ResponseFormat::Markdown);

  match arch_analyzer.run_async(&shared).await {
    Ok(_) => {
      if let Some(analysis) = shared.get("arch_analyzer_output") {
        println!("✅ Architecture Analysis:");
        println!("{}\n", analysis.as_str().unwrap_or("Could not parse response"));
      }
    }
    Err(e) => {
      println!("❌ Architecture analysis failed: {}", e);
      println!("💡 Check your StepFun API configuration\n");
    }
  }

  // 2. Multiple Images Comparison
  println!("🔄 Step 2: Multiple Images Analysis");
  let multi_analyzer = LlmNode::new("multi_analyzer", "step-1o-turbo-vision")
    .with_prompt("Compare these two diagrams. What similarities and differences do you observe between the architecture and flow representations?")
    .with_system("You are an expert at visual analysis and comparison of technical diagrams.")
    .with_images(vec!["architecture_diagram".to_string(), "flow_chart".to_string()])
    .with_temperature(0.4)
    .with_max_tokens(500)
    .with_response_format(ResponseFormat::Markdown);

  match multi_analyzer.run_async(&shared).await {
    Ok(_) => {
      if let Some(comparison) = shared.get("multi_analyzer_output") {
        println!("✅ Multi-Image Analysis:");
        println!("{}\n", comparison.as_str().unwrap_or("Could not parse response"));
      }
    }
    Err(e) => {
      println!("❌ Multi-image analysis failed: {}\n", e);
    }
  }

  // 3. Context Integration
  println!("🧠 Step 3: Contextual Analysis Integration");
  let context_analyzer = LlmNode::new("context_analyzer", "step-1o-turbo-vision")
    .with_prompt("Based on the previous analysis: {{arch_analyzer_output}}\n\nAnd considering this is for {{project_name}}, provide recommendations for {{analysis_type}}.")
    .with_system("You are a technical consultant. Integrate visual analysis with project context.")
    .with_images(vec!["architecture_diagram".to_string()])
    .with_temperature(0.2)
    .with_max_tokens(600)
    .with_response_format(ResponseFormat::Markdown);

  match context_analyzer.run_async(&shared).await {
    Ok(_) => {
      if let Some(context_analysis) = shared.get("context_analyzer_output") {
        println!("✅ Contextual Analysis:");
        let analysis_text = context_analysis.as_str().unwrap_or("Could not parse response");
        if analysis_text.len() > 800 {
          println!("{}...\n[Truncated for display]\n", &analysis_text[..800]);
        } else {
          println!("{}\n", analysis_text);
        }
      }
    }
    Err(e) => {
      println!("❌ Contextual analysis failed: {}\n", e);
    }
  }

  // 4. Structured Output Test
  println!("📊 Step 4: Structured Analysis Output");
  let structured_analyzer = LlmNode::new("structured_analyzer", "step-1o-turbo-vision")
    .with_prompt("Analyze this diagram and provide structured insights in JSON format with: summary, key_components, relationships, recommendations")
    .with_system("You are a technical analyst. Provide structured JSON analysis of technical diagrams.")
    .with_images(vec!["architecture_diagram".to_string()])
    .with_temperature(0.2)
    .with_max_tokens(500)
    .with_response_format(ResponseFormat::loose_json());

  match structured_analyzer.run_async(&shared).await {
    Ok(_) => {
      if let Some(structured_result) = shared.get("structured_analyzer_output") {
        println!("✅ Structured Analysis:");
        if let Ok(parsed) = serde_json::from_str::<Value>(structured_result.as_str().unwrap_or("{}")) {
          println!("{}\n", serde_json::to_string_pretty(&parsed)?);
        } else {
          println!("{}\n", structured_result.as_str().unwrap_or("Could not parse JSON"));
        }
      }
    }
    Err(e) => {
      println!("❌ Structured analysis failed: {}\n", e);
    }
  }

  // Results Summary
  println!("📋 Results Summary:");
  let analyses = [
    ("Architecture Analysis", "arch_analyzer_output"),
    ("Multi-Image Analysis", "multi_analyzer_output"),
    ("Contextual Analysis", "context_analyzer_output"),
    ("Structured Analysis", "structured_analyzer_output")
  ];

  for (name, key) in analyses {
    if let Some(result) = shared.get(key) {
      let preview = result.as_str().unwrap_or("").chars().take(100).collect::<String>();
      println!("   ✅ {}: {}...", name, preview);
    } else {
      println!("   ❌ {}: No result", name);
    }
  }

  println!("\n🏁 HTTP multimodal example completed!");
  println!("\n💡 Key Benefits of HTTP URLs:");
  println!("   • Direct API support - no conversion needed");
  println!("   • No size limits for base64 encoding");
  println!("   • Faster processing - no local file I/O");
  println!("   • Easy sharing and testing");
  println!("   • Works with any publicly accessible image");

  println!("\n🔧 For Production Use:");
  println!("   1. Host images on reliable CDN or cloud storage");
  println!("   2. Use proper image formats (JPEG, PNG, WebP)");
  println!("   3. Optimize image sizes for API performance");
  println!("   4. Implement proper error handling and retries");
  println!("   5. Consider caching for frequently analyzed images");

  println!("\n💻 Alternative Approaches:");
  println!("   • Base64 encoding for local files (see multimodal_base64_example)");
  println!("   • Temporary image hosting services");
  println!("   • Upload to cloud storage and use generated URLs");
  println!("   • Local development server for serving images");

  Ok(())
}
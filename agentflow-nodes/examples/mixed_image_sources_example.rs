//! Mixed Image Sources with ImageUnderstandNode Example
//!
//! This example demonstrates how ImageUnderstandNode seamlessly handles both local 
//! image files and remote HTTP/HTTPS URLs in the same workflow, with automatic 
//! base64 conversion for local files and direct URL usage for remote images.

use agentflow_core::{AsyncNode, SharedState};
use agentflow_nodes::{LlmNode, ImageUnderstandNode};
use agentflow_nodes::nodes::{llm::ResponseFormat, image_understand::VisionResponseFormat};
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("🚀 Mixed Image Sources Example");
  println!("==============================\n");

  // Create shared state
  let shared = SharedState::new();
  
  // Mix of local and remote images
  let local_image = "../assets/AgentFlow-crates.jpeg";
  let remote_image = "https://yuxuetr.com/assets/images/lancedb-get-started-4588d196f793040e30459060ab55f331.png";
  
  println!("📁 Local image: {} (will be auto-converted to base64)", local_image);
  println!("🌐 Remote image: {} (will be used as-is)", remote_image);
  
  // Add context for analysis
  shared.insert("local_analysis_focus".to_string(), Value::String("system components and architecture".to_string()));
  shared.insert("remote_analysis_focus".to_string(), Value::String("technical concepts and workflows".to_string()));
  shared.insert("comparison_criteria".to_string(), Value::String("similarities, differences, and design patterns".to_string()));
  println!();

  // 1. Single Local Image Analysis
  println!("🏠 Step 1: Local Image Analysis");
  if std::path::Path::new(local_image).exists() {
    let local_analyzer = ImageUnderstandNode::image_analyzer("local_analyzer", "step-1o-turbo-vision", local_image)
      .with_system_message("You are a software architect analyzing system diagrams.")
      .with_text_prompt("Analyze this local architecture diagram. Focus on {{local_analysis_focus}}.")
      .with_input_keys(vec!["local_analysis_focus".to_string()])
      .with_temperature(0.3)
      .with_max_tokens(400)
      .with_response_format(VisionResponseFormat::Markdown);

    match local_analyzer.run_async(&shared).await {
      Ok(_) => {
        if let Some(analysis) = shared.get("local_analyzer_output") {
          println!("✅ Local Image Analysis:");
          let text = analysis.as_str().unwrap_or("Could not parse response");
          if text.len() > 500 {
            println!("{}...\n[Truncated for display]\n", &text[..500]);
          } else {
            println!("{}\n", text);
          }
        }
      }
      Err(e) => {
        println!("❌ Local image analysis failed: {}\n", e);
      }
    }
  } else {
    println!("⚠️ Local image not found, skipping local analysis\n");
  }

  // 2. Remote Image Analysis
  println!("🌐 Step 2: Remote Image Analysis");
  let remote_analyzer = ImageUnderstandNode::image_analyzer("remote_analyzer", "step-1o-turbo-vision", remote_image)
    .with_system_message("You are a technical analyst examining diagrams.")
    .with_text_prompt("Analyze this remote diagram. Focus on {{remote_analysis_focus}}.")
    .with_input_keys(vec!["remote_analysis_focus".to_string()])
    .with_temperature(0.3)
    .with_max_tokens(400)
    .with_response_format(VisionResponseFormat::Markdown);

  match remote_analyzer.run_async(&shared).await {
    Ok(_) => {
      if let Some(analysis) = shared.get("remote_analyzer_output") {
        println!("✅ Remote Image Analysis:");
        let text = analysis.as_str().unwrap_or("Could not parse response");
        if text.len() > 500 {
          println!("{}...\n[Truncated for display]\n", &text[..500]);
        } else {
          println!("{}\n", text);
        }
      }
    }
    Err(e) => {
      println!("❌ Remote image analysis failed: {}\n", e);
    }
  }

  // 3. Multi-Image Comparison using ImageUnderstandNode
  println!("🔄 Step 3: Comparative Analysis (Both Images)");
  
  if std::path::Path::new(local_image).exists() {
    let comparison_analyzer = ImageUnderstandNode::image_comparator(
      "comparison_analyzer", 
      "step-1o-turbo-vision", 
      local_image, 
      vec![remote_image.to_string()]
    )
    .with_system_message("You are an expert at visual analysis and comparison of technical diagrams.")
    .with_text_prompt("Compare these technical diagrams. Identify {{comparison_criteria}}.")
    .with_input_keys(vec!["comparison_criteria".to_string()])
    .with_temperature(0.4)
    .with_max_tokens(600)
    .with_response_format(VisionResponseFormat::Markdown);

    match comparison_analyzer.run_async(&shared).await {
      Ok(_) => {
        if let Some(comparison) = shared.get("comparison_analyzer_output") {
          println!("✅ Comparative Analysis:");
          let text = comparison.as_str().unwrap_or("Could not parse response");
          if text.len() > 800 {
            println!("{}...\n[Truncated for display]\n", &text[..800]);
          } else {
            println!("{}\n", text);
          }
        }
      }
      Err(e) => {
        println!("❌ Comparative analysis failed: {}\n", e);
      }
    }
  }

  // 4. Text-based Summary using LLMNode (combining previous analyses)
  println!("📊 Step 4: Technical Summary and Insights");
  println!("   (Using LLMNode to synthesize image analysis results)");
  
  let summary_analyzer = LlmNode::new("summary_analyzer", "gpt-4") // Text-only model
    .with_prompt("Based on the local image analysis: {{local_analyzer_output}} and remote image analysis: {{remote_analyzer_output}}, provide a technical summary highlighting key architectural patterns and insights.")
    .with_system("You are a technical consultant providing executive summaries.")
    .with_temperature(0.2)
    .with_max_tokens(500)
    .with_response_format(ResponseFormat::Markdown);

  match summary_analyzer.run_async(&shared).await {
    Ok(_) => {
      if let Some(summary) = shared.get("summary_analyzer_output") {
        println!("✅ Technical Summary:");
        println!("{}\n", summary.as_str().unwrap_or("Could not parse response"));
      }
    }
    Err(e) => {
      println!("❌ Summary generation failed: {}\n", e);
    }
  }

  // 5. Results Overview
  println!("📋 Results Overview:");
  let analyses = [
    ("Local Image Analysis", "local_analyzer_output"),
    ("Remote Image Analysis", "remote_analyzer_output"),
    ("Comparative Analysis", "comparison_analyzer_output"),
    ("Technical Summary", "summary_analyzer_output")
  ];

  for (name, key) in analyses {
    if let Some(result) = shared.get(key) {
      let preview = result.as_str().unwrap_or("").chars().take(100).collect::<String>();
      println!("   ✅ {}: {}...", name, preview);
    } else {
      println!("   ❌ {}: No result", name);
    }
  }

  println!("\n🏁 Mixed image sources example completed!");
  println!("\n💡 Key Features Demonstrated:");
  println!("   • Seamless handling of local files and remote URLs");
  println!("   • Automatic base64 conversion for local images");
  println!("   • Direct URL usage for remote images");
  println!("   • Mixed image sources in single multimodal request");
  println!("   • Error handling for missing local files");
  println!("   • Comparative analysis across different image sources");
  
  println!("\n🔧 Implementation Details:");
  println!("   • Local files: Detected by absence of 'http://' or 'https://' prefix");
  println!("   • Remote URLs: Used directly without modification");
  println!("   • Base64 conversion: Automatic and transparent");
  println!("   • File validation: Built-in error handling for missing files");
  println!("   • Mixed workflows: Both types work together seamlessly");

  println!("\n🚀 Production Benefits:");
  println!("   • No manual image preprocessing required");
  println!("   • Flexible image source handling");
  println!("   • Consistent API regardless of image location");
  println!("   • Automatic optimization for different image types");
  println!("   • Simplified workflow development");

  Ok(())
}
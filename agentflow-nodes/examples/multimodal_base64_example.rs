//! Image Understanding with Automatic Local File Conversion Example
//! 
//! This example demonstrates using the ImageUnderstandNode for multimodal image analysis,
//! which automatically handles local image file conversion to base64 data URLs.

use agentflow_core::{AsyncNode, SharedState};
use agentflow_nodes::ImageUnderstandNode;
use agentflow_nodes::nodes::image_understand::VisionResponseFormat;
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("🚀 Image Understanding with Automatic Local File Conversion");
  println!("============================================================\n");

  // Create shared state
  let shared = SharedState::new();
  
  // Use local image path directly - ImageUnderstandNode will automatically convert to base64
  let image_path = "../assets/AgentFlow-crates.jpeg";
  
  if !std::path::Path::new(image_path).exists() {
    println!("❌ Error: Image not found at {}", image_path);
    println!("   Please ensure the AgentFlow-crates.jpeg file exists.");
    return Ok(());
  }

  println!("📁 Using local image path directly: {}", image_path);
  println!("   🤖 ImageUnderstandNode will automatically convert to base64 internally\n");
  
  // Add context variables to shared state
  shared.insert("analysis_focus".to_string(), Value::String("crate relationships and system architecture".to_string()));
  shared.insert("project_context".to_string(), Value::String("AgentFlow workflow orchestration platform".to_string()));

  // 1. Architecture Analysis with Automatic Local Image Conversion
  println!("🏗️ Step 1: Architecture Analysis with Automatic Image Conversion");
  let architecture_analyzer = ImageUnderstandNode::image_analyzer("arch_analyzer", "step-1o-turbo-vision", image_path)
    .with_system_message("You are an expert software architect. Provide detailed analysis of system architecture diagrams.")
    .with_text_prompt("Analyze this AgentFlow architecture diagram. Focus on {{analysis_focus}}.")
    .with_input_keys(vec!["analysis_focus".to_string()])
    .with_temperature(0.3)
    .with_max_tokens(600)
    .with_response_format(VisionResponseFormat::Markdown);

  match architecture_analyzer.run_async(&shared).await {
    Ok(_) => {
      if let Some(analysis) = shared.get("arch_analyzer_output") {
        println!("✅ Architecture Analysis:");
        let analysis_text = analysis.as_str().unwrap_or("Could not parse response");
        if analysis_text.len() > 1000 {
          println!("{}...\n[Truncated for display]\n", &analysis_text[..1000]);
        } else {
          println!("{}\n", analysis_text);
        }
      }
    }
    Err(e) => {
      println!("❌ Architecture analysis failed: {}", e);
      println!("💡 This might be due to:");
      println!("   • Missing StepFun API key or incorrect model configuration");
      println!("   • Local image file access issues or file not found");
      println!("   • Image too large for the vision model API");
      println!("   • API rate limits or quota exceeded\n");
    }
  }

  // 2. Technical Documentation Generation  
  println!("📖 Step 2: Technical Documentation Generation");
  let doc_generator = ImageUnderstandNode::new("docs_generator", "step-1o-turbo-vision", 
    "Generate comprehensive technical documentation explaining the system components and their interactions in this {{project_context}} diagram.", 
    image_path)
    .with_system_message("You are a technical writer specializing in software architecture documentation.")
    .with_input_keys(vec!["project_context".to_string()])
    .with_temperature(0.2)
    .with_max_tokens(800)
    .with_response_format(VisionResponseFormat::Markdown);

  match doc_generator.run_async(&shared).await {
    Ok(_) => {
      if let Some(docs) = shared.get("docs_generator_output") {
        println!("✅ Generated Documentation:");
        let docs_text = docs.as_str().unwrap_or("Could not parse response");
        if docs_text.len() > 1200 {
          println!("{}...\n[Truncated for display]\n", &docs_text[..1200]);
        } else {
          println!("{}\n", docs_text);
        }
      }
    }
    Err(e) => {
      println!("❌ Documentation generation failed: {}\n", e);
    }
  }

  // 3. Alternative: Test with a remote image URL
  println!("🌐 Step 3: Remote Image Analysis Test");
  println!("   Testing with a remote image URL to verify URL handling...");
  
  let remote_image_url = "https://yuxuetr.com/assets/ideal-img/about-dark.6816e71.1920.jpg";
  let online_test_node = ImageUnderstandNode::image_describer("online_test", "step-1o-turbo-vision", remote_image_url)
    .with_text_prompt("What do you see in this image? Provide a brief description.")
    .with_temperature(0.4)
    .with_max_tokens(200);

  match online_test_node.run_async(&shared).await {
    Ok(_) => {
      if let Some(result) = shared.get("online_test_output") {
        println!("✅ Online Image Test Result:");
        println!("{}\n", result.as_str().unwrap_or("Could not parse response"));
      }
    }
    Err(e) => {
      println!("❌ Online image test also failed: {}", e);
      println!("💡 This suggests a configuration or API issue rather than image format.\n");
    }
  }

  // 4. Image File Information  
  println!("📊 Image File Analysis:");
  println!("   • File path: {}", image_path);
  println!("   • Format: Local file (automatically converted to base64 internally)");
  
  if let Ok(metadata) = std::fs::metadata(image_path) {
    println!("   • File size: {} bytes ({:.1} KB)", metadata.len(), metadata.len() as f64 / 1024.0);
    
    // Estimate base64 size (roughly 4/3 of original size)
    let estimated_base64_size = (metadata.len() as f64 * 4.0 / 3.0) as usize;
    println!("   • Estimated base64 size: ~{} characters ({:.1} KB)", estimated_base64_size, estimated_base64_size as f64 / 1024.0);
    
    if estimated_base64_size > 1_000_000 { // > 1MB base64
      println!("   ⚠️  Warning: Image is quite large. Some APIs have size limits.");
    } else {
      println!("   ✅ Image size is reasonable for most multimodal APIs.");
    }
  }

  println!("\n🏁 Image understanding example completed!");
  println!("\n💡 Key Learning Points:");
  println!("   • ImageUnderstandNode automatically handles both local files and remote URLs");
  println!("   • Local files are converted to base64 data URLs internally"); 
  println!("   • HTTP/HTTPS URLs are used directly without conversion");
  println!("   • Purpose-built API for vision tasks with optimized defaults");
  println!("   • Template support for dynamic prompts with shared state");
  println!("   • Specialized constructors (image_analyzer, image_describer, etc.)");
  
  println!("\n🔧 Architecture Benefits:");
  println!("   • Clear separation: ImageUnderstandNode for vision, LLMNode for text");
  println!("   • Vision-optimized parameters and response formats");
  println!("   • Better error handling for image-specific issues");
  println!("   • Multi-image support for comparison and analysis workflows");
  
  println!("\n🚀 Usage Recommendations:");
  println!("   • Use ImageUnderstandNode for image analysis tasks");
  println!("   • Use LLMNode for text-only language model interactions");
  println!("   • Combine both in complex workflows that need text + vision");
  println!("   • Leverage specialized constructors for common vision patterns");

  Ok(())
}
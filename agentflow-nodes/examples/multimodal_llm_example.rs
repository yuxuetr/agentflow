//! Image Understanding Workflow Example
//! 
//! This example demonstrates a comprehensive workflow using ImageUnderstandNode
//! for multi-step architectural analysis combining vision and text processing.

use agentflow_core::{AsyncNode, SharedState};
use agentflow_nodes::{LlmNode, ImageUnderstandNode};
use agentflow_nodes::nodes::{llm::ResponseFormat, image_understand::VisionResponseFormat};
use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  // Initialize AgentFlow to load ~/.agentflow/.env
  agentflow_llm::AgentFlow::init().await.expect("Failed to initialize AgentFlow");

  println!("🚀 Image Understanding Workflow Example");
  println!("=======================================\n");

  // Create shared state with local image and context
  let shared = SharedState::new();
  
  // Use real local image - AgentFlow architecture diagram
  let image_path = "../assets/AgentFlow-crates.jpeg";
  
  // Check if the image exists
  if !std::path::Path::new(image_path).exists() {
    println!("⚠️  Warning: Image not found at {}", image_path);
    println!("   The example will continue but image analysis may fail.");
    println!("   Make sure the AgentFlow-crates.jpeg file exists in the assets directory.\n");
  } else {
    println!("✅ Found local image: {}\n", image_path);
  }
  
  // Add context variables for template resolution
  shared.insert("analysis_focus".to_string(), Value::String("crate relationships and overall system structure".to_string()));
  shared.insert("report_audience".to_string(), Value::String("executive team".to_string()));
  shared.insert("development_context".to_string(), 
    Value::String("The AgentFlow project is a Rust-based workflow orchestration platform with LLM integration and MCP support.".to_string()));

  // 1. Image Analysis using ImageUnderstandNode - AgentFlow Architecture
  println!("🖼️ Step 1: AgentFlow Architecture Analysis");
  let image_analyzer = ImageUnderstandNode::image_analyzer("architecture_analyst", "step-1o-turbo-vision", image_path)
    .with_system_message("You are an expert software architect. Analyze technical diagrams and explain system architecture clearly.")
    .with_text_prompt("Analyze this AgentFlow architecture diagram. Focus on {{analysis_focus}}.")
    .with_input_keys(vec!["analysis_focus".to_string()])
    .with_temperature(0.3)
    .with_max_tokens(500)
    .with_response_format(VisionResponseFormat::Markdown);

  match image_analyzer.run_async(&shared).await {
    Ok(_) => {
      if let Some(analysis) = shared.get("architecture_analyst_output") {
        println!("✅ Architecture Analysis:");
        println!("{}\n", analysis.as_str().unwrap_or("Could not parse response"));
      }
    }
    Err(e) => {
      println!("❌ Architecture analysis failed: {}", e);
      println!("💡 Note: This requires a multimodal model like step-1o-turbo-vision");
    }
  }

  // 2. Technical Documentation Generation from Image
  println!("📝 Step 2: Technical Documentation Generation");
  let doc_generator = ImageUnderstandNode::new("doc_generator", "step-1o-turbo-vision",
    "Based on this AgentFlow architecture diagram and the context: {{development_context}}, generate technical documentation that explains the system structure, crate relationships, and data flow.",
    image_path)
    .with_system_message("You are a technical writer. Create clear, comprehensive documentation from architectural diagrams.")
    .with_input_keys(vec!["development_context".to_string()])
    .with_temperature(0.3)
    .with_max_tokens(600)
    .with_response_format(VisionResponseFormat::Markdown);

  match doc_generator.run_async(&shared).await {
    Ok(_) => {
      if let Some(docs) = shared.get("doc_generator_output") {
        println!("✅ Generated Documentation:");
        let doc_preview = docs.as_str().unwrap_or("No documentation generated");
        if doc_preview.len() > 800 {
          println!("{}...\n[Documentation truncated for display]\n", &doc_preview[..800]);
        } else {
          println!("{}\n", doc_preview);
        }
      }
    }
    Err(e) => {
      println!("❌ Documentation generation failed: {}", e);
    }
  }

  // 3. Text-based Analysis using LLMNode (combining image analysis with text processing)
  println!("🏗️ Step 3: Architectural Analysis Report");
  println!("   (Using LLMNode for text analysis based on image analysis results)");

  let code_analyzer = LlmNode::new("code_analyzer", "moonshot-v1-8k") // Text-only model
    .with_prompt(r#"
Based on the previous AgentFlow architecture analysis: {{architecture_analyst_output}}

Development context: {{development_context}}

Please provide a code review and architectural analysis covering:
1. Crate separation and responsibilities
2. Potential architectural improvements
3. Dependencies and coupling analysis
4. Scalability considerations

Focus on software engineering best practices.
"#)
    .with_system("You are a senior software architect. Provide detailed architectural analysis and recommendations.")
    .with_temperature(0.2) // Low temperature for consistent technical analysis
    .with_max_tokens(700)
    .with_response_format(ResponseFormat::Markdown);

  match code_analyzer.run_async(&shared).await {
    Ok(_) => {
      if let Some(analysis) = shared.get("code_analyzer_output") {
        println!("✅ Architectural Analysis Report:");
        let analysis_preview = analysis.as_str().unwrap_or("Could not parse response");
        if analysis_preview.len() > 1000 {
          println!("{}...\n[Analysis truncated for display]\n", &analysis_preview[..1000]);
        } else {
          println!("{}\n", analysis_preview);
        }
      }
    }
    Err(e) => {
      println!("❌ Code analysis failed: {}", e);
    }
  }

  // 4. Executive Summary Generation (Text-only based on previous analyses)
  println!("📊 Step 4: Executive Summary Generation");
  println!("   (Using LLMNode for text synthesis of previous analyses)");
  
  let summary_node = LlmNode::new("executive_summary", "claude-3-haiku-20240307") // Claude model
    .with_prompt("Based on the architectural analysis: {{code_analyzer_output}}, and considering the {{report_audience}}, create an executive summary covering key findings and strategic recommendations.")
    .with_system("You are an executive technical consultant. Create concise, actionable summaries for leadership.")
    .with_temperature(0.2)
    .with_max_tokens(400)
    .with_response_format(ResponseFormat::Markdown);

  match summary_node.run_async(&shared).await {
    Ok(_) => {
      if let Some(summary) = shared.get("executive_summary_output") {
        println!("✅ Executive Summary:");
        println!("{}\n", summary.as_str().unwrap_or("Could not parse response"));
      }
    }
    Err(e) => {
      println!("❌ Summary generation failed: {}", e);
    }
  }

  // Display final state summary
  println!("📊 Final Results Summary:");
  let result_keys = [
    ("Architecture Analysis", "architecture_analyst_output"),
    ("Documentation Generation", "doc_generator_output"),
    ("Code Analysis", "code_analyzer_output"),
    ("Executive Summary", "executive_summary_output")
  ];

  for (name, key) in result_keys {
    if let Some(result) = shared.get(key) {
      let preview = result.as_str().unwrap_or("")
        .chars().take(100).collect::<String>();
      println!("   ✅ {}: {}...", name, preview);
    } else {
      println!("   ❌ {}: No result", name);
    }
  }

  println!("\n🏁 Image understanding workflow example completed!");
  println!("\n💡 This example demonstrated:");
  println!("   • Image analysis using specialized ImageUnderstandNode");
  println!("   • Text processing using dedicated LLMNode");
  println!("   • Clear separation between vision and text-only models");
  println!("   • Multi-step workflow combining image and text analysis");
  println!("   • Template-based prompt resolution with shared state");
  println!("   • Automatic local image file conversion to base64");
  
  println!("\n🏗️ Architectural Benefits:");
  println!("   • ImageUnderstandNode: Purpose-built for vision tasks");
  println!("   • LLMNode: Focused on text-only language models");
  println!("   • Clear separation of concerns and responsibilities");
  println!("   • Optimized APIs for their specific use cases");
  println!("   • Better error handling and debugging for each domain");
  
  println!("\n🔧 Setup Requirements:");
  println!("   1. Configure API keys (StepFun for vision, OpenAI/others for text)");
  println!("   2. Add vision models (step-1o-turbo-vision) and text models (moonshot-v1-8k)");
  println!("   3. Ensure AgentFlow-crates.jpeg exists in /Users/hal/arch/agentflow/assets/");
  println!("   4. ImageUnderstandNode automatically handles base64 conversion");
  
  println!("\n🚀 Usage Guidelines:");
  println!("   • Use ImageUnderstandNode for: image analysis, OCR, visual Q&A");
  println!("   • Use LLMNode for: text generation, reasoning, code analysis");
  println!("   • Combine both for: complex workflows with vision + language tasks");

  Ok(())
}
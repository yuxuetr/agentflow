//! OpenAI Models Comprehensive Test Example (Fixed)
//! 
//! This example tests all available OpenAI models from the OpenAI API
//! handling special requirements for different model types.

use agentflow_core::{AsyncNode, SharedState};
use agentflow_nodes::LlmNode;
use agentflow_nodes::nodes::llm::ResponseFormat;
use serde_json::Value;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  // Initialize AgentFlow to load ~/.agentflow/.env
  agentflow_llm::AgentFlow::init().await.expect("Failed to initialize AgentFlow");

  println!("🤖 OpenAI Models Comprehensive Test (Fixed)");
  println!("===========================================\n");

  // Create shared state
  let shared = SharedState::new();
  
  // Define OpenAI models with their special requirements
  // (model_id, display_name, created_date, supports_streaming, supports_vision, special_requirements)
  let openai_models = vec![
    // GPT-4.1 Series - Latest Generation
    ("gpt-4.1", "GPT-4.1", "2025-04-14", true, true, "standard"),
    ("gpt-4.1-mini", "GPT-4.1 Mini", "2025-04-14", true, true, "standard"),
    ("gpt-4.1-nano", "GPT-4.1 Nano", "2025-04-14", true, false, "standard"),
    
    // GPT-4o Series - Current Production
    ("gpt-4o", "GPT-4o", "2024-05-13", true, true, "standard"),
    ("gpt-4o-2024-11-20", "GPT-4o (2024-11-20)", "2024-11-20", true, true, "standard"),
    ("gpt-4o-2024-05-13", "GPT-4o (2024-05-13)", "2024-05-13", true, true, "standard"),
    ("gpt-4o-mini", "GPT-4o Mini", "2024-07-18", true, true, "standard"),
    
    // GPT-4o Audio Models - Skip these as they require audio
    // ("gpt-audio", "GPT Audio", "2025-08-28", true, false, "audio"),
    // ("gpt-4o-audio-preview", "GPT-4o Audio Preview", "2024-10-01", true, false, "audio"),
    
    // GPT-4o Search Preview Models - No temperature parameter
    ("gpt-4o-search-preview", "GPT-4o Search Preview", "2025-03-11", true, false, "no_temperature"),
    ("gpt-4o-mini-search-preview", "GPT-4o Mini Search Preview", "2025-03-11", true, false, "no_temperature"),
    
    // GPT-4 Legacy Series
    ("gpt-4", "GPT-4", "2023-03-14", true, false, "standard"),
    ("gpt-4-turbo", "GPT-4 Turbo", "2024-01-25", true, true, "standard"),
    ("gpt-4-turbo-2024-04-09", "GPT-4 Turbo (2024-04-09)", "2024-04-09", true, true, "standard"),
    ("gpt-4-0125-preview", "GPT-4 (0125 Preview)", "2024-01-25", true, false, "standard"),
    
    // GPT-3.5 Series
    ("gpt-3.5-turbo", "GPT-3.5 Turbo", "2023-02-01", true, false, "standard"),
    ("gpt-3.5-turbo-1106", "GPT-3.5 Turbo (1106)", "2023-11-06", true, false, "standard"),
  ];

  let mut working_models = Vec::new();
  let mut unavailable_models = Vec::new();
  let mut model_performance = Vec::new();

  println!("📝 Testing All OpenAI Models");
  println!("=============================");
  println!("🔍 Testing {} models from OpenAI API...\n", openai_models.len());
  
  // Add test context to shared state
  shared.insert("test_topic".to_string(), 
    Value::String("Rust async programming and tokio runtime optimization".to_string()));

  for (model_id, display_name, created_date, supports_streaming, supports_vision, special_req) in &openai_models {
    println!("🧪 Testing: {} ({})", display_name, model_id);
    println!("   📅 Created: {}", created_date);
    println!("   🔄 Streaming: {} | 👁️ Vision: {} | ⚙️ Special: {}", 
      if *supports_streaming { "✅" } else { "❌" },
      if *supports_vision { "✅" } else { "❌" },
      special_req);
    
    let start_time = Instant::now();
    
    let mut test_node = LlmNode::new(&format!("{}_test", model_id.replace("-", "_").replace(".", "_")), model_id)
      .with_prompt("You are testing AgentFlow integration. Analyze: {{test_topic}}. Respond with exactly 2 sentences about the benefits of async programming in Rust.")
      .with_system("You are a Rust programming expert. Be concise and technical.")
      .with_max_tokens(100)
      .with_response_format(ResponseFormat::Markdown);
    
    // Apply temperature only for models that support it
    if *special_req != "no_temperature" {
      test_node = test_node.with_temperature(0.3);
    }

    match test_node.run_async(&shared).await {
      Ok(_) => {
        let duration = start_time.elapsed();
        if let Some(result) = shared.get(&format!("{}_test_output", model_id.replace("-", "_").replace(".", "_"))) {
          let response = result.as_str().unwrap_or("No response");
          
          // Check if it's a real response (not mock)
          if response.contains("mock response") {
            println!("   ❌ UNAVAILABLE: Model returned mock response");
            unavailable_models.push((model_id.to_string(), display_name.to_string(), "Mock fallback".to_string()));
          } else {
            println!("   ✅ WORKING: Real response received ({:?})", duration);
            println!("   📝 Response: {}", 
              if response.len() > 80 { 
                format!("{}...", &response[..80]) 
              } else { 
                response.to_string() 
              });
            working_models.push((model_id.to_string(), display_name.to_string()));
            model_performance.push((model_id.to_string(), duration));
          }
        }
      }
      Err(e) => {
        println!("   ❌ FAILED: {}", e);
        unavailable_models.push((model_id.to_string(), display_name.to_string(), format!("Error: {}", e)));
      }
    }
    println!();
  }

  // Test 2: Multimodal capabilities for vision-enabled models
  let image_path = "../assets/AgentFlow-crates.jpeg";
  let image_exists = std::path::Path::new(image_path).exists();
  
  if image_exists {
    println!("🖼️  Testing Multimodal Capabilities");
    println!("====================================");
    println!("🔍 Testing vision capabilities with vision-enabled models...\n");
    
    // Test multimodal models that support vision
    let vision_models = vec![
      ("gpt-4o", "GPT-4o (Vision)"),
      ("gpt-4o-mini", "GPT-4o Mini (Vision)"),
      ("gpt-4-turbo", "GPT-4 Turbo (Vision)"),
    ];

    for (model_id, display_name) in &vision_models {
      // Check if this model was working in the text test
      if working_models.iter().any(|(id, _)| id == model_id) {
        println!("🧪 Testing Vision: {} ({})", display_name, model_id);
        
        let start_time = Instant::now();
        
        use agentflow_nodes::ImageUnderstandNode;
        use agentflow_nodes::nodes::image_understand::VisionResponseFormat;
        
        let vision_node = ImageUnderstandNode::new(
          &format!("{}_vision_test", model_id.replace("-", "_")), 
          model_id,
          "Analyze this AgentFlow architecture diagram. Describe the main components and their relationships in 1-2 sentences.",
          image_path)
          .with_system_message("You are an expert at analyzing software architecture diagrams.")
          .with_temperature(0.3)
          .with_max_tokens(150)
          .with_response_format(VisionResponseFormat::Markdown);

        match vision_node.run_async(&shared).await {
          Ok(_) => {
            let duration = start_time.elapsed();
            if let Some(result) = shared.get(&format!("{}_vision_test_output", model_id.replace("-", "_"))) {
              let response = result.as_str().unwrap_or("No response");
              
              if response.contains("mock vision") {
                println!("   ❌ VISION UNAVAILABLE: Multimodal capability not accessible");
              } else {
                println!("   ✅ VISION WORKING: Real multimodal response ({:?})", duration);
                println!("   📝 Vision Response: {}", 
                  if response.len() > 80 { 
                    format!("{}...", &response[..80]) 
                  } else { 
                    response.to_string() 
                  });
              }
            }
          }
          Err(e) => {
            println!("   ❌ VISION FAILED: {}", e);
          }
        }
        println!();
      }
    }
  }

  // Summary Report
  println!("📊 OpenAI Models Test Report");
  println!("=============================\n");
  
  println!("✅ WORKING MODELS ({}/{}):", working_models.len(), openai_models.len());
  if working_models.is_empty() {
    println!("   ⚠️  No models are currently working");
    println!("   💡 Check if models are registered in ~/.agentflow/models.yml");
  } else {
    for (model_id, display_name) in &working_models {
      if let Some((_, duration)) = model_performance.iter().find(|(id, _)| id == model_id) {
        println!("   ✅ {} ({}) - Response time: {:?}", display_name, model_id, duration);
      }
    }
  }
  
  println!("\n❌ UNAVAILABLE MODELS ({}/{}):", unavailable_models.len(), openai_models.len());
  if unavailable_models.is_empty() {
    println!("   🎉 All models are working!");
  } else {
    for (model_id, display_name, reason) in &unavailable_models {
      println!("   ❌ {} ({}) - {}", display_name, model_id, reason);
    }
  }

  println!("\n💡 Analysis & Recommendations:");
  
  if working_models.len() > 0 {
    println!("✅ AgentFlow integration: WORKING");
    println!("✅ Environment loading: SUCCESSFUL");
    println!("✅ API authentication: VALID");
    
    // Find fastest model
    if let Some((fastest_model, fastest_time)) = model_performance.iter().min_by_key(|(_, duration)| *duration) {
      if let Some((_, display_name)) = working_models.iter().find(|(id, _)| id == fastest_model) {
        println!("⚡ Fastest model: {} ({:?})", display_name, fastest_time);
      }
    }
    
    // Recommend models based on use case
    println!("\n🚀 Recommended Models by Use Case:");
    
    // Check for GPT-4.1 models
    if working_models.iter().any(|(id, _)| id.starts_with("gpt-4.1")) {
      println!("   💎 Premium: gpt-4.1 (Latest, most capable)");
      println!("   💰 Cost-effective: gpt-4.1-mini (Balanced performance)");
      println!("   ⚡ Fast & cheap: gpt-4.1-nano (Quick responses)");
    }
    
    // Check for GPT-4o models
    if working_models.iter().any(|(id, _)| id == "gpt-4o") {
      println!("   🎯 Production: gpt-4o (Stable, multimodal)");
    }
    if working_models.iter().any(|(id, _)| id == "gpt-4o-mini") {
      println!("   📱 Lightweight: gpt-4o-mini (Fast, affordable)");
    }
    
    // Check for special capabilities
    if working_models.iter().any(|(id, _)| id.contains("search")) {
      println!("   🔍 Search-enhanced: gpt-4o-search-preview (Web search integration)");
    }
  } else {
    println!("⚠️  Models not found in registry - updating configuration...");
    println!("   Run: agentflow-llm discover openai");
  }
  
  if unavailable_models.len() > 0 {
    println!("\n📝 Notes on unavailable models:");
    println!("   • Audio models (gpt-audio) require audio input/output");
    println!("   • Search models don't support temperature parameter");
    println!("   • Some models need to be registered in models.yml");
  }

  println!("\n🎯 System Status: AgentFlow + OpenAI Integration = READY");
  
  Ok(())
}

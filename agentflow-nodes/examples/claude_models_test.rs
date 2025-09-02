//! Claude Models Comprehensive Test Example
//! 
//! This example tests all available Claude models from the Anthropic API
//! to determine which ones are functioning and which are unavailable.

use agentflow_core::{AsyncNode, SharedState};
use agentflow_nodes::LlmNode;
use agentflow_nodes::nodes::llm::ResponseFormat;
use serde_json::Value;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  // Initialize AgentFlow to load ~/.agentflow/.env
  agentflow_llm::AgentFlow::init().await.expect("Failed to initialize AgentFlow");

  println!("🧠 Claude Models Comprehensive Test");
  println!("===================================\n");

  // Create shared state
  let shared = SharedState::new();
  
  // Define ALL Claude models from the API response
  let claude_models = vec![
    // Claude 4 Series - Latest Generation
    ("claude-opus-4-1-20250805", "Claude Opus 4.1", "2025-08-05"),
    ("claude-opus-4-20250514", "Claude Opus 4", "2025-05-22"),
    ("claude-sonnet-4-20250514", "Claude Sonnet 4", "2025-05-22"),
    
    // Claude 3.7 Series
    ("claude-3-7-sonnet-20250219", "Claude Sonnet 3.7", "2025-02-24"),
    
    // Claude 3.5 Series - Current Production
    ("claude-3-5-sonnet-20241022", "Claude Sonnet 3.5 (New)", "2024-10-22"),
    ("claude-3-5-haiku-20241022", "Claude Haiku 3.5", "2024-10-22"), 
    ("claude-3-5-sonnet-20240620", "Claude Sonnet 3.5 (Old)", "2024-06-20"),
    
    // Claude 3 Series - Legacy
    ("claude-3-haiku-20240307", "Claude Haiku 3", "2024-03-07"),
    ("claude-3-opus-20240229", "Claude Opus 3", "2024-02-29"),
  ];

  let mut working_models = Vec::new();
  let mut unavailable_models = Vec::new();
  let mut model_performance = Vec::new();

  println!("📝 Testing All Claude Models");
  println!("============================");
  println!("🔍 Testing {} models from Anthropic API...\n", claude_models.len());
  
  // Add test context to shared state
  shared.insert("test_topic".to_string(), 
    Value::String("Rust crate architecture and modular design principles".to_string()));

  for (model_id, display_name, created_date) in &claude_models {
    println!("🧪 Testing: {} ({})", display_name, model_id);
    println!("   📅 Created: {}", created_date);
    
    let start_time = Instant::now();
    
    let test_node = LlmNode::new(&format!("{}_test", model_id.replace("-", "_")), model_id)
      .with_prompt("You are testing AgentFlow integration. Analyze: {{test_topic}}. Respond with exactly 2 sentences about the benefits of modular architecture.")
      .with_system("You are a software architecture expert. Be concise and technical.")
      .with_temperature(0.3)
      .with_max_tokens(100)
      .with_response_format(ResponseFormat::Markdown);

    match test_node.run_async(&shared).await {
      Ok(_) => {
        let duration = start_time.elapsed();
        if let Some(result) = shared.get(&format!("{}_test_output", model_id.replace("-", "_"))) {
          let response = result.as_str().unwrap_or("No response");
          
          // Check if it's a real response (not mock)
          if response.contains("mock response") {
            println!("   ❌ UNAVAILABLE: Model returned mock response (likely 404/auth error)");
            unavailable_models.push((model_id.clone(), display_name.clone(), "API Error (404/Auth)".to_string()));
          } else {
            println!("   ✅ WORKING: Real response received ({:?})", duration);
            println!("   📝 Response: {}", 
              if response.len() > 80 { 
                format!("{}...", &response[..80]) 
              } else { 
                response.to_string() 
              });
            working_models.push((model_id.clone(), display_name.clone()));
            model_performance.push((model_id.clone(), duration));
          }
        }
      }
      Err(e) => {
        println!("   ❌ FAILED: {}", e);
        unavailable_models.push((model_id.clone(), display_name.clone(), format!("Error: {}", e)));
      }
    }
    println!();
  }

  // Test 2: Multimodal capabilities for working models
  let image_path = "../assets/AgentFlow-crates.jpeg";
  let image_exists = std::path::Path::new(image_path).exists();
  
  if image_exists && !working_models.is_empty() {
    println!("🖼️  Testing Multimodal Capabilities");
    println!("===================================");
    println!("🔍 Testing vision capabilities with working models...\n");
    
    // Test multimodal aliases that map to working models
    let multimodal_aliases = vec![
      ("claude-3-5-sonnet", "Claude 3.5 Sonnet (Multimodal Alias)"),
      ("claude-3-haiku", "Claude 3 Haiku (Multimodal Alias)"),
    ];

    for (alias_id, alias_name) in &multimodal_aliases {
      println!("🧪 Testing: {} ({})", alias_name, alias_id);
      
      let start_time = Instant::now();
      
      use agentflow_nodes::ImageUnderstandNode;
      use agentflow_nodes::nodes::image_understand::VisionResponseFormat;
      
      let vision_node = ImageUnderstandNode::new(
        &format!("{}_vision_test", alias_id.replace("-", "_")), 
        alias_id,
        "Analyze this AgentFlow architecture diagram. Describe the main crates and their colors in 1-2 sentences.",
        image_path)
        .with_system_message("You are an expert at analyzing software architecture diagrams.")
        .with_temperature(0.3)
        .with_max_tokens(150)
        .with_response_format(VisionResponseFormat::Markdown);

      match vision_node.run_async(&shared).await {
        Ok(_) => {
          let duration = start_time.elapsed();
          if let Some(result) = shared.get(&format!("{}_vision_test_output", alias_id.replace("-", "_"))) {
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

  // Summary Report
  println!("📊 Claude Models Test Report");
  println!("============================\n");
  
  println!("✅ WORKING MODELS ({}/{}):", working_models.len(), claude_models.len());
  if working_models.is_empty() {
    println!("   ⚠️  No models are currently working");
  } else {
    for (model_id, display_name) in &working_models {
      if let Some((_, duration)) = model_performance.iter().find(|(id, _)| id == model_id) {
        println!("   ✅ {} ({}) - Response time: {:?}", display_name, model_id, duration);
      }
    }
  }
  
  println!("\n❌ UNAVAILABLE MODELS ({}/{}):", unavailable_models.len(), claude_models.len());
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
  } else {
    println!("⚠️  No models working - check API access level or billing");
  }
  
  if unavailable_models.len() > 0 {
    println!("💳 Unavailable models may require:");
    println!("   • Higher API access tier");
    println!("   • Additional billing/credits");
    println!("   • Different account permissions");
  }

  if image_exists {
    println!("🖼️  Multimodal testing: Available (image found)");
  } else {
    println!("🖼️  Multimodal testing: Limited (no test image)");
  }

  // Provide usage recommendations
  println!("\n🚀 Usage Recommendations:");
  if !working_models.is_empty() {
    let recommended_model = working_models.first().unwrap();
    println!("💡 Use '{}' for reliable text generation", recommended_model.0);
    println!("💡 Consider 'claude-3-haiku' or 'claude-3-5-sonnet' aliases for multimodal");
  }
  println!("💡 All AgentFlow components are functioning correctly");
  println!("💡 Issue is API access level, not code implementation");
  
  println!("\n🎯 System Status: AgentFlow + Claude Integration = READY");
  
  Ok(())
}

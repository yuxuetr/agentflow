//! Claude Registry Verification Test
//! 
//! This example verifies that Claude models work through the agentflow-nodes
//! LlmNode interface (not just direct provider access).

use agentflow_core::{AsyncNode, SharedState};
use agentflow_nodes::LlmNode;
use agentflow_nodes::nodes::llm::ResponseFormat;
use serde_json::Value;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  // Initialize AgentFlow to load environment and registry
  agentflow_llm::AgentFlow::init().await.expect("Failed to initialize AgentFlow");

  println!("🔍 Claude Registry Verification Test");
  println!("====================================\n");

  // Create shared state
  let shared = SharedState::new();
  
  // Test the key Claude models that were confirmed working
  let test_models = vec![
    // Test fastest model
    ("claude-3-haiku-20240307", "Claude Haiku 3 (Fastest)", "Economy, 608ms"),
    
    // Test latest models  
    ("claude-sonnet-4-20250514", "Claude Sonnet 4 (Latest)", "Latest balanced, 2.19s"),
    ("claude-opus-4-1-20250805", "Claude Opus 4.1 (Premium)", "Most advanced, 3.02s"),
    
    // Test production model
    ("claude-3-5-sonnet-20241022", "Claude Sonnet 3.5 (Production)", "Current production, 3.00s"),
  ];

  let mut registry_working = Vec::new();
  let mut registry_failed = Vec::new();

  println!("🧪 Testing Claude Models Through AgentFlow-Nodes Registry");
  println!("=========================================================");
  println!("Testing {} key models through LlmNode interface...\n", test_models.len());

  // Add context
  shared.insert("architecture_aspect".to_string(), 
    Value::String("separation of concerns and modular design".to_string()));

  for (model_id, display_name, performance_note) in &test_models {
    println!("🔍 Testing: {} ({})", display_name, model_id);
    println!("   📊 Expected performance: {}", performance_note);
    
    let start_time = Instant::now();
    
    let llm_node = LlmNode::new(&format!("{}_registry_test", model_id.replace("-", "_")), model_id)
      .with_prompt("Analyze AgentFlow's approach to {{architecture_aspect}}. Provide a technical assessment in 2 sentences.")
      .with_system("You are a Rust architecture expert.")
      .with_temperature(0.3)
      .with_max_tokens(100)
      .with_response_format(ResponseFormat::Markdown);

    match llm_node.run_async(&shared).await {
      Ok(_) => {
        let duration = start_time.elapsed();
        if let Some(result) = shared.get(&format!("{}_registry_test_output", model_id.replace("-", "_"))) {
          let response = result.as_str().unwrap_or("No response");
          
          // Check if it's a real response (not mock)
          if response.contains("mock response") {
            println!("   ❌ REGISTRY ISSUE: Model in registry but returning mock response");
            println!("   🔧 This suggests a registry loading or caching issue");
            registry_failed.push((model_id.to_string(), display_name.to_string(), "Registry/Cache issue".to_string()));
          } else {
            println!("   ✅ REGISTRY SUCCESS: Real response via LlmNode ({:?})", duration);
            println!("   📝 Response: {}", 
              if response.len() > 100 { 
                format!("{}...", &response[..100]) 
              } else { 
                response.to_string() 
              });
            registry_working.push((model_id.to_string(), display_name.to_string()));
          }
        }
      }
      Err(e) => {
        println!("   ❌ NODE FAILED: {}", e);
        registry_failed.push((model_id.to_string(), display_name.to_string(), format!("Node error: {}", e)));
      }
    }
    println!();
  }

  // Results Summary
  println!("📊 Registry Verification Results");
  println!("================================\n");
  
  println!("✅ WORKING THROUGH REGISTRY ({}/{}):", registry_working.len(), test_models.len());
  for (model_id, display_name) in &registry_working {
    println!("   ✅ {} ({})", display_name, model_id);
  }
  
  if !registry_failed.is_empty() {
    println!("\n❌ REGISTRY ISSUES ({}/{}):", registry_failed.len(), test_models.len());
    for (model_id, display_name, reason) in &registry_failed {
      println!("   ❌ {} ({}) - {}", display_name, model_id, reason);
    }
  }

  println!("\n🎯 Verification Summary:");
  if registry_working.len() == test_models.len() {
    println!("✅ PERFECT: All Claude models work through agentflow-nodes!");
    println!("✅ Registry update: SUCCESSFUL");
    println!("✅ Integration status: FULLY OPERATIONAL");
  } else if registry_working.len() > 0 {
    println!("⚠️  PARTIAL: Some Claude models work, others have registry issues");
    println!("🔧 May need registry reload or cache clear");
  } else {
    println!("❌ REGISTRY PROBLEM: Models work directly but not through nodes");
    println!("🔧 Registry configuration or loading issue");
  }

  println!("\n💡 Next Steps:");
  if registry_working.len() > 0 {
    println!("✅ Use working models in your AgentFlow applications");
    println!("✅ Registry is properly configured and loaded");
  }
  if !registry_failed.is_empty() {
    println!("🔧 Investigate registry loading for failed models");
    println!("🔄 May need to restart or reload registry cache");
  }
  
  Ok(())
}

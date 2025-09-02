//! Claude Comprehensive Test and Registry Update
//! 
//! This example tests ALL Claude models directly with the provider,
//! identifies working models, and updates the registry accordingly.

use agentflow_llm::providers::{AnthropicProvider, LLMProvider, ProviderRequest};
use serde_json::json;
use std::collections::HashMap;
use tokio::io::AsyncWriteExt;
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("🧠 Claude Comprehensive Test & Registry Update");
  println!("==============================================\n");

  // Initialize AgentFlow to load environment variables
  agentflow_llm::AgentFlow::init().await.expect("Failed to initialize AgentFlow");

  // Get API key from environment
  let api_key = std::env::var("ANTHROPIC_API_KEY")
    .expect("ANTHROPIC_API_KEY must be set");
  
  println!("✅ Found API key: {}...", &api_key[..20]);

  // Create provider directly
  let provider = AnthropicProvider::new(&api_key, None)?;
  println!("✅ Created Anthropic provider\n");

  // Define ALL Claude models from your API response
  let all_claude_models = vec![
    // Claude 4 Series - Latest Generation
    ("claude-opus-4-1-20250805", "Claude Opus 4.1", "Most advanced reasoning"),
    ("claude-opus-4-20250514", "Claude Opus 4", "Advanced reasoning"),
    ("claude-sonnet-4-20250514", "Claude Sonnet 4", "Latest balanced model"),
    
    // Claude 3.7 Series
    ("claude-3-7-sonnet-20250219", "Claude Sonnet 3.7", "Enhanced 3.5"),
    
    // Claude 3.5 Series - Current Production
    ("claude-3-5-sonnet-20241022", "Claude Sonnet 3.5 (New)", "Production balanced"),
    ("claude-3-5-haiku-20241022", "Claude Haiku 3.5", "Fast and efficient"), 
    ("claude-3-5-sonnet-20240620", "Claude Sonnet 3.5 (Old)", "Legacy balanced"),
    
    // Claude 3 Series - Legacy
    ("claude-3-haiku-20240307", "Claude Haiku 3", "Economy option"),
    ("claude-3-opus-20240229", "Claude Opus 3", "Legacy premium"),
  ];

  let mut working_models = Vec::new();
  let mut unavailable_models = Vec::new();
  let mut performance_data = Vec::new();

  println!("🧪 PHASE 1: Direct Provider Testing");
  println!("===================================");
  println!("Testing {} models directly with Anthropic provider...\n", all_claude_models.len());

  for (model_id, display_name, description) in &all_claude_models {
    println!("🔍 Testing: {} ({})", display_name, model_id);
    println!("   📝 Description: {}", description);
    
    let start_time = Instant::now();
    
    // Create test request
    let request = ProviderRequest {
      model: model_id.to_string(),
      messages: vec![
        json!({"role": "user", "content": "Hello! Test AgentFlow integration. Respond with exactly one sentence about Rust modular architecture."})
      ],
      stream: false,
      parameters: {
        let mut params = HashMap::new();
        params.insert("max_tokens".to_string(), json!(50));
        params.insert("temperature".to_string(), json!(0.3));
        params
      },
    };

    match provider.execute(&request).await {
      Ok(response) => {
        let duration = start_time.elapsed();
        match &response.content {
          agentflow_llm::providers::ContentType::Text(text) => {
            println!("   ✅ WORKING: Real response received ({:?})", duration);
            println!("   📝 Response: {}", 
              if text.len() > 100 { 
                format!("{}...", &text[..100]) 
              } else { 
                text.clone() 
              });
            
            if let Some(usage) = &response.usage {
              println!("   📊 Tokens: prompt={:?}, completion={:?}", 
                usage.prompt_tokens, usage.completion_tokens);
            }
            
            working_models.push((model_id.to_string(), display_name.to_string(), description.to_string()));
            performance_data.push((model_id.to_string(), duration, text.len()));
          }
          _ => {
            println!("   ⚠️  Unexpected content type received");
          }
        }
      }
      Err(e) => {
        println!("   ❌ UNAVAILABLE: {}", e);
        unavailable_models.push((model_id.to_string(), display_name.to_string(), format!("{}", e)));
      }
    }
    println!();
  }

  // Phase 2: Summary and Registry Update
  println!("📊 PHASE 2: Results Analysis");
  println!("============================\n");
  
  println!("✅ WORKING MODELS ({}/{}):", working_models.len(), all_claude_models.len());
  if working_models.is_empty() {
    println!("   ⚠️  No models are working with your current API access");
  } else {
    for (model_id, display_name, description) in &working_models {
      if let Some((_, duration, response_len)) = performance_data.iter().find(|(id, _, _)| id == model_id) {
        println!("   ✅ {} ({}) - {:?}, {} chars", display_name, model_id, duration, response_len);
        println!("      💡 {}", description);
      }
    }
  }
  
  println!("\n❌ UNAVAILABLE MODELS ({}/{}):", unavailable_models.len(), all_claude_models.len());
  if unavailable_models.is_empty() {
    println!("   🎉 All models are working!");
  } else {
    for (model_id, display_name, reason) in &unavailable_models {
      println!("   ❌ {} ({}) - {}", display_name, model_id, reason);
    }
  }

  // Phase 3: Registry Update
  if !working_models.is_empty() {
    println!("\n🔧 PHASE 3: Registry Update");
    println!("===========================");
    println!("Adding working models to ~/.agentflow/models.yml...\n");
    
    // Read current models.yml
    let models_content = tokio::fs::read_to_string("~/.agentflow/models.yml".replace("~", &std::env::var("HOME")?)).await?;
    
    for (model_id, display_name, description) in &working_models {
      // Check if model already exists
      if models_content.contains(model_id) {
        println!("   ✅ {} already in registry", display_name);
      } else {
        println!("   ➕ Adding {} to registry", display_name);
        
        // Add model configuration
        let model_config = format!(r#"  {}:
    vendor: anthropic
    type: text
    model_id: null
    base_url: null
    temperature: 0.6
    top_p: null
    max_tokens: 8192
    frequency_penalty: null
    stop: null
    n: null
    supports_streaming: true
    supports_tools: true
    supports_multimodal: true
    response_format: null
"#, model_id);

        let home_dir = std::env::var("HOME")?;
        let models_path = format!("{}/.agentflow/models.yml", home_dir);
        
        // Append to models.yml
        tokio::fs::OpenOptions::new()
          .create(true)
          .append(true)
          .open(&models_path)
          .await?
          .write_all(model_config.as_bytes())
          .await?;
      }
    }
  }

  // Phase 4: Performance Analysis
  println!("\n⚡ PHASE 4: Performance Analysis");
  println!("================================");
  
  if !performance_data.is_empty() {
    // Find fastest model
    if let Some((fastest_model, fastest_time, _)) = performance_data.iter().min_by_key(|(_, duration, _)| *duration) {
      if let Some((_, display_name, _)) = working_models.iter().find(|(id, _, _)| id == fastest_model) {
        println!("🏃 Fastest model: {} ({:?})", display_name, fastest_time);
      }
    }
    
    // Find most verbose model
    if let Some((verbose_model, _, max_chars)) = performance_data.iter().max_by_key(|(_, _, chars)| *chars) {
      if let Some((_, display_name, _)) = working_models.iter().find(|(id, _, _)| id == verbose_model) {
        println!("💬 Most detailed: {} ({} chars)", display_name, max_chars);
      }
    }
    
    // Calculate average performance
    let avg_time = performance_data.iter().map(|(_, duration, _)| duration.as_millis()).sum::<u128>() / performance_data.len() as u128;
    println!("📊 Average response time: {}ms", avg_time);
  }

  // Final recommendations
  println!("\n🎯 FINAL RECOMMENDATIONS");
  println!("========================");
  
  if !working_models.is_empty() {
    println!("✅ AgentFlow + Claude Integration: FULLY OPERATIONAL");
    println!("✅ Environment configuration: CORRECT");
    println!("✅ Provider implementation: WORKING");
    
    let recommended_model = working_models.first().unwrap();
    println!("\n💡 Recommended models for different use cases:");
    
    // Find best model for each use case
    if let Some((_, haiku_name, _)) = working_models.iter().find(|(id, _, _)| id.contains("haiku")) {
      println!("   🏃 Fast tasks: {} (fastest response)", haiku_name);
    }
    if let Some((_, sonnet_name, _)) = working_models.iter().find(|(id, _, _)| id.contains("sonnet")) {
      println!("   ⚖️  Balanced tasks: {} (speed + quality)", sonnet_name);
    }
    if let Some((_, opus_name, _)) = working_models.iter().find(|(id, _, _)| id.contains("opus")) {
      println!("   🧠 Complex tasks: {} (best reasoning)", opus_name);
    }
    
    println!("\n🔧 Usage in AgentFlow:");
    println!("   • LlmNode::new(\"my_node\", \"{}\")", recommended_model.0);
    println!("   • ImageUnderstandNode with Claude multimodal aliases");
    
  } else {
    println!("⚠️  No Claude models available with current API access");
    println!("💳 Consider upgrading API tier or checking billing");
  }

  println!("\n🏁 Comprehensive test completed!");
  println!("   📈 Tested: {} models", all_claude_models.len());
  println!("   ✅ Working: {} models", working_models.len());
  println!("   ❌ Unavailable: {} models", unavailable_models.len());
  
  Ok(())
}

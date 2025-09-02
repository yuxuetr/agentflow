//! Debug Full Request Flow
//! 
//! This traces the complete request flow in the fluent API

use agentflow_llm::registry::ModelRegistry;
use agentflow_llm::config::LLMConfig;
use agentflow_llm::providers::{ProviderRequest, LLMProvider};
use std::collections::HashMap;
use serde_json::{json, Value};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Debug Full Request Flow");
    println!("===========================\n");

    // Initialize AgentFlow 
    agentflow_llm::AgentFlow::init().await?;
    
    let registry = ModelRegistry::global();
    
    // Step 1: Model lookup (this works)
    println!("🔧 Step 1: Model Lookup");
    let model_config = registry.get_model("claude-3-haiku-20240307")?;
    println!("✅ Model found: claude-3-haiku-20240307");
    println!("   Vendor: {}", model_config.vendor);
    
    // Step 2: Provider lookup (this works)
    println!("\n🔧 Step 2: Provider Lookup");
    let provider = registry.get_provider(&model_config.vendor)?;
    println!("✅ Provider found: {}", model_config.vendor);
    
    // Step 3: Manually build the request exactly like the fluent API does
    println!("\n🔧 Step 3: Manual Request Building (mimicking fluent API)");
    
    let mut params = HashMap::new();
    
    // Add temperature (fluent API sets this)
    params.insert(
        "temperature".to_string(),
        Value::Number(serde_json::Number::from_f64(0.1f64).unwrap()),
    );
    
    // Add max_tokens (fluent API sets this)
    params.insert(
        "max_tokens".to_string(),
        Value::Number(serde_json::Number::from(15)),
    );
    
    // Build messages like the fluent API does
    let messages = vec![json!({
        "role": "user",
        "content": "What is 2+2? Answer briefly."
    })];
    
    // The critical line - determine the model name exactly like fluent API does
    let request_model_name = model_config
        .model_id
        .clone()
        .unwrap_or_else(|| "claude-3-haiku-20240307".to_string());
    
    println!("📋 Request details being built:");
    println!("   Model name: '{}'", request_model_name);
    println!("   Messages: {:?}", messages);
    println!("   Parameters: {:?}", params);
    
    let manual_request = ProviderRequest {
        model: request_model_name.clone(),
        messages,
        stream: false,
        parameters: params,
    };
    
    // Step 4: Test this exact request
    println!("\n🔧 Step 4: Testing Manual Request");
    match provider.execute(&manual_request).await {
        Ok(response) => {
            println!("✅ Manual request: SUCCESS");
            println!("   Response: {:?}", response.content);
        }
        Err(e) => {
            println!("❌ Manual request: FAILED - {}", e);
            
            // Check if the model name in our request matches what Anthropic expects
            if let agentflow_llm::LLMError::HttpError { status_code, message } = &e {
                if *status_code == 404 {
                    println!("\n🔍 HTTP 404 Analysis:");
                    println!("   Model sent to API: '{}'", request_model_name);
                    println!("   Expected by Anthropic: 'claude-3-haiku-20240307'");
                    
                    if request_model_name != "claude-3-haiku-20240307" {
                        println!("   ❌ MODEL NAME MISMATCH - This is the issue!");
                    } else {
                        println!("   ✅ Model names match - issue might be elsewhere");
                    }
                    
                    println!("   Raw error: {}", message);
                }
            }
        }
    }
    
    // Step 5: Compare with working direct provider call
    println!("\n🔧 Step 5: Test Known Working Request");
    
    let working_request = ProviderRequest {
        model: "claude-3-haiku-20240307".to_string(), // Hardcoded working name
        messages: vec![json!({
            "role": "user", 
            "content": "What is 2+2? Answer briefly."
        })],
        stream: false,
        parameters: {
            let mut p = HashMap::new();
            p.insert("max_tokens".to_string(), json!(15));
            p.insert("temperature".to_string(), json!(0.1));
            p
        },
    };
    
    match provider.execute(&working_request).await {
        Ok(response) => {
            println!("✅ Known working request: SUCCESS");
            println!("   Response: {:?}", response.content);
        }
        Err(e) => {
            println!("❌ Known working request: FAILED - {}", e);
        }
    }

    println!("\n🎯 Analysis:");
    println!("- Compare the manual request (failing) with working request");
    println!("- Any differences will show us the exact issue");

    Ok(())
}

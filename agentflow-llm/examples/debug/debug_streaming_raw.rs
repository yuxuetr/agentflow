//! Debug Raw Streaming Response
//! 
//! Debug the raw streaming data to see what's coming from Anthropic

use agentflow_llm::registry::ModelRegistry;
use agentflow_llm::config::LLMConfig;
use agentflow_llm::providers::{ProviderRequest, LLMProvider};
use std::collections::HashMap;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Debug Raw Streaming Response");
    println!("================================\n");

    // Initialize AgentFlow
    agentflow_llm::AgentFlow::init().await?;
    
    // Get the provider directly
    let registry = ModelRegistry::global();
    let model_config = registry.get_model("claude-3-haiku-20240307")?;
    let provider = registry.get_provider(&model_config.vendor)?;
    
    // Create a streaming request manually
    let mut params = HashMap::new();
    params.insert("max_tokens".to_string(), json!(50));
    params.insert("temperature".to_string(), json!(0.3));
    
    let streaming_request = ProviderRequest {
        model: "claude-3-haiku-20240307".to_string(),
        messages: vec![json!({
            "role": "user", 
            "content": "Count to 5, slowly."
        })],
        stream: true,  // Explicitly enable streaming
        parameters: params,
    };
    
    println!("📋 Request details:");
    println!("   Model: {}", streaming_request.model);
    println!("   Stream: {}", streaming_request.stream);
    println!("   Messages: {:?}", streaming_request.messages);
    
    // Test streaming
    println!("\n🔧 Testing Raw Streaming Response:");
    match provider.execute_streaming(&streaming_request).await {
        Ok(mut stream) => {
            println!("✅ Streaming started successfully");
            
            let mut chunk_count = 0;
            let mut content_received = String::new();
            
            loop {
                match stream.next_chunk().await {
                    Ok(Some(chunk)) => {
                        chunk_count += 1;
                        println!("   📦 Chunk #{}: ", chunk_count);
                        println!("      Content: '{}'", chunk.content);
                        println!("      Is final: {}", chunk.is_final);
                        println!("      Content length: {}", chunk.content.len());
                        
                        if let Some(metadata) = &chunk.metadata {
                            println!("      Metadata: {}", metadata);
                        }
                        
                        content_received.push_str(&chunk.content);
                        
                        if chunk.is_final {
                            println!("   🔚 Final chunk received");
                            break;
                        }
                    }
                    Ok(None) => {
                        println!("   🔚 Stream ended (None received)");
                        break;
                    }
                    Err(e) => {
                        println!("   ❌ Streaming error: {}", e);
                        break;
                    }
                }
            }
            
            println!("\n📊 Summary:");
            println!("   Total chunks: {}", chunk_count);
            println!("   Total content: '{}' ({} chars)", content_received, content_received.len());
        }
        Err(e) => {
            println!("❌ Streaming failed: {}", e);
        }
    }
    
    // Compare with non-streaming
    println!("\n🔧 Comparison: Non-Streaming Same Request:");
    
    let non_streaming_request = ProviderRequest {
        model: "claude-3-haiku-20240307".to_string(),
        messages: vec![json!({
            "role": "user", 
            "content": "Count to 5, slowly."
        })],
        stream: false,  // Non-streaming
        parameters: {
            let mut p = HashMap::new();
            p.insert("max_tokens".to_string(), json!(50));
            p.insert("temperature".to_string(), json!(0.3));
            p
        },
    };
    
    match provider.execute(&non_streaming_request).await {
        Ok(response) => {
            println!("✅ Non-streaming: SUCCESS");
            println!("   Content: '{}'", format!("{:?}", response.content));
            println!("   Length: {} characters", format!("{:?}", response.content).len());
        }
        Err(e) => {
            println!("❌ Non-streaming: FAILED - {}", e);
        }
    }

    Ok(())
}

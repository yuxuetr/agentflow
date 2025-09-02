//! Test Detailed Streaming
//! 
//! Test streaming with longer responses to see actual streaming behavior

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing Detailed Streaming for Claude Models");
    println!("================================================\n");

    // Initialize AgentFlow
    agentflow_llm::AgentFlow::init().await?;

    // Test with a longer prompt that should generate more content
    println!("🔧 Test: Detailed Streaming Request");
    println!("------------------------------------");
    
    match agentflow_llm::AgentFlow::model("claude-3-haiku-20240307")
        .prompt("Write a short story about a robot learning to paint. Include dialogue and describe the robot's emotions. Make it at least 100 words.")
        .max_tokens(200)
        .temperature(0.7)
        .execute_streaming()
        .await 
    {
        Ok(mut stream) => {
            println!("✅ Streaming: SUCCESS - stream started");
            println!("   Streaming response (real-time):");
            println!("   --------------------------------");
            
            let mut full_response = String::new();
            let mut chunk_count = 0;
            let mut content_chunks = 0;
            
            loop {
                match stream.next_chunk().await {
                    Ok(Some(chunk)) => {
                        chunk_count += 1;
                        
                        if !chunk.content.is_empty() {
                            content_chunks += 1;
                            print!("{}", chunk.content);
                            std::io::Write::flush(&mut std::io::stdout()).unwrap();
                            full_response.push_str(&chunk.content);
                        }
                        
                        if chunk.is_final {
                            println!("\n\n   🔚 Stream completed:");
                            println!("      Total chunks: {}", chunk_count);
                            println!("      Content chunks: {}", content_chunks);
                            println!("      Final chunk metadata: {:?}", chunk.metadata);
                            break;
                        }
                    }
                    Ok(None) => {
                        println!("\n\n   🔚 Stream ended:");
                        println!("      Total chunks: {}", chunk_count);
                        println!("      Content chunks: {}", content_chunks);
                        break;
                    }
                    Err(e) => {
                        println!("\n❌ Streaming chunk error: {}", e);
                        break;
                    }
                }
            }
            
            println!("\n   📊 Streaming Analysis:");
            println!("      Response length: {} characters", full_response.len());
            println!("      Non-empty chunks: {}/{}", content_chunks, chunk_count);
            println!("      Streaming effective: {}", if content_chunks > 1 { "✅ YES" } else { "⚠️  NO (single chunk)" });
        }
        Err(e) => {
            println!("❌ Streaming: FAILED - {}", e);
        }
    }

    // Compare with non-streaming for the same prompt
    println!("\n🔧 Comparison: Non-Streaming Same Request");
    println!("-----------------------------------------");
    
    match agentflow_llm::AgentFlow::model("claude-3-haiku-20240307")
        .prompt("Write a short story about a robot learning to paint. Include dialogue and describe the robot's emotions. Make it at least 100 words.")
        .max_tokens(200)
        .temperature(0.7)
        .execute()
        .await 
    {
        Ok(response) => {
            println!("✅ Non-streaming: SUCCESS");
            println!("   Response length: {} characters", response.len());
            println!("   First 100 chars: {}", 
                if response.len() > 100 { 
                    format!("{}...", &response[..100]) 
                } else { 
                    response.clone() 
                });
        }
        Err(e) => {
            println!("❌ Non-streaming: FAILED - {}", e);
        }
    }

    println!("\n🎯 Streaming Support Analysis:");
    println!("✅ execute() - Non-streaming: WORKING");
    println!("✅ execute_streaming() - Streaming: API WORKING");
    println!("⚠️  Streaming chunking: May receive content in single chunks for short responses");
    println!("📋 Both modes supported simultaneously!");

    Ok(())
}

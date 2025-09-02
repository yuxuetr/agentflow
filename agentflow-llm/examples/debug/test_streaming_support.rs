//! Test Streaming Support
//! 
//! Verify that AgentFlow supports both non-streaming and streaming for Claude models

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing Streaming Support for Claude Models");
    println!("===============================================\n");

    // Initialize AgentFlow
    agentflow_llm::AgentFlow::init().await?;

    // Test 1: Non-streaming (we know this works now)
    println!("🔧 Test 1: Non-Streaming Request");
    println!("--------------------------------");
    
    match agentflow_llm::AgentFlow::model("claude-3-haiku-20240307")
        .prompt("List 3 programming languages. Be concise.")
        .max_tokens(50)
        .temperature(0.3)
        .execute()
        .await 
    {
        Ok(response) => {
            println!("✅ Non-streaming: SUCCESS");
            println!("   Response: {}", response);
        }
        Err(e) => {
            println!("❌ Non-streaming: FAILED - {}", e);
        }
    }
    
    // Test 2: Streaming 
    println!("\n🔧 Test 2: Streaming Request");
    println!("----------------------------");
    
    match agentflow_llm::AgentFlow::model("claude-3-haiku-20240307")
        .prompt("Count from 1 to 5, one number per line.")
        .max_tokens(30)
        .temperature(0.1)
        .execute_streaming()
        .await 
    {
        Ok(mut stream) => {
            println!("✅ Streaming: SUCCESS - stream started");
            println!("   Streaming response:");
            
            let mut full_response = String::new();
            let mut chunk_count = 0;
            
            loop {
                match stream.next_chunk().await {
                    Ok(Some(chunk)) => {
                        chunk_count += 1;
                        print!("{}", chunk.content);
                        full_response.push_str(&chunk.content);
                        
                        if chunk.is_final {
                            println!("\n   🔚 Stream completed (received {} chunks)", chunk_count);
                            break;
                        }
                    }
                    Ok(None) => {
                        println!("\n   🔚 Stream ended (received {} chunks)", chunk_count);
                        break;
                    }
                    Err(e) => {
                        println!("\n❌ Streaming chunk error: {}", e);
                        break;
                    }
                }
            }
            
            if !full_response.is_empty() {
                println!("   📝 Complete response: '{}'", full_response.trim());
            }
        }
        Err(e) => {
            println!("❌ Streaming: FAILED - {}", e);
        }
    }
    
    // Test 3: Multiple Claude models streaming support
    println!("\n🔧 Test 3: Multiple Models Streaming Support");
    println!("--------------------------------------------");
    
    let test_models = vec![
        ("claude-3-haiku-20240307", "Claude Haiku 3"),
        ("claude-3-5-sonnet-20241022", "Claude Sonnet 3.5"),
        ("claude-sonnet-4-20250514", "Claude Sonnet 4"),
    ];
    
    for (model_id, model_name) in test_models {
        println!("Testing streaming with {}", model_name);
        
        match agentflow_llm::AgentFlow::model(model_id)
            .prompt("Say 'Hello from streaming!'")
            .max_tokens(10)
            .execute_streaming()
            .await 
        {
            Ok(mut stream) => {
                println!("   ✅ {} streaming: Started", model_name);
                
                // Just read the first chunk to verify it works
                match stream.next_chunk().await {
                    Ok(Some(chunk)) => {
                        println!("   📝 First chunk: '{}'", chunk.content.trim());
                    }
                    Ok(None) => {
                        println!("   📝 Stream ended immediately (no content)");
                    }
                    Err(e) => {
                        println!("   ❌ Chunk error: {}", e);
                    }
                }
            }
            Err(e) => {
                println!("   ❌ {} streaming: FAILED - {}", model_name, e);
            }
        }
    }

    // Test 4: Manual collect streaming chunks 
    println!("\n🔧 Test 4: Manual Streaming Collection");
    println!("--------------------------------------");
    
    match agentflow_llm::AgentFlow::model("claude-3-haiku-20240307")
        .prompt("Say exactly: 'Streaming works!'")
        .max_tokens(20)
        .execute_streaming()
        .await 
    {
        Ok(mut stream) => {
            println!("✅ Stream started, manually collecting chunks...");
            
            let mut collected = String::new();
            let mut total_chunks = 0;
            
            loop {
                match stream.next_chunk().await {
                    Ok(Some(chunk)) => {
                        total_chunks += 1;
                        collected.push_str(&chunk.content);
                        
                        if chunk.is_final {
                            break;
                        }
                    }
                    Ok(None) => {
                        break;
                    }
                    Err(e) => {
                        println!("❌ Collection error: {}", e);
                        break;
                    }
                }
            }
            
            println!("✅ Manual collection: SUCCESS");
            println!("   Total chunks: {}", total_chunks);
            println!("   Complete response: '{}'", collected.trim());
        }
        Err(e) => {
            println!("❌ Stream creation: FAILED - {}", e);
        }
    }

    println!("\n🎯 Summary:");
    println!("- Non-streaming: Check if standard execute() works");
    println!("- Streaming: Check if execute_streaming() works");  
    println!("- Multiple models: Check if streaming works across different Claude models");
    println!("- Manual collection: Check if streaming chunks can be collected manually");
    println!("- If all work, AgentFlow has full streaming + non-streaming support!");

    Ok(())
}

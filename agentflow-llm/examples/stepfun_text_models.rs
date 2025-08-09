/// StepFun Text Models - Real API Test Cases
/// 
/// Tests actual text completion models with both streaming and non-streaming modes.
/// Demonstrates real requests and responses from StepFun API.
/// 
/// Usage:
/// ```bash
/// export STEP_API_KEY="your-stepfun-api-key-here"
/// cargo run --example stepfun_text_models
/// ```

use reqwest::Client;
use serde_json::{json, Value};
use std::env;
use tokio::time::{timeout, Duration};
use futures_util::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    let api_key = env::var("STEP_API_KEY")
        .expect("STEP_API_KEY environment variable is required");

    println!("🚀 StepFun Text Models - Real API Tests");
    println!("======================================\n");

    // Test different text models
    test_step_2_16k(&api_key).await?;
    test_step_1_32k_streaming(&api_key).await?;
    test_step_2_mini(&api_key).await?;
    test_step_1_256k_long_context(&api_key).await?;
    
    println!("✅ All text model tests completed successfully!");
    Ok(())
}

/// Test step-2-16k with non-streaming request
async fn test_step_2_16k(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("📝 Testing step-2-16k (Non-streaming)");
    println!("Model: step-2-16k | Max tokens: 16K | Task: Code generation\n");

    let client = Client::new();
    let request_body = json!({
        "model": "step-2-16k",
        "messages": [
            {
                "role": "system",
                "content": "你是一个专业的Python程序员，擅长编写高质量、易读的代码。"
            },
            {
                "role": "user",
                "content": "用Python实现一个快速排序算法，要求包含详细注释和测试用例。"
            }
        ],
        "max_tokens": 1000,
        "temperature": 0.7,
        "top_p": 0.9
    });

    println!("📤 Sending request to step-2-16k...");
    let start_time = std::time::Instant::now();
    
    let response = client
        .post("https://api.stepfun.com/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request_body)
        .send()
        .await?;

    let duration = start_time.elapsed();
    let status = response.status();
    
    if !status.is_success() {
        let error_text = response.text().await?;
        eprintln!("❌ Request failed with status {}: {}", status, error_text);
        return Err(format!("HTTP {}", status).into());
    }

    let response_json: Value = response.json().await?;
    
    println!("📥 Response received in {:?}", duration);
    println!("📊 Response details:");
    
    if let Some(usage) = response_json.get("usage") {
        println!("   Prompt tokens: {}", usage.get("prompt_tokens").unwrap_or(&json!(0)));
        println!("   Completion tokens: {}", usage.get("completion_tokens").unwrap_or(&json!(0)));
        println!("   Total tokens: {}", usage.get("total_tokens").unwrap_or(&json!(0)));
    }

    if let Some(choices) = response_json.get("choices").and_then(|c| c.as_array()) {
        if let Some(first_choice) = choices.first() {
            if let Some(message) = first_choice.get("message") {
                if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                    println!("📝 Generated code (first 500 chars):");
                    println!("{}", truncate_string(content, 500));
                    
                    // Validate response contains expected elements
                    let content_lower = content.to_lowercase();
                    let has_quicksort = content_lower.contains("快速排序") || content_lower.contains("quicksort");
                    let has_function = content_lower.contains("def ");
                    let has_comments = content_lower.contains("#");
                    
                    println!("✅ Response validation:");
                    println!("   Contains quicksort logic: {}", has_quicksort);
                    println!("   Contains function definition: {}", has_function);
                    println!("   Contains comments: {}", has_comments);
                }
            }
        }
    }

    println!(); // Empty line for spacing
    Ok(())
}

/// Test step-1-32k with streaming request
async fn test_step_1_32k_streaming(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("🌊 Testing step-1-32k (Streaming)");
    println!("Model: step-1-32k | Max tokens: 32K | Task: Detailed explanation\n");

    let client = Client::new();
    let request_body = json!({
        "model": "step-1-32k",
        "messages": [
            {
                "role": "user",
                "content": "详细解释量子计算的基本原理，包括量子比特、叠加态、纠缠现象，以及它与经典计算的主要区别。"
            }
        ],
        "stream": true,
        "max_tokens": 800,
        "temperature": 0.8
    });

    println!("📤 Sending streaming request to step-1-32k...");
    let start_time = std::time::Instant::now();
    
    let response = client
        .post("https://api.stepfun.com/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request_body)
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        eprintln!("❌ Streaming request failed: {}", error_text);
        return Err("Streaming request failed".into());
    }

    println!("📥 Receiving streaming response:");
    print!("   ");
    
    let mut stream = response.bytes_stream();
    let mut full_content = String::new();
    let mut chunk_count = 0;
    
    // Process streaming response with timeout
    while let Ok(Some(chunk_result)) = timeout(Duration::from_secs(30), stream.next()).await {
        match chunk_result {
            Ok(chunk) => {
                let chunk_str = String::from_utf8_lossy(&chunk);
                
                // Process each line in the chunk
                for line in chunk_str.lines() {
                    if line.starts_with("data: ") {
                        let data = &line[6..]; // Remove "data: " prefix
                        
                        if data.trim() == "[DONE]" {
                            let duration = start_time.elapsed();
                            println!("\n\n🏁 Streaming completed in {:?}", duration);
                            println!("📊 Stream statistics:");
                            println!("   Total chunks received: {}", chunk_count);
                            println!("   Total content length: {} chars", full_content.len());
                            println!("   Average chunk size: {:.1} chars", 
                                   if chunk_count > 0 { full_content.len() as f32 / chunk_count as f32 } else { 0.0 });
                            
                            // Show content preview
                            println!("📝 Content preview (first 300 chars):");
                            println!("{}", truncate_string(&full_content, 300));
                            
                            // Validate streaming content
                            let content_lower = full_content.to_lowercase();
                            let has_quantum = content_lower.contains("量子") || content_lower.contains("quantum");
                            let has_qubit = content_lower.contains("量子比特") || content_lower.contains("qubit");
                            let has_superposition = content_lower.contains("叠加") || content_lower.contains("superposition");
                            
                            println!("✅ Content validation:");
                            println!("   Contains quantum concepts: {}", has_quantum);
                            println!("   Contains qubit explanation: {}", has_qubit);
                            println!("   Contains superposition: {}", has_superposition);
                            
                            println!(); // Empty line for spacing
                            return Ok(());
                        }
                        
                        // Parse streaming JSON
                        if let Ok(json_data) = serde_json::from_str::<Value>(data) {
                            if let Some(choices) = json_data.get("choices").and_then(|c| c.as_array()) {
                                if let Some(choice) = choices.first() {
                                    if let Some(delta) = choice.get("delta") {
                                        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                            print!("{}", content);
                                            full_content.push_str(content);
                                            chunk_count += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("\n❌ Stream error: {}", e);
                break;
            }
        }
    }

    Ok(())
}

/// Test step-2-mini with simple question
async fn test_step_2_mini(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("⚡ Testing step-2-mini (Fast model)");
    println!("Model: step-2-mini | Max tokens: 8K | Task: Quick reasoning\n");

    let client = Client::new();
    let request_body = json!({
        "model": "step-2-mini",
        "messages": [
            {
                "role": "user",
                "content": "解释为什么天空是蓝色的？用简单易懂的语言回答。"
            }
        ],
        "max_tokens": 300,
        "temperature": 0.5
    });

    println!("📤 Sending request to step-2-mini...");
    let start_time = std::time::Instant::now();
    
    let response = client
        .post("https://api.stepfun.com/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request_body)
        .send()
        .await?;

    let duration = start_time.elapsed();
    
    if !response.status().is_success() {
        let error_text = response.text().await?;
        eprintln!("❌ Request failed: {}", error_text);
        return Err("Request failed".into());
    }

    let response_json: Value = response.json().await?;
    
    println!("📥 Response received in {:?} (fast!)", duration);
    
    if let Some(choices) = response_json.get("choices").and_then(|c| c.as_array()) {
        if let Some(first_choice) = choices.first() {
            if let Some(message) = first_choice.get("message") {
                if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                    println!("📝 Explanation:");
                    println!("{}", content);
                    
                    // Validate response quality
                    let content_lower = content.to_lowercase();
                    let has_light = content_lower.contains("光") || content_lower.contains("light");
                    let has_scattering = content_lower.contains("散射") || content_lower.contains("scattering");
                    let has_blue = content_lower.contains("蓝") || content_lower.contains("blue");
                    
                    println!("✅ Response validation:");
                    println!("   Mentions light: {}", has_light);
                    println!("   Explains scattering: {}", has_scattering);
                    println!("   Addresses blue color: {}", has_blue);
                    println!("   Response length: {} chars", content.len());
                }
            }
        }
    }

    println!(); // Empty line for spacing
    Ok(())
}

/// Test step-1-256k with long context
async fn test_step_1_256k_long_context(api_key: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("📚 Testing step-1-256k (Long context)");
    println!("Model: step-1-256k | Max tokens: 256K | Task: Document analysis\n");

    // Create a longer context to test the model's capabilities
    let long_context = r#"
    人工智能（Artificial Intelligence，简称AI）是计算机科学的一个分支，它试图理解智能的实质，
    并生产出一种新的能以人类智能相似的方式做出反应的智能机器。该领域的研究包括机器人、
    语言识别、图像识别、自然语言处理和专家系统等。自从人工智能诞生以来，理论和技术日益成熟，
    应用领域也不断扩大。可以设想，未来人工智能带来的科技产品，将会是人类智慧的"容器"。
    
    机器学习是人工智能的核心，是使计算机具有智能的根本途径。机器学习是一门多领域交叉学科，
    涉及概率论、统计学、逼近论、凸分析、算法复杂度理论等多门学科。专门研究计算机怎样模拟或实现人类的学习行为，
    以获取新的知识或技能，重新组织已有的知识结构使之不断改善自身的性能。
    
    深度学习是机器学习的一个分支，它基于人工神经网络。深度学习的概念由Hinton等人于2006年提出。
    深度学习通过建立、模拟人脑进行分析学习的神经网络，它模仿人脑的机制来解释数据，例如图像、声音和文本。
    "#.repeat(3); // Repeat to create longer context

    let client = Client::new();
    let request_body = json!({
        "model": "step-1-256k",
        "messages": [
            {
                "role": "system",
                "content": format!("你是一个AI专家，请基于以下文档内容回答问题：\n\n{}", long_context)
            },
            {
                "role": "user",
                "content": "根据上述文档，总结人工智能、机器学习和深度学习之间的关系，并解释它们的发展脉络。"
            }
        ],
        "max_tokens": 600,
        "temperature": 0.7
    });

    println!("📤 Sending long context request to step-1-256k...");
    println!("   Context length: {} chars", long_context.len());
    let start_time = std::time::Instant::now();
    
    let response = client
        .post("https://api.stepfun.com/v1/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request_body)
        .send()
        .await?;

    let duration = start_time.elapsed();
    
    if !response.status().is_success() {
        let error_text = response.text().await?;
        eprintln!("❌ Long context request failed: {}", error_text);
        return Err("Request failed".into());
    }

    let response_json: Value = response.json().await?;
    
    println!("📥 Long context response received in {:?}", duration);
    
    if let Some(usage) = response_json.get("usage") {
        println!("📊 Token usage for long context:");
        println!("   Prompt tokens: {}", usage.get("prompt_tokens").unwrap_or(&json!(0)));
        println!("   Completion tokens: {}", usage.get("completion_tokens").unwrap_or(&json!(0)));
        println!("   Total tokens: {}", usage.get("total_tokens").unwrap_or(&json!(0)));
    }

    if let Some(choices) = response_json.get("choices").and_then(|c| c.as_array()) {
        if let Some(first_choice) = choices.first() {
            if let Some(message) = first_choice.get("message") {
                if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                    println!("📝 Long context analysis:");
                    println!("{}", content);
                    
                    // Validate long context understanding
                    let content_lower = content.to_lowercase();
                    let mentions_ai = content_lower.contains("人工智能") || content_lower.contains("ai");
                    let mentions_ml = content_lower.contains("机器学习");
                    let mentions_dl = content_lower.contains("深度学习");
                    let shows_relationship = content_lower.contains("关系") || content_lower.contains("分支");
                    
                    println!("✅ Long context validation:");
                    println!("   Mentions AI: {}", mentions_ai);
                    println!("   Mentions ML: {}", mentions_ml);
                    println!("   Mentions DL: {}", mentions_dl);
                    println!("   Shows relationships: {}", shows_relationship);
                }
            }
        }
    }

    println!(); // Empty line for spacing
    Ok(())
}

/// Utility function to truncate strings for display (Unicode-safe)
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len).collect();
        format!("{}...\n[Truncated - showing first {} of {} characters]", 
                truncated, max_len, s.chars().count())
    }
}
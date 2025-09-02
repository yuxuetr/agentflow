//! Debug Environment Loading in AgentFlow-Nodes
//! 
//! This checks if environment variables are properly loaded through agentflow-nodes

use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Debug Environment Loading Test");
    println!("=================================\n");

    println!("📋 Environment before AgentFlow::init():");
    println!("ANTHROPIC_API_KEY: {}", 
        if env::var("ANTHROPIC_API_KEY").is_ok() { "✅ SET" } else { "❌ NOT SET" });
    println!("OPENAI_API_KEY: {}", 
        if env::var("OPENAI_API_KEY").is_ok() { "✅ SET" } else { "❌ NOT SET" });

    // Initialize AgentFlow
    println!("\n🔧 Calling AgentFlow::init()...");
    agentflow_llm::AgentFlow::init().await.expect("Failed to initialize AgentFlow");
    println!("✅ AgentFlow::init() completed");

    println!("\n📋 Environment after AgentFlow::init():");
    match env::var("ANTHROPIC_API_KEY") {
        Ok(key) => {
            let masked_key = if key.len() > 8 {
                format!("{}...{}", &key[..4], &key[key.len()-4..])
            } else {
                "***MASKED***".to_string()
            };
            println!("ANTHROPIC_API_KEY: ✅ SET ({})", masked_key);
        }
        Err(_) => println!("ANTHROPIC_API_KEY: ❌ NOT SET"),
    }
    
    match env::var("OPENAI_API_KEY") {
        Ok(key) => {
            let masked_key = if key.len() > 8 {
                format!("{}...{}", &key[..4], &key[key.len()-4..])
            } else {
                "***MASKED***".to_string()
            };
            println!("OPENAI_API_KEY: ✅ SET ({})", masked_key);
        }
        Err(_) => println!("OPENAI_API_KEY: ❌ NOT SET"),
    }

    // Check the .env file
    println!("\n📁 Checking ~/.agentflow/.env file:");
    match std::fs::read_to_string(format!("{}/.agentflow/.env", env::var("HOME").unwrap_or_default())) {
        Ok(content) => {
            println!("✅ File exists");
            let lines: Vec<&str> = content.lines().collect();
            println!("📄 Contains {} lines", lines.len());
            for line in lines {
                if line.starts_with("ANTHROPIC_API_KEY=") {
                    println!("✅ Found ANTHROPIC_API_KEY line");
                } else if line.starts_with("OPENAI_API_KEY=") {
                    println!("✅ Found OPENAI_API_KEY line");
                }
            }
        }
        Err(e) => println!("❌ Cannot read file: {}", e),
    }

    // Test direct curl with env var
    println!("\n🔗 Testing direct curl with loaded environment:");
    let result = std::process::Command::new("curl")
        .arg("-s")
        .arg("-X")
        .arg("POST")
        .arg("https://api.anthropic.com/v1/messages")
        .arg("-H")
        .arg(&format!("x-api-key: {}", env::var("ANTHROPIC_API_KEY").unwrap_or("MISSING".to_string())))
        .arg("-H")
        .arg("anthropic-version: 2023-06-01")
        .arg("-H")
        .arg("content-type: application/json")
        .arg("-d")
        .arg(r#"{"model":"claude-3-haiku-20240307","max_tokens":10,"messages":[{"role":"user","content":"test"}]}"#)
        .output();

    match result {
        Ok(output) => {
            let response = String::from_utf8_lossy(&output.stdout);
            if response.contains("\"type\":\"error\"") && response.contains("not_found") {
                println!("❌ Direct curl: 404 error (API key issue)");
                println!("🔍 Response: {}", response.chars().take(200).collect::<String>());
            } else if response.contains("\"content\"") || response.contains("\"completion\"") {
                println!("✅ Direct curl: SUCCESS");
            } else {
                println!("❓ Direct curl: Unexpected response");
                println!("🔍 Response: {}", response.chars().take(200).collect::<String>());
            }
        }
        Err(e) => println!("❌ Curl failed: {}", e),
    }

    Ok(())
}

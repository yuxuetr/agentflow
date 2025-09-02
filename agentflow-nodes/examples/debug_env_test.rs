//! Debug Environment Variables Test
//! Tests if environment variables are properly loaded

use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("🔍 Environment Variables Debug Test");
  println!("===================================\n");

  // Test before AgentFlow init
  println!("📋 BEFORE AgentFlow::init():");
  check_env_vars();

  // Initialize AgentFlow
  agentflow_llm::AgentFlow::init().await.expect("Failed to initialize AgentFlow");

  // Test after AgentFlow init
  println!("\n📋 AFTER AgentFlow::init():");
  check_env_vars();

  // Test direct API call using loaded env
  println!("\n🧪 Testing Direct API Call with Loaded Environment:");
  test_direct_api().await;

  Ok(())
}

fn check_env_vars() {
  let vars_to_check = vec![
    "ANTHROPIC_API_KEY",
    "OPENAI_API_KEY", 
    "GEMINI_API_KEY",
    "MOONSHOT_API_KEY",
    "STEPFUN_API_KEY",
  ];

  for var in vars_to_check {
    match env::var(var) {
      Ok(value) => println!("   ✅ {}: {}...", var, &value[..std::cmp::min(20, value.len())]),
      Err(_) => println!("   ❌ {}: NOT SET", var),
    }
  }
}

async fn test_direct_api() {
  if let Ok(api_key) = env::var("ANTHROPIC_API_KEY") {
    println!("   🔑 Found ANTHROPIC_API_KEY: {}...", &api_key[..20]);
    
    let client = reqwest::Client::new();
    let response = client
      .post("https://api.anthropic.com/v1/messages")
      .header("x-api-key", &api_key)
      .header("anthropic-version", "2023-06-01")
      .header("content-type", "application/json")
      .json(&serde_json::json!({
        "model": "claude-3-haiku-20240307",
        "max_tokens": 10,
        "messages": [{"role": "user", "content": "Test"}]
      }))
      .send()
      .await;

    match response {
      Ok(resp) => {
        println!("   ✅ Direct API call status: {}", resp.status());
        if resp.status().is_success() {
          println!("   🎉 Direct API call SUCCESSFUL - API key is working!");
        } else {
          let error_text = resp.text().await.unwrap_or_default();
          println!("   ❌ Direct API call failed: {}", error_text);
        }
      }
      Err(e) => {
        println!("   ❌ Direct API call error: {}", e);
      }
    }
  } else {
    println!("   ❌ ANTHROPIC_API_KEY not found in environment after init");
  }
}

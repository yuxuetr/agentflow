//! Model Discovery Example
//! 
//! This example demonstrates how to:
//! 1. Fetch model lists from supported vendors
//! 2. Validate existing model configurations
//! 3. Update the default models configuration
//! 4. Check if specific models exist

use agentflow_llm::{AgentFlow, LLMError};
use std::env;

#[tokio::main]
async fn main() -> Result<(), LLMError> {
  // Initialize logging to see what's happening
  AgentFlow::init_logging().ok();
  
  println!("🔍 AgentFlow Model Discovery Demo");
  println!("==================================\n");

  // Check which API keys are available
  let api_keys = check_available_api_keys();
  if api_keys.is_empty() {
    println!("⚠️  No API keys found. Set environment variables like:");
    println!("   export MOONSHOT_API_KEY=your_key_here");
    println!("   export DASHSCOPE_API_KEY=your_key_here");
    println!("   export ANTHROPIC_API_KEY=your_key_here");
    println!("   export GEMINI_API_KEY=your_key_here");
    println!("\n🔄 Running in demo mode (will show errors for missing keys)...\n");
  } else {
    println!("✅ Found API keys for: {}\n", api_keys.join(", "));
  }

  // 1. Fetch models from all vendors
  println!("1️⃣ Fetching models from all supported vendors...");
  match AgentFlow::fetch_all_models().await {
    Ok(all_models) => {
      for (vendor, models) in &all_models {
        println!("   📦 {}: {} models", vendor, models.len());
        
        // Show first few models as examples
        for model in models.iter().take(3) {
          println!("      - {}", model.id);
        }
        if models.len() > 3 {
          println!("      ... and {} more", models.len() - 3);
        }
      }
      println!();
    }
    Err(e) => {
      println!("   ❌ Error fetching models: {}\n", e);
    }
  }

  // 2. Fetch models from a specific vendor (if API key available)
  if api_keys.contains(&"moonshot".to_string()) {
    println!("2️⃣ Fetching MoonShot models specifically...");
    match AgentFlow::fetch_vendor_models("moonshot").await {
      Ok(models) => {
        println!("   🌙 Found {} MoonShot models:", models.len());
        for model in models.iter().take(5) {
          let display_name = model.display_name.as_ref().unwrap_or(&model.id);
          println!("      - {} ({})", model.id, display_name);
        }
        println!();
      }
      Err(e) => {
        println!("   ❌ Error fetching MoonShot models: {}\n", e);
      }
    }
  }

  // 3. Validate specific models
  println!("3️⃣ Validating specific models...");
  let test_models = vec![
    ("moonshot-v1-8k", "moonshot"),
    ("claude-3-5-sonnet-20241022", "anthropic"),
    ("gemini-1.5-pro", "google"),
    ("qwen-turbo", "dashscope"),
    ("fake-model", "moonshot"), // This should fail
  ];

  for (model, vendor) in test_models {
    match AgentFlow::validate_model(model, vendor).await {
      Ok(is_valid) => {
        let status = if is_valid { "✅ Valid" } else { "❌ Invalid" };
        println!("   {} {}/{}", status, vendor, model);
      }
      Err(e) => {
        println!("   ⚠️  Could not validate {}/{}: {}", vendor, model, e);
      }
    }
  }
  println!();

  // 4. Check if specific models exist
  println!("4️⃣ Checking model existence...");
  let models_to_check = vec![
    ("moonshot-v1-128k", "moonshot"),
    ("claude-opus-4-20250514", "anthropic"),
    ("nonexistent-model", "google"),
  ];

  for (model, vendor) in models_to_check {
    match AgentFlow::model_exists(model, vendor).await {
      Ok(exists) => {
        let status = if exists { "✅ Exists" } else { "❌ Not found" };
        println!("   {} {}/{}", status, vendor, model);
      }
      Err(e) => {
        println!("   ⚠️  Error checking {}/{}: {}", vendor, model, e);
      }
    }
  }
  println!();

  // 5. Get model information
  println!("5️⃣ Getting model information...");
  if api_keys.contains(&"moonshot".to_string()) {
    match AgentFlow::get_model_info("moonshot-v1-8k", "moonshot").await {
      Ok(Some(model_info)) => {
        println!("   📋 Model Info for moonshot-v1-8k:");
        println!("      - ID: {}", model_info.id);
        println!("      - Vendor: {}", model_info.vendor);
        if let Some(owned_by) = &model_info.owned_by {
          println!("      - Owned by: {}", owned_by);
        }
        println!();
      }
      Ok(None) => {
        println!("   ❌ Model moonshot-v1-8k not found\n");
      }
      Err(e) => {
        println!("   ⚠️  Error getting model info: {}\n", e);
      }
    }
  }

  // 6. Suggest similar models
  println!("6️⃣ Model suggestions for typos...");
  if api_keys.contains(&"moonshot".to_string()) {
    match AgentFlow::suggest_similar_models("moonshot-v1-8000", "moonshot").await { // Typo: 8000 instead of 8k
      Ok(suggestions) => {
        println!("   💡 Suggestions for 'moonshot-v1-8000':");
        for suggestion in suggestions.iter().take(3) {
          println!("      - {}", suggestion);
        }
        println!();
      }
      Err(e) => {
        println!("   ⚠️  Error getting suggestions: {}\n", e);
      }
    }
  }

  // 7. Initialize AgentFlow and validate current configuration
  println!("7️⃣ Validating current configuration...");
  match AgentFlow::init().await {
    Ok(()) => {
      println!("   ✅ AgentFlow initialized successfully");
      
      match AgentFlow::validate_models().await {
        Ok(result) => {
          println!("   📊 Validation Report:");
          println!("      - Valid models: {}", result.valid_models.len());
          println!("      - Invalid models: {}", result.invalid_models.len());
          println!("      - Unavailable vendors: {}", result.unavailable_vendors.len());
          
          if !result.invalid_models.is_empty() {
            println!("      Invalid models found:");
            for invalid in result.invalid_models.iter().take(3) {
              println!("        - {}: {}", invalid.model_name, invalid.error);
            }
          }
        }
        Err(e) => {
          println!("   ⚠️  Error validating models: {}", e);
        }
      }
    }
    Err(e) => {
      println!("   ⚠️  Error initializing AgentFlow: {}", e);
    }
  }
  println!();

  // 8. Update models configuration (commented out to avoid overwriting)
  println!("8️⃣ Updating models configuration (simulation)...");
  println!("   💡 To actually update the configuration, uncomment the following code:");
  println!("   // let result = AgentFlow::update_models_config(\"templates/default_models.yml\").await?;");
  println!("   // println!(\"Update report:\\n{}\", result.create_report());");
  println!();

  /*
  // Uncomment this section to actually update the configuration
  match AgentFlow::update_models_config("templates/default_models.yml").await {
    Ok(result) => {
      println!("   ✅ Configuration updated successfully!");
      println!("   📈 Update Report:");
      println!("      - Added models: {}", result.added_models);
      println!("      - Updated models: {}", result.updated_models);
      
      if !result.added_model_names.is_empty() {
        println!("      New models added:");
        for name in result.added_model_names.iter().take(5) {
          println!("        - {}", name);
        }
        if result.added_model_names.len() > 5 {
          println!("        ... and {} more", result.added_model_names.len() - 5);
        }
      }
    }
    Err(e) => {
      println!("   ❌ Error updating configuration: {}", e);
    }
  }
  */

  println!("🎉 Model discovery demo completed!");
  println!("\n💡 Next steps:");
  println!("   1. Set up API keys for the vendors you want to use");
  println!("   2. Run `AgentFlow::update_models_config()` to update your configuration");
  println!("   3. Use `AgentFlow::validate_models()` regularly to check model availability");
  
  Ok(())
}

fn check_available_api_keys() -> Vec<String> {
  let mut available = Vec::new();
  
  let keys_to_check = vec![
    ("MOONSHOT_API_KEY", "moonshot"),
    ("DASHSCOPE_API_KEY", "dashscope"),
    ("ANTHROPIC_API_KEY", "anthropic"),
    ("GEMINI_API_KEY", "google"),
  ];
  
  for (env_var, vendor) in keys_to_check {
    if env::var(env_var).is_ok() {
      available.push(vendor.to_string());
    }
  }
  
  available
}
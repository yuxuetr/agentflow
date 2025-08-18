//! Implement Split Configuration Example
//!
//! This example shows how to:
//! 1. Split your current monolithic config into vendor files
//! 2. Set up a production-ready split configuration structure
//! 3. Update your AgentFlow initialization to use split configs

use agentflow_llm::{AgentFlow, LLMConfig, VendorConfigManager};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  println!("🔧 Implementing Split Configuration");
  println!("===================================\n");

  let monolithic_path = "templates/default_models.yml";
  let split_config_dir = "config"; // This would be your actual config directory

  if !Path::new(monolithic_path).exists() {
    println!("❌ Monolithic config not found. Run update_models_config example first.");
    return Ok(());
  }

  // Step 1: Load current monolithic configuration
  println!("📋 Step 1: Loading current configuration...");
  let config = LLMConfig::from_file(monolithic_path).await?;
  println!(
    "   ✅ Loaded {} models from {} vendors",
    config.models.len(),
    config
      .models
      .values()
      .map(|m| &m.vendor)
      .collect::<std::collections::HashSet<_>>()
      .len()
  );

  // Step 2: Create split configuration structure
  println!("\n🗂️  Step 2: Creating split configuration...");
  let manager = VendorConfigManager::new(&split_config_dir);
  let split_result = manager.split_config(&config).await?;

  println!("   ✅ Split configuration created successfully!");
  println!(
    "   📁 Main config: {}",
    split_result.main_config_file.display()
  );
  println!("   📊 Vendor files: {}", split_result.vendor_files.len());

  for (vendor, count) in &split_result.models_per_vendor {
    println!("      - {}: {} models", vendor, count);
  }

  // Step 3: Demonstrate loading the split configuration
  println!("\n🔄 Step 3: Testing split configuration loading...");
  let start_time = std::time::Instant::now();
  let loaded_config = manager.load_config().await?;
  let load_time = start_time.elapsed();

  println!(
    "   ✅ Loaded {} models in {:?}",
    loaded_config.models.len(),
    load_time
  );

  // Verify the loaded config matches the original
  if loaded_config.models.len() == config.models.len() {
    println!("   ✅ Model count matches original configuration");
  } else {
    println!(
      "   ⚠️  Model count mismatch: {} vs {}",
      loaded_config.models.len(),
      config.models.len()
    );
  }

  // Step 4: Show how to update specific vendors
  println!("\n🔄 Step 4: Demonstrating vendor-specific updates...");

  // Example: Update only OpenAI models (simulate adding a new model)
  let mut openai_models = std::collections::HashMap::new();

  // Copy existing OpenAI models
  for (name, model_config) in &loaded_config.models {
    if model_config.vendor == "openai" {
      openai_models.insert(name.clone(), model_config.clone());
    }
  }

  // Add a simulated new model
  let mut new_model = openai_models.values().next().unwrap().clone();
  new_model.model_id = Some("gpt-4o-2024-08-06".to_string());
  openai_models.insert("gpt-4o-latest".to_string(), new_model);

  // Update just the OpenAI vendor file
  manager
    .update_vendor_models("openai", openai_models)
    .await?;
  println!("   ✅ Updated OpenAI vendor file with new model");

  // Step 5: Production usage examples
  println!("\n🚀 Step 5: Production Usage Examples");
  println!("====================================");

  show_production_usage_examples(&split_config_dir);

  // Step 6: File structure overview
  println!("\n📁 Step 6: Generated File Structure");
  println!("===================================");
  show_file_structure(&split_config_dir).await?;

  // Cleanup note
  println!("\n🧹 Cleanup");
  println!("========");
  println!(
    "The split configuration has been created in: {}",
    split_config_dir
  );
  println!("You can now:");
  println!("  1. Review the generated files");
  println!("  2. Customize the main config.yml as needed");
  println!("  3. Update your application to use VendorConfigManager");
  println!("  4. Remove the old monolithic file when ready");

  Ok(())
}

fn show_production_usage_examples(config_dir: &str) {
  println!("Here's how to use split configuration in your applications:\n");

  println!("```rust");
  println!("// Option 1: Load full configuration (all vendors)");
  println!("use agentflow_llm::VendorConfigManager;");
  println!();
  println!(
    "let manager = VendorConfigManager::new(\"{}\");",
    config_dir
  );
  println!("let config = manager.load_config().await?;");
  println!("// Use config with AgentFlow...");
  println!("```\n");

  println!("```rust");
  println!("// Option 2: Selective loading (load only specific vendors)");
  println!("// This requires implementing a custom selective loader");
  println!(
    "let manager = VendorConfigManager::new(\"{}\");",
    config_dir
  );
  println!("let openai_models = manager.load_vendor_models(&[\"openai\", \"anthropic\"]).await?;");
  println!("// Use only the models you need");
  println!("```\n");

  println!("```rust");
  println!("// Option 3: Lazy loading with AgentFlow integration");
  println!("// AgentFlow could be extended to support this");
  println!(
    "AgentFlow::init_with_split_config(\"{}\").await?;",
    config_dir
  );
  println!("// Models loaded on-demand as needed");
  println!("```\n");

  println!("**Benefits in Production:**");
  println!("  🚀 Faster startup for single-vendor applications");
  println!("  💾 Lower memory usage when only using subset of models");
  println!("  🔧 Easier maintenance and vendor-specific updates");
  println!("  📦 Better organization for large teams");
  println!("  🔄 Simplified CI/CD for vendor-specific model updates");
}

async fn show_file_structure(config_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
  use tokio::fs;

  if !Path::new(config_dir).exists() {
    println!("Config directory not found: {}", config_dir);
    return Ok(());
  }

  println!("{}/", config_dir);

  // Show main config
  let main_config = Path::new(config_dir).join("config.yml");
  if main_config.exists() {
    let metadata = fs::metadata(&main_config).await?;
    println!(
      "├── config.yml ({} KB) - Main configuration (providers, defaults)",
      metadata.len() / 1024
    );
  }

  // Show models directory
  let models_dir = Path::new(config_dir).join("models");
  if models_dir.exists() {
    println!("└── models/");

    let mut entries = fs::read_dir(&models_dir).await?;
    let mut files = Vec::new();

    while let Some(entry) = entries.next_entry().await? {
      if entry.path().extension().and_then(|s| s.to_str()) == Some("yml") {
        let metadata = entry.metadata().await?;
        files.push((entry.file_name(), metadata.len()));
      }
    }

    // Sort files by name
    files.sort_by(|a, b| a.0.cmp(&b.0));

    for (i, (filename, size)) in files.iter().enumerate() {
      let connector = if i == files.len() - 1 {
        "└──"
      } else {
        "├──"
      };
      println!(
        "    {} {} ({} KB)",
        connector,
        filename.to_string_lossy(),
        size / 1024
      );
    }
  }

  // File size summary
  let mut total_size = 0;
  if let Ok(mut entries) = fs::read_dir(config_dir).await {
    while let Some(entry) = entries.next_entry().await? {
      if entry.file_type().await?.is_file() {
        total_size += entry.metadata().await?.len();
      }
    }
  }

  if let Ok(mut entries) = fs::read_dir(&models_dir).await {
    while let Some(entry) = entries.next_entry().await? {
      if entry.file_type().await?.is_file() {
        total_size += entry.metadata().await?.len();
      }
    }
  }

  println!("\n📊 Total configuration size: {} KB", total_size / 1024);

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use tempfile::tempdir;

  #[tokio::test]
  async fn test_split_config_workflow() {
    // Create a simple test configuration
    let config = LLMConfig::default();

    let temp_dir = tempdir().unwrap();
    let manager = VendorConfigManager::new(temp_dir.path());

    // This should work even with empty config
    let result = manager.split_config(&config).await;
    assert!(result.is_ok());
  }
}

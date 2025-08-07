//! Configuration Organization Example
//! 
//! This example demonstrates:
//! 1. Splitting monolithic config into vendor-specific files
//! 2. Loading performance comparison between approaches
//! 3. Benefits and trade-offs of different organization strategies

use agentflow_llm::{LLMConfig, VendorConfigManager, PerformanceComparison};
use std::path::Path;
use tempfile::tempdir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  // Initialize logging
  env_logger::init();

  println!("🏗️  Configuration Organization Analysis");
  println!("======================================\n");

  // Load the current monolithic configuration
  let monolithic_path = "templates/default_models.yml";
  
  if !Path::new(monolithic_path).exists() {
    println!("❌ Monolithic config file not found: {}", monolithic_path);
    println!("   Run the update_models_config example first to generate it.");
    return Ok(());
  }

  println!("📊 Analyzing current monolithic configuration...");
  let config = LLMConfig::from_file(monolithic_path).await?;
  
  // Analyze current configuration
  analyze_current_config(&config);

  // Create temporary directory for split configuration
  let temp_dir = tempdir()?;
  let split_config_dir = temp_dir.path().join("split_config");
  
  println!("\n🔄 Splitting configuration by vendor...");
  let manager = VendorConfigManager::new(&split_config_dir);
  let split_result = manager.split_config(&config).await?;
  
  println!("{}", split_result.create_report());

  // Performance comparison
  println!("\n⚖️  Performance Comparison");
  println!("========================");
  
  let benchmark = PerformanceComparison::benchmark_loading(
    Path::new(monolithic_path),
    &split_config_dir
  ).await?;
  
  println!("{}", benchmark.create_report());

  // Demonstrate selective loading (loading only specific vendors)
  println!("\n🎯 Selective Loading Demo");
  println!("========================");
  demonstrate_selective_loading(&split_config_dir).await?;

  // Recommendations
  println!("\n💡 Recommendations");
  println!("==================");
  provide_recommendations(&config, &benchmark);

  // Cleanup note
  println!("\n🧹 Note: Split configuration files created in temporary directory");
  println!("   Directory: {}", split_config_dir.display());
  println!("   Files will be automatically cleaned up when program exits.");

  Ok(())
}

fn analyze_current_config(config: &LLMConfig) {
  let mut vendor_counts = std::collections::HashMap::new();
  let mut total_models = 0;

  for model_config in config.models.values() {
    *vendor_counts.entry(model_config.vendor.clone()).or_insert(0) += 1;
    total_models += 1;
  }

  println!("  📈 Total models: {}", total_models);
  println!("  🏢 Vendors: {}", vendor_counts.len());
  
  for (vendor, count) in &vendor_counts {
    let percentage = (count * 100) as f64 / total_models as f64;
    println!("    - {}: {} models ({:.1}%)", vendor, count, percentage);
  }

  // File size analysis
  if let Ok(metadata) = std::fs::metadata("templates/default_models.yml") {
    let size_kb = metadata.len() / 1024;
    println!("  💾 File size: {} KB", size_kb);
    
    if size_kb > 100 {
      println!("  ⚠️  Large file - consider splitting for better maintainability");
    }
  }
}

async fn demonstrate_selective_loading(split_config_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
  println!("Demonstrating loading only specific vendors...\n");

  // Simulate loading only OpenAI and Anthropic models
  let vendors_to_load = vec!["openai", "anthropic"];
  
  for vendor in vendors_to_load {
    let vendor_file = split_config_dir.join("models").join(format!("{}.yml", vendor));
    
    if vendor_file.exists() {
      let start_time = std::time::Instant::now();
      
      // Read just this vendor's models
      let content = tokio::fs::read_to_string(&vendor_file).await?;
      let _: serde_yaml::Value = serde_yaml::from_str(&content)?;
      
      let load_time = start_time.elapsed();
      let file_size = tokio::fs::metadata(&vendor_file).await?.len();
      
      println!("  🚀 {} models loaded in {:?} ({} KB)", 
        vendor, load_time, file_size / 1024);
    } else {
      println!("  ❌ {} vendor file not found", vendor);
    }
  }

  println!("\n💡 Selective loading allows applications to:");
  println!("   - Load only the models they actually use");
  println!("   - Reduce memory usage for smaller applications");
  println!("   - Speed up initialization for single-vendor apps");
  
  Ok(())
}

fn provide_recommendations(config: &LLMConfig, benchmark: &agentflow_llm::LoadingBenchmark) {
  let total_models = config.models.len();
  let monolithic_size_kb = benchmark.monolithic_file_size / 1024;
  
  println!("Based on your configuration analysis:\n");

  // Size-based recommendations
  if monolithic_size_kb > 50 {
    println!("✅ **Recommended: Split Configuration**");
    println!("   Reasons:");
    println!("   - Large file size ({} KB) benefits from organization", monolithic_size_kb);
    println!("   - {} models across multiple vendors", total_models);
    println!("   - Easier maintenance and selective loading");
  } else {
    println!("ℹ️  **Consider: Keep Monolithic for Now**");
    println!("   Reasons:");
    println!("   - File size ({} KB) is still manageable", monolithic_size_kb);
    println!("   - Splitting overhead may not be worth it yet");
  }

  // Performance-based recommendations
  let performance_impact = benchmark.split_load_time.as_millis() as f64 / benchmark.monolithic_load_time.as_millis() as f64;
  
  if performance_impact < 1.5 {
    println!("\n⚡ **Performance Impact: Low** ({:.1}x slower)", performance_impact);
    println!("   Split configuration has minimal performance overhead");
  } else if performance_impact < 3.0 {
    println!("\n⚠️  **Performance Impact: Moderate** ({:.1}x slower)", performance_impact);
    println!("   Consider lazy loading or caching strategies");
  } else {
    println!("\n🚨 **Performance Impact: High** ({:.1}x slower)", performance_impact);
    println!("   May want to stick with monolithic config for now");
  }

  // Growth-based recommendations
  println!("\n📈 **Future Growth Considerations:**");
  if total_models > 100 {
    println!("   - You already have {} models - splitting is beneficial", total_models);
    println!("   - New vendor integration will be easier with split config");
    println!("   - Consider automated model discovery per vendor");
  } else {
    println!("   - With {} models, you're approaching the complexity threshold", total_models);
    println!("   - Plan for split configuration when you exceed 100-150 models");
  }

  // Hybrid approach
  println!("\n🔀 **Hybrid Approach:**");
  println!("   - Keep commonly used models in main config");
  println!("   - Split large vendor collections (Google, DashScope)");
  println!("   - Use lazy loading for specialty models");

  // Implementation strategy
  println!("\n🛠️  **Implementation Strategy:**");
  println!("   1. Start with vendor-split organization");
  println!("   2. Add lazy loading for rarely used vendors");
  println!("   3. Implement model caching for frequently accessed configs");
  println!("   4. Consider database storage for very large deployments");
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_config_analysis() {
    // This test ensures our analysis functions don't panic
    let config = LLMConfig::default();
    analyze_current_config(&config);
    // Should not panic with empty config
  }
}
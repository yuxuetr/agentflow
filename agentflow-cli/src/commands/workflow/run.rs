use anyhow::{Context, Result};
use std::collections::HashMap;

// Use CLI's real WorkflowRunner with LLM integration instead of Core's mock version
use crate::executor::runner::WorkflowRunner;
use crate::utils::output::OutputFormatter;

pub async fn execute(
  workflow_file: String,
  watch: bool,
  output: Option<String>,
  input: Vec<(String, String)>,
  dry_run: bool,
  _timeout: String,
  _max_retries: u32,
) -> Result<()> {
  if dry_run {
    println!("🔍 Validating workflow configuration...");
    let _runner = WorkflowRunner::new(&workflow_file)
      .await
      .with_context(|| format!("Failed to create workflow runner for: {}", workflow_file))?;
    println!("✅ Workflow configuration is valid");
    return Ok(());
  }

  println!("🚀 Starting workflow execution: {}", workflow_file);
  println!("🔍 Loading workflow file: {}", workflow_file);

  // Check file exists and get size
  let metadata = tokio::fs::metadata(&workflow_file)
    .await
    .with_context(|| format!("Failed to read workflow file: {}", workflow_file))?;
  println!(
    "✅ File loaded successfully, length: {} bytes",
    metadata.len()
  );

  // Create workflow runner
  let runner = WorkflowRunner::new(&workflow_file)
    .await
    .with_context(|| format!("Failed to create workflow runner for: {}", workflow_file))?;

  // Convert input parameters
  let input_map: HashMap<String, String> = input.into_iter().collect();

  if !input_map.is_empty() {
    println!("📝 Input parameters:");
    for (key, value) in &input_map {
      println!("  - {}: {}", key, value);
    }
  }

  // Execute workflow
  let start_time = std::time::Instant::now();
  let results = runner
    .run(input_map)
    .await
    .context("Workflow execution failed")?;
  let duration = start_time.elapsed();

  println!("✅ Workflow completed in {:.2}s", duration.as_secs_f64());

  // Format and display results
  if !results.is_empty() {
    println!("\n📊 Results:");
    let formatter = OutputFormatter::new();
    for (key, value) in &results {
      println!("  - {}: {}", key, formatter.format_value(value));
    }
  }

  // Save results to file if specified
  if let Some(output_file) = output {
    let output_content =
      serde_json::to_string_pretty(&results).context("Failed to serialize results")?;
    tokio::fs::write(&output_file, output_content)
      .await
      .with_context(|| format!("Failed to write results to: {}", output_file))?;
    println!("💾 Results saved to: {}", output_file);
  }

  // TODO: Implement watch mode
  if watch {
    println!("⚠️  Watch mode not yet implemented");
  }

  Ok(())
}

use anyhow::{Context, Result};

// Use CLI's real WorkflowRunner instead of Core's mock version
use crate::executor::runner::WorkflowRunner;

pub async fn execute(workflow_file: String) -> Result<()> {
  println!("🔍 Validating workflow: {}", workflow_file);

  // Try to create a workflow runner (this validates YAML parsing and structure)
  let _runner = WorkflowRunner::new(&workflow_file)
    .await
    .with_context(|| format!("Failed to validate workflow file: {}", workflow_file))?;

  println!("✅ Workflow configuration is valid");
  println!("📄 File: {}", workflow_file);
  println!("📋 Workflow can be parsed and loaded successfully");

  Ok(())
}

use crate::config::{
  schema::{
    validate_flow_definition_with_options, UnknownParameterMode, WorkflowValidationOptions,
  },
  v2::FlowDefinitionV2,
};
use anyhow::{bail, Context, Result};
use std::fs;

pub async fn execute(workflow_file: String, format: String, strict: bool) -> Result<()> {
  let yaml_content = fs::read_to_string(&workflow_file)
    .with_context(|| format!("Failed to read workflow file: {}", workflow_file))?;
  let flow_def: FlowDefinitionV2 =
    serde_yaml::from_str(&yaml_content).with_context(|| "Failed to parse workflow YAML")?;

  let report = validate_flow_definition_with_options(
    &flow_def,
    WorkflowValidationOptions {
      unknown_parameters: if strict {
        UnknownParameterMode::Error
      } else {
        UnknownParameterMode::Warning
      },
    },
  );
  match format.as_str() {
    "json" => {
      let payload = serde_json::json!({
        "workflow": &flow_def.name,
        "valid": report.is_valid(),
        "issues": &report.issues,
        "warnings": &report.warnings,
      });
      println!("{}", serde_json::to_string_pretty(&payload)?);
    }
    _ => print_schema_report(&flow_def.name, &report),
  }

  if !report.is_valid() {
    bail!(
      "workflow '{}' failed schema validation with {} issue(s)",
      flow_def.name,
      report.issues.len()
    );
  }

  Ok(())
}

pub fn print_schema_report(
  workflow_name: &str,
  report: &crate::config::schema::WorkflowValidationReport,
) {
  println!("Workflow: {}", workflow_name);
  if report.issues.is_empty() && report.warnings.is_empty() {
    println!("✅ Schema validation passed");
    return;
  }

  if !report.issues.is_empty() {
    println!("❌ Schema issues: {}", report.issues.len());
    for (idx, issue) in report.issues.iter().enumerate() {
      println!("  {}. {}", idx + 1, issue);
    }
  }
  if !report.warnings.is_empty() {
    println!("⚠️  Schema warnings: {}", report.warnings.len());
    for (idx, warning) in report.warnings.iter().enumerate() {
      println!("  {}. {}", idx + 1, warning);
    }
  }
}

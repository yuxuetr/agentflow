use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;

use super::error_context::mcp_context;
use agentflow_skills::{SkillBuilder, SkillLoader};

pub async fn execute(skill_dir: String) -> Result<()> {
  let dir = Path::new(&skill_dir);
  let manifest =
    SkillLoader::load(dir).with_context(|| format!("Failed to load skill from '{}'", skill_dir))?;
  SkillLoader::validate(&manifest, dir).with_context(|| "Skill validation failed")?;

  let registry = SkillBuilder::build_registry(&manifest, dir)
    .await
    .with_context(|| mcp_context("Failed to build skill tool registry", &manifest))?;

  let mut definitions = registry
    .list()
    .into_iter()
    .map(|tool| tool.definition())
    .collect::<Vec<_>>();
  definitions.sort_by(|a, b| a.name.cmp(&b.name));

  println!(
    "🔧 Tools for skill '{}' ({}):",
    manifest.skill.name,
    definitions.len()
  );

  if definitions.is_empty() {
    println!("   none");
    return Ok(());
  }

  for definition in definitions {
    println!("   - {}", definition.name);
    println!("     source: {}", definition.metadata.source.as_str());
    if !definition.metadata.permissions.permissions.is_empty() {
      let permissions = definition
        .metadata
        .permissions
        .permissions
        .iter()
        .map(|permission| permission.as_str())
        .collect::<Vec<_>>()
        .join(", ");
      println!("     permissions: {}", permissions);
    }
    if let Some(server) = &definition.metadata.mcp_server_name {
      println!("     mcp_server: {}", server);
    }
    if let Some(tool) = &definition.metadata.mcp_tool_name {
      println!("     mcp_tool: {}", tool);
    }
    if !definition.description.trim().is_empty() {
      println!("     {}", definition.description);
    }
    print_schema_summary(&definition.parameters);
  }

  Ok(())
}

fn print_schema_summary(schema: &Value) {
  let Some(properties) = schema.get("properties").and_then(|v| v.as_object()) else {
    return;
  };

  if properties.is_empty() {
    return;
  }

  println!("     parameters:");
  for (name, schema) in properties {
    let ty = schema
      .get("type")
      .and_then(|v| v.as_str())
      .unwrap_or("unknown");
    let description = schema
      .get("description")
      .and_then(|v| v.as_str())
      .unwrap_or("");
    if description.is_empty() {
      println!("       - {} ({})", name, ty);
    } else {
      println!("       - {} ({}): {}", name, ty, description);
    }
  }
}

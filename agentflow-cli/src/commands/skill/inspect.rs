use anyhow::{Context, Result};
use std::path::Path;

use agentflow_skills::SkillLoader;

pub async fn execute(skill_dir: String) -> Result<()> {
  let dir = Path::new(&skill_dir);
  let manifest =
    SkillLoader::load(dir).with_context(|| format!("Failed to load skill from '{}'", skill_dir))?;
  let warnings =
    SkillLoader::validate(&manifest, dir).with_context(|| "Skill validation failed")?;

  println!("🔎 Skill: {}", manifest.skill.name);
  println!("Version: {}", manifest.skill.version);
  println!("Description: {}", manifest.skill.description);
  println!("Path: {}", dir.display());
  println!();

  println!("Persona:");
  println!("  role: {}", one_line(&manifest.persona.role));
  if let Some(language) = &manifest.persona.language {
    println!("  language: {}", language);
  }
  println!();

  println!("Model:");
  println!("  name: {}", manifest.model.resolved_model());
  println!(
    "  max_iterations: {}",
    manifest.model.resolved_max_iterations()
  );
  println!(
    "  budget_tokens: {}",
    manifest.model.resolved_budget_tokens()
  );
  println!();

  println!("Memory:");
  if let Some(memory) = &manifest.memory {
    println!("  type: {}", memory.memory_type);
    if let Some(window_tokens) = memory.window_tokens {
      println!("  window_tokens: {}", window_tokens);
    }
  } else {
    println!("  type: session");
  }
  println!();

  println!("Tools:");
  if manifest.tools.is_empty() {
    println!("  none");
  } else {
    for tool in &manifest.tools {
      println!("  - {}", tool.name);
      if !tool.allowed_commands.is_empty() {
        println!("    allowed_commands: {}", tool.allowed_commands.join(", "));
      }
      if !tool.allowed_paths.is_empty() {
        println!("    allowed_paths: {}", tool.allowed_paths.join(", "));
      }
      if !tool.allowed_domains.is_empty() {
        println!("    allowed_domains: {}", tool.allowed_domains.join(", "));
      }
    }
  }
  println!();

  println!("MCP Servers:");
  if manifest.mcp_servers.is_empty() {
    println!("  none");
  } else {
    for server in &manifest.mcp_servers {
      println!("  - {} ({})", server.name, server.command);
      if !server.args.is_empty() {
        println!("    args: {}", server.args.join(" "));
      }
      println!("    timeout_secs: {}", server.resolved_timeout_secs());
      println!(
        "    max_concurrent_calls: {}",
        server.resolved_max_concurrent_calls()
      );
    }
  }
  println!();

  println!("Knowledge:");
  if manifest.knowledge.is_empty() {
    println!("  none");
  } else {
    for item in &manifest.knowledge {
      println!("  - {}", item.path);
      if let Some(description) = &item.description {
        println!("    description: {}", description);
      }
    }
  }
  println!();

  println!("Security:");
  println!(
    "  mcp_command_allowlist: {}",
    manifest
      .security
      .resolved_mcp_command_allowlist()
      .join(", ")
  );
  println!(
    "  mcp_default_timeout_secs: {}",
    manifest.security.resolved_mcp_default_timeout_secs()
  );
  println!(
    "  mcp_max_concurrent_calls: {}",
    manifest.security.resolved_mcp_max_concurrent_calls()
  );
  println!(
    "  mcp_max_servers: {}",
    manifest.security.resolved_mcp_max_servers()
  );

  if warnings.is_empty() {
    println!("\nStatus: valid");
  } else {
    println!("\nStatus: valid with warnings");
    for warning in warnings {
      println!("  - {}", warning);
    }
  }

  Ok(())
}

fn one_line(value: &str) -> String {
  value.split_whitespace().collect::<Vec<_>>().join(" ")
}

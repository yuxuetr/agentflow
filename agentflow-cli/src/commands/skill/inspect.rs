use anyhow::{Context, Result};
use std::path::Path;

use agentflow_skills::SkillLoader;
use agentflow_tools::{Capability, EffectiveCapabilities, ToolPermission};

pub async fn execute(skill_dir: String, explain_permissions: bool) -> Result<()> {
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

  if explain_permissions {
    print_capability_explanations(
      &manifest.tools,
      &manifest.security.tool_permission_allowlist,
    );
  }

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

fn print_capability_explanations(
  tools: &[agentflow_skills::manifest::ToolConfig],
  skill_permission_allowlist: &[ToolPermission],
) {
  println!("\nCapability decisions:");
  if tools.is_empty() {
    println!("  (no built-in tools declared)");
    return;
  }

  let skill_grant: Option<Vec<Capability>> = if skill_permission_allowlist.is_empty() {
    None
  } else {
    Some(Capability::from_permissions(skill_permission_allowlist))
  };

  for tool_cfg in tools {
    let required = match builtin_tool_required_capabilities(&tool_cfg.name) {
      Some(caps) => caps,
      None => {
        println!("  - {}: (unknown built-in tool, skipped)", tool_cfg.name);
        continue;
      }
    };

    let effective = EffectiveCapabilities::resolve(
      &tool_cfg.name,
      &required,
      skill_grant.as_deref(),
      None, // tool policy is permissive in inspect (no runtime context)
      None, // CLI flag override is also permissive in inspect
    );

    let verdict = if effective.allowed {
      "ALLOWED"
    } else {
      "DENIED"
    };
    println!("  - {} [{}]", tool_cfg.name, verdict,);
    println!("    required:  {}", format_caps(&effective.required));
    println!("    effective: {}", format_caps(&effective.effective));
    if !effective.denied.is_empty() {
      println!("    denied:    {}", format_caps(&effective.denied));
      if let Some(reason) = &effective.deny_reason {
        println!("    reason:    {}", reason);
      }
    }
    println!("    layers:");
    for entry in &effective.trace {
      let allowed = match &entry.allowed {
        Some(caps) => format_caps(caps),
        None => "(permissive)".to_string(),
      };
      let dropped = if entry.dropped.is_empty() {
        String::new()
      } else {
        format!("  dropped={}", format_caps(&entry.dropped))
      };
      println!(
        "      {:<14} allowed={}  running={}{}",
        entry.source.as_str(),
        allowed,
        format_caps(&entry.running),
        dropped,
      );
    }
  }
}

fn builtin_tool_required_capabilities(name: &str) -> Option<Vec<Capability>> {
  match name.to_lowercase().as_str() {
    "shell" => Some(vec![Capability::Exec]),
    "file" => Some(vec![Capability::FsRead, Capability::FsWrite]),
    "http" => Some(vec![Capability::Net]),
    "script" => Some(vec![Capability::Exec, Capability::FsRead]),
    _ => None,
  }
}

fn format_caps(caps: &[Capability]) -> String {
  if caps.is_empty() {
    "[]".to_string()
  } else {
    format!(
      "[{}]",
      caps
        .iter()
        .map(Capability::as_str)
        .collect::<Vec<_>>()
        .join(", ")
    )
  }
}

fn one_line(value: &str) -> String {
  value.split_whitespace().collect::<Vec<_>>().join(" ")
}

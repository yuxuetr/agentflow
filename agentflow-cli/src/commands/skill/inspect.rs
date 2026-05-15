use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::Path;

use agentflow_skills::SkillLoader;
use agentflow_skills::policy::{
  McpCapabilityMap, PolicyResolutionInput, ResolvedToolPolicy, ToolAdmission, resolve_tool_policy,
};
use agentflow_tools::{Capability, EffectiveCapabilities, ToolPermission};

pub async fn execute(
  skill_dir: String,
  explain_permissions: bool,
  allow_tools: Vec<String>,
  deny_tools: Vec<String>,
) -> Result<()> {
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
    let resolved = resolve_skill_policy(&manifest, &allow_tools, &deny_tools);
    print_policy_admissions(&resolved, &allow_tools, &deny_tools);
  } else if !allow_tools.is_empty() || !deny_tools.is_empty() {
    println!("\n(note: --allow-tool / --deny-tool are only honored with --explain-permissions)");
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

fn resolve_skill_policy(
  manifest: &agentflow_skills::SkillManifest,
  cli_allow: &[String],
  cli_deny: &[String],
) -> ResolvedToolPolicy {
  let skill_allowed: Vec<String> = manifest.tools.iter().map(|t| t.name.clone()).collect();
  let skill_denied: Vec<String> = Vec::new();
  let mcp_caps: McpCapabilityMap = McpCapabilityMap::new();
  let mcp_allowlist: Vec<String> = manifest.security.mcp_server_allowlist.clone();
  let tool_metadata = BTreeMap::new();

  let mut known: std::collections::BTreeSet<String> = skill_allowed.iter().cloned().collect();
  for t in cli_allow {
    known.insert(t.clone());
  }
  for t in cli_deny {
    known.insert(t.clone());
  }
  let known_vec: Vec<String> = known.into_iter().collect();

  resolve_tool_policy(PolicyResolutionInput {
    known_tools: &known_vec,
    skill_allowed_tools: &skill_allowed,
    skill_denied_tools: &skill_denied,
    mcp_server_capabilities: &mcp_caps,
    skill_mcp_server_allowlist: &mcp_allowlist,
    cli_allow_tools: cli_allow,
    cli_deny_tools: cli_deny,
    fallback_policy: None,
    tool_metadata: &tool_metadata,
  })
}

fn print_policy_admissions(
  resolved: &ResolvedToolPolicy,
  cli_allow: &[String],
  cli_deny: &[String],
) {
  println!("\nTool admission decisions:");
  if resolved.decisions.is_empty() {
    println!("  (no tools declared and no --allow-tool / --deny-tool overrides)");
    return;
  }
  println!(
    "  {} allowed, {} denied",
    resolved.allow_count(),
    resolved.deny_count()
  );
  if !cli_allow.is_empty() {
    println!("  cli_allow_tools: {}", cli_allow.join(", "));
  }
  if !cli_deny.is_empty() {
    println!("  cli_deny_tools: {}", cli_deny.join(", "));
  }
  for (tool, admission) in resolved.iter() {
    print_admission_row(tool, admission);
  }
}

fn print_admission_row(tool: &str, admission: &ToolAdmission) {
  let verdict = if admission.allowed {
    "ALLOWED"
  } else {
    "DENIED"
  };
  println!("  - {} [{}]", tool, verdict);
  println!("    source: {}", admission.source.as_str());
  println!("    reason: {}", admission.reason);
  if let Some(server) = &admission.mcp_server {
    println!("    mcp_server: {}", server);
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

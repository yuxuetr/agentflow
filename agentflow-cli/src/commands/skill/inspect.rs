use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use agentflow_skills::policy::{
  McpCapabilityMap, PolicyResolutionInput, ResolvedToolPolicy, ToolAdmission, resolve_tool_policy,
};
use agentflow_skills::{SkillBuilder, SkillLoader};
use agentflow_tools::sandbox::{SandboxEnforcement, default_backend};
use agentflow_tools::{Capability, EffectiveCapabilities, ToolPermission};

use super::error_context::mcp_context;
use super::mcp_discovery_cache::{
  DEFAULT_TTL, DiscoveryCache, from_cache_value, hash_mcp_servers, to_cache_value,
};

/// How the MCP discovery cache was consulted on this run — used by
/// the human-readable summary line so operators see whether a fast
/// path was taken without having to grep their cache directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheOutcome {
  /// Discovery was skipped entirely (`--no-mcp-discovery` or no MCP
  /// servers declared).
  Skipped,
  /// Fresh entry found — no MCP servers were spawned.
  Hit,
  /// Stale or missing entry — discovery ran and the cache was
  /// updated.
  Miss,
  /// `--refresh-mcp-cache` was passed — discovery ran regardless of
  /// cache state.
  Refresh,
}

pub async fn execute(
  skill_dir: String,
  explain_permissions: bool,
  allow_tools: Vec<String>,
  deny_tools: Vec<String>,
  no_mcp_discovery: bool,
  refresh_mcp_cache: bool,
  with_mcp_discovery: bool,
) -> Result<()> {
  if with_mcp_discovery {
    // P10.9.1 flipped the default: MCP discovery is on whenever
    // `--explain-permissions` is set + servers are declared. The
    // old `--with-mcp-discovery` flag becomes a no-op; warn so
    // operators eventually drop it from their scripts but don't
    // break anyone today.
    eprintln!(
      "⚠  --with-mcp-discovery is now the default and the flag is a no-op; \
       safe to remove. Use --no-mcp-discovery to opt out."
    );
  }
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
    print_sandbox_profile(manifest.security.os_sandbox, &manifest.tools);
    // P10.9.1: MCP discovery is now default-on whenever the
    // manifest declares servers. The cache means repeat-inspects
    // are free; the spinner keeps fresh discoveries visible.
    // Skip both when the operator opts out OR when there are no
    // servers to query (clippy collapses the two skip-paths into
    // one `||` arm).
    let (mcp_caps, outcome) = if no_mcp_discovery || manifest.mcp_servers.is_empty() {
      (None, CacheOutcome::Skipped)
    } else {
      let (caps, outcome) = run_discovery_with_cache(&manifest, dir, refresh_mcp_cache)
        .await
        .with_context(|| mcp_context("MCP capability discovery failed", &manifest))?;
      (Some(caps), outcome)
    };
    if let Some(caps) = mcp_caps.as_ref() {
      print_mcp_discovery_summary(caps, outcome);
    } else if no_mcp_discovery && !manifest.mcp_servers.is_empty() {
      println!(
        "\nMCP discovery: skipped (--no-mcp-discovery). \
         Tool admission rows below will not include MCP-advertised tools."
      );
    }
    let resolved = resolve_skill_policy(&manifest, &allow_tools, &deny_tools, mcp_caps.as_ref());
    print_policy_admissions(&resolved, &allow_tools, &deny_tools);
  } else if !allow_tools.is_empty() || !deny_tools.is_empty() {
    println!("\n(note: --allow-tool / --deny-tool are only honored with --explain-permissions)");
  } else if no_mcp_discovery || refresh_mcp_cache {
    println!(
      "\n(note: --no-mcp-discovery / --refresh-mcp-cache are only honored with --explain-permissions)"
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

/// Print the effective OS-level sandbox state: detected platform backend,
/// its enforcement tri-state, and the skill manifest's `security.os_sandbox`
/// opt-in. The backend probe is hermetic (no spawn, just metadata) so this
/// runs on every inspect call without touching the network or spawning a
/// subprocess.
fn print_sandbox_profile(os_sandbox_optin: bool, tools: &[agentflow_skills::manifest::ToolConfig]) {
  let backend = default_backend();
  let enforcement = backend.enforcement_level();
  println!("\nSandbox profile:");
  println!("  backend:           {}", backend.name());
  println!("  enforcement:       {}", enforcement.as_str());
  println!(
    "  manifest opt-in:   security.os_sandbox = {}",
    os_sandbox_optin
  );

  let has_sandboxable_tool = tools
    .iter()
    .any(|t| matches!(t.name.to_lowercase().as_str(), "shell" | "script"));

  // P10.4.1: surface any per-tool overrides. Operators with mixed
  // heterogeneous-enforcement skills want to confirm at a glance which
  // sandboxable tool actually inherits / opts in / opts out — without
  // this, the manifest-level line alone hides the resolved value.
  let mut override_lines: Vec<(String, bool, &'static str)> = Vec::new();
  for tool in tools {
    let lc = tool.name.to_lowercase();
    if !matches!(lc.as_str(), "shell" | "script") {
      continue;
    }
    match tool.os_sandbox {
      Some(value) => override_lines.push((tool.name.clone(), value, "per-tool override")),
      None => override_lines.push((tool.name.clone(), os_sandbox_optin, "inherited")),
    }
  }
  if !override_lines.is_empty() {
    println!("  tool resolution:");
    for (name, effective, source) in &override_lines {
      println!("    {name}: {effective} ({source})");
    }
  }

  let mut notes: Vec<String> = Vec::new();
  if has_sandboxable_tool {
    if !os_sandbox_optin && enforcement == SandboxEnforcement::Enforcing {
      notes.push(
        "skill declares shell/script tools but has not opted in to OS sandbox (security.os_sandbox = false) while the platform backend is available — consider enabling".to_string(),
      );
    }
    if os_sandbox_optin && enforcement != SandboxEnforcement::Enforcing {
      notes.push(format!(
        "skill opted in to OS sandbox but the active backend reports '{}'; shell/script tools will run unsandboxed unless deployed to a platform with an enforcing backend",
        enforcement.as_str()
      ));
    }
  } else if os_sandbox_optin {
    notes.push(
      "security.os_sandbox is enabled but no shell/script tool is declared — flag has no effect for this skill"
        .to_string(),
    );
  }
  if notes.is_empty() {
    println!("  notes:             (none)");
  } else {
    println!("  notes:");
    for note in &notes {
      println!("    - {}", note);
    }
  }
}

fn resolve_skill_policy(
  manifest: &agentflow_skills::SkillManifest,
  cli_allow: &[String],
  cli_deny: &[String],
  mcp_caps_override: Option<&McpCapabilityMap>,
) -> ResolvedToolPolicy {
  let skill_allowed: Vec<String> = manifest.tools.iter().map(|t| t.name.clone()).collect();
  let skill_denied: Vec<String> = Vec::new();
  let empty_caps: McpCapabilityMap = McpCapabilityMap::new();
  let mcp_caps: &McpCapabilityMap = mcp_caps_override.unwrap_or(&empty_caps);
  let mcp_allowlist: Vec<String> = manifest.security.mcp_server_allowlist.clone();
  let tool_metadata = BTreeMap::new();

  let mut known: std::collections::BTreeSet<String> = skill_allowed.iter().cloned().collect();
  for t in cli_allow {
    known.insert(t.clone());
  }
  for t in cli_deny {
    known.insert(t.clone());
  }
  // MCP-discovered tools are valid `known_tools` so the resolver
  // produces an admission row for each one (without this they'd be
  // filtered out before reaching the resolver and never surface).
  for tools in mcp_caps.values() {
    for tool in tools {
      known.insert(tool.clone());
    }
  }
  let known_vec: Vec<String> = known.into_iter().collect();

  resolve_tool_policy(PolicyResolutionInput {
    known_tools: &known_vec,
    skill_allowed_tools: &skill_allowed,
    skill_denied_tools: &skill_denied,
    mcp_server_capabilities: mcp_caps,
    skill_mcp_server_allowlist: &mcp_allowlist,
    cli_allow_tools: cli_allow,
    cli_deny_tools: cli_deny,
    fallback_policy: None,
    tool_metadata: &tool_metadata,
  })
}

/// Run MCP discovery, consulting the on-disk cache first. Returns
/// the discovered capabilities + the outcome label for the summary.
///
/// Cache miss / `--refresh-mcp-cache` paths show a spinner while
/// the servers are being spawned — the work happens inside one
/// `SkillBuilder::build_registry` call which contacts every
/// declared MCP server in parallel, so a single spinner accurately
/// represents the wall-clock cost.
async fn run_discovery_with_cache(
  manifest: &agentflow_skills::SkillManifest,
  dir: &Path,
  refresh_mcp_cache: bool,
) -> Result<(McpCapabilityMap, CacheOutcome)> {
  let cache_path = DiscoveryCache::default_path();
  let manifest_hash = hash_mcp_servers(&manifest.mcp_servers);

  if !refresh_mcp_cache && let Some(path) = cache_path.as_ref() {
    let cache = DiscoveryCache::load(path);
    if let Some(entry) = cache.lookup_fresh(&manifest_hash, DEFAULT_TTL) {
      return Ok((from_cache_value(&entry.tools_by_server), CacheOutcome::Hit));
    }
  }

  // Fresh discovery — spinner runs for the duration of
  // `SkillBuilder::build_registry`. The spinner is sent to stderr
  // (indicatif default) so it doesn't corrupt stdout consumers.
  let spinner = ProgressBar::new_spinner();
  spinner.set_style(
    ProgressStyle::with_template("{spinner:.cyan} {msg}")
      .unwrap_or_else(|_| ProgressStyle::default_spinner()),
  );
  spinner.enable_steady_tick(Duration::from_millis(120));
  let server_count = manifest.mcp_servers.len();
  spinner.set_message(format!(
    "Discovering MCP tools for {server_count} server{}...",
    if server_count == 1 { "" } else { "s" }
  ));

  let result = discover_mcp_capabilities(manifest, dir).await;
  spinner.finish_and_clear();
  let caps = result?;

  // Persist on success. Cache write errors are non-fatal — log to
  // stderr but don't fail the inspect call; the operator gets the
  // discovery output and the next inspect will just re-discover.
  if let Some(path) = cache_path.as_ref() {
    let mut cache = DiscoveryCache::load(path);
    cache.upsert(manifest_hash, to_cache_value(&caps));
    if let Err(err) = cache.save(path) {
      eprintln!("⚠  failed to write MCP discovery cache: {err}");
    }
  }

  let outcome = if refresh_mcp_cache {
    CacheOutcome::Refresh
  } else {
    CacheOutcome::Miss
  };
  Ok((caps, outcome))
}

/// Spawn every MCP server declared in the manifest and group the
/// resulting tool registry by server name → tool names. This is
/// expensive (one subprocess per server, JSON-RPC handshake each)
/// — `run_discovery_with_cache` is the entry point most callers
/// should use because it short-circuits on a fresh cache hit.
async fn discover_mcp_capabilities(
  manifest: &agentflow_skills::SkillManifest,
  dir: &Path,
) -> Result<McpCapabilityMap> {
  let registry = SkillBuilder::build_registry(manifest, dir).await?;
  let mut caps: McpCapabilityMap = McpCapabilityMap::new();
  for tool in registry.list() {
    let definition = tool.definition();
    if let (Some(server), Some(mcp_tool)) = (
      definition.metadata.mcp_server_name,
      definition.metadata.mcp_tool_name,
    ) {
      caps.entry(server).or_default().push(mcp_tool);
    }
  }
  // Stable order so the printed output is deterministic across runs.
  for tools in caps.values_mut() {
    tools.sort();
    tools.dedup();
  }
  Ok(caps)
}

fn print_mcp_discovery_summary(caps: &McpCapabilityMap, outcome: CacheOutcome) {
  let badge = match outcome {
    CacheOutcome::Hit => "  source: cache hit",
    CacheOutcome::Miss => "  source: fresh discovery (cached for next run)",
    CacheOutcome::Refresh => "  source: forced re-discovery (--refresh-mcp-cache)",
    CacheOutcome::Skipped => "  source: skipped",
  };
  println!("\nMCP discovery:");
  println!("{badge}");
  if caps.is_empty() {
    println!("  (no MCP-advertised tools found)");
    return;
  }
  for (server, tools) in caps {
    println!(
      "  - {} ({} tool{})",
      server,
      tools.len(),
      if tools.len() == 1 { "" } else { "s" }
    );
    for tool in tools {
      println!("      • {tool}");
    }
  }
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

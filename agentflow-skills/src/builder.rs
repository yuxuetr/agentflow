use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentflow_agents::react::{ReActAgent, ReActConfig};
use agentflow_memory::{MemoryStore, SessionMemory, SqliteMemory};
use agentflow_tools::builtin::{FileTool, HttpTool, ShellTool};
use agentflow_tools::{SandboxPolicy, ToolRegistry};
use tracing::info;

use crate::{
    error::SkillError,
    loader::resolve_knowledge_path,
    manifest::{MemoryConfig, SkillManifest, ToolConfig},
};

/// Assembles a [`ReActAgent`] from a loaded [`SkillManifest`].
///
/// `skill_dir` is the directory containing `skill.toml`; it is used as the
/// base for resolving relative knowledge file paths and the default SQLite db
/// path.
pub struct SkillBuilder;

impl SkillBuilder {
    /// Build a [`ReActAgent`] ready to run.
    pub async fn build(
        manifest: &SkillManifest,
        skill_dir: &Path,
    ) -> Result<ReActAgent, SkillError> {
        info!(
            skill = %manifest.skill.name,
            version = %manifest.skill.version,
            "Building agent from skill manifest"
        );

        // 1. Build persona string (role + optional knowledge context)
        let persona = build_persona(manifest, skill_dir)?;

        // 2. Assemble ReActConfig
        let config = ReActConfig::new(manifest.model.resolved_model())
            .with_persona(persona)
            .with_max_iterations(manifest.model.resolved_max_iterations())
            .with_budget_tokens(manifest.model.resolved_budget_tokens());

        // 3. Build ToolRegistry
        let registry = build_tool_registry(&manifest.tools);

        // 4. Build MemoryStore
        let memory =
            build_memory(manifest.memory.as_ref(), &manifest.skill.name).await?;

        Ok(ReActAgent::new(config, memory, Arc::new(registry)))
    }
}

// ── Persona builder ──────────────────────────────────────────────────────────

fn build_persona(manifest: &SkillManifest, skill_dir: &Path) -> Result<String, SkillError> {
    let mut parts: Vec<String> = Vec::new();

    // Base role
    parts.push(manifest.persona.role.clone());

    // Language hint
    if let Some(lang) = &manifest.persona.language {
        parts.push(format!("\nPlease respond in: {}", lang));
    }

    // Knowledge files injected into context
    if !manifest.knowledge.is_empty() {
        parts.push("\n\n## Knowledge Context".to_string());
        for kc in &manifest.knowledge {
            let paths = resolve_knowledge_path(&kc.path, skill_dir);
            for path in &paths {
                let label = kc
                    .description
                    .clone()
                    .unwrap_or_else(|| path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| kc.path.clone()));
                let content = std::fs::read_to_string(path).map_err(|e| {
                    SkillError::IoError(format!(
                        "Cannot read knowledge file {}: {}",
                        path.display(),
                        e
                    ))
                })?;
                parts.push(format!("\n### {}\n\n{}", label, content.trim()));
            }
        }
    }

    Ok(parts.join("\n"))
}

// ── ToolRegistry builder ─────────────────────────────────────────────────────

fn build_tool_registry(tool_configs: &[ToolConfig]) -> ToolRegistry {
    let mut registry = ToolRegistry::new();

    if tool_configs.is_empty() {
        // No tools declared — return an empty registry.
        return registry;
    }

    // Merge all per-tool constraints into a single SandboxPolicy.
    // Each built-in tool only checks its relevant policy field, so merging is safe.
    let policy = Arc::new(build_sandbox_policy(tool_configs));

    for tool_cfg in tool_configs {
        match tool_cfg.name.to_lowercase().as_str() {
            "shell" => {
                registry.register(Arc::new(ShellTool::new(policy.clone())));
            }
            "file" => {
                registry.register(Arc::new(FileTool::new(policy.clone())));
            }
            "http" => {
                registry.register(Arc::new(HttpTool::new(policy.clone())));
            }
            other => {
                // Already validated by SkillLoader; log and skip unknown tools.
                tracing::warn!(tool = other, "Skipping unknown tool during registry build");
            }
        }
    }

    registry
}

/// Merge all tool constraints into a unified `SandboxPolicy`.
fn build_sandbox_policy(tool_configs: &[ToolConfig]) -> SandboxPolicy {
    let mut allowed_commands: Vec<String> = Vec::new();
    let mut allowed_paths: Vec<PathBuf> = Vec::new();
    let mut allowed_domains: Vec<String> = Vec::new();
    let mut max_exec_time_secs: u64 = 30;

    for tc in tool_configs {
        // Shell commands
        if !tc.allowed_commands.is_empty() {
            allowed_commands.extend(tc.allowed_commands.iter().cloned());
        }
        // File paths
        for p in &tc.allowed_paths {
            let expanded = expand_tilde(p);
            allowed_paths.push(PathBuf::from(expanded));
        }
        // HTTP domains
        allowed_domains.extend(tc.allowed_domains.iter().cloned());
        // Exec time (take the maximum across tools)
        if let Some(t) = tc.max_exec_time_secs {
            max_exec_time_secs = max_exec_time_secs.max(t);
        }
    }

    // Deduplicate
    allowed_commands.sort();
    allowed_commands.dedup();
    allowed_paths.sort();
    allowed_paths.dedup();
    allowed_domains.sort();
    allowed_domains.dedup();

    // If the skill declares a shell tool but leaves allowed_commands empty,
    // use the built-in safe default list rather than allowing everything.
    let has_shell = tool_configs.iter().any(|t| t.name.to_lowercase() == "shell");
    if has_shell && allowed_commands.is_empty() {
        // Default safe command list from SandboxPolicy::default()
        allowed_commands = SandboxPolicy::default().allowed_commands;
    }

    SandboxPolicy {
        allowed_commands,
        allowed_paths,
        allowed_domains,
        max_exec_time_secs,
        max_file_read_bytes: 10 * 1024 * 1024, // 10 MB
    }
}

// ── MemoryStore builder ──────────────────────────────────────────────────────

async fn build_memory(
    config: Option<&MemoryConfig>,
    skill_name: &str,
) -> Result<Box<dyn MemoryStore>, SkillError> {
    match config {
        None => {
            Ok(Box::new(SessionMemory::default_window()))
        }
        Some(mem) if mem.memory_type == "none" => {
            // Explicitly disabled — use in-memory session store.
            Ok(Box::new(SessionMemory::default_window()))
        }
        Some(mem) if mem.memory_type == "sqlite" => {
            let db_path = resolve_db_path(mem.db_path.as_deref(), skill_name);
            // Ensure parent directory exists.
            if let Some(parent) = db_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    SkillError::IoError(format!(
                        "Cannot create memory directory {}: {}",
                        parent.display(),
                        e
                    ))
                })?;
            }
            let store = SqliteMemory::open(&db_path).await?;
            Ok(Box::new(store))
        }
        _ => {
            // "session" or anything unrecognised — use in-memory.
            let window = config
                .map(|m| m.resolved_window_tokens())
                .unwrap_or(8_000);
            Ok(Box::new(SessionMemory::new(window)))
        }
    }
}
/// Resolve
/// Resolve the SQLite db path, expanding `~` and supplying a default.
fn resolve_db_path(db_path: Option<&str>, skill_name: &str) -> PathBuf {
    match db_path {
        Some(p) => PathBuf::from(expand_tilde(p)),
        None => {
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            home.join(".agentflow")
                .join("memory")
                .join(format!("{}.db", skill_name))
        }
    }
}

/// Expand a leading `~` to the user's home directory.
fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") || path == "~" {
        if let Some(home) = dirs::home_dir() {
            return path.replacen('~', &home.to_string_lossy(), 1);
        }
    }
    path.to_string()
}

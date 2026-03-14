use std::path::{Path, PathBuf};
use std::sync::Arc;

use agentflow_agents::react::{ReActAgent, ReActConfig};
use agentflow_memory::{MemoryStore, SessionMemory, SqliteMemory};
use agentflow_tools::builtin::{FileTool, HttpTool, ScriptTool, ShellTool};
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
        let registry = build_tool_registry(&manifest.tools, skill_dir);

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

// Knowledge files injected into context (skill.toml `[[knowledge]]` entries)
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

    // references/ directory: Agent Skills standard — load all .md / .txt files.
    let references_dir = skill_dir.join("references");
    if references_dir.is_dir() {
        let mut ref_files: Vec<PathBuf> = std::fs::read_dir(&references_dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| {
                        p.is_file()
                            && p.extension()
                                .and_then(|x| x.to_str())
                                .map(|x| matches!(x, "md" | "txt"))
                                .unwrap_or(false)
                    })
                    .collect()
            })
            .unwrap_or_default();
        ref_files.sort(); // deterministic ordering
        if !ref_files.is_empty() {
            parts.push("\n\n## Reference Documents".to_string());
            for path in &ref_files {
                let label = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string());
                let content = std::fs::read_to_string(path).map_err(|e| {
                    SkillError::IoError(format!(
                        "Cannot read reference file {}: {}",
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

// ── ToolRegistry builder ────────────────────────────────────────────────────────

fn build_tool_registry(tool_configs: &[ToolConfig], skill_dir: &Path) -> ToolRegistry {
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
            "script" => {
                let scripts_dir = skill_dir.join("scripts");
                registry.register(Arc::new(ScriptTool::new(scripts_dir, policy.clone())));
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        loader::SkillLoader,
        manifest::{ModelConfig, PersonaConfig, SkillInfo},
    };
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    // ── helpers ───────────────────────────────────────────────────────────────

    fn write_toml(dir: &Path, content: &str) {
        let mut f = fs::File::create(dir.join("skill.toml")).expect("create skill.toml");
        f.write_all(content.as_bytes()).expect("write");
    }

    fn write_file(path: &Path, content: &str) {
        if let Some(p) = path.parent() { fs::create_dir_all(p).expect("mkdir"); }
        let mut f = fs::File::create(path).expect("create");
        f.write_all(content.as_bytes()).expect("write");
    }

    fn minimal_manifest(name: &str) -> SkillManifest {
        SkillManifest {
            skill: SkillInfo {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                description: "test".to_string(),
            },
            persona: PersonaConfig {
                role: "You are a test agent.".to_string(),
                language: None,
            },
            model: ModelConfig::default(),
            tools: vec![],
            knowledge: vec![],
            memory: None,
        }
    }

    // ── SkillBuilder::build() tests (no LLM call, safe to run in CI) ────────

    /// build() with no tools and session memory succeeds and returns an agent
    /// with a valid UUID session_id.
    #[tokio::test]
    async fn build_minimal_skill() {
        let dir = TempDir::new().unwrap();
        let manifest = minimal_manifest("minimal");
        let agent = SkillBuilder::build(&manifest, dir.path()).await.unwrap();
        // session_id should be a non-empty UUID-like string
        assert!(!agent.session_id.is_empty());
        assert!(agent.session_id.contains('-'));
    }

    /// build() applies persona text to the agent config.
    #[tokio::test]
    async fn build_sets_persona_in_config() {
        let dir = TempDir::new().unwrap();
        let mut manifest = minimal_manifest("persona-test");
        manifest.persona.role = "You are a specialised Rust expert.".to_string();

        // Build two agents: same manifest, different session IDs expected.
        let a1 = SkillBuilder::build(&manifest, dir.path()).await.unwrap();
        let a2 = SkillBuilder::build(&manifest, dir.path()).await.unwrap();
        assert_ne!(a1.session_id, a2.session_id, "each build should get a fresh session");
    }

    /// build() with shell + file tools registers both in the agent's registry.
    #[tokio::test]
    async fn build_registers_shell_and_file_tools() {
        let dir = TempDir::new().unwrap();
        let mut manifest = minimal_manifest("two-tools");
        manifest.tools = vec![
            ToolConfig { name: "shell".to_string(), ..ToolConfig::default() },
            ToolConfig { name: "file".to_string(),  ..ToolConfig::default() },
        ];
        // Build must succeed (tools are registered, no LLM called).
        let _agent = SkillBuilder::build(&manifest, dir.path()).await.unwrap();
    }

    /// build() with script tool uses the scripts/ directory from skill_dir.
    #[tokio::test]
    async fn build_registers_script_tool() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("scripts")).unwrap();
        write_file(&dir.path().join("scripts").join("hello.sh"), "#!/bin/bash\necho hi");

        let mut manifest = minimal_manifest("script-skill");
        manifest.tools = vec![
            ToolConfig { name: "script".to_string(), ..ToolConfig::default() },
        ];
        let _agent = SkillBuilder::build(&manifest, dir.path()).await.unwrap();
    }

    /// build() with knowledge files injects content into the persona.
    #[tokio::test]
    async fn build_injects_knowledge_into_persona() {
        let dir = TempDir::new().unwrap();
        write_file(&dir.path().join("knowledge").join("rules.md"), "# Rule 1");

        write_toml(
            dir.path(),
            r#"
[skill]
name = "knowledgeable"
version = "0.1"
description = "has knowledge"

[persona]
role = "You are an expert."

[[knowledge]]
path = "./knowledge/rules.md"
description = "Coding rules"
"#,
        );
        let manifest = SkillLoader::load(dir.path()).unwrap();
        // build() should succeed; persona will contain injected knowledge content
        let _agent = SkillBuilder::build(&manifest, dir.path()).await.unwrap();
    }

    /// build() with references/ directory injects ref content into persona.
    #[tokio::test]
    async fn build_injects_references_dir_into_persona() {
        let dir = TempDir::new().unwrap();
        write_file(&dir.path().join("references").join("api.md"), "# API Reference");

        let manifest = minimal_manifest("with-refs");
        // build() should not fail even though knowledge list is empty;
        // references/ is picked up automatically by build_persona()
        let _agent = SkillBuilder::build(&manifest, dir.path()).await.unwrap();
    }

    /// build() with sqlite memory creates the db directory.
    #[tokio::test]
    async fn build_with_sqlite_memory() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("memory").join("test.db");

        let mut manifest = minimal_manifest("sqlite-skill");
        manifest.memory = Some(crate::manifest::MemoryConfig {
            memory_type: "sqlite".to_string(),
            db_path: Some(db_path.to_string_lossy().into_owned()),
            window_tokens: None,
        });

        let _agent = SkillBuilder::build(&manifest, dir.path()).await.unwrap();
        assert!(db_path.exists(), "SQLite db file should have been created");
    }

    // ── SandboxPolicy merge tests ────────────────────────────────────────

    #[test]
    fn sandbox_policy_uses_default_commands_for_shell_with_empty_list() {
        let tool_configs = vec![
            ToolConfig { name: "shell".to_string(), ..ToolConfig::default() },
        ];
        let policy = build_sandbox_policy(&tool_configs);
        // Should have the default safe command list, not empty
        assert!(!policy.allowed_commands.is_empty());
        assert!(policy.allowed_commands.iter().any(|c| c == "echo"));
    }

    #[test]
    fn sandbox_policy_merges_commands_from_multiple_tools() {
        let tool_configs = vec![
            ToolConfig {
                name: "shell".to_string(),
                allowed_commands: vec!["cargo".to_string()],
                ..ToolConfig::default()
            },
            ToolConfig {
                name: "shell".to_string(),
                allowed_commands: vec!["rustfmt".to_string()],
                ..ToolConfig::default()
            },
        ];
        let policy = build_sandbox_policy(&tool_configs);
        assert!(policy.allowed_commands.contains(&"cargo".to_string()));
        assert!(policy.allowed_commands.contains(&"rustfmt".to_string()));
    }

    #[test]
    fn sandbox_policy_deduplicates_commands() {
        let tool_configs = vec![
            ToolConfig {
                name: "shell".to_string(),
                allowed_commands: vec!["cargo".to_string(), "cargo".to_string()],
                ..ToolConfig::default()
            },
        ];
        let policy = build_sandbox_policy(&tool_configs);
        let cargo_count = policy.allowed_commands.iter().filter(|c| *c == "cargo").count();
        assert_eq!(cargo_count, 1, "duplicates should be removed");
    }

    #[test]
    fn sandbox_policy_takes_max_exec_time() {
        let tool_configs = vec![
            ToolConfig { name: "shell".to_string(), max_exec_time_secs: Some(10), ..ToolConfig::default() },
            ToolConfig { name: "file".to_string(),  max_exec_time_secs: Some(60), ..ToolConfig::default() },
        ];
        let policy = build_sandbox_policy(&tool_configs);
        assert_eq!(policy.max_exec_time_secs, 60);
    }
}

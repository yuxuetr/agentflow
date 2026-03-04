use std::path::{Path, PathBuf};

use crate::{error::SkillError, manifest::SkillManifest};

const MANIFEST_FILE: &str = "skill.toml";
const KNOWN_TOOLS: &[&str] = &["shell", "file", "http"];
const KNOWN_MEMORY_TYPES: &[&str] = &["session", "sqlite", "none"];

/// Loads and validates `skill.toml` from a skill directory.
pub struct SkillLoader;

impl SkillLoader {
    /// Load a `SkillManifest` from `skill_dir/skill.toml`.
    pub fn load(skill_dir: &Path) -> Result<SkillManifest, SkillError> {
        let manifest_path = skill_dir.join(MANIFEST_FILE);
        if !manifest_path.exists() {
            return Err(SkillError::ManifestNotFound {
                path: manifest_path.display().to_string(),
            });
        }
        let content = std::fs::read_to_string(&manifest_path)?;
        let manifest: SkillManifest = toml::from_str(&content)?;
        Ok(manifest)
    }

    /// Validate a loaded manifest and return a list of warnings.
    /// Returns `Err` for hard failures, `Ok(warnings)` for soft issues.
    pub fn validate(
        manifest: &SkillManifest,
        skill_dir: &Path,
    ) -> Result<Vec<String>, SkillError> {
        let mut warnings: Vec<String> = Vec::new();

        // ── skill section ───────────────────────────────────────────────────
        if manifest.skill.name.trim().is_empty() {
            return Err(SkillError::ValidationError {
                message: "[skill].name must not be empty".to_string(),
            });
        }
        if manifest.skill.version.trim().is_empty() {
            warnings.push("[skill].version is empty".to_string());
        }
        if manifest.skill.description.trim().is_empty() {
            warnings.push("[skill].description is empty".to_string());
        }

        // ── persona section ─────────────────────────────────────────────────
        if manifest.persona.role.trim().is_empty() {
            return Err(SkillError::ValidationError {
                message: "[persona].role must not be empty".to_string(),
            });
        }

        // ── tools ───────────────────────────────────────────────────────────
        for tool in &manifest.tools {
            let name_lc = tool.name.to_lowercase();
            if !KNOWN_TOOLS.contains(&name_lc.as_str()) {
                return Err(SkillError::UnknownTool {
                    name: tool.name.clone(),
                });
            }
        }

        // ── knowledge ───────────────────────────────────────────────────────
        for kc in &manifest.knowledge {
            let resolved = resolve_knowledge_path(&kc.path, skill_dir);
            if resolved.is_empty() {
                return Err(SkillError::KnowledgeFileNotFound {
                    path: format!(
                        "{} (in {})",
                        kc.path,
                        skill_dir.display()
                    ),
                });
            }
        }

        // ── memory ──────────────────────────────────────────────────────────
        if let Some(mem) = &manifest.memory {
            let t = mem.memory_type.as_str();
            if !KNOWN_MEMORY_TYPES.contains(&t) {
                return Err(SkillError::ValidationError {
                    message: format!(
                        "[memory].type '{}' is unknown. Expected one of: {}",
                        t,
                        KNOWN_MEMORY_TYPES.join(", ")
                    ),
                });
            }
            if t == "sqlite" && manifest.skill.name.trim().is_empty() {
                warnings.push(
                    "[memory] type is sqlite but skill.name is empty; db path may be invalid"
                        .to_string(),
                );
            }
        }

        Ok(warnings)
    }
}

/// Resolve a knowledge path (possibly a glob) relative to `skill_dir`.
/// Returns all matching absolute paths.
pub fn resolve_knowledge_path(pattern: &str, skill_dir: &Path) -> Vec<PathBuf> {
    // If the pattern is absolute, use it directly.
    let base = if Path::new(pattern).is_absolute() {
        pattern.to_string()
    } else {
        skill_dir.join(pattern).to_string_lossy().into_owned()
    };

    // Try as a glob first.
    match glob::glob(&base) {
        Ok(entries) => entries.filter_map(|e| e.ok()).collect(),
        Err(_) => {
            // Fall back to exact path check.
            let p = PathBuf::from(&base);
            if p.exists() { vec![p] } else { vec![] }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn write_manifest(dir: &Path, content: &str) {
        let mut f = std::fs::File::create(dir.join(MANIFEST_FILE))
            .expect("create manifest");
        f.write_all(content.as_bytes()).expect("write manifest");
    }

    #[test]
    fn loads_minimal_manifest() {
        let dir = TempDir::new().unwrap();
        write_manifest(
            dir.path(),
            r#"
[skill]
name = "test"
version = "0.1"
description = "test skill"

[persona]
role = "You are a helpful assistant."
"#,
        );
        let m = SkillLoader::load(dir.path()).unwrap();
        assert_eq!(m.skill.name, "test");
        assert!(m.tools.is_empty());
        assert!(m.knowledge.is_empty());
        assert!(m.memory.is_none());
    }

    #[test]
    fn rejects_unknown_tool() {
        let dir = TempDir::new().unwrap();
        write_manifest(
            dir.path(),
            r#"
[skill]
name = "bad"
version = "0.1"
description = "bad skill"

[persona]
role = "test"

[[tools]]
name = "laser_cannon"
"#,
        );
        let m = SkillLoader::load(dir.path()).unwrap();
        let result = SkillLoader::validate(&m, dir.path());
        assert!(matches!(result, Err(SkillError::UnknownTool { .. })));
    }
}

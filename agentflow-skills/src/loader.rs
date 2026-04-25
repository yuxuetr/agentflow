use std::path::{Path, PathBuf};

use crate::{error::SkillError, manifest::SkillManifest, skill_md::SkillMd};

const MANIFEST_FILE: &str = "skill.toml";
const SKILL_MD_FILE: &str = "SKILL.md";
const KNOWN_TOOLS: &[&str] = &["shell", "file", "http", "script"];
const KNOWN_MEMORY_TYPES: &[&str] = &["session", "sqlite", "none"];

/// Loads and validates a skill manifest from a skill directory.
///
/// Supported manifest formats:
/// - `SKILL.md` is the recommended human-facing skill format.
/// - `skill.toml` is retained for compatibility and structured runtime config.
///
/// If both files exist in the same directory, `skill.toml` is loaded. This
/// preserves existing AgentFlow behavior and lets a structured manifest override
/// the portable `SKILL.md` entrypoint when needed.
pub struct SkillLoader;

impl SkillLoader {
  /// Load a [`SkillManifest`] from `skill_dir`.
  ///
  /// Loads `skill.toml` first when present; falls back to `SKILL.md`.
  pub fn load(skill_dir: &Path) -> Result<SkillManifest, SkillError> {
    let toml_path = skill_dir.join(MANIFEST_FILE);
    if toml_path.exists() {
      let content = std::fs::read_to_string(&toml_path)?;
      let manifest: SkillManifest = toml::from_str(&content)?;
      return Ok(manifest);
    }

    let md_path = skill_dir.join(SKILL_MD_FILE);
    if md_path.exists() {
      let content = std::fs::read_to_string(&md_path)?;
      let skill_md = SkillMd::parse(&content)?;
      return Ok(skill_md.into_manifest());
    }

    Err(SkillError::ManifestNotFound {
      path: format!("{} (tried skill.toml and SKILL.md)", skill_dir.display()),
    })
  }

  /// Validate a loaded manifest and return a list of warnings.
  /// Returns `Err` for hard failures, `Ok(warnings)` for soft issues.
  pub fn validate(manifest: &SkillManifest, skill_dir: &Path) -> Result<Vec<String>, SkillError> {
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
      // "script" tool requires a scripts/ directory to exist.
      if name_lc == "script" {
        let scripts_dir = skill_dir.join("scripts");
        if !scripts_dir.is_dir() {
          return Err(SkillError::ValidationError {
            message: format!(
              "Tool 'script' declared but scripts/ directory not found at {}",
              scripts_dir.display()
            ),
          });
        }
      }
    }

    // ── MCP servers ─────────────────────────────────────────────────────
    for server in &manifest.mcp_servers {
      if server.name.trim().is_empty() {
        return Err(SkillError::ValidationError {
          message: "[[mcp_servers]].name must not be empty".to_string(),
        });
      }
      if server.command.trim().is_empty() {
        return Err(SkillError::ValidationError {
          message: format!(
            "[[mcp_servers]] '{}' command must not be empty",
            server.name
          ),
        });
      }
    }

    // ── knowledge ───────────────────────────────────────────────────────
    for kc in &manifest.knowledge {
      let resolved = resolve_knowledge_path(&kc.path, skill_dir);
      if resolved.is_empty() {
        return Err(SkillError::KnowledgeFileNotFound {
          path: format!("{} (in {})", kc.path, skill_dir.display()),
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
          "[memory] type is sqlite but skill.name is empty; db path may be invalid".to_string(),
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
      if p.exists() {
        vec![p]
      } else {
        vec![]
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;
  use std::io::Write;
  use tempfile::TempDir;

  // ── helpers ───────────────────────────────────────────────────────────────

  fn write_toml(dir: &Path, content: &str) {
    let mut f = fs::File::create(dir.join(MANIFEST_FILE)).expect("create skill.toml");
    f.write_all(content.as_bytes()).expect("write skill.toml");
  }

  fn write_skill_md(dir: &Path, content: &str) {
    let mut f = fs::File::create(dir.join(SKILL_MD_FILE)).expect("create SKILL.md");
    f.write_all(content.as_bytes()).expect("write SKILL.md");
  }

  fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent).expect("create dirs");
    }
    let mut f = fs::File::create(path).expect("create file");
    f.write_all(content.as_bytes()).expect("write file");
  }

  const MINIMAL_TOML: &str = r#"
[skill]
name = "test"
version = "0.1"
description = "test skill"

[persona]
role = "You are a helpful assistant."
"#;

  // ── skill.toml tests ──────────────────────────────────────────────────────

  #[test]
  fn loads_minimal_toml_manifest() {
    let dir = TempDir::new().unwrap();
    write_toml(dir.path(), MINIMAL_TOML);
    let m = SkillLoader::load(dir.path()).unwrap();
    assert_eq!(m.skill.name, "test");
    assert!(m.tools.is_empty());
    assert!(m.knowledge.is_empty());
    assert!(m.memory.is_none());
  }

  #[test]
  fn toml_preferred_over_skill_md_when_both_present() {
    let dir = TempDir::new().unwrap();
    write_toml(dir.path(), MINIMAL_TOML);
    write_skill_md(
      dir.path(),
      "---\nname: md-skill\ndescription: From SKILL.md.\n---\nBody.\n",
    );
    let m = SkillLoader::load(dir.path()).unwrap();
    // skill.toml wins
    assert_eq!(m.skill.name, "test");
  }

  #[test]
  fn rejects_unknown_tool() {
    let dir = TempDir::new().unwrap();
    write_toml(
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

  #[test]
  fn rejects_missing_persona_role() {
    let dir = TempDir::new().unwrap();
    write_toml(
      dir.path(),
      r#"
[skill]
name = "no-persona"
version = "0.1"
description = "test"

[persona]
role = "   "
"#,
    );
    let m = SkillLoader::load(dir.path()).unwrap();
    let result = SkillLoader::validate(&m, dir.path());
    assert!(matches!(result, Err(SkillError::ValidationError { .. })));
  }

  #[test]
  fn warns_on_empty_description() {
    let dir = TempDir::new().unwrap();
    write_toml(
      dir.path(),
      r#"
[skill]
name = "sparse"
version = "0.1"
description = ""

[persona]
role = "test"
"#,
    );
    let m = SkillLoader::load(dir.path()).unwrap();
    let warnings = SkillLoader::validate(&m, dir.path()).unwrap();
    assert!(warnings.iter().any(|w| w.contains("description")));
  }

  // ── knowledge tests ───────────────────────────────────────────────────────

  #[test]
  fn validates_existing_knowledge_file() {
    let dir = TempDir::new().unwrap();
    let kb_path = dir.path().join("knowledge").join("guide.md");
    write_file(&kb_path, "# Guide");
    write_toml(
      dir.path(),
      r#"
[skill]
name = "knows"
version = "0.1"
description = "has knowledge"

[persona]
role = "expert"

[[knowledge]]
path = "./knowledge/guide.md"
"#,
    );
    let m = SkillLoader::load(dir.path()).unwrap();
    let warnings = SkillLoader::validate(&m, dir.path()).unwrap();
    assert!(warnings.is_empty());
  }

  #[test]
  fn rejects_missing_knowledge_file() {
    let dir = TempDir::new().unwrap();
    write_toml(
      dir.path(),
      r#"
[skill]
name = "broken"
version = "0.1"
description = "missing knowledge"

[persona]
role = "expert"

[[knowledge]]
path = "./knowledge/missing.md"
"#,
    );
    let m = SkillLoader::load(dir.path()).unwrap();
    let result = SkillLoader::validate(&m, dir.path());
    assert!(matches!(
      result,
      Err(SkillError::KnowledgeFileNotFound { .. })
    ));
  }

  #[test]
  fn knowledge_glob_matches_multiple_files() {
    let dir = TempDir::new().unwrap();
    write_file(&dir.path().join("knowledge").join("a.md"), "A");
    write_file(&dir.path().join("knowledge").join("b.md"), "B");
    let paths = resolve_knowledge_path("./knowledge/*.md", dir.path());
    assert_eq!(paths.len(), 2);
  }

  // ── script tool tests ─────────────────────────────────────────────────────

  #[test]
  fn validates_script_tool_with_scripts_dir() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("scripts")).unwrap();
    write_file(
      &dir.path().join("scripts").join("run.sh"),
      "#!/bin/bash\necho ok",
    );
    write_toml(
      dir.path(),
      r#"
[skill]
name = "scripter"
version = "0.1"
description = "has scripts"

[persona]
role = "expert"

[[tools]]
name = "script"
"#,
    );
    let m = SkillLoader::load(dir.path()).unwrap();
    let warnings = SkillLoader::validate(&m, dir.path()).unwrap();
    assert!(warnings.is_empty());
  }

  #[test]
  fn rejects_script_tool_without_scripts_dir() {
    let dir = TempDir::new().unwrap();
    write_toml(
      dir.path(),
      r#"
[skill]
name = "no-scripts"
version = "0.1"
description = "missing scripts dir"

[persona]
role = "expert"

[[tools]]
name = "script"
"#,
    );
    let m = SkillLoader::load(dir.path()).unwrap();
    let result = SkillLoader::validate(&m, dir.path());
    assert!(matches!(result, Err(SkillError::ValidationError { .. })));
  }

  // ── SKILL.md loading tests ────────────────────────────────────────────────

  #[test]
  fn loads_skill_md_when_no_toml() {
    let dir = TempDir::new().unwrap();
    write_skill_md(
            dir.path(),
            "---\nname: my-skill\ndescription: A test skill loaded from SKILL.md.\n---\n\nInstructions here.\n",
        );
    let m = SkillLoader::load(dir.path()).unwrap();
    assert_eq!(m.skill.name, "my-skill");
    assert!(m.persona.role.contains("Instructions here."));
  }

  #[test]
  fn skill_md_with_allowed_tools_and_scripts_dir_validates() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("scripts")).unwrap();
    write_file(&dir.path().join("scripts").join("run.py"), "print('ok')");
    write_skill_md(
            dir.path(),
            "---\nname: scripted\ndescription: Has a script tool.\nallowed-tools: script\n---\n\nUse the script tool to run things.\n",
        );
    let m = SkillLoader::load(dir.path()).unwrap();
    assert_eq!(m.tools.len(), 1);
    assert_eq!(m.tools[0].name, "script");
    let warnings = SkillLoader::validate(&m, dir.path()).unwrap();
    assert!(warnings.is_empty());
  }

  #[test]
  fn skill_md_with_script_tool_but_no_scripts_dir_fails_validation() {
    let dir = TempDir::new().unwrap();
    write_skill_md(
            dir.path(),
            "---\nname: broken\ndescription: Declares script tool without scripts dir.\nallowed-tools: script\n---\n\nBody.\n",
        );
    let m = SkillLoader::load(dir.path()).unwrap();
    let result = SkillLoader::validate(&m, dir.path());
    assert!(matches!(result, Err(SkillError::ValidationError { .. })));
  }

  // ── fallback / not-found tests ────────────────────────────────────────────

  #[test]
  fn returns_manifest_not_found_when_neither_file_exists() {
    let dir = TempDir::new().unwrap();
    let result = SkillLoader::load(dir.path());
    assert!(matches!(result, Err(SkillError::ManifestNotFound { .. })));
  }

  // ── memory validation ─────────────────────────────────────────────────────

  #[test]
  fn rejects_unknown_memory_type() {
    let dir = TempDir::new().unwrap();
    write_toml(
      dir.path(),
      r#"
[skill]
name = "memtest"
version = "0.1"
description = "test"

[persona]
role = "agent"

[memory]
type = "redis"
"#,
    );
    let m = SkillLoader::load(dir.path()).unwrap();
    let result = SkillLoader::validate(&m, dir.path());
    assert!(matches!(result, Err(SkillError::ValidationError { .. })));
  }
}

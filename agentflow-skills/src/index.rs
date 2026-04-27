use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{SkillError, SkillLoader};

pub const DEFAULT_INDEX_FILE: &str = "skills.index.toml";
const DEFAULT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRegistryIndex {
  #[serde(default = "default_schema_version")]
  pub schema_version: u32,
  pub name: String,
  #[serde(default)]
  pub description: Option<String>,
  #[serde(default)]
  pub skills: Vec<SkillRegistryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRegistryEntry {
  pub name: String,
  pub version: String,
  pub path: String,
  #[serde(default)]
  pub manifest: Option<String>,
  #[serde(default)]
  pub manifest_sha256: Option<String>,
  #[serde(default)]
  pub aliases: Vec<String>,
  #[serde(default)]
  pub channel: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedSkillRegistryEntry {
  pub name: String,
  pub version: String,
  pub path: PathBuf,
  pub manifest: PathBuf,
  pub manifest_sha256: Option<String>,
  pub aliases: Vec<String>,
  pub channel: Option<String>,
}

impl SkillRegistryIndex {
  pub fn load(path: &Path) -> Result<Self, SkillError> {
    let content = fs::read_to_string(path)?;
    let index: SkillRegistryIndex = toml::from_str(&content)?;
    index.validate_structure()?;
    Ok(index)
  }

  pub fn validate_at(&self, index_path: &Path) -> Result<Vec<String>, SkillError> {
    self.validate_structure()?;
    let index_dir = index_path.parent().unwrap_or_else(|| Path::new("."));
    let mut warnings = Vec::new();
    let mut seen_names = BTreeSet::new();
    let mut seen_lookup_keys = BTreeSet::new();

    for entry in &self.skills {
      validate_version(&entry.version)?;
      let entry_name = entry.name.to_lowercase();
      if !seen_names.insert(entry_name.clone()) || !seen_lookup_keys.insert(entry_name) {
        return Err(SkillError::ValidationError {
          message: format!("Duplicate skill index entry '{}'", entry.name),
        });
      }
      for alias in &entry.aliases {
        let alias_key = alias.to_lowercase();
        if !seen_lookup_keys.insert(alias_key) {
          return Err(SkillError::ValidationError {
            message: format!("Duplicate skill alias '{}' in index '{}'", alias, self.name),
          });
        }
      }

      let resolved = entry.resolve(index_dir)?;
      if !resolved.path.exists() {
        return Err(SkillError::ValidationError {
          message: format!(
            "Indexed skill '{}' path '{}' does not exist",
            entry.name,
            resolved.path.display()
          ),
        });
      }

      let manifest_path = resolved.manifest.clone();
      if !manifest_path.exists() {
        return Err(SkillError::ValidationError {
          message: format!(
            "Indexed skill '{}' manifest '{}' does not exist",
            entry.name,
            manifest_path.display()
          ),
        });
      }

      if let Some(expected) = &entry.manifest_sha256 {
        let actual = sha256_file(&manifest_path)?;
        if normalize_digest(expected) != actual {
          return Err(SkillError::ValidationError {
            message: format!(
              "Indexed skill '{}' manifest hash mismatch (expected {}, got {})",
              entry.name, expected, actual
            ),
          });
        }
      }

      let manifest = SkillLoader::load(&resolved.path)?;
      let mut skill_warnings = SkillLoader::validate(&manifest, &resolved.path)?;
      if manifest.skill.name != entry.name {
        return Err(SkillError::ValidationError {
          message: format!(
            "Indexed skill '{}' does not match manifest skill.name '{}'",
            entry.name, manifest.skill.name
          ),
        });
      }
      if manifest.skill.version != entry.version {
        return Err(SkillError::ValidationError {
          message: format!(
            "Indexed skill '{}' version '{}' does not match manifest version '{}'",
            entry.name, entry.version, manifest.skill.version
          ),
        });
      }

      warnings.append(&mut skill_warnings);
    }

    Ok(warnings)
  }

  pub fn resolve_skill(
    &self,
    skill_name: &str,
    index_path: &Path,
  ) -> Result<ResolvedSkillRegistryEntry, SkillError> {
    self.validate_structure()?;
    let index_dir = index_path.parent().unwrap_or_else(|| Path::new("."));
    let entry = self
      .skills
      .iter()
      .find(|entry| {
        entry.name == skill_name || entry.aliases.iter().any(|alias| alias == skill_name)
      })
      .ok_or_else(|| SkillError::ValidationError {
        message: format!(
          "Skill '{}' not found in registry index '{}'",
          skill_name, self.name
        ),
      })?;
    entry.resolve(index_dir)
  }

  pub fn entries(&self) -> &[SkillRegistryEntry] {
    &self.skills
  }

  fn validate_structure(&self) -> Result<(), SkillError> {
    if self.schema_version != DEFAULT_SCHEMA_VERSION {
      return Err(SkillError::ValidationError {
        message: format!(
          "Unsupported registry index schema_version {} (expected {})",
          self.schema_version, DEFAULT_SCHEMA_VERSION
        ),
      });
    }
    if self.name.trim().is_empty() {
      return Err(SkillError::ValidationError {
        message: "Registry index name must not be empty".to_string(),
      });
    }
    if self.skills.is_empty() {
      return Err(SkillError::ValidationError {
        message: "Registry index must contain at least one skill".to_string(),
      });
    }
    Ok(())
  }
}

impl SkillRegistryEntry {
  fn resolve(&self, index_dir: &Path) -> Result<ResolvedSkillRegistryEntry, SkillError> {
    validate_version(&self.version)?;
    let path = resolve_index_path(index_dir, &self.path);
    let manifest = self
      .manifest
      .as_deref()
      .map(|manifest| resolve_index_path(&path, manifest))
      .unwrap_or_else(|| detect_manifest_path(&path).unwrap_or_else(|| path.join("SKILL.md")));

    Ok(ResolvedSkillRegistryEntry {
      name: self.name.clone(),
      version: self.version.clone(),
      path,
      manifest,
      manifest_sha256: self.manifest_sha256.clone(),
      aliases: self.aliases.clone(),
      channel: self.channel.clone(),
    })
  }
}

fn resolve_index_path(base_dir: &Path, raw: &str) -> PathBuf {
  let path = PathBuf::from(raw);
  if path.is_relative() {
    base_dir.join(path)
  } else {
    path
  }
}

fn detect_manifest_path(skill_dir: &Path) -> Option<PathBuf> {
  let toml_path = skill_dir.join("skill.toml");
  if toml_path.exists() {
    return Some(toml_path);
  }
  let skill_md = skill_dir.join("SKILL.md");
  if skill_md.exists() {
    return Some(skill_md);
  }
  None
}

fn validate_version(version: &str) -> Result<(), SkillError> {
  Version::parse(version)
    .map(|_| ())
    .map_err(|e| SkillError::ValidationError {
      message: format!("Invalid skill version '{}': {}", version, e),
    })
}

fn sha256_file(path: &Path) -> Result<String, SkillError> {
  let bytes = fs::read(path)?;
  let mut hasher = Sha256::new();
  hasher.update(bytes);
  Ok(format!("{:x}", hasher.finalize()))
}

fn normalize_digest(value: &str) -> String {
  value.trim().trim_start_matches("sha256:").to_lowercase()
}

fn default_schema_version() -> u32 {
  DEFAULT_SCHEMA_VERSION
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::Write;
  use tempfile::TempDir;

  fn write_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
      fs::create_dir_all(parent).unwrap();
    }
    fs::File::create(path)
      .and_then(|mut file| file.write_all(content.as_bytes()))
      .unwrap();
  }

  fn sample_skill(dir: &Path, version: &str) {
    write_file(
      &dir.join("skill.toml"),
      &format!(
        r#"
[skill]
name = "sample-skill"
version = "{version}"
description = "Sample skill."

[persona]
role = "Sample persona."
"#
      ),
    );
    let manifest_path = dir.join("skill.toml");
    let hash = sha256_file(&manifest_path).unwrap();
    let index = SkillRegistryIndex {
      schema_version: 1,
      name: "org".to_string(),
      description: Some("Org".to_string()),
      skills: vec![SkillRegistryEntry {
        name: "sample-skill".to_string(),
        version: version.to_string(),
        path: ".".to_string(),
        manifest: Some("skill.toml".to_string()),
        manifest_sha256: Some(hash),
        aliases: vec!["sample".to_string()],
        channel: Some("stable".to_string()),
      }],
    };
    write_file(
      &dir.join("skills.index.toml"),
      &toml::to_string(&index).unwrap(),
    );
  }

  #[test]
  fn loads_and_resolves_skill_index() {
    let dir = TempDir::new().unwrap();
    sample_skill(dir.path(), "1.2.3");

    let index = SkillRegistryIndex::load(&dir.path().join("skills.index.toml")).unwrap();
    let warnings = index
      .validate_at(&dir.path().join("skills.index.toml"))
      .unwrap();
    assert!(warnings.is_empty());

    let resolved = index
      .resolve_skill("sample", &dir.path().join("skills.index.toml"))
      .unwrap();
    assert_eq!(resolved.name, "sample-skill");
    assert_eq!(resolved.version, "1.2.3");
  }

  #[test]
  fn rejects_version_mismatch() {
    let dir = TempDir::new().unwrap();
    sample_skill(dir.path(), "1.2.3");
    let mut index = SkillRegistryIndex::load(&dir.path().join("skills.index.toml")).unwrap();
    index.skills[0].version = "1.2.4".to_string();

    let result = index.validate_at(&dir.path().join("skills.index.toml"));
    assert!(matches!(result, Err(SkillError::ValidationError { .. })));
  }
}

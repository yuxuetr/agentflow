use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{SkillError, SkillRegistryIndex};

const DEFAULT_MARKETPLACE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMarketplace {
  #[serde(default = "default_marketplace_schema_version")]
  pub schema_version: u32,
  pub name: String,
  #[serde(default)]
  pub description: Option<String>,
  #[serde(default)]
  pub homepage: Option<String>,
  #[serde(default)]
  pub indexes: Vec<SkillMarketplaceIndex>,
  #[serde(default)]
  pub featured: Vec<FeaturedSkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMarketplaceIndex {
  pub name: String,
  #[serde(default = "default_index_kind")]
  pub kind: String,
  pub source: String,
  #[serde(default)]
  pub description: Option<String>,
  #[serde(default)]
  pub trust: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturedSkill {
  pub skill: String,
  pub index: String,
  #[serde(default)]
  pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketplaceSkillListing {
  pub index_name: String,
  pub index_source: PathBuf,
  pub skill_name: String,
  pub version: String,
  pub channel: Option<String>,
  pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketplaceResolvedSkill {
  pub index_name: String,
  pub index_source: PathBuf,
  pub skill_name: String,
  pub version: String,
}

impl SkillMarketplace {
  pub fn load(path: &Path) -> Result<Self, SkillError> {
    let content = fs::read_to_string(path)?;
    let marketplace: SkillMarketplace = toml::from_str(&content)?;
    marketplace.validate_structure()?;
    Ok(marketplace)
  }

  pub fn validate_at(&self, marketplace_path: &Path) -> Result<Vec<String>, SkillError> {
    self.validate_structure()?;
    let base_dir = marketplace_path.parent().unwrap_or_else(|| Path::new("."));
    let mut warnings = Vec::new();
    let mut seen_indexes = BTreeSet::new();

    for index in &self.indexes {
      if !seen_indexes.insert(index.name.to_lowercase()) {
        return Err(SkillError::ValidationError {
          message: format!("Duplicate marketplace index '{}'", index.name),
        });
      }

      match index.kind.as_str() {
        "local" | "organization" => {
          let index_path = index.resolve_source(base_dir)?;
          let registry = SkillRegistryIndex::load(&index_path)?;
          let mut index_warnings = registry.validate_at(&index_path)?;
          warnings.append(&mut index_warnings);
        }
        "remote" => warnings.push(format!(
          "Remote marketplace index '{}' is declared but not fetched by local validation",
          index.name
        )),
        other => {
          return Err(SkillError::ValidationError {
            message: format!(
              "Unsupported marketplace index kind '{}' for '{}'",
              other, index.name
            ),
          });
        }
      }
    }

    for featured in &self.featured {
      if !self
        .indexes
        .iter()
        .any(|index| index.name == featured.index)
      {
        return Err(SkillError::ValidationError {
          message: format!(
            "Featured skill '{}' references missing index '{}'",
            featured.skill, featured.index
          ),
        });
      }
    }

    Ok(warnings)
  }

  pub fn list_local_skills(
    &self,
    marketplace_path: &Path,
  ) -> Result<Vec<MarketplaceSkillListing>, SkillError> {
    self.validate_structure()?;
    let base_dir = marketplace_path.parent().unwrap_or_else(|| Path::new("."));
    let mut listings = Vec::new();

    for index in &self.indexes {
      if matches!(index.kind.as_str(), "local" | "organization") {
        let index_path = index.resolve_source(base_dir)?;
        let registry = SkillRegistryIndex::load(&index_path)?;
        for entry in registry.entries() {
          listings.push(MarketplaceSkillListing {
            index_name: index.name.clone(),
            index_source: index_path.clone(),
            skill_name: entry.name.clone(),
            version: entry.version.clone(),
            channel: entry.channel.clone(),
            aliases: entry.aliases.clone(),
          });
        }
      }
    }

    Ok(listings)
  }

  pub fn resolve_local_skill(
    &self,
    skill: &str,
    marketplace_path: &Path,
  ) -> Result<MarketplaceResolvedSkill, SkillError> {
    let base_dir = marketplace_path.parent().unwrap_or_else(|| Path::new("."));

    for index in &self.indexes {
      if !matches!(index.kind.as_str(), "local" | "organization") {
        continue;
      }
      let index_path = index.resolve_source(base_dir)?;
      let registry = SkillRegistryIndex::load(&index_path)?;
      if let Ok(resolved) = registry.resolve_skill(skill, &index_path) {
        return Ok(MarketplaceResolvedSkill {
          index_name: index.name.clone(),
          index_source: index_path,
          skill_name: resolved.name,
          version: resolved.version,
        });
      }
    }

    Err(SkillError::ValidationError {
      message: format!("Skill '{}' not found in local marketplace indexes", skill),
    })
  }

  pub fn indexes(&self) -> &[SkillMarketplaceIndex] {
    &self.indexes
  }

  fn validate_structure(&self) -> Result<(), SkillError> {
    if self.schema_version != DEFAULT_MARKETPLACE_SCHEMA_VERSION {
      return Err(SkillError::ValidationError {
        message: format!(
          "Unsupported marketplace schema_version {} (expected {})",
          self.schema_version, DEFAULT_MARKETPLACE_SCHEMA_VERSION
        ),
      });
    }
    if self.name.trim().is_empty() {
      return Err(SkillError::ValidationError {
        message: "Marketplace name must not be empty".to_string(),
      });
    }
    if self.indexes.is_empty() {
      return Err(SkillError::ValidationError {
        message: "Marketplace must contain at least one index".to_string(),
      });
    }
    Ok(())
  }
}

impl SkillMarketplaceIndex {
  pub fn resolve_source(&self, base_dir: &Path) -> Result<PathBuf, SkillError> {
    if self.source.trim().is_empty() {
      return Err(SkillError::ValidationError {
        message: format!("Marketplace index '{}' source must not be empty", self.name),
      });
    }
    let source = PathBuf::from(&self.source);
    Ok(if source.is_relative() {
      base_dir.join(source)
    } else {
      source
    })
  }
}

fn default_marketplace_schema_version() -> u32 {
  DEFAULT_MARKETPLACE_SCHEMA_VERSION
}

fn default_index_kind() -> String {
  "local".to_string()
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

  fn write_skill_and_index(root: &Path) {
    write_file(
      &root.join("skills/sample/skill.toml"),
      r#"
[skill]
name = "sample-skill"
version = "1.0.0"
description = "Sample."

[persona]
role = "Sample."
"#,
    );
    write_file(
      &root.join("skills.index.toml"),
      r#"
schema_version = 1
name = "local"

[[skills]]
name = "sample-skill"
version = "1.0.0"
path = "skills/sample"
manifest = "skill.toml"
aliases = ["sample"]
"#,
    );
  }

  #[test]
  fn marketplace_lists_and_resolves_local_indexes() {
    let dir = TempDir::new().unwrap();
    write_skill_and_index(dir.path());
    write_file(
      &dir.path().join("marketplace.toml"),
      r#"
schema_version = 1
name = "local-marketplace"

[[indexes]]
name = "local"
kind = "organization"
source = "skills.index.toml"
"#,
    );

    let path = dir.path().join("marketplace.toml");
    let marketplace = SkillMarketplace::load(&path).unwrap();
    marketplace.validate_at(&path).unwrap();

    let listings = marketplace.list_local_skills(&path).unwrap();
    assert_eq!(listings.len(), 1);
    assert_eq!(listings[0].skill_name, "sample-skill");

    let resolved = marketplace.resolve_local_skill("sample", &path).unwrap();
    assert_eq!(resolved.index_name, "local");
    assert_eq!(resolved.skill_name, "sample-skill");
  }
}

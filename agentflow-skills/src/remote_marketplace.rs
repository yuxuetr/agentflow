//! Unified remote marketplace manifest for Skills and Plugins.
//!
//! This schema is intentionally package-type neutral. It is the catalog format
//! the future `agentflow marketplace ...` CLI will fetch over read-only HTTP
//! before installing either a Skill package or a Plugin package.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::SkillError;

pub const DEFAULT_REMOTE_MARKETPLACE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteMarketplaceManifest {
  #[serde(default = "default_schema_version")]
  pub schema_version: u32,
  pub name: String,
  #[serde(default)]
  pub description: Option<String>,
  #[serde(default)]
  pub homepage: Option<String>,
  #[serde(default)]
  pub entries: Vec<RemoteMarketplaceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteMarketplaceEntry {
  pub name: String,
  pub version: String,
  #[serde(rename = "type")]
  pub package_type: MarketplacePackageType,
  pub source: MarketplaceSource,
  #[serde(default)]
  pub signature: Option<MarketplaceSignature>,
  #[serde(default)]
  pub aliases: Vec<String>,
  #[serde(default)]
  pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketplacePackageType {
  Skill,
  Plugin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceSource {
  /// URL of the read-only registry document this entry came from.
  pub registry_url: String,
  /// URL of the package archive or repository snapshot to install.
  pub artifact_url: String,
  /// SHA-256 digest of the artifact. Accepts raw 64-char hex or
  /// `sha256:<hex>`; validation normalizes both forms.
  pub checksum_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceSignature {
  pub algorithm: String,
  pub key_id: String,
  pub value: String,
}

impl RemoteMarketplaceManifest {
  pub fn load(path: &Path) -> Result<Self, SkillError> {
    let content = fs::read_to_string(path)?;
    let manifest: RemoteMarketplaceManifest = toml::from_str(&content)?;
    manifest.validate()?;
    Ok(manifest)
  }

  pub fn validate(&self) -> Result<(), SkillError> {
    if self.schema_version != DEFAULT_REMOTE_MARKETPLACE_SCHEMA_VERSION {
      return Err(validation_error(format!(
        "Unsupported remote marketplace schema_version {} (expected {})",
        self.schema_version, DEFAULT_REMOTE_MARKETPLACE_SCHEMA_VERSION
      )));
    }
    if self.name.trim().is_empty() {
      return Err(validation_error(
        "Remote marketplace name must not be empty".to_string(),
      ));
    }
    if self.entries.is_empty() {
      return Err(validation_error(
        "Remote marketplace must contain at least one entry".to_string(),
      ));
    }

    let mut entry_keys = BTreeSet::new();
    let mut lookup_keys = BTreeSet::new();
    for entry in &self.entries {
      entry.validate()?;
      let entry_key = (
        entry.package_type,
        entry.name.to_ascii_lowercase(),
        entry.version.clone(),
      );
      if !entry_keys.insert(entry_key) {
        return Err(validation_error(format!(
          "Duplicate marketplace entry '{}@{}' for type {:?}",
          entry.name, entry.version, entry.package_type
        )));
      }

      let lookup_key = (entry.package_type, entry.name.to_ascii_lowercase());
      if !lookup_keys.insert(lookup_key) {
        return Err(validation_error(format!(
          "Duplicate marketplace package name '{}' for type {:?}",
          entry.name, entry.package_type
        )));
      }
      for alias in &entry.aliases {
        let alias = alias.trim();
        if alias.is_empty() {
          return Err(validation_error(format!(
            "Marketplace entry '{}' has an empty alias",
            entry.name
          )));
        }
        let alias_key = (entry.package_type, alias.to_ascii_lowercase());
        if !lookup_keys.insert(alias_key) {
          return Err(validation_error(format!(
            "Duplicate marketplace alias '{}' for type {:?}",
            alias, entry.package_type
          )));
        }
      }
    }
    Ok(())
  }

  pub fn entries(&self) -> &[RemoteMarketplaceEntry] {
    &self.entries
  }
}

impl RemoteMarketplaceEntry {
  pub fn validate(&self) -> Result<(), SkillError> {
    if self.name.trim().is_empty() {
      return Err(validation_error(
        "Marketplace entry name must not be empty".to_string(),
      ));
    }
    Version::parse(&self.version).map_err(|err| {
      validation_error(format!(
        "Invalid marketplace entry version '{}': {}",
        self.version, err
      ))
    })?;
    self.source.validate(&self.name)?;
    if let Some(signature) = &self.signature {
      signature.validate(&self.name)?;
    }
    Ok(())
  }
}

impl MarketplaceSource {
  pub fn normalized_checksum(&self) -> Result<String, SkillError> {
    normalize_sha256(&self.checksum_sha256)
  }

  fn validate(&self, entry_name: &str) -> Result<(), SkillError> {
    validate_http_url("registry_url", entry_name, &self.registry_url)?;
    validate_http_url("artifact_url", entry_name, &self.artifact_url)?;
    self.normalized_checksum()?;
    Ok(())
  }
}

impl MarketplaceSignature {
  fn validate(&self, entry_name: &str) -> Result<(), SkillError> {
    if self.algorithm.trim().is_empty() {
      return Err(validation_error(format!(
        "Marketplace entry '{}' signature algorithm must not be empty",
        entry_name
      )));
    }
    if self.key_id.trim().is_empty() {
      return Err(validation_error(format!(
        "Marketplace entry '{}' signature key_id must not be empty",
        entry_name
      )));
    }
    if self.value.trim().is_empty() {
      return Err(validation_error(format!(
        "Marketplace entry '{}' signature value must not be empty",
        entry_name
      )));
    }
    Ok(())
  }
}

fn validate_http_url(field: &str, entry_name: &str, value: &str) -> Result<(), SkillError> {
  let value = value.trim();
  if value.is_empty() {
    return Err(validation_error(format!(
      "Marketplace entry '{}' source.{} must not be empty",
      entry_name, field
    )));
  }
  if !(value.starts_with("https://") || value.starts_with("http://")) {
    return Err(validation_error(format!(
      "Marketplace entry '{}' source.{} must be an http(s) URL",
      entry_name, field
    )));
  }
  Ok(())
}

fn normalize_sha256(value: &str) -> Result<String, SkillError> {
  let digest = value.trim().strip_prefix("sha256:").unwrap_or(value.trim());
  if digest.len() != 64 || !digest.chars().all(|ch| ch.is_ascii_hexdigit()) {
    return Err(validation_error(format!(
      "Invalid artifact checksum '{}': expected sha256:<64 hex chars>",
      value
    )));
  }
  Ok(digest.to_ascii_lowercase())
}

fn validation_error(message: String) -> SkillError {
  SkillError::ValidationError { message }
}

fn default_schema_version() -> u32 {
  DEFAULT_REMOTE_MARKETPLACE_SCHEMA_VERSION
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::Write;
  use tempfile::TempDir;

  const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

  fn valid_manifest() -> RemoteMarketplaceManifest {
    RemoteMarketplaceManifest {
      schema_version: DEFAULT_REMOTE_MARKETPLACE_SCHEMA_VERSION,
      name: "agentflow-remote".into(),
      description: None,
      homepage: Some("https://example.com".into()),
      entries: vec![
        RemoteMarketplaceEntry {
          name: "rust-expert".into(),
          version: "1.2.3".into(),
          package_type: MarketplacePackageType::Skill,
          source: MarketplaceSource {
            registry_url: "https://registry.example.com/marketplace.toml".into(),
            artifact_url: "https://registry.example.com/skills/rust-expert.tar.gz".into(),
            checksum_sha256: format!("sha256:{DIGEST}"),
          },
          signature: Some(MarketplaceSignature {
            algorithm: "minisign".into(),
            key_id: "agentflow-dev".into(),
            value: "trusted-signature".into(),
          }),
          aliases: vec!["rust".into()],
          description: Some("Rust review skill".into()),
        },
        RemoteMarketplaceEntry {
          name: "echo-plugin".into(),
          version: "0.1.0".into(),
          package_type: MarketplacePackageType::Plugin,
          source: MarketplaceSource {
            registry_url: "https://registry.example.com/marketplace.toml".into(),
            artifact_url: "https://registry.example.com/plugins/echo-plugin.tar.gz".into(),
            checksum_sha256: DIGEST.to_string(),
          },
          signature: None,
          aliases: Vec::new(),
          description: None,
        },
      ],
    }
  }

  #[test]
  fn remote_marketplace_accepts_skill_and_plugin_entries() {
    let manifest = valid_manifest();
    manifest.validate().unwrap();
    assert_eq!(manifest.entries().len(), 2);
    assert_eq!(
      manifest.entries()[0].source.normalized_checksum().unwrap(),
      DIGEST
    );
  }

  #[test]
  fn remote_marketplace_loads_from_toml() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("marketplace.remote.toml");
    let mut file = fs::File::create(&path).unwrap();
    write!(
      file,
      r#"
schema_version = 1
name = "remote"

[[entries]]
name = "rust-expert"
version = "1.0.0"
type = "skill"
aliases = ["rust"]

[entries.source]
registry_url = "https://registry.example.com/marketplace.toml"
artifact_url = "https://registry.example.com/rust-expert.tar.gz"
checksum_sha256 = "sha256:{DIGEST}"

[entries.signature]
algorithm = "minisign"
key_id = "agentflow-dev"
value = "abc"
"#
    )
    .unwrap();

    let manifest = RemoteMarketplaceManifest::load(&path).unwrap();
    assert_eq!(manifest.name, "remote");
    assert_eq!(
      manifest.entries()[0].package_type,
      MarketplacePackageType::Skill
    );
  }

  #[test]
  fn remote_marketplace_rejects_duplicate_lookup_keys_per_type() {
    let mut manifest = valid_manifest();
    manifest.entries.push(RemoteMarketplaceEntry {
      name: "other".into(),
      version: "0.1.0".into(),
      package_type: MarketplacePackageType::Skill,
      source: MarketplaceSource {
        registry_url: "https://registry.example.com/marketplace.toml".into(),
        artifact_url: "https://registry.example.com/other.tar.gz".into(),
        checksum_sha256: DIGEST.to_string(),
      },
      signature: None,
      aliases: vec!["rust".into()],
      description: None,
    });

    let err = manifest.validate().unwrap_err();
    assert!(err.to_string().contains("Duplicate marketplace alias"));
  }

  #[test]
  fn remote_marketplace_allows_same_name_across_skill_and_plugin() {
    let mut manifest = valid_manifest();
    manifest.entries.push(RemoteMarketplaceEntry {
      name: "rust-expert".into(),
      version: "0.1.0".into(),
      package_type: MarketplacePackageType::Plugin,
      source: MarketplaceSource {
        registry_url: "https://registry.example.com/marketplace.toml".into(),
        artifact_url: "https://registry.example.com/rust-plugin.tar.gz".into(),
        checksum_sha256: DIGEST.to_string(),
      },
      signature: None,
      aliases: Vec::new(),
      description: None,
    });

    manifest.validate().unwrap();
  }

  #[test]
  fn remote_marketplace_rejects_invalid_checksum() {
    let mut manifest = valid_manifest();
    manifest.entries[0].source.checksum_sha256 = "sha256:not-a-digest".into();

    let err = manifest.validate().unwrap_err();
    assert!(err.to_string().contains("Invalid artifact checksum"));
  }

  #[test]
  fn remote_marketplace_rejects_non_http_sources() {
    let mut manifest = valid_manifest();
    manifest.entries[0].source.artifact_url = "file:///tmp/pkg.tar.gz".into();

    let err = manifest.validate().unwrap_err();
    assert!(err.to_string().contains("must be an http(s) URL"));
  }
}

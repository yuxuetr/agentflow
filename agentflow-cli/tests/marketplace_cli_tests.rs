use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::io::Cursor;
use std::path::Path;
use tempfile::TempDir;

use agentflow_skills::{MarketplacePackageType, MarketplaceSignature, MarketplaceSource};
use agentflow_skills::{RemoteMarketplaceCache, RemoteMarketplaceEntry};

const DIGEST: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn write_marketplace(path: &Path, checksum: &str) {
  fs::write(
    path,
    format!(
      r#"
schema_version = 1
name = "remote-test"
description = "Test remote marketplace"

[[entries]]
name = "rust-expert"
version = "1.0.0"
type = "skill"
aliases = ["rust"]
description = "Rust review skill"

[entries.source]
registry_url = "https://registry.example.com/marketplace.toml"
artifact_url = "https://registry.example.com/rust-expert.tar.gz"
checksum_sha256 = "{checksum}"

[entries.signature]
algorithm = "checksum-sha256"
key_id = "test"
value = "{checksum}"

[[entries]]
name = "echo-plugin"
version = "0.1.0"
type = "plugin"

[entries.source]
registry_url = "https://registry.example.com/marketplace.toml"
artifact_url = "https://registry.example.com/echo-plugin.tar.gz"
checksum_sha256 = "{DIGEST}"
"#
    ),
  )
  .unwrap();
}

fn entry_for_bytes(bytes: &[u8]) -> RemoteMarketplaceEntry {
  let checksum = sha256_hex(bytes);
  RemoteMarketplaceEntry {
    name: "rust-expert".into(),
    version: "1.0.0".into(),
    package_type: MarketplacePackageType::Skill,
    source: MarketplaceSource {
      registry_url: "https://registry.example.com/marketplace.toml".into(),
      artifact_url: "https://registry.example.com/rust-expert.tar.gz".into(),
      checksum_sha256: checksum.clone(),
    },
    signature: Some(MarketplaceSignature {
      algorithm: "checksum-sha256".into(),
      key_id: "test".into(),
      value: checksum,
    }),
    aliases: vec!["rust".into()],
    description: Some("Rust review skill".into()),
  }
}

#[cfg(feature = "plugin")]
fn plugin_entry_for_bytes(bytes: &[u8]) -> RemoteMarketplaceEntry {
  let checksum = sha256_hex(bytes);
  RemoteMarketplaceEntry {
    name: "echo-plugin".into(),
    version: "0.1.0".into(),
    package_type: MarketplacePackageType::Plugin,
    source: MarketplaceSource {
      registry_url: "https://registry.example.com/marketplace.toml".into(),
      artifact_url: "https://registry.example.com/echo-plugin.tar".into(),
      checksum_sha256: checksum.clone(),
    },
    signature: Some(MarketplaceSignature {
      algorithm: "checksum-sha256".into(),
      key_id: "test".into(),
      value: checksum,
    }),
    aliases: vec![],
    description: Some("Echo plugin".into()),
  }
}

fn write_marketplace_for_entry(path: &Path, entry: &RemoteMarketplaceEntry) {
  let aliases = if entry.aliases.is_empty() {
    String::new()
  } else {
    format!(
      "aliases = [{}]\n",
      entry
        .aliases
        .iter()
        .map(|alias| format!("\"{alias}\""))
        .collect::<Vec<_>>()
        .join(", ")
    )
  };
  let signature = entry
    .signature
    .as_ref()
    .map_or_else(String::new, |signature| {
      format!(
        r#"
[entries.signature]
algorithm = "{}"
key_id = "{}"
value = "{}"
"#,
        signature.algorithm, signature.key_id, signature.value
      )
    });
  fs::write(
    path,
    format!(
      r#"
schema_version = 1
name = "remote-test"

[[entries]]
name = "{}"
version = "{}"
type = "{}"
{}description = "{}"

[entries.source]
registry_url = "{}"
artifact_url = "{}"
checksum_sha256 = "{}"
{}
"#,
      entry.name,
      entry.version,
      entry.package_type.as_str(),
      aliases,
      entry.description.as_deref().unwrap_or_default(),
      entry.source.registry_url,
      entry.source.artifact_url,
      entry.source.checksum_sha256,
      signature
    ),
  )
  .unwrap();
}

fn tar_bytes(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
  let mut bytes = Vec::new();
  {
    let cursor = Cursor::new(&mut bytes);
    let mut builder = tar::Builder::new(cursor);
    for (path, content, mode) in entries {
      let mut header = tar::Header::new_gnu();
      header.set_size(content.len() as u64);
      header.set_mode(*mode);
      header.set_cksum();
      builder
        .append_data(&mut header, path, Cursor::new(*content))
        .unwrap();
    }
    builder.finish().unwrap();
  }
  bytes
}

fn sha256_hex(bytes: &[u8]) -> String {
  use sha2::{Digest, Sha256};
  let mut hasher = Sha256::new();
  hasher.update(bytes);
  format!("{:x}", hasher.finalize())
}

#[test]
fn marketplace_search_lists_matching_packages() {
  let work = TempDir::new().unwrap();
  let marketplace = work.path().join("marketplace.toml");
  write_marketplace(&marketplace, DIGEST);

  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "marketplace",
      "search",
      marketplace.to_str().unwrap(),
      "rust",
      "--type",
      "skill",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Marketplace: remote-test"))
    .stdout(predicate::str::contains("rust-expert @ 1.0.0"))
    .stdout(predicate::str::contains("type: skill"));
}

#[test]
fn marketplace_update_writes_registry_cache() {
  let work = TempDir::new().unwrap();
  let marketplace = work.path().join("marketplace.toml");
  let cache = work.path().join("cache");
  write_marketplace(&marketplace, DIGEST);

  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "marketplace",
      "update",
      marketplace.to_str().unwrap(),
      "--cache-dir",
      cache.to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains(
      "Updated marketplace registry cache",
    ));

  assert!(cache.join("registries").join("remote-test.toml").is_file());
}

#[test]
fn marketplace_verify_checks_cached_artifact() {
  let work = TempDir::new().unwrap();
  let marketplace = work.path().join("marketplace.toml");
  let cache_dir = work.path().join("cache");
  let bytes = b"verified package";
  let entry = entry_for_bytes(bytes);
  write_marketplace(&marketplace, &entry.source.checksum_sha256);
  RemoteMarketplaceCache::new(&cache_dir)
    .cache_artifact_bytes(&entry, bytes)
    .unwrap();

  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "marketplace",
      "verify",
      marketplace.to_str().unwrap(),
      "rust-expert",
      "--type",
      "skill",
      "--cache-dir",
      cache_dir.to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains(
      "Verified skill package: rust-expert",
    ))
    .stdout(predicate::str::contains("signature_checked: true"));
}

#[test]
fn marketplace_install_skill_package_from_verified_cache() {
  let work = TempDir::new().unwrap();
  let marketplace = work.path().join("marketplace.toml");
  let cache_dir = work.path().join("cache");
  let install_dir = work.path().join("skills");
  let package = tar_bytes(&[(
    "rust-expert/SKILL.md",
    br#"---
name: rust-expert
description: Rust review skill
allowed-tools: file
---

# Rust Expert

Review Rust code.
"#,
    0o644,
  )]);
  let entry = entry_for_bytes(&package);
  write_marketplace_for_entry(&marketplace, &entry);
  RemoteMarketplaceCache::new(&cache_dir)
    .cache_artifact_bytes(&entry, &package)
    .unwrap();

  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "marketplace",
      "install",
      marketplace.to_str().unwrap(),
      "rust-expert",
      "--type",
      "skill",
      "--cache-dir",
      cache_dir.to_str().unwrap(),
      "--dir",
      install_dir.to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains(
      "Cached skill package: rust-expert",
    ))
    .stdout(predicate::str::contains(
      "Installed skill package: rust-expert",
    ));

  assert!(install_dir.join("rust-expert").join("SKILL.md").is_file());
}

#[test]
fn marketplace_install_cache_only_skips_unpack() {
  let work = TempDir::new().unwrap();
  let marketplace = work.path().join("marketplace.toml");
  let cache_dir = work.path().join("cache");
  let install_dir = work.path().join("skills");
  let package = tar_bytes(&[(
    "rust-expert/SKILL.md",
    br#"---
name: rust-expert
description: Rust review skill
---

# Rust Expert
"#,
    0o644,
  )]);
  let entry = entry_for_bytes(&package);
  write_marketplace_for_entry(&marketplace, &entry);
  RemoteMarketplaceCache::new(&cache_dir)
    .cache_artifact_bytes(&entry, &package)
    .unwrap();

  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "marketplace",
      "install",
      marketplace.to_str().unwrap(),
      "rust-expert",
      "--type",
      "skill",
      "--cache-dir",
      cache_dir.to_str().unwrap(),
      "--dir",
      install_dir.to_str().unwrap(),
      "--cache-only",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("cache_only: true"));

  assert!(!install_dir.join("rust-expert").exists());
}

#[cfg(feature = "plugin")]
#[test]
fn marketplace_install_plugin_package_from_verified_cache() {
  let work = TempDir::new().unwrap();
  let marketplace = work.path().join("marketplace.toml");
  let cache_dir = work.path().join("cache");
  let install_dir = work.path().join("plugins");
  let package = tar_bytes(&[
    (
      "echo-plugin/plugin.toml",
      br#"[plugin]
name = "echo-plugin"
version = "0.1.0"
runtime = "subprocess"
entrypoint = "bin/echo"
protocol = "agentflow.plugin/1"

[[plugin.nodes]]
type = "echo"
description = "Echo node"
"#,
      0o644,
    ),
    ("echo-plugin/bin/echo", b"#!/bin/sh\necho ok\n", 0o755),
  ]);
  let entry = plugin_entry_for_bytes(&package);
  write_marketplace_for_entry(&marketplace, &entry);
  RemoteMarketplaceCache::new(&cache_dir)
    .cache_artifact_bytes(&entry, &package)
    .unwrap();

  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "marketplace",
      "install",
      marketplace.to_str().unwrap(),
      "echo-plugin",
      "--type",
      "plugin",
      "--cache-dir",
      cache_dir.to_str().unwrap(),
      "--dir",
      install_dir.to_str().unwrap(),
      "--force",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains(
      "Cached plugin package: echo-plugin",
    ))
    .stdout(predicate::str::contains(
      "Installed plugin package: echo-plugin",
    ));

  assert!(
    install_dir
      .join("echo-plugin")
      .join("plugin.toml")
      .is_file()
  );
  assert!(
    install_dir
      .join("echo-plugin")
      .join("bin")
      .join("echo")
      .is_file()
  );
}

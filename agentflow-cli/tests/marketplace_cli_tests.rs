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

fn tar_link_bytes(path: &str, target: &str, entry_type: tar::EntryType) -> Vec<u8> {
  let mut bytes = Vec::new();
  {
    let cursor = Cursor::new(&mut bytes);
    let mut builder = tar::Builder::new(cursor);
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(entry_type);
    header.set_size(0);
    header.set_mode(0o644);
    builder.append_link(&mut header, path, target).unwrap();
    builder.finish().unwrap();
  }
  bytes
}

fn raw_tar_bytes(path: &str, content: &[u8]) -> Vec<u8> {
  let mut header = [0u8; 512];
  write_tar_field(&mut header[0..100], path.as_bytes());
  write_octal(&mut header[100..108], 0o644);
  write_octal(&mut header[108..116], 0);
  write_octal(&mut header[116..124], 0);
  write_octal(&mut header[124..136], content.len() as u64);
  write_octal(&mut header[136..148], 0);
  for byte in &mut header[148..156] {
    *byte = b' ';
  }
  header[156] = b'0';
  write_tar_field(&mut header[257..263], b"ustar\0");
  write_tar_field(&mut header[263..265], b"00");
  let checksum: u32 = header.iter().map(|byte| *byte as u32).sum();
  write_checksum(&mut header[148..156], checksum);

  let mut bytes = Vec::new();
  bytes.extend_from_slice(&header);
  bytes.extend_from_slice(content);
  let padding = (512 - (content.len() % 512)) % 512;
  bytes.extend(std::iter::repeat_n(0, padding));
  bytes.extend_from_slice(&[0u8; 1024]);
  bytes
}

fn write_tar_field(field: &mut [u8], value: &[u8]) {
  let len = value.len().min(field.len());
  field[..len].copy_from_slice(&value[..len]);
}

fn write_octal(field: &mut [u8], value: u64) {
  let rendered = format!("{value:0width$o}\0", width = field.len() - 1);
  field.copy_from_slice(rendered.as_bytes());
}

fn write_checksum(field: &mut [u8], value: u32) {
  let rendered = format!("{value:06o}\0 ",);
  field.copy_from_slice(rendered.as_bytes());
}

fn sha256_hex(bytes: &[u8]) -> String {
  use sha2::{Digest, Sha256};
  let mut hasher = Sha256::new();
  hasher.update(bytes);
  format!("{:x}", hasher.finalize())
}

fn install_cached_skill_package_asserts_failure(package: &[u8], expected_stderr: &'static str) {
  let work = TempDir::new().unwrap();
  let marketplace = work.path().join("marketplace.toml");
  let cache_dir = work.path().join("cache");
  let install_dir = work.path().join("skills");
  let entry = entry_for_bytes(package);
  write_marketplace_for_entry(&marketplace, &entry);
  RemoteMarketplaceCache::new(&cache_dir)
    .cache_artifact_bytes(&entry, package)
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
    .failure()
    .stderr(predicate::str::contains(expected_stderr));
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
fn marketplace_verify_strict_rejects_unsigned_artifact() {
  let work = TempDir::new().unwrap();
  let marketplace = work.path().join("marketplace.toml");
  let cache_dir = work.path().join("cache");
  let bytes = b"unsigned package";
  let mut entry = entry_for_bytes(bytes);
  entry.signature = None;
  write_marketplace_for_entry(&marketplace, &entry);
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
      "--strict",
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains(
      "Strict verification requires signature metadata",
    ));
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

#[test]
fn marketplace_install_rejects_absolute_archive_path() {
  let package = raw_tar_bytes("/tmp/agentflow-escape", b"escape");

  install_cached_skill_package_asserts_failure(&package, "unsafe archive path");
}

#[test]
fn marketplace_install_rejects_traversal_archive_path() {
  let package = raw_tar_bytes("../agentflow-escape", b"escape");

  install_cached_skill_package_asserts_failure(&package, "unsafe archive path");
}

#[test]
fn marketplace_install_rejects_symlink_archive_entry() {
  let package = tar_link_bytes("rust-expert/escape", "/tmp/escape", tar::EntryType::Symlink);

  install_cached_skill_package_asserts_failure(&package, "unsafe archive entry");
}

#[test]
fn marketplace_install_rejects_hardlink_archive_entry() {
  let package = tar_link_bytes("rust-expert/escape", "/tmp/escape", tar::EntryType::Link);

  install_cached_skill_package_asserts_failure(&package, "unsafe archive entry");
}

#[test]
fn marketplace_install_rejects_duplicate_archive_path() {
  let package = tar_bytes(&[
    (
      "rust-expert/SKILL.md",
      br#"---
name: rust-expert
description: Rust review skill
---

# First
"#,
      0o644,
    ),
    (
      "rust-expert/SKILL.md",
      br#"---
name: rust-expert
description: Rust review skill
---

# Second
"#,
      0o644,
    ),
  ]);

  install_cached_skill_package_asserts_failure(&package, "duplicate archive path");
}

#[test]
fn marketplace_install_rejects_oversized_archive_file() {
  let large = vec![b'x'; 16 * 1024 * 1024 + 1];
  let package = tar_bytes(&[("rust-expert/large.bin", &large, 0o644)]);

  install_cached_skill_package_asserts_failure(&package, "oversized archive file");
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

#[cfg(feature = "plugin")]
#[test]
fn marketplace_install_plugin_rejects_entrypoint_outside_package() {
  let work = TempDir::new().unwrap();
  let marketplace = work.path().join("marketplace.toml");
  let cache_dir = work.path().join("cache");
  let install_dir = work.path().join("plugins");
  let package = tar_bytes(&[(
    "echo-plugin/plugin.toml",
    br#"[plugin]
name = "echo-plugin"
version = "0.1.0"
runtime = "subprocess"
entrypoint = "../outside-entry"
protocol = "agentflow.plugin/1"

[[plugin.nodes]]
type = "echo"
description = "Echo node"
"#,
    0o644,
  )]);
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
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("entrypoint"))
    .stderr(predicate::str::contains("outside package root"));
}

//! P5.3 — Marketplace unpack hardening.
//!
//! These tests cover the unpack-side edge cases the basic
//! marketplace tests (`tests/marketplace_cli_tests.rs`) do not
//! exercise:
//!
//! - nested archives (zip-bytes inside a tar)
//! - duplicate metadata (multiple `SKILL.md` at the archive root)
//! - executable bits survive the install copy
//! - very large entry counts (10k+ files)
//! - invalid UTF-8 paths
//! - zip-bomb / decompression-ratio limits
//!
//! All failing cases must error cleanly without writing any file
//! outside the install root.

use std::fs;
use std::io::{Cursor, Write};
use std::path::Path;

use assert_cmd::Command;
use flate2::Compression;
use flate2::write::GzEncoder;
use predicates::prelude::*;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use agentflow_skills::{
  MarketplacePackageType, MarketplaceSignature, MarketplaceSource, RemoteMarketplaceCache,
  RemoteMarketplaceEntry,
};

fn sha256_hex(bytes: &[u8]) -> String {
  let mut hasher = Sha256::new();
  hasher.update(bytes);
  format!("{:x}", hasher.finalize())
}

fn entry_for_bytes(bytes: &[u8]) -> RemoteMarketplaceEntry {
  let checksum = sha256_hex(bytes);
  RemoteMarketplaceEntry {
    name: "rust-expert".into(),
    version: "1.0.0".into(),
    package_type: MarketplacePackageType::Skill,
    source: MarketplaceSource {
      registry_url: "https://registry.example.com/marketplace.toml".into(),
      artifact_url: "https://registry.example.com/rust-expert.tar".into(),
      checksum_sha256: checksum.clone(),
    },
    signature: Some(MarketplaceSignature {
      algorithm: "checksum-sha256".into(),
      key_id: "test".into(),
      value: checksum,
    }),
    aliases: Vec::new(),
    description: None,
  }
}

fn write_marketplace_for_entry(path: &Path, entry: &RemoteMarketplaceEntry) {
  let signature = entry
    .signature
    .as_ref()
    .map(|signature| {
      format!(
        "\n[entries.signature]\nalgorithm = \"{}\"\nkey_id = \"{}\"\nvalue = \"{}\"\n",
        signature.algorithm, signature.key_id, signature.value
      )
    })
    .unwrap_or_default();
  let toml = format!(
    r#"
schema_version = 1
name = "remote-test"

[[entries]]
name = "{}"
version = "{}"
type = "{}"

[entries.source]
registry_url = "{}"
artifact_url = "{}"
checksum_sha256 = "{}"
{}
"#,
    entry.name,
    entry.version,
    entry.package_type.as_str(),
    entry.source.registry_url,
    entry.source.artifact_url,
    entry.source.checksum_sha256,
    signature
  );
  fs::write(path, toml).unwrap();
}

/// Build a deterministic tar from `(path, content, mode)` tuples.
fn build_tar(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
  let mut bytes = Vec::new();
  {
    let mut builder = tar::Builder::new(Cursor::new(&mut bytes));
    for (path, content, mode) in entries {
      let mut header = tar::Header::new_gnu();
      header.set_size(content.len() as u64);
      header.set_mode(*mode);
      header.set_mtime(0);
      header.set_uid(0);
      header.set_gid(0);
      header.set_cksum();
      builder
        .append_data(&mut header, path, Cursor::new(*content))
        .unwrap();
    }
    builder.finish().unwrap();
  }
  bytes
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
  let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
  encoder.write_all(bytes).unwrap();
  encoder.finish().unwrap()
}

/// Cache + invoke `agentflow marketplace install` for the given
/// archive bytes, asserting that the install succeeds and producing
/// the resulting install_dir for further inspection.
fn install_skill_package(package: &[u8]) -> (TempDir, std::path::PathBuf) {
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
    .success();
  (work, install_dir)
}

/// Same as `install_skill_package`, but asserts failure with the
/// given substring in stderr — and verifies install_dir contains no
/// half-installed package directory (defense in depth: even if the
/// unpack bails halfway, install_dir must remain empty of our name).
fn assert_install_failure(package: &[u8], expected_stderr: &'static str) {
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

  // Defense in depth: when unpack rejects an archive the install
  // directory should never contain the failing package.
  let rust_expert = install_dir.join("rust-expert");
  assert!(
    !rust_expert.exists(),
    "install dir {} unexpectedly contains the rejected package",
    rust_expert.display()
  );
  drop(work);
}

const MINIMAL_SKILL_MD: &[u8] = br#"---
name: rust-expert
description: Rust review skill
allowed-tools: file
---

# Rust Expert
"#;

#[test]
fn unpack_accepts_nested_archive_as_opaque_file() {
  // A zip-shaped blob ("PK\x03\x04...") embedded as a regular tar
  // entry should not trigger automatic recursion. The marketplace
  // unpacker is a single-pass tar reader; anything that walks like
  // a file gets copied verbatim. Without this guarantee a malicious
  // Skill could smuggle code through a second archive layer.
  let inner_zip: &[u8] = b"PK\x03\x04 fake zip payload \x00\x00";
  let package = build_tar(&[
    ("rust-expert/SKILL.md", MINIMAL_SKILL_MD, 0o644),
    ("rust-expert/assets/data.zip", inner_zip, 0o644),
  ]);
  let (_work, install_dir) = install_skill_package(&package);
  let nested = install_dir.join("rust-expert/assets/data.zip");
  assert!(
    nested.is_file(),
    "nested archive must be unpacked as a file"
  );
  assert_eq!(fs::read(&nested).unwrap(), inner_zip);
  // Sanity check: no auto-extraction of the inner zip.
  assert!(!install_dir.join("rust-expert/assets/data").exists());
}

#[test]
fn unpack_rejects_duplicate_top_level_skill_md() {
  // Two SKILL.md files at archive root would let an attacker shadow
  // the manifest the loader picks up. The duplicate-path check
  // catches the second one before unpack even touches disk.
  let package = build_tar(&[
    ("SKILL.md", MINIMAL_SKILL_MD, 0o644),
    ("SKILL.md", b"---\nname: shadow\n---\n", 0o644),
  ]);
  assert_install_failure(&package, "duplicate archive path");
}

#[cfg(unix)]
#[test]
fn unpack_preserves_executable_bit_on_install() {
  use std::os::unix::fs::PermissionsExt;
  let package = build_tar(&[
    ("rust-expert/SKILL.md", MINIMAL_SKILL_MD, 0o644),
    ("rust-expert/bin/hook.sh", b"#!/bin/sh\necho hi\n", 0o755),
  ]);
  let (_work, install_dir) = install_skill_package(&package);
  let hook = install_dir.join("rust-expert/bin/hook.sh");
  assert!(hook.is_file());
  let mode = fs::metadata(&hook).unwrap().permissions().mode() & 0o777;
  assert_eq!(
    mode, 0o755,
    "exec bit must survive the cache → install copy"
  );
}

#[test]
fn unpack_accepts_archive_with_thousands_of_entries() {
  // The unpacker caps total entries at 16k. 4k entries is well
  // below that cap but still large enough to catch O(n^2) regressions
  // in seen-path tracking.
  let mut entries: Vec<(String, Vec<u8>, u32)> = Vec::with_capacity(4001);
  entries.push((
    "rust-expert/SKILL.md".to_string(),
    MINIMAL_SKILL_MD.to_vec(),
    0o644,
  ));
  for i in 0..4_000 {
    entries.push((
      format!("rust-expert/fixtures/file_{i:05}.txt"),
      format!("file {i}\n").into_bytes(),
      0o644,
    ));
  }
  // Materialize references for build_tar.
  let refs: Vec<(&str, &[u8], u32)> = entries
    .iter()
    .map(|(p, c, m)| (p.as_str(), c.as_slice(), *m))
    .collect();
  let package = build_tar(&refs);
  let (_work, install_dir) = install_skill_package(&package);
  assert!(
    install_dir
      .join("rust-expert/fixtures/file_03999.txt")
      .is_file(),
    "all 4000 fixture files must land on disk"
  );
}

#[test]
fn unpack_rejects_archive_with_too_many_entries() {
  // 16_384 is the hard cap. Build something modestly over it to keep
  // the test fast. A real package would never need this many files.
  let mut entries: Vec<(String, Vec<u8>, u32)> = Vec::with_capacity(16_400);
  entries.push((
    "rust-expert/SKILL.md".to_string(),
    MINIMAL_SKILL_MD.to_vec(),
    0o644,
  ));
  for i in 0..16_400 {
    entries.push((format!("rust-expert/spam_{i:06}.txt"), b"x".to_vec(), 0o644));
  }
  let refs: Vec<(&str, &[u8], u32)> = entries
    .iter()
    .map(|(p, c, m)| (p.as_str(), c.as_slice(), *m))
    .collect();
  let package = build_tar(&refs);
  assert_install_failure(&package, "more than 16384 entries");
}

#[test]
fn unpack_rejects_invalid_utf8_path() {
  // tar headers allow arbitrary bytes in the name field. A path of
  // `r\xffust-expert/SKILL.md` round-trips as a valid `Path` on Unix
  // but is not valid UTF-8 and so cannot be safely round-tripped to
  // platforms that store names as UTF-16 (Windows). We reject it
  // outright.
  let mut bytes = Vec::new();
  {
    let mut builder = tar::Builder::new(Cursor::new(&mut bytes));
    let mut header = tar::Header::new_gnu();
    header.set_size(MINIMAL_SKILL_MD.len() as u64);
    header.set_mode(0o644);
    header.set_mtime(0);
    // Inject an invalid-UTF-8 byte into the name. `set_path` would
    // try to validate, so we write the name field of the header
    // directly through the GNU long-name path: we use append_data
    // with a Path constructed from raw bytes via OsStr.
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let raw = b"r\xffust-expert/SKILL.md";
    let path = Path::new(OsStr::from_bytes(raw));
    builder
      .append_data(&mut header, path, Cursor::new(MINIMAL_SKILL_MD))
      .unwrap();
    builder.finish().unwrap();
  }
  assert_install_failure(&bytes, "non-UTF-8 bytes");
}

#[test]
fn unpack_rejects_per_file_bomb() {
  // 17 MiB single-file bomb — just over the 16 MiB per-file cap.
  // This is the cheapest decompression-ratio defense and the one
  // most attacks rely on (one giant entry, near-zero entropy).
  let huge = vec![0u8; 16 * 1024 * 1024 + 1];
  let package = build_tar(&[("rust-expert/big.bin", &huge, 0o644)]);
  assert_install_failure(&package, "oversized archive file");
}

#[test]
#[ignore = "needs ~256 MiB of tar data; run manually to validate cumulative-cap defense"]
fn unpack_rejects_cumulative_bomb_total_size() {
  // 17 chunks of exactly 16 MiB each (each chunk sits right at the
  // per-file cap). After 16 chunks the cumulative is 256 MiB and the
  // 17th tips it over the 256 MiB cap. Marked `#[ignore]` because
  // the cargo bin still has to read 256 MiB of cached bytes before
  // the gate fires; for a release-time sanity check, run
  // `cargo test --test marketplace_unpack_hardening_tests -- --ignored`.
  let chunk = vec![b'x'; 16 * 1024 * 1024];
  let mut entries: Vec<(String, &[u8], u32)> = Vec::with_capacity(18);
  entries.push(("rust-expert/SKILL.md".to_string(), MINIMAL_SKILL_MD, 0o644));
  for i in 0..17 {
    entries.push((format!("rust-expert/chunk_{i:02}.bin"), &chunk, 0o644));
  }
  let refs: Vec<(&str, &[u8], u32)> = entries
    .iter()
    .map(|(p, c, m)| (p.as_str(), *c, *m))
    .collect();
  let package = build_tar(&refs);
  assert_install_failure(&package, "possible decompression bomb");
}

#[test]
fn unpack_rejects_gzipped_zip_bomb() {
  // A gzipped tar of mostly-zeroes compresses near a 1000:1 ratio.
  // A 256 KiB gzip blob can expand to ~256 MiB of zeroes — well
  // above the 16 MiB per-file cap. Confirm the gzip path also
  // catches the per-file size before the unpacker writes anything
  // to disk.
  let big = vec![0u8; 16 * 1024 * 1024 + 1];
  let tar = build_tar(&[("rust-expert/big.bin", &big, 0o644)]);
  let gzipped = gzip(&tar);
  // The gzipped form must be substantially smaller than the tar to
  // prove the compression ratio is what we are defending against.
  assert!(
    gzipped.len() * 10 < tar.len(),
    "gzip not compact enough for a bomb test: {} > {}",
    gzipped.len(),
    tar.len() / 10
  );
  assert_install_failure(&gzipped, "oversized archive file");
}

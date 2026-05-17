//! P5.2 — Signed fixture artifacts.
//!
//! These tests build deterministic `.tar.gz` archives from the source
//! files under `tests/fixtures/signed/` (Skill) and the sibling
//! `agentflow-core/tests/fixtures/signed/` directory (Plugin), then
//! drive them through `RemoteMarketplaceCache` to cover the strict
//! (`--require-signature`) and non-strict (`--allow-unsigned`) paths.
//!
//! "Locally signed" here means the `signature.value` is the SHA-256
//! digest of the archive bytes — the bootstrap signature flow
//! documented in `docs/MARKETPLACE.md` "Local signing". Production
//! deployments swap the verifier for a real signing system; this
//! fixture only exercises the policy plumbing.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use agentflow_skills::{
  MarketplacePackageType, MarketplaceSignature, MarketplaceSource, RemoteMarketplaceCache,
  RemoteMarketplaceEntry,
};
use flate2::Compression;
use flate2::write::GzEncoder;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

/// Source files for one fixture archive, keyed on POSIX path.
type ArchiveFiles = BTreeMap<String, Vec<u8>>;

/// Walk a fixture source dir and return its contents in deterministic
/// archive order (paths are POSIX, sorted via `BTreeMap`).
fn collect_archive_files(root: &Path) -> ArchiveFiles {
  let mut files: ArchiveFiles = BTreeMap::new();
  walk_dir(root, root, &mut files);
  files
}

fn walk_dir(root: &Path, dir: &Path, files: &mut ArchiveFiles) {
  let entries: Vec<_> = std::fs::read_dir(dir)
    .expect("read fixture dir")
    .filter_map(|entry| entry.ok())
    .collect();
  for entry in entries {
    let path = entry.path();
    let metadata = entry.metadata().expect("metadata");
    if metadata.is_dir() {
      walk_dir(root, &path, files);
    } else if metadata.is_file() {
      let relative = path
        .strip_prefix(root)
        .expect("relative path")
        .to_string_lossy()
        .replace('\\', "/");
      let bytes = std::fs::read(&path).expect("read fixture file");
      files.insert(relative, bytes);
    }
  }
}

/// Build a deterministic `.tar.gz` from `files`.
///
/// Determinism matters because the test signs the archive bytes —
/// non-deterministic order or timestamps would produce a different
/// SHA-256 each run and the "signature must match" assertion would
/// be testing nothing. We fix mtime to 0, owner/group to 0, and mode
/// to 0o644 for regular files (or 0o755 if the source bit was set).
fn build_signed_archive(files: &ArchiveFiles) -> Vec<u8> {
  let buf: Vec<u8> = Vec::new();
  let gz = GzEncoder::new(buf, Compression::default());
  let mut builder = tar::Builder::new(gz);
  for (path, bytes) in files {
    let mut header = tar::Header::new_gnu();
    header.set_path(path).expect("set path");
    header.set_size(bytes.len() as u64);
    header.set_mtime(0);
    header.set_uid(0);
    header.set_gid(0);
    let mode: u32 = if path.starts_with("bin/") {
      0o755
    } else {
      0o644
    };
    header.set_mode(mode);
    header.set_cksum();
    builder.append(&header, Cursor::new(bytes)).expect("append");
  }
  let gz = builder.into_inner().expect("close tar");
  gz.finish().expect("finish gz")
}

fn sha256_hex(bytes: &[u8]) -> String {
  let mut hasher = Sha256::new();
  hasher.update(bytes);
  format!("{:x}", hasher.finalize())
}

fn signed_entry(
  package_type: MarketplacePackageType,
  name: &str,
  archive_bytes: &[u8],
  signed: bool,
) -> RemoteMarketplaceEntry {
  let digest = sha256_hex(archive_bytes);
  RemoteMarketplaceEntry {
    name: name.to_string(),
    version: "1.0.0".to_string(),
    package_type,
    source: MarketplaceSource {
      registry_url: "https://registry.example.com/marketplace.toml".to_string(),
      artifact_url: format!("https://registry.example.com/{name}-1.0.0.tar.gz"),
      checksum_sha256: digest.clone(),
    },
    signature: signed.then(|| MarketplaceSignature {
      algorithm: "checksum-sha256".to_string(),
      key_id: "agentflow-dev-test".to_string(),
      value: digest,
    }),
    aliases: Vec::new(),
    description: Some(format!("signed test fixture: {name}")),
  }
}

fn skill_fixture_root() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/signed/skill-rust-expert")
}

fn plugin_fixture_root() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("../agentflow-core/tests/fixtures/signed/plugin-echo")
}

fn signature_required_violation(cached: &agentflow_skills::CachedMarketplaceArtifact) -> bool {
  // Replicates the CLI's `--require-signature` policy gate. The
  // cache layer accepts unsigned artifacts; rejecting them is a
  // policy decision the caller layers on top.
  !cached.signature_checked
}

#[test]
fn signed_skill_strict_path_succeeds() {
  let files = collect_archive_files(&skill_fixture_root());
  assert!(
    files.contains_key("SKILL.md"),
    "skill fixture must contain SKILL.md"
  );
  let bytes = build_signed_archive(&files);
  let entry = signed_entry(MarketplacePackageType::Skill, "rust-expert", &bytes, true);

  let tmp = TempDir::new().unwrap();
  let cache = RemoteMarketplaceCache::new(tmp.path());
  let cached = cache
    .cache_artifact_bytes(&entry, &bytes)
    .expect("strict signed cache write");
  assert!(
    cached.signature_checked,
    "signed entry must report signature_checked"
  );
  assert!(
    !signature_required_violation(&cached),
    "strict policy must accept a verified signature"
  );
  assert!(cached.path.is_file());
}

#[test]
fn unsigned_skill_non_strict_path_succeeds() {
  let files = collect_archive_files(&skill_fixture_root());
  let bytes = build_signed_archive(&files);
  let entry = signed_entry(MarketplacePackageType::Skill, "rust-expert", &bytes, false);

  let tmp = TempDir::new().unwrap();
  let cache = RemoteMarketplaceCache::new(tmp.path());
  let cached = cache
    .cache_artifact_bytes(&entry, &bytes)
    .expect("non-strict cache write");
  assert!(
    !cached.signature_checked,
    "unsigned entry must report signature_checked=false"
  );
  assert!(
    signature_required_violation(&cached),
    "strict policy would reject this entry (no signature attached)"
  );
}

#[test]
fn signed_skill_strict_path_rejects_tampered_signature() {
  let files = collect_archive_files(&skill_fixture_root());
  let bytes = build_signed_archive(&files);
  let mut entry = signed_entry(MarketplacePackageType::Skill, "rust-expert", &bytes, true);
  // Tamper with the signature value (still valid hex, just wrong).
  entry.signature.as_mut().unwrap().value =
    "feedfacefeedfacefeedfacefeedfacefeedfacefeedfacefeedfacefeedface".to_string();

  let tmp = TempDir::new().unwrap();
  let cache = RemoteMarketplaceCache::new(tmp.path());
  let err = cache
    .cache_artifact_bytes(&entry, &bytes)
    .expect_err("tampered signature must be rejected");
  let msg = err.to_string();
  assert!(
    msg.contains("Signature checksum mismatch"),
    "unexpected error: {msg}"
  );
}

#[test]
fn signed_skill_strict_path_rejects_tampered_artifact() {
  let files = collect_archive_files(&skill_fixture_root());
  let bytes = build_signed_archive(&files);
  let entry = signed_entry(MarketplacePackageType::Skill, "rust-expert", &bytes, true);

  // Flip a byte in the archive so the checksum gate (which runs
  // before the signature verifier) catches it. This ensures the
  // strict path is layered correctly — checksum first, signature
  // second.
  let mut tampered = bytes.clone();
  let last = tampered.len() - 1;
  tampered[last] ^= 0xff;

  let tmp = TempDir::new().unwrap();
  let cache = RemoteMarketplaceCache::new(tmp.path());
  let err = cache
    .cache_artifact_bytes(&entry, &tampered)
    .expect_err("artifact tampering must be rejected");
  let msg = err.to_string();
  assert!(
    msg.contains("Artifact checksum mismatch"),
    "unexpected error: {msg}"
  );
}

#[test]
fn signed_plugin_strict_path_succeeds() {
  let root = plugin_fixture_root();
  assert!(
    root.join("plugin.toml").is_file(),
    "plugin fixture missing plugin.toml at {}",
    root.display()
  );
  let files = collect_archive_files(&root);
  let bytes = build_signed_archive(&files);
  let entry = signed_entry(MarketplacePackageType::Plugin, "echo-plugin", &bytes, true);

  let tmp = TempDir::new().unwrap();
  let cache = RemoteMarketplaceCache::new(tmp.path());
  let cached = cache
    .cache_artifact_bytes(&entry, &bytes)
    .expect("strict signed cache write (plugin)");
  assert!(cached.signature_checked);
  assert_eq!(cached.package_type, MarketplacePackageType::Plugin);
}

#[test]
fn unsigned_plugin_non_strict_path_succeeds() {
  let files = collect_archive_files(&plugin_fixture_root());
  let bytes = build_signed_archive(&files);
  let entry = signed_entry(MarketplacePackageType::Plugin, "echo-plugin", &bytes, false);

  let tmp = TempDir::new().unwrap();
  let cache = RemoteMarketplaceCache::new(tmp.path());
  let cached = cache
    .cache_artifact_bytes(&entry, &bytes)
    .expect("non-strict cache write (plugin)");
  assert!(!cached.signature_checked);
  assert!(signature_required_violation(&cached));
}

#[test]
fn signed_archive_round_trip_is_deterministic() {
  // Determinism guard: two builds from the same fixture must produce
  // byte-identical archives. If this fails, the signature in the
  // other tests would be effectively random and they would silently
  // stop covering the strict path.
  let files = collect_archive_files(&skill_fixture_root());
  let a = build_signed_archive(&files);
  let b = build_signed_archive(&files);
  assert_eq!(
    sha256_hex(&a),
    sha256_hex(&b),
    "archive build must be deterministic"
  );
  // Sanity-check that the archive is a real gzipped tar by gunzip+untar
  // round-trip.
  let mut decoder = flate2::read::GzDecoder::new(Cursor::new(&a));
  let mut tar_bytes = Vec::new();
  decoder.read_to_end(&mut tar_bytes).expect("gunzip");
  let mut archive = tar::Archive::new(Cursor::new(tar_bytes));
  let mut seen = Vec::new();
  for entry in archive.entries().expect("tar entries") {
    let entry = entry.expect("tar entry");
    seen.push(entry.path().expect("path").to_string_lossy().into_owned());
  }
  assert!(seen.contains(&"SKILL.md".to_string()));
}

use anyhow::{Context, Result, anyhow, bail};
use flate2::read::GzDecoder;
use std::collections::BTreeSet;
use std::fs;
use std::io::{Cursor, Read};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Component;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use walkdir::WalkDir;

use agentflow_skills::{
  MarketplacePackageType, RemoteMarketplaceCache, RemoteMarketplaceClient, RemoteMarketplaceEntry,
  RemoteMarketplaceManifest, SkillLoader,
};

const MAX_MARKETPLACE_ARCHIVE_FILE_BYTES: u64 = 16 * 1024 * 1024;

// Zip-bomb defense. Marketplace packages are expected to be at most a
// handful of MiB; a 256 MiB cumulative cap leaves a comfortable margin
// for legitimate plugins while rejecting decompression-ratio attacks
// (a gzipped tar of mostly-zeroes can easily expand past a GiB).
const MAX_MARKETPLACE_ARCHIVE_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

// Plugin manifests often ship hundreds of small fixture files, but
// 16k entries is more than the runtime should ever care about — beyond
// that we are looking at a directory bomb, not a real package.
const MAX_MARKETPLACE_ARCHIVE_ENTRIES: usize = 16_384;

pub async fn search(
  registry: String,
  query: Option<String>,
  package_type: Option<String>,
  format: String,
) -> Result<()> {
  let manifest = load_manifest(&registry).await?;
  let package_type = parse_package_type_opt(package_type.as_deref())?;
  let lowercased_query = query.as_ref().map(|q| q.to_ascii_lowercase());
  let entries = matching_entries(&manifest, lowercased_query.as_deref(), package_type);

  match format.as_str() {
    "json" => render_search_json(
      &registry,
      query.as_deref(),
      package_type,
      &manifest,
      &entries,
    ),
    "json-envelope" => render_search_envelope(
      &registry,
      query.as_deref(),
      package_type,
      &manifest,
      &entries,
    ),
    _ => render_search_text(&manifest, &entries),
  }
}

fn render_search_text(
  manifest: &RemoteMarketplaceManifest,
  entries: &[&RemoteMarketplaceEntry],
) -> Result<()> {
  println!(
    "🛒 Marketplace: {} [{} entries]",
    manifest.name,
    manifest.entries().len()
  );
  if let Some(description) = &manifest.description {
    println!("   {}", description);
  }
  if entries.is_empty() {
    println!("   No matching packages");
    return Ok(());
  }
  for entry in entries {
    print_entry(entry);
  }
  Ok(())
}

/// Build the structured search payload that both `--format json` and
/// `--format json-envelope` render. Shared so the envelope body stays
/// byte-identical to the bare-json output — the additive-field
/// contract pinned by the json_envelope_migration_tests harness.
fn build_search_payload(
  registry: &str,
  query: Option<&str>,
  package_type: Option<MarketplacePackageType>,
  manifest: &RemoteMarketplaceManifest,
  entries: &[&RemoteMarketplaceEntry],
) -> serde_json::Value {
  serde_json::json!({
    "registry": registry,
    "query": query,
    "package_type_filter": package_type.map(|t| t.as_str().to_string()),
    "manifest": {
      "schema_version": manifest.schema_version,
      "name": manifest.name,
      "description": manifest.description,
      "homepage": manifest.homepage,
      "total_entries": manifest.entries().len(),
    },
    "entries": entries,
    "matched_count": entries.len(),
  })
}

fn render_search_json(
  registry: &str,
  query: Option<&str>,
  package_type: Option<MarketplacePackageType>,
  manifest: &RemoteMarketplaceManifest,
  entries: &[&RemoteMarketplaceEntry],
) -> Result<()> {
  let payload = build_search_payload(registry, query, package_type, manifest, entries);
  println!("{}", serde_json::to_string_pretty(&payload)?);
  Ok(())
}

fn render_search_envelope(
  registry: &str,
  query: Option<&str>,
  package_type: Option<MarketplacePackageType>,
  manifest: &RemoteMarketplaceManifest,
  entries: &[&RemoteMarketplaceEntry],
) -> Result<()> {
  let payload = build_search_payload(registry, query, package_type, manifest, entries);
  let envelope = crate::json_envelope::CliJsonEnvelope::ok("marketplace search", &payload);
  println!("{}", serde_json::to_string_pretty(&envelope)?);
  Ok(())
}

pub async fn install(
  registry: String,
  package: String,
  package_type: Option<String>,
  cache_dir: Option<String>,
  install_dir: Option<String>,
  force: bool,
  cache_only: bool,
) -> Result<()> {
  let manifest = load_manifest(&registry).await?;
  let package_type = parse_package_type_opt(package_type.as_deref())?;
  let entry = resolve_entry(&manifest, &package, package_type)?;
  let cache = cache_from_dir(cache_dir);
  let cached = if cache.is_cached(entry)? {
    cache
      .verify_cached_artifact(entry)
      .with_context(|| format!("Failed to verify cached package '{}'", entry.name))?
  } else {
    cache
      .fetch_and_cache_artifact(entry)
      .await
      .with_context(|| format!("Failed to fetch and cache package '{}'", entry.name))?
  };

  println!(
    "✅ Cached {} package: {}",
    entry.package_type.as_str(),
    entry.name
  );
  println!("   version: {}", entry.version);
  println!("   path: {}", cached.path.display());
  println!("   checksum: sha256:{}", cached.checksum_sha256);
  println!("   signature_checked: {}", cached.signature_checked);

  if cache_only {
    println!("   cache_only: true");
    return Ok(());
  }

  let installed =
    install_cached_package(entry, &cached.path, install_dir, force).map_err(|err| {
      anyhow!(
        "Failed to install cached package '{}': {:#}",
        entry.name,
        err
      )
    })?;
  println!(
    "✅ Installed {} package: {}",
    entry.package_type.as_str(),
    entry.name
  );
  println!("   version: {}", entry.version);
  println!("   to: {}", installed.display());
  Ok(())
}

pub async fn update(registry: String, cache_dir: Option<String>) -> Result<()> {
  let manifest = load_manifest(&registry).await?;
  let cache = cache_from_dir(cache_dir);
  let registry_dir = cache.root().join("registries");
  fs::create_dir_all(&registry_dir).with_context(|| {
    format!(
      "Failed to create registry cache '{}'",
      registry_dir.display()
    )
  })?;
  let path = registry_dir.join(format!("{}.toml", sanitize_path_segment(&manifest.name)));
  let content =
    toml::to_string_pretty(&manifest).context("Failed to serialize remote marketplace manifest")?;
  fs::write(&path, content)
    .with_context(|| format!("Failed to write registry cache '{}'", path.display()))?;

  println!("✅ Updated marketplace registry cache");
  println!("   marketplace: {}", manifest.name);
  println!("   entries: {}", manifest.entries().len());
  println!("   path: {}", path.display());
  Ok(())
}

pub async fn verify(
  registry: String,
  package: Option<String>,
  package_type: Option<String>,
  cache_dir: Option<String>,
  strict: bool,
) -> Result<()> {
  let manifest = load_manifest(&registry).await?;
  let package_type = parse_package_type_opt(package_type.as_deref())?;
  let cache = cache_from_dir(cache_dir);
  let entries: Vec<&RemoteMarketplaceEntry> = if let Some(package) = package {
    vec![resolve_entry(&manifest, &package, package_type)?]
  } else {
    matching_entries(&manifest, None, package_type)
  };

  if entries.is_empty() {
    bail!("No marketplace entries matched the verify request");
  }

  for entry in entries {
    let cached = cache
      .verify_cached_artifact(entry)
      .with_context(|| format!("Failed to verify cached package '{}'", entry.name))?;
    if strict && !cached.signature_checked {
      bail!(
        "Strict verification requires signature metadata for '{}@{}'",
        entry.name,
        entry.version
      );
    }
    println!(
      "✅ Verified {} package: {}",
      entry.package_type.as_str(),
      entry.name
    );
    println!("   version: {}", entry.version);
    println!("   path: {}", cached.path.display());
    println!("   checksum: sha256:{}", cached.checksum_sha256);
    println!("   signature_checked: {}", cached.signature_checked);
  }
  Ok(())
}

async fn load_manifest(registry: &str) -> Result<RemoteMarketplaceManifest> {
  if registry.starts_with("http://") || registry.starts_with("https://") {
    RemoteMarketplaceClient::new()
      .fetch_manifest(registry)
      .await
      .with_context(|| format!("Failed to fetch remote marketplace '{}'", registry))
  } else {
    RemoteMarketplaceManifest::load(Path::new(registry))
      .with_context(|| format!("Failed to load remote marketplace manifest '{}'", registry))
  }
}

fn cache_from_dir(cache_dir: Option<String>) -> RemoteMarketplaceCache {
  let root = cache_dir
    .map(PathBuf::from)
    .unwrap_or_else(RemoteMarketplaceCache::default_root);
  RemoteMarketplaceCache::new(root)
}

fn matching_entries<'a>(
  manifest: &'a RemoteMarketplaceManifest,
  query: Option<&str>,
  package_type: Option<MarketplacePackageType>,
) -> Vec<&'a RemoteMarketplaceEntry> {
  manifest
    .entries()
    .iter()
    .filter(|entry| package_type.is_none_or(|kind| entry.package_type == kind))
    .filter(|entry| {
      query.is_none_or(|query| {
        entry.name.to_ascii_lowercase().contains(query)
          || entry
            .aliases
            .iter()
            .any(|alias| alias.to_ascii_lowercase().contains(query))
          || entry
            .description
            .as_ref()
            .is_some_and(|desc| desc.to_ascii_lowercase().contains(query))
      })
    })
    .collect()
}

fn resolve_entry<'a>(
  manifest: &'a RemoteMarketplaceManifest,
  package: &str,
  package_type: Option<MarketplacePackageType>,
) -> Result<&'a RemoteMarketplaceEntry> {
  let matches: Vec<_> = manifest
    .entries()
    .iter()
    .filter(|entry| package_type.is_none_or(|kind| entry.package_type == kind))
    .filter(|entry| entry.name == package || entry.aliases.iter().any(|alias| alias == package))
    .collect();
  match matches.as_slice() {
    [entry] => Ok(*entry),
    [] => bail!(
      "Package '{}' not found in marketplace '{}'",
      package,
      manifest.name
    ),
    _ => bail!(
      "Package '{}' matches multiple types; pass --type skill or --type plugin",
      package
    ),
  }
}

fn parse_package_type_opt(value: Option<&str>) -> Result<Option<MarketplacePackageType>> {
  value.map(parse_package_type).transpose()
}

fn parse_package_type(value: &str) -> Result<MarketplacePackageType> {
  match value {
    "skill" => Ok(MarketplacePackageType::Skill),
    "plugin" => Ok(MarketplacePackageType::Plugin),
    other => bail!("Unsupported marketplace package type '{}'", other),
  }
}

fn print_entry(entry: &RemoteMarketplaceEntry) {
  println!("   - {} @ {}", entry.name, entry.version);
  println!("     type: {}", entry.package_type.as_str());
  println!("     artifact: {}", entry.source.artifact_url);
  println!(
    "     checksum: sha256:{}",
    entry.source.normalized_checksum().unwrap_or_default()
  );
  if let Some(signature) = &entry.signature {
    println!(
      "     signature: {} ({})",
      signature.algorithm, signature.key_id
    );
  }
  if !entry.aliases.is_empty() {
    println!("     aliases: {}", entry.aliases.join(", "));
  }
}

fn install_cached_package(
  entry: &RemoteMarketplaceEntry,
  artifact_path: &Path,
  install_dir: Option<String>,
  force: bool,
) -> Result<PathBuf> {
  let package = extract_package_archive(artifact_path)?;
  let package_root = find_package_root(package.path(), entry.package_type)?;
  match entry.package_type {
    MarketplacePackageType::Skill => {
      install_skill_package(entry, &package_root, install_dir, force)
    }
    MarketplacePackageType::Plugin => {
      install_plugin_package(entry, &package_root, install_dir, force)
    }
  }
}

fn extract_package_archive(artifact_path: &Path) -> Result<TempDir> {
  let bytes = fs::read(artifact_path)
    .with_context(|| format!("Failed to read artifact '{}'", artifact_path.display()))?;
  let reader: Box<dyn Read> = if bytes.starts_with(&[0x1f, 0x8b]) {
    Box::new(GzDecoder::new(Cursor::new(bytes)))
  } else {
    Box::new(Cursor::new(bytes))
  };
  let temp_dir = TempDir::new().context("Failed to create marketplace unpack directory")?;
  let mut archive = tar::Archive::new(reader);
  let mut seen_paths = BTreeSet::new();
  let mut total_bytes: u64 = 0;
  let mut entry_count: usize = 0;

  for entry in archive.entries().with_context(|| {
    format!(
      "Failed to read tar entries from '{}'",
      artifact_path.display()
    )
  })? {
    entry_count += 1;
    if entry_count > MAX_MARKETPLACE_ARCHIVE_ENTRIES {
      bail!(
        "Refusing to unpack archive with more than {} entries from '{}'",
        MAX_MARKETPLACE_ARCHIVE_ENTRIES,
        artifact_path.display()
      );
    }
    let mut entry = entry.with_context(|| {
      format!(
        "Failed to read an archive entry from '{}'",
        artifact_path.display()
      )
    })?;
    let entry_type = entry.header().entry_type();
    if !(entry_type.is_dir() || entry_type.is_file()) {
      bail!(
        "Refusing to unpack unsafe archive entry '{}' from '{}'",
        entry.path()?.display(),
        artifact_path.display()
      );
    }

    let relative = safe_archive_path(&entry.path()?, &entry.path_bytes())?;
    if !seen_paths.insert(relative.clone()) {
      bail!(
        "Refusing to unpack duplicate archive path '{}' from '{}'",
        relative.display(),
        artifact_path.display()
      );
    }
    if entry_type.is_file() {
      let size = entry.header().size().with_context(|| {
        format!(
          "Failed to read archive entry size for '{}' from '{}'",
          relative.display(),
          artifact_path.display()
        )
      })?;
      if size > MAX_MARKETPLACE_ARCHIVE_FILE_BYTES {
        bail!(
          "Refusing to unpack oversized archive file '{}' ({} bytes exceeds {} bytes)",
          relative.display(),
          size,
          MAX_MARKETPLACE_ARCHIVE_FILE_BYTES
        );
      }
      total_bytes = total_bytes.saturating_add(size);
      if total_bytes > MAX_MARKETPLACE_ARCHIVE_TOTAL_BYTES {
        bail!(
          "Refusing to unpack oversized archive (cumulative {} bytes exceeds {} bytes; possible decompression bomb)",
          total_bytes,
          MAX_MARKETPLACE_ARCHIVE_TOTAL_BYTES
        );
      }
    }
    let target = temp_dir.path().join(relative);
    if entry_type.is_dir() {
      fs::create_dir_all(&target)
        .with_context(|| format!("Failed to create unpacked directory '{}'", target.display()))?;
      continue;
    }

    if let Some(parent) = target.parent() {
      fs::create_dir_all(parent).with_context(|| {
        format!(
          "Failed to create unpacked parent directory '{}'",
          parent.display()
        )
      })?;
    }
    entry
      .unpack(&target)
      .with_context(|| format!("Failed to unpack archive file '{}'", target.display()))?;
  }

  Ok(temp_dir)
}

fn safe_archive_path(path: &Path, raw_bytes: &[u8]) -> Result<PathBuf> {
  // Reject any path whose raw tar bytes aren't valid UTF-8. Marketplace
  // packages travel across operating systems, and a path that round-trips
  // through `Path` on Unix but breaks on Windows (or vice-versa) is a
  // portability footgun that has no legitimate use. Erroring early is
  // strictly safer than letting `tar::Entry::unpack` decide.
  if std::str::from_utf8(raw_bytes).is_err() {
    bail!(
      "Refusing to unpack unsafe archive path (non-UTF-8 bytes): {:?}",
      raw_bytes
    );
  }
  let mut safe = PathBuf::new();
  for component in path.components() {
    match component {
      Component::Normal(value) => safe.push(value),
      Component::CurDir => {}
      Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
        bail!(
          "Refusing to unpack unsafe archive path '{}'",
          path.display()
        )
      }
    }
  }
  if safe.as_os_str().is_empty() {
    bail!("Refusing to unpack empty archive path");
  }
  Ok(safe)
}

fn find_package_root(unpack_root: &Path, package_type: MarketplacePackageType) -> Result<PathBuf> {
  let manifest_name = match package_type {
    MarketplacePackageType::Skill => "SKILL.md",
    MarketplacePackageType::Plugin => "plugin.toml",
  };
  if unpack_root.join(manifest_name).is_file() {
    return Ok(unpack_root.to_path_buf());
  }

  let directories = fs::read_dir(unpack_root)
    .with_context(|| {
      format!(
        "Failed to inspect unpack directory '{}'",
        unpack_root.display()
      )
    })?
    .filter_map(|entry| entry.ok())
    .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
    .map(|entry| entry.path())
    .collect::<Vec<_>>();

  match directories.as_slice() {
    [dir] if dir.join(manifest_name).is_file() => Ok(dir.clone()),
    _ => bail!(
      "Marketplace package does not contain a {} at archive root or one top-level directory",
      manifest_name
    ),
  }
}

fn install_skill_package(
  entry: &RemoteMarketplaceEntry,
  package_root: &Path,
  install_dir: Option<String>,
  force: bool,
) -> Result<PathBuf> {
  let manifest = SkillLoader::load(package_root).with_context(|| {
    format!(
      "Marketplace Skill package '{}' is invalid at '{}'",
      entry.name,
      package_root.display()
    )
  })?;
  let warnings = SkillLoader::validate(&manifest, package_root).with_context(|| {
    format!(
      "Marketplace Skill package '{}' failed validation",
      entry.name
    )
  })?;

  let install_root = install_dir
    .map(PathBuf::from)
    .unwrap_or_else(default_skills_dir);
  let destination = install_root.join(&entry.name);
  install_directory(package_root, &destination, force, "skill")?;
  if !warnings.is_empty() {
    for warning in warnings {
      println!("   ⚠  {}", warning);
    }
  }
  Ok(destination)
}

fn install_plugin_package(
  entry: &RemoteMarketplaceEntry,
  package_root: &Path,
  install_dir: Option<String>,
  force: bool,
) -> Result<PathBuf> {
  #[cfg(not(feature = "plugin"))]
  {
    let _ = (entry, package_root, install_dir, force);
    bail!(
      "Installing marketplace plugin packages requires a binary built with the `plugin` feature"
    );
  }

  #[cfg(feature = "plugin")]
  {
    use agentflow_core::plugin::PluginManifest;

    let manifest_path = package_root.join("plugin.toml");
    let (manifest, _manifest_dir) =
      PluginManifest::load_from_path(&manifest_path).with_context(|| {
        format!(
          "Failed to parse plugin manifest '{}'",
          manifest_path.display()
        )
      })?;
    manifest.validate().with_context(|| {
      format!(
        "Marketplace Plugin package '{}' failed validation",
        entry.name
      )
    })?;
    validate_marketplace_plugin_entrypoint(entry, package_root, &manifest)?;

    let install_root = install_dir
      .map(PathBuf::from)
      .unwrap_or_else(default_plugins_dir);
    let destination = install_root.join(&manifest.plugin.name);
    install_directory(package_root, &destination, force, "plugin")?;
    Ok(destination)
  }
}

#[cfg(feature = "plugin")]
fn validate_marketplace_plugin_entrypoint(
  entry: &RemoteMarketplaceEntry,
  package_root: &Path,
  manifest: &agentflow_core::plugin::PluginManifest,
) -> Result<()> {
  if manifest.plugin.entrypoint.is_absolute()
    || manifest
      .plugin
      .entrypoint
      .components()
      .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
  {
    bail!(
      "Marketplace Plugin package '{}' entrypoint '{}' points outside package root '{}'",
      entry.name,
      manifest.plugin.entrypoint.display(),
      package_root.display()
    );
  }

  let canonical_root = fs::canonicalize(package_root).with_context(|| {
    format!(
      "Failed to canonicalize marketplace plugin package root '{}'",
      package_root.display()
    )
  })?;
  let resolved_entrypoint = manifest.resolve_entrypoint(package_root);
  let canonical_entrypoint = fs::canonicalize(&resolved_entrypoint).with_context(|| {
    format!(
      "Marketplace Plugin package '{}' entrypoint '{}' is missing or cannot be resolved",
      entry.name,
      resolved_entrypoint.display()
    )
  })?;

  if !canonical_entrypoint.starts_with(&canonical_root) {
    bail!(
      "Marketplace Plugin package '{}' entrypoint '{}' resolves outside package root '{}'",
      entry.name,
      resolved_entrypoint.display(),
      package_root.display()
    );
  }
  if !canonical_entrypoint.is_file() {
    bail!(
      "Marketplace Plugin package '{}' entrypoint '{}' is not a file",
      entry.name,
      resolved_entrypoint.display()
    );
  }
  Ok(())
}

/// Atomically install a package directory at `destination` (P5.1).
///
/// Sequence:
///  1. Validate the destination collision policy up front so a busy target
///     errors before any filesystem work.
///  2. Stage every file into a sibling temp dir
///     `<destination_parent>/.<dest_name>.installing-<pid>-<nanos>`.
///  3. If the staging copy fails for any reason, remove the temp dir and
///     leave the existing destination untouched.
///  4. If `force` is set and a destination already exists, move it aside to
///     `<dest>.replacing-<pid>-<nanos>`. We only delete the prior install
///     after the new directory is renamed into place — so a `rename` failure
///     can roll back to the original install instead of leaving the target
///     missing.
///  5. `fs::rename(temp, destination)` swaps the staged copy in.
///  6. Clean up the moved-aside prior install.
fn install_directory(source: &Path, destination: &Path, force: bool, label: &str) -> Result<()> {
  let parent = destination.parent().unwrap_or_else(|| Path::new("."));
  fs::create_dir_all(parent).with_context(|| {
    format!(
      "Failed to create {} install parent for '{}'",
      label,
      destination.display()
    )
  })?;
  prevent_recursive_install(source, destination, label)?;

  // Early collision check: refuse before staging anything if the target
  // exists without --force.
  if destination.exists() && !force {
    bail!(
      "Target {} directory '{}' already exists; pass --force to overwrite",
      label,
      destination.display()
    );
  }

  let suffix = atomic_suffix();
  let staging = staged_path(destination, "installing", &suffix);
  if staging.exists() {
    // Should be impossible (suffix is timestamp+pid) but guard anyway —
    // a leftover staging dir from a hard-killed process must be cleaned
    // before we trust the path.
    let _ = fs::remove_dir_all(&staging);
  }

  // Stage into temp dir; on any failure remove the staging tree.
  if let Err(err) = copy_dir_recursive(source, &staging) {
    let _ = fs::remove_dir_all(&staging);
    return Err(err.context(format!(
      "Failed to stage {label} package into '{}'",
      staging.display()
    )));
  }

  // If the destination is occupied, move it aside instead of deleting it.
  // The prior install stays recoverable until the rename below succeeds.
  let moved_aside = if destination.exists() {
    let backup = staged_path(destination, "replacing", &suffix);
    if let Err(err) = fs::rename(destination, &backup) {
      let _ = fs::remove_dir_all(&staging);
      return Err(err).with_context(|| {
        format!(
          "Failed to move aside existing {label} directory '{}'",
          destination.display()
        )
      });
    }
    Some(backup)
  } else {
    None
  };

  // Swap staged → destination.
  if let Err(err) = fs::rename(&staging, destination) {
    // Roll back: restore the prior install, then remove the staged tree.
    if let Some(ref backup) = moved_aside {
      let _ = fs::rename(backup, destination);
    }
    let _ = fs::remove_dir_all(&staging);
    return Err(err).with_context(|| {
      format!(
        "Failed to atomically install {label} into '{}'",
        destination.display()
      )
    });
  }

  // Final cleanup: drop the moved-aside prior install. Failure here is
  // logged as a warning but does not undo a successful install.
  if let Some(backup) = moved_aside
    && let Err(err) = fs::remove_dir_all(&backup)
  {
    eprintln!(
      "⚠  could not remove previous {label} directory '{}': {err}",
      backup.display()
    );
  }
  Ok(())
}

/// Build a unique suffix for staging directories. Uses pid + nanos since
/// epoch — racing two installers against the same target would collide
/// inside `copy_dir_recursive` anyway because both would try to write to
/// the final `destination`, so the suffix only needs to disambiguate
/// against stale leftovers.
fn atomic_suffix() -> String {
  let nanos = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_nanos())
    .unwrap_or(0);
  format!("{}-{}", std::process::id(), nanos)
}

fn staged_path(destination: &Path, role: &str, suffix: &str) -> PathBuf {
  let parent = destination
    .parent()
    .map(Path::to_path_buf)
    .unwrap_or_else(|| PathBuf::from("."));
  let name = destination
    .file_name()
    .map(|s| s.to_string_lossy().into_owned())
    .unwrap_or_else(|| "package".to_string());
  parent.join(format!(".{name}.{role}-{suffix}"))
}

fn prevent_recursive_install(source: &Path, destination: &Path, label: &str) -> Result<()> {
  let source = fs::canonicalize(source)?;
  let destination_parent = destination
    .parent()
    .map(Path::to_path_buf)
    .unwrap_or_else(|| PathBuf::from("."));
  fs::create_dir_all(&destination_parent)?;
  let destination_parent = fs::canonicalize(destination_parent)?;

  if destination_parent.starts_with(&source) {
    bail!(
      "Refusing to install {} '{}' into its own source tree '{}'",
      label,
      source.display(),
      destination.display()
    );
  }
  Ok(())
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<()> {
  for entry in WalkDir::new(source) {
    let entry = entry?;
    let relative = entry.path().strip_prefix(source)?;
    if relative.as_os_str().is_empty() {
      continue;
    }

    let target = destination.join(relative);
    if entry.file_type().is_dir() {
      fs::create_dir_all(&target)?;
    } else if entry.file_type().is_file() {
      if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
      }
      fs::copy(entry.path(), &target)?;
      copy_executable_bit(entry.path(), &target)?;
    } else {
      bail!(
        "Unsupported package entry '{}' while copying '{}'",
        entry.path().display(),
        source.display()
      );
    }
  }
  Ok(())
}

#[cfg(unix)]
fn copy_executable_bit(source: &Path, destination: &Path) -> Result<()> {
  let perms = fs::metadata(source)?.permissions();
  fs::set_permissions(destination, fs::Permissions::from_mode(perms.mode()))?;
  Ok(())
}

#[cfg(not(unix))]
fn copy_executable_bit(_source: &Path, _destination: &Path) -> Result<()> {
  Ok(())
}

fn default_skills_dir() -> PathBuf {
  dirs::home_dir()
    .unwrap_or_else(|| PathBuf::from("."))
    .join(".agentflow")
    .join("skills")
}

#[cfg(feature = "plugin")]
fn default_plugins_dir() -> PathBuf {
  dirs::home_dir()
    .unwrap_or_else(|| PathBuf::from("."))
    .join(".agentflow")
    .join("plugins")
}

fn sanitize_path_segment(value: &str) -> String {
  value
    .chars()
    .map(|ch| {
      if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
        ch
      } else {
        '_'
      }
    })
    .collect()
}

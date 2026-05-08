use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

use agentflow_skills::{
  MarketplacePackageType, RemoteMarketplaceCache, RemoteMarketplaceClient, RemoteMarketplaceEntry,
  RemoteMarketplaceManifest,
};

pub async fn search(
  registry: String,
  query: Option<String>,
  package_type: Option<String>,
) -> Result<()> {
  let manifest = load_manifest(&registry).await?;
  let package_type = parse_package_type_opt(package_type.as_deref())?;
  let query = query.map(|q| q.to_ascii_lowercase());
  let entries = matching_entries(&manifest, query.as_deref(), package_type);

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

pub async fn install(
  registry: String,
  package: String,
  package_type: Option<String>,
  cache_dir: Option<String>,
) -> Result<()> {
  let manifest = load_manifest(&registry).await?;
  let package_type = parse_package_type_opt(package_type.as_deref())?;
  let entry = resolve_entry(&manifest, &package, package_type)?;
  let cache = cache_from_dir(cache_dir);
  let cached = cache
    .fetch_and_cache_artifact(entry)
    .await
    .with_context(|| format!("Failed to fetch and cache package '{}'", entry.name))?;

  println!(
    "✅ Cached {} package: {}",
    entry.package_type.as_str(),
    entry.name
  );
  println!("   version: {}", entry.version);
  println!("   path: {}", cached.path.display());
  println!("   checksum: sha256:{}", cached.checksum_sha256);
  println!("   signature_checked: {}", cached.signature_checked);
  println!("   next: unpack/install integration lands in the package-specific installer");
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

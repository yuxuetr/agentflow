use anyhow::{Context, Result};
use std::path::Path;

use agentflow_skills::SkillRegistryIndex;

pub async fn validate(index_file: String) -> Result<()> {
  let path = Path::new(&index_file);
  let index = SkillRegistryIndex::load(path)
    .with_context(|| format!("Failed to load skill registry index '{}'", index_file))?;
  let warnings = index
    .validate_at(path)
    .with_context(|| format!("Skill registry index '{}' is invalid", index_file))?;

  println!(
    "📚 Registry index: {} [{} skills]",
    index.name,
    index.entries().len()
  );
  if let Some(description) = &index.description {
    println!("   {}", description);
  }
  for warning in &warnings {
    println!("   ⚠  {}", warning);
  }
  println!("\n✅ Skill registry index is valid");

  Ok(())
}

pub async fn list(index_file: String) -> Result<()> {
  let path = Path::new(&index_file);
  let index = SkillRegistryIndex::load(path)
    .with_context(|| format!("Failed to load skill registry index '{}'", index_file))?;

  println!(
    "📚 Registry index: {} [{} skills]",
    index.name,
    index.entries().len()
  );
  if let Some(description) = &index.description {
    println!("   {}", description);
  }

  for entry in index.entries() {
    println!("   - {} @ {}", entry.name, entry.version);
    println!("     path: {}", entry.path);
    if let Some(channel) = &entry.channel {
      println!("     channel: {}", channel);
    }
    if let Some(checksum) = &entry.manifest_sha256 {
      println!(
        "     lock: sha256:{}",
        checksum.trim().trim_start_matches("sha256:")
      );
    } else {
      println!("     lock: version only");
    }
    if !entry.aliases.is_empty() {
      println!("     aliases: {}", entry.aliases.join(", "));
    }
  }

  Ok(())
}

pub async fn resolve(index_file: String, skill: String) -> Result<()> {
  let path = Path::new(&index_file);
  let index = SkillRegistryIndex::load(path)
    .with_context(|| format!("Failed to load skill registry index '{}'", index_file))?;
  let resolved = index
    .resolve_skill(&skill, path)
    .with_context(|| format!("Failed to resolve skill '{}' from '{}'", skill, index_file))?;

  println!("📦 {}", resolved.name);
  println!("   version: {}", resolved.version);
  println!("   path: {}", resolved.path.display());
  println!("   manifest: {}", resolved.manifest.display());
  if let Some(channel) = &resolved.channel {
    println!("   channel: {}", channel);
  }
  if let Some(checksum) = &resolved.manifest_sha256 {
    println!(
      "   lock: sha256:{}",
      checksum.trim().trim_start_matches("sha256:")
    );
  }
  if !resolved.aliases.is_empty() {
    println!("   aliases: {}", resolved.aliases.join(", "));
  }

  Ok(())
}

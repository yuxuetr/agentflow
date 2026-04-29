use anyhow::{Context, Result};
use std::path::Path;

use super::install as skill_install;
use agentflow_skills::SkillMarketplace;

pub async fn validate(marketplace_file: String) -> Result<()> {
  let path = Path::new(&marketplace_file);
  let marketplace = SkillMarketplace::load(path)
    .with_context(|| format!("Failed to load skill marketplace '{}'", marketplace_file))?;
  let warnings = marketplace
    .validate_at(path)
    .with_context(|| format!("Skill marketplace '{}' is invalid", marketplace_file))?;

  println!(
    "🛒 Skill marketplace: {} [{} indexes]",
    marketplace.name,
    marketplace.indexes().len()
  );
  if let Some(description) = &marketplace.description {
    println!("   {}", description);
  }
  for warning in &warnings {
    println!("   ⚠  {}", warning);
  }
  println!("\n✅ Skill marketplace is valid");

  Ok(())
}

pub async fn list(marketplace_file: String) -> Result<()> {
  let path = Path::new(&marketplace_file);
  let marketplace = SkillMarketplace::load(path)
    .with_context(|| format!("Failed to load skill marketplace '{}'", marketplace_file))?;
  let listings = marketplace.list_local_skills(path).with_context(|| {
    format!(
      "Failed to list skills from marketplace '{}'",
      marketplace_file
    )
  })?;

  println!(
    "🛒 Skill marketplace: {} [{} indexes]",
    marketplace.name,
    marketplace.indexes().len()
  );
  if let Some(description) = &marketplace.description {
    println!("   {}", description);
  }

  for listing in listings {
    println!("   - {} @ {}", listing.skill_name, listing.version);
    println!("     index: {}", listing.index_name);
    println!(
      "     install: agentflow skill install {} {}",
      listing.index_source.display(),
      listing.skill_name
    );
    if let Some(channel) = listing.channel {
      println!("     channel: {channel}");
    }
    if !listing.aliases.is_empty() {
      println!("     aliases: {}", listing.aliases.join(", "));
    }
  }

  Ok(())
}

pub async fn resolve(marketplace_file: String, skill: String) -> Result<()> {
  let path = Path::new(&marketplace_file);
  let marketplace = SkillMarketplace::load(path)
    .with_context(|| format!("Failed to load skill marketplace '{}'", marketplace_file))?;
  let resolved = marketplace
    .resolve_local_skill(&skill, path)
    .with_context(|| {
      format!(
        "Failed to resolve skill '{}' from '{}'",
        skill, marketplace_file
      )
    })?;

  println!("📦 {}", resolved.skill_name);
  println!("   version: {}", resolved.version);
  println!("   index: {}", resolved.index_name);
  println!("   index_file: {}", resolved.index_source.display());
  println!(
    "   install: agentflow skill install {} {}",
    resolved.index_source.display(),
    resolved.skill_name
  );

  Ok(())
}

pub async fn install(
  marketplace_file: String,
  skill: String,
  target_dir: Option<String>,
  force: bool,
) -> Result<()> {
  let path = Path::new(&marketplace_file);
  let marketplace = SkillMarketplace::load(path)
    .with_context(|| format!("Failed to load skill marketplace '{}'", marketplace_file))?;
  let resolved = marketplace
    .resolve_local_skill(&skill, path)
    .with_context(|| {
      format!(
        "Failed to resolve skill '{}' from '{}'",
        skill, marketplace_file
      )
    })?;

  println!(
    "🛒 Resolved '{}' from marketplace '{}' via index '{}'",
    resolved.skill_name, marketplace.name, resolved.index_name
  );
  skill_install::execute(
    resolved.index_source.display().to_string(),
    resolved.skill_name,
    target_dir,
    force,
  )
  .await
}

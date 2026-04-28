use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use agentflow_skills::{SkillLoader, SkillRegistryIndex};

pub async fn execute(
  index_file: String,
  skill: String,
  target_dir: Option<String>,
  force: bool,
) -> Result<()> {
  let index_path = Path::new(&index_file);
  let index = SkillRegistryIndex::load(index_path)
    .with_context(|| format!("Failed to load skill registry index '{}'", index_file))?;
  let resolved = index.resolve_skill(&skill, index_path).with_context(|| {
    format!(
      "Failed to resolve skill '{}' from registry index '{}'",
      skill, index_file
    )
  })?;

  if !resolved.path.is_dir() {
    anyhow::bail!(
      "Resolved skill '{}' from index '{}' points to a missing directory '{}'",
      skill,
      index_file,
      resolved.path.display()
    );
  }

  let manifest = SkillLoader::load(&resolved.path).with_context(|| {
    format!(
      "Resolved skill '{}' from index '{}' is not a valid skill at '{}'",
      skill,
      index_file,
      resolved.path.display()
    )
  })?;
  let warnings = SkillLoader::validate(&manifest, &resolved.path).with_context(|| {
    format!(
      "Resolved skill '{}' from index '{}' failed validation before install",
      skill, index_file
    )
  })?;

  let install_root = resolve_target_dir(target_dir);
  fs::create_dir_all(&install_root).with_context(|| {
    format!(
      "Failed to create target directory '{}' for skill '{}' from index '{}'",
      install_root.display(),
      skill,
      index_file
    )
  })?;

  let destination = install_root.join(&resolved.name);
  prevent_recursive_install(&resolved.path, &destination)?;

  if destination.exists() {
    if !force {
      anyhow::bail!(
        "Target skill directory '{}' already exists for skill '{}' from index '{}'; pass --force to overwrite",
        destination.display(),
        skill,
        index_file
      );
    }
    fs::remove_dir_all(&destination).with_context(|| {
      format!(
        "Failed to remove existing target directory '{}' for skill '{}' from index '{}'",
        destination.display(),
        skill,
        index_file
      )
    })?;
  }

  copy_dir_recursive(&resolved.path, &destination).with_context(|| {
    format!(
      "Failed to install skill '{}' from index '{}' into '{}'",
      skill,
      index_file,
      destination.display()
    )
  })?;

  println!(
    "📦 Installed skill: {} @ {}",
    resolved.name, resolved.version
  );
  println!("   from: {}", resolved.path.display());
  println!("   to: {}", destination.display());
  let installed_manifest = resolved
    .manifest
    .strip_prefix(&resolved.path)
    .map(|relative| destination.join(relative))
    .unwrap_or_else(|_| destination.join(manifest_path_name(&resolved.manifest)));
  println!("   manifest: {}", installed_manifest.display());
  if let Some(channel) = &resolved.channel {
    println!("   channel: {}", channel);
  }
  if !warnings.is_empty() {
    for warning in warnings {
      println!("   ⚠  {}", warning);
    }
  }
  println!(
    "\nValidate with: agentflow skill validate {}",
    destination.display()
  );

  Ok(())
}

fn resolve_target_dir(target_dir: Option<String>) -> PathBuf {
  match target_dir {
    Some(dir) => PathBuf::from(dir),
    None => dirs::home_dir()
      .unwrap_or_else(|| PathBuf::from("."))
      .join(".agentflow")
      .join("skills"),
  }
}

fn prevent_recursive_install(source: &Path, destination: &Path) -> Result<()> {
  let source = fs::canonicalize(source)?;
  let destination_parent = destination
    .parent()
    .map(Path::to_path_buf)
    .unwrap_or_else(|| PathBuf::from("."));
  let destination_parent = fs::canonicalize(destination_parent)?;

  if destination_parent.starts_with(&source) {
    anyhow::bail!(
      "Refusing to install skill '{}' into its own source tree '{}'",
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
    } else {
      anyhow::bail!(
        "Unsupported skill package entry '{}' while copying '{}'",
        entry.path().display(),
        source.display()
      );
    }
  }

  Ok(())
}

fn manifest_path_name(manifest: &Path) -> &Path {
  manifest
    .file_name()
    .map(Path::new)
    .unwrap_or_else(|| Path::new("SKILL.md"))
}

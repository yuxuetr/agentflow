//! `agentflow plugin uninstall` — remove an installed plugin directory.
//!
//! Resolves `<plugins_dir>/<name>/`, refuses to delete a directory that
//! does not contain a `plugin.toml` manifest (defense-in-depth: the path
//! is constructed from user input and we don't want a typo to wipe an
//! arbitrary directory), then removes the tree.

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use super::install::default_plugins_dir;

pub async fn execute(name: String, plugins_dir: Option<String>, force: bool) -> Result<()> {
  let root = plugins_dir
    .map(PathBuf::from)
    .unwrap_or_else(default_plugins_dir);
  let target = root.join(&name);

  if !target.exists() {
    if force {
      println!(
        "ℹ  No plugin '{}' installed at '{}', nothing to do.",
        name,
        target.display()
      );
      return Ok(());
    }
    anyhow::bail!(
      "Plugin '{}' is not installed at '{}'",
      name,
      target.display()
    );
  }

  if !target.is_dir() {
    anyhow::bail!(
      "Cannot uninstall '{}': '{}' is not a directory",
      name,
      target.display()
    );
  }

  let manifest_path = target.join("plugin.toml");
  if !manifest_path.is_file() {
    anyhow::bail!(
      "Refusing to remove '{}': missing plugin.toml (not a recognised plugin install)",
      target.display()
    );
  }

  fs::remove_dir_all(&target).with_context(|| {
    format!(
      "Failed to remove plugin directory '{}' for plugin '{}'",
      target.display(),
      name
    )
  })?;

  println!(
    "🗑  Uninstalled plugin '{}' from '{}'",
    name,
    target.display()
  );

  Ok(())
}

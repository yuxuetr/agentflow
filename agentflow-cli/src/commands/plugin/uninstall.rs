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

pub async fn execute(
  name: String,
  plugins_dir: Option<String>,
  force: bool,
  format: String,
) -> Result<()> {
  let root = plugins_dir
    .map(PathBuf::from)
    .unwrap_or_else(default_plugins_dir);
  let target = root.join(&name);
  let is_envelope = format == "json-envelope";

  if !target.exists() {
    if force {
      let msg = format!(
        "No plugin '{}' installed at '{}', nothing to do.",
        name,
        target.display()
      );
      if is_envelope {
        let payload = serde_json::json!({
          "name": name,
          "plugins_dir": root,
          "target": target,
          "removed": false,
          "reason": "not_installed_force_acked",
        });
        let envelope = crate::json_envelope::CliJsonEnvelope::ok("plugin uninstall", &payload);
        println!("{}", serde_json::to_string_pretty(&envelope)?);
      } else {
        println!("ℹ  {msg}");
      }
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

  if is_envelope {
    // P3.3 migration: envelope-wrap the uninstall outcome so shell
    // automation can `jq '.result.removed'` instead of grep'ing the
    // emoji line. `reason` distinguishes the happy path from the
    // `--force` short-circuit when the plugin wasn't installed.
    let payload = serde_json::json!({
      "name": name,
      "plugins_dir": root,
      "target": target,
      "removed": true,
      "reason": "removed",
    });
    let envelope = crate::json_envelope::CliJsonEnvelope::ok("plugin uninstall", &payload);
    println!("{}", serde_json::to_string_pretty(&envelope)?);
  } else {
    println!(
      "🗑  Uninstalled plugin '{}' from '{}'",
      name,
      target.display()
    );
  }

  Ok(())
}

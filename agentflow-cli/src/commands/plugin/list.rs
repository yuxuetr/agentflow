//! `agentflow plugin list` — enumerate installed plugins and the node
//! types each one declares.

use anyhow::Result;
use std::path::PathBuf;

use agentflow_core::plugin::PluginManifest;

use super::install::default_plugins_dir;

pub async fn execute(plugins_dir: Option<String>) -> Result<()> {
  let dir = plugins_dir
    .map(PathBuf::from)
    .unwrap_or_else(default_plugins_dir);

  if !dir.exists() {
    println!("📂 Plugins directory not found: {}", dir.display());
    println!("   Install one with: agentflow plugin install <source-dir>");
    return Ok(());
  }

  println!("📂 Scanning plugins in: {}\n", dir.display());

  let entries = std::fs::read_dir(&dir)
    .map_err(|e| anyhow::anyhow!("Cannot read plugins dir '{}': {}", dir.display(), e))?;

  let mut found = 0usize;

  for entry in entries.flatten() {
    let path = entry.path();
    if !path.is_dir() {
      continue;
    }
    let manifest_path = path.join("plugin.toml");
    if !manifest_path.is_file() {
      continue;
    }

    match PluginManifest::load_from_path(&manifest_path) {
      Ok((manifest, _)) => {
        let validation = manifest.validate();
        let status = if validation.is_ok() { "✅" } else { "⚠ " };
        println!(
          "{} {} v{} [runtime: {:?}]",
          status, manifest.plugin.name, manifest.plugin.version, manifest.plugin.runtime,
        );
        let entrypoint = manifest.resolve_entrypoint(&path);
        let entry_status = if entrypoint.exists() { "ok" } else { "missing" };
        println!(
          "   📦 entrypoint: {} [{}]",
          entrypoint.display(),
          entry_status
        );
        if manifest.plugin.nodes.is_empty() {
          println!("   🧩 nodes: none");
        } else {
          let names: Vec<&str> = manifest
            .plugin
            .nodes
            .iter()
            .map(|n| n.node_type.as_str())
            .collect();
          println!("   🧩 nodes: {}", names.join(", "));
        }
        let caps = &manifest.plugin.capabilities;
        let cap_summary = format!(
          "fs:{} net:{} proc:{} env:{}",
          caps.filesystem.len(),
          caps.network.len(),
          caps.processes.len(),
          caps.env_vars.len(),
        );
        println!("   🛡  capabilities: {}", cap_summary);
        println!("   📁 {}", path.display());
        if let Err(e) = validation {
          println!("   ⚠  manifest invalid: {}", e);
        }
        println!();
        found += 1;
      }
      Err(e) => {
        println!("❌ {} — {}", path.display(), e);
        println!();
      }
    }
  }

  if found == 0 {
    println!("No valid plugins found.");
    println!("Install one with: agentflow plugin install <source-dir>");
  } else {
    println!("Found {} plugin(s).", found);
  }

  Ok(())
}

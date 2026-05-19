//! `agentflow plugin list` — enumerate installed plugins and the node
//! types each one declares.

use anyhow::Result;
use serde::Serialize;
use std::path::PathBuf;

use agentflow_core::plugin::PluginManifest;

use super::install::default_plugins_dir;

/// JSON shape for one installed plugin under the envelope's
/// `result.plugins[]` array. Mirrors the text view's columns but
/// preserves richer typing (paths as strings, capabilities as full
/// arrays not just counts).
#[derive(Debug, Clone, Serialize)]
struct PluginListEntry {
  name: String,
  version: String,
  runtime: String,
  entrypoint: PathBuf,
  entrypoint_exists: bool,
  nodes: Vec<String>,
  capabilities: PluginCapabilitySummary,
  install_dir: PathBuf,
  /// `true` when `PluginManifest::validate()` returned `Ok`. Failed
  /// validation also surfaces in `envelope.errors[]`.
  manifest_valid: bool,
  /// Validation error message when `manifest_valid == false`.
  #[serde(skip_serializing_if = "Option::is_none")]
  manifest_error: Option<String>,
}

/// Full capability arrays (not just counts) so the JSON consumer can
/// answer "which plugin grants writing to /tmp" without re-reading
/// every plugin.toml.
#[derive(Debug, Clone, Serialize)]
struct PluginCapabilitySummary {
  filesystem: Vec<String>,
  network: Vec<String>,
  processes: Vec<String>,
  env_vars: Vec<String>,
}

pub async fn execute(plugins_dir: Option<String>, format: String) -> Result<()> {
  let dir = plugins_dir
    .map(PathBuf::from)
    .unwrap_or_else(default_plugins_dir);
  let is_json_envelope = format == "json-envelope";

  if !is_json_envelope && !dir.exists() {
    println!("📂 Plugins directory not found: {}", dir.display());
    println!("   Install one with: agentflow plugin install <source-dir>");
    return Ok(());
  }

  let entries: Vec<PluginListEntry> = if dir.exists() {
    collect_plugin_entries(&dir)?
  } else {
    Vec::new()
  };

  if is_json_envelope {
    // Surface invalid / unreadable manifests in `errors[]` so tooling
    // sees them without walking the array.
    let errors: Vec<String> = entries
      .iter()
      .filter_map(|e| {
        e.manifest_error
          .as_ref()
          .map(|m| format!("plugin '{}': {}", e.name, m))
      })
      .collect();
    let payload = serde_json::json!({
      "plugins_dir": dir,
      "plugins": &entries,
      "total": entries.len(),
    });
    let envelope =
      crate::json_envelope::CliJsonEnvelope::with_errors("plugin list", &payload, errors);
    println!("{}", serde_json::to_string_pretty(&envelope)?);
    return Ok(());
  }

  println!("📂 Scanning plugins in: {}\n", dir.display());
  let mut valid = 0usize;
  for entry in &entries {
    let status = if entry.manifest_valid { "✅" } else { "⚠ " };
    println!(
      "{} {} v{} [runtime: {}]",
      status, entry.name, entry.version, entry.runtime
    );
    let entry_status = if entry.entrypoint_exists {
      "ok"
    } else {
      "missing"
    };
    println!(
      "   📦 entrypoint: {} [{}]",
      entry.entrypoint.display(),
      entry_status
    );
    if entry.nodes.is_empty() {
      println!("   🧩 nodes: none");
    } else {
      println!("   🧩 nodes: {}", entry.nodes.join(", "));
    }
    let c = &entry.capabilities;
    println!(
      "   🛡  capabilities: fs:{} net:{} proc:{} env:{}",
      c.filesystem.len(),
      c.network.len(),
      c.processes.len(),
      c.env_vars.len()
    );
    println!("   📁 {}", entry.install_dir.display());
    if let Some(err) = &entry.manifest_error {
      println!("   ⚠  manifest invalid: {}", err);
    }
    println!();
    if entry.manifest_valid {
      valid += 1;
    }
  }

  if valid == 0 {
    println!("No valid plugins found.");
    println!("Install one with: agentflow plugin install <source-dir>");
  } else {
    println!("Found {} plugin(s).", valid);
  }

  Ok(())
}

fn collect_plugin_entries(dir: &PathBuf) -> Result<Vec<PluginListEntry>> {
  let entries = std::fs::read_dir(dir)
    .map_err(|e| anyhow::anyhow!("Cannot read plugins dir '{}': {}", dir.display(), e))?;
  let mut out = Vec::new();
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
        let entrypoint = manifest.resolve_entrypoint(&path);
        let caps = &manifest.plugin.capabilities;
        out.push(PluginListEntry {
          name: manifest.plugin.name.clone(),
          version: manifest.plugin.version.clone(),
          runtime: format!("{:?}", manifest.plugin.runtime),
          entrypoint: entrypoint.clone(),
          entrypoint_exists: entrypoint.exists(),
          nodes: manifest
            .plugin
            .nodes
            .iter()
            .map(|n| n.node_type.clone())
            .collect(),
          capabilities: PluginCapabilitySummary {
            filesystem: caps.filesystem.clone(),
            network: caps.network.clone(),
            processes: caps.processes.clone(),
            env_vars: caps.env_vars.clone(),
          },
          install_dir: path.clone(),
          manifest_valid: validation.is_ok(),
          manifest_error: validation.err().map(|e| e.to_string()),
        });
      }
      Err(e) => {
        // Synthetic entry so JSON consumers still see the broken
        // install. `name` falls back to the directory's basename
        // since we couldn't parse the manifest.
        let basename = path
          .file_name()
          .and_then(|n| n.to_str())
          .unwrap_or("<unknown>")
          .to_string();
        out.push(PluginListEntry {
          name: basename,
          version: String::new(),
          runtime: String::new(),
          entrypoint: PathBuf::new(),
          entrypoint_exists: false,
          nodes: Vec::new(),
          capabilities: PluginCapabilitySummary {
            filesystem: Vec::new(),
            network: Vec::new(),
            processes: Vec::new(),
            env_vars: Vec::new(),
          },
          install_dir: path.clone(),
          manifest_valid: false,
          manifest_error: Some(e.to_string()),
        });
      }
    }
  }
  Ok(out)
}

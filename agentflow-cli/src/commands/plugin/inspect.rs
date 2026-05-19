//! `agentflow plugin inspect` — print the manifest of a single plugin
//! without spawning the subprocess.
//!
//! Accepts either a plugin directory (containing `plugin.toml`) or the
//! `plugin.toml` path directly. Reports the resolved absolute entrypoint
//! and whether it exists / is executable, but never starts the plugin —
//! diagnosis only.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use agentflow_core::plugin::PluginManifest;

pub async fn execute(target: String, format: String) -> Result<()> {
  let target_path = Path::new(&target);
  let manifest_path = resolve_manifest_path(target_path)?;

  let (manifest, manifest_dir) =
    PluginManifest::load_from_path(&manifest_path).with_context(|| {
      format!(
        "Failed to parse plugin manifest at '{}'",
        manifest_path.display()
      )
    })?;
  let validation = manifest.validate();
  let resolved_entrypoint = manifest.resolve_entrypoint(&manifest_dir);

  if format == "json-envelope" {
    // P3.3 migration: emit the full manifest + resolved metadata
    // (entrypoint absolute path / exists / executable flags) as a
    // canonical envelope. Validation failure surfaces in `errors[]`
    // so consumers don't have to inspect `result.manifest_valid`.
    let payload = serde_json::json!({
      "manifest_path": manifest_path,
      "manifest_dir": manifest_dir,
      "manifest": &manifest,
      "resolved_entrypoint": resolved_entrypoint,
      "entrypoint_exists": resolved_entrypoint.exists(),
      "entrypoint_executable": is_executable(&resolved_entrypoint),
      "manifest_valid": validation.is_ok(),
      "manifest_error": validation.as_ref().err().map(|e| e.to_string()),
    });
    let errors: Vec<String> = validation
      .err()
      .map(|e| vec![e.to_string()])
      .unwrap_or_default();
    let envelope =
      crate::json_envelope::CliJsonEnvelope::with_errors("plugin inspect", &payload, errors);
    println!("{}", serde_json::to_string_pretty(&envelope)?);
    return Ok(());
  }

  println!("🔌 Plugin: {}", manifest.plugin.name);
  println!("Version: {}", manifest.plugin.version);
  println!("Runtime: {:?}", manifest.plugin.runtime);
  println!("Protocol: {}", manifest.plugin.protocol);
  println!("Manifest: {}", manifest_path.display());
  println!("Manifest dir: {}", manifest_dir.display());

  println!(
    "Entrypoint: {} (declared: {})",
    resolved_entrypoint.display(),
    manifest.plugin.entrypoint.display()
  );
  println!(
    "  exists: {}",
    if resolved_entrypoint.exists() {
      "yes"
    } else {
      "no"
    }
  );
  println!(
    "  executable: {}",
    if is_executable(&resolved_entrypoint) {
      "yes"
    } else {
      "no"
    }
  );
  println!();

  println!("Nodes:");
  if manifest.plugin.nodes.is_empty() {
    println!("  none");
  } else {
    for node in &manifest.plugin.nodes {
      println!("  - {}", node.node_type);
      if !node.description.is_empty() {
        println!("    {}", node.description);
      }
    }
  }
  println!();

  println!("Capabilities:");
  let caps = &manifest.plugin.capabilities;
  print_cap("filesystem", &caps.filesystem);
  print_cap("network", &caps.network);
  print_cap("processes", &caps.processes);
  print_cap("env_vars", &caps.env_vars);

  println!();
  match validation {
    Ok(()) => println!("Status: valid"),
    Err(e) => println!("Status: invalid — {}", e),
  }

  Ok(())
}

fn resolve_manifest_path(target: &Path) -> Result<PathBuf> {
  if target.is_file() {
    return Ok(target.to_path_buf());
  }
  let candidate = target.join("plugin.toml");
  if candidate.is_file() {
    return Ok(candidate);
  }
  anyhow::bail!(
    "'{}' is neither a plugin manifest file nor a directory containing plugin.toml",
    target.display()
  );
}

fn print_cap(label: &str, values: &[String]) {
  if values.is_empty() {
    println!("  {}: (none)", label);
  } else {
    println!("  {}: {}", label, values.join(", "));
  }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
  use std::os::unix::fs::PermissionsExt;
  match std::fs::metadata(path) {
    Ok(meta) => meta.is_file() && (meta.permissions().mode() & 0o111 != 0),
    Err(_) => false,
  }
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
  path.is_file()
}

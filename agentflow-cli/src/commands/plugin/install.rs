//! `agentflow plugin install` — copy a plugin source directory into the
//! local plugins root (default `~/.agentflow/plugins/<name>/`).
//!
//! Mirrors `agentflow skill install`'s structure: validate the source
//! manifest before copying anything, refuse to install into the source's
//! own subtree, and require `--force` to overwrite an existing target.

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use agentflow_core::plugin::PluginManifest;
use agentflow_tools::sandbox::{SandboxEnforcement, default_backend};
use agentflow_tools::{PluginEvaluationInput, PluginPolicy, SecurityProfile};

#[allow(clippy::too_many_arguments)]
pub async fn execute(
  source_dir: String,
  target_dir: Option<String>,
  force: bool,
  allow_unsandboxed_plugin: bool,
  has_signature: bool,
  format: String,
) -> Result<()> {
  let source = Path::new(&source_dir);
  if !source.is_dir() {
    anyhow::bail!("Plugin source '{}' is not a directory", source.display());
  }

  let manifest_path = source.join("plugin.toml");
  if !manifest_path.is_file() {
    anyhow::bail!(
      "Plugin source '{}' does not contain a plugin.toml manifest",
      source.display()
    );
  }

  let (manifest, _manifest_dir) =
    PluginManifest::load_from_path(&manifest_path).with_context(|| {
      format!(
        "Failed to parse plugin manifest at '{}'",
        manifest_path.display()
      )
    })?;
  manifest.validate().with_context(|| {
    format!(
      "Plugin manifest '{}' failed validation",
      manifest_path.display()
    )
  })?;

  // Plugin policy gate (P1.8). Evaluate before any filesystem write so
  // a denied install never half-installs.
  let profile = SecurityProfile::from_env().unwrap_or_default();
  let policy = PluginPolicy::for_profile(profile);
  let sandbox = default_backend();
  let sandbox_enforcing = matches!(sandbox.enforcement_level(), SandboxEnforcement::Enforcing);
  let network_grants = &manifest.plugin.capabilities.network;
  let mut input = PluginEvaluationInput::new(&manifest.plugin.name);
  input.has_signature = has_signature;
  input.sandbox_enforcing = sandbox_enforcing;
  input.network_requested = !network_grants.is_empty();
  // The current manifest treats any non-empty list as explicit
  // origins; bare `["*"]` (deliberate wildcard) collapses to a
  // single non-explicit entry so production rejects it.
  input.network_origins_explicit = network_grants
    .iter()
    .all(|origin| origin != "*" && !origin.is_empty());
  input.allow_unsandboxed_opt_in = allow_unsandboxed_plugin;
  let decision = policy.evaluate(&input);
  tracing::info!(
    target: "agentflow.plugin.policy",
    plugin = %decision.plugin_name,
    profile = %decision.profile,
    allowed = decision.allowed,
    sandbox_active = decision.sandbox_active,
    signature_checked = decision.signature_checked,
    network_policy = decision.network_policy.as_str(),
    "plugin policy decision"
  );
  if !decision.allowed {
    let reason = decision.deny_reason().unwrap_or_else(|| "denied".into());
    anyhow::bail!(
      "plugin '{}' rejected by `{}` policy: {reason}",
      manifest.plugin.name,
      decision.profile
    );
  }

  let resolved_entrypoint = manifest.resolve_entrypoint(source);
  if !resolved_entrypoint.exists() {
    eprintln!(
      "⚠  Plugin manifest at '{}' declares entrypoint '{}', but the file is missing in the source tree.",
      manifest_path.display(),
      resolved_entrypoint.display()
    );
    eprintln!(
      "   The plugin will install but won't run until the entrypoint is built and present at this path."
    );
  }

  let install_root = resolve_target_dir(target_dir);
  fs::create_dir_all(&install_root).with_context(|| {
    format!(
      "Failed to create plugins target directory '{}'",
      install_root.display()
    )
  })?;

  let destination = install_root.join(&manifest.plugin.name);
  prevent_recursive_install(source, &destination)?;

  if destination.exists() {
    if !force {
      anyhow::bail!(
        "Plugin directory '{}' already exists; pass --force to overwrite",
        destination.display()
      );
    }
    fs::remove_dir_all(&destination).with_context(|| {
      format!(
        "Failed to remove existing plugin directory '{}'",
        destination.display()
      )
    })?;
  }

  copy_dir_recursive(source, &destination).with_context(|| {
    format!(
      "Failed to install plugin '{}' into '{}'",
      manifest.plugin.name,
      destination.display()
    )
  })?;

  if format == "json-envelope" {
    // P3.3 migration: structured install result. Carries everything
    // the text view prints plus the policy decision so consumers
    // can audit which security profile gated the install.
    let payload = serde_json::json!({
      "name": manifest.plugin.name,
      "version": manifest.plugin.version,
      "source": source,
      "destination": destination,
      "manifest_path": destination.join("plugin.toml"),
      "entrypoint": destination.join(&manifest.plugin.entrypoint),
      "nodes": manifest
        .plugin
        .nodes
        .iter()
        .map(|n| n.node_type.clone())
        .collect::<Vec<_>>(),
      "policy": {
        "profile": decision.profile.to_string(),
        "allowed": decision.allowed,
        "sandbox_active": decision.sandbox_active,
        "signature_checked": decision.signature_checked,
        "network_policy": decision.network_policy.as_str(),
      },
    });
    let envelope =
      crate::json_envelope::CliJsonEnvelope::ok("plugin install", &payload);
    println!("{}", serde_json::to_string_pretty(&envelope)?);
    return Ok(());
  }

  println!(
    "🔌 Installed plugin: {} @ {}",
    manifest.plugin.name, manifest.plugin.version
  );
  println!("   from: {}", source.display());
  println!("   to: {}", destination.display());
  println!("   manifest: {}", destination.join("plugin.toml").display());
  println!(
    "   entrypoint: {}",
    destination.join(&manifest.plugin.entrypoint).display()
  );
  if !manifest.plugin.nodes.is_empty() {
    let names: Vec<&str> = manifest
      .plugin
      .nodes
      .iter()
      .map(|n| n.node_type.as_str())
      .collect();
    println!("   nodes: {}", names.join(", "));
  }
  println!(
    "\nInspect with: agentflow plugin inspect {}",
    destination.display()
  );

  Ok(())
}

fn resolve_target_dir(target_dir: Option<String>) -> PathBuf {
  match target_dir {
    Some(dir) => PathBuf::from(dir),
    None => default_plugins_dir(),
  }
}

pub(super) fn default_plugins_dir() -> PathBuf {
  dirs::home_dir()
    .unwrap_or_else(|| PathBuf::from("."))
    .join(".agentflow")
    .join("plugins")
}

fn prevent_recursive_install(source: &Path, destination: &Path) -> Result<()> {
  let source_canon = fs::canonicalize(source)?;
  let dest_parent = destination
    .parent()
    .map(Path::to_path_buf)
    .unwrap_or_else(|| PathBuf::from("."));
  fs::create_dir_all(&dest_parent).ok();
  let dest_parent_canon = fs::canonicalize(&dest_parent)?;

  if dest_parent_canon.starts_with(&source_canon) {
    anyhow::bail!(
      "Refusing to install plugin '{}' into its own source tree '{}'",
      source_canon.display(),
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
      anyhow::bail!(
        "Unsupported plugin source entry '{}' while copying '{}'",
        entry.path().display(),
        source.display()
      );
    }
  }

  Ok(())
}

#[cfg(unix)]
fn copy_executable_bit(source: &Path, destination: &Path) -> Result<()> {
  use std::os::unix::fs::PermissionsExt;
  let perms = fs::metadata(source)?.permissions();
  fs::set_permissions(destination, fs::Permissions::from_mode(perms.mode()))?;
  Ok(())
}

#[cfg(not(unix))]
fn copy_executable_bit(_source: &Path, _destination: &Path) -> Result<()> {
  Ok(())
}

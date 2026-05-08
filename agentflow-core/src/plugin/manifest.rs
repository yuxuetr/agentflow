//! `plugin.toml` manifest schema and loader.
//!
//! The manifest is a TOML file that lives next to (or refers to) the plugin
//! executable. It declares the plugin's name, version, the node types the
//! plugin contributes, and the OS capabilities it requires. See
//! `docs/PLUGIN_DESIGN.md` §6.2 for the full schema.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Wire protocol version this host implements. Plugins must declare the same
/// value (or an empty string, in which case it is filled in for them) in
/// their `plugin.toml`.
pub const SUPPORTED_PROTOCOL_VERSION: &str = "agentflow.plugin/1";

/// The runtime mechanism used to load and execute the plugin.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginRuntime {
  /// Spawn the plugin as a child process and talk JSON-RPC over stdio.
  /// This is the only runtime implemented in v0.3.0.
  #[default]
  Subprocess,
  /// Reserved for v1.1+: in-process WebAssembly runtime.
  Wasm,
}

/// Top-level manifest envelope. The TOML file looks like `[plugin]\n...`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginManifest {
  pub plugin: PluginSection,
}

/// The `[plugin]` section of `plugin.toml`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginSection {
  pub name: String,
  pub version: String,
  #[serde(default)]
  pub runtime: PluginRuntime,
  /// Path to the plugin executable. Relative paths are resolved against the
  /// directory containing the manifest.
  pub entrypoint: PathBuf,
  /// Wire protocol version the plugin claims to speak. Empty string is
  /// treated as "matches host".
  #[serde(default)]
  pub protocol: String,
  #[serde(default)]
  pub nodes: Vec<NodeSpec>,
  #[serde(default)]
  pub capabilities: Capabilities,
}

/// One declared node type. The host pre-registers a `PluginNode` for each
/// entry so workflows can reference it by `type`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NodeSpec {
  #[serde(rename = "type")]
  pub node_type: String,
  #[serde(default)]
  pub description: String,
}

/// OS capability declarations. The PoC only records these; actual sandbox
/// enforcement is wired up in a follow-up task (see PLUGIN_DESIGN §8).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Capabilities {
  #[serde(default)]
  pub filesystem: Vec<String>,
  #[serde(default)]
  pub network: Vec<String>,
  #[serde(default)]
  pub processes: Vec<String>,
  #[serde(default)]
  pub env_vars: Vec<String>,
}

#[derive(Debug, Error)]
pub enum ManifestError {
  #[error("failed to read plugin manifest: {0}")]
  Io(#[from] std::io::Error),
  #[error("failed to parse plugin manifest: {0}")]
  Parse(#[from] toml::de::Error),
  #[error("plugin manifest declares protocol '{actual}' but host implements '{expected}'")]
  ProtocolMismatch {
    actual: String,
    expected: &'static str,
  },
  #[error("plugin runtime '{runtime:?}' is not supported by this host")]
  UnsupportedRuntime { runtime: PluginRuntime },
}

impl PluginManifest {
  /// Load a manifest from `path` and return both the parsed value and the
  /// directory the manifest lives in (for resolving relative `entrypoint`).
  pub fn load_from_path(path: &Path) -> Result<(Self, PathBuf), ManifestError> {
    let raw = std::fs::read_to_string(path)?;
    let mut manifest: PluginManifest = toml::from_str(&raw)?;
    if manifest.plugin.protocol.is_empty() {
      manifest.plugin.protocol = SUPPORTED_PROTOCOL_VERSION.to_string();
    }
    let manifest_dir = match path.parent() {
      Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
      _ => PathBuf::from("."),
    };
    Ok((manifest, manifest_dir))
  }

  /// Reject manifests this host cannot run.
  pub fn validate(&self) -> Result<(), ManifestError> {
    if self.plugin.protocol != SUPPORTED_PROTOCOL_VERSION {
      return Err(ManifestError::ProtocolMismatch {
        actual: self.plugin.protocol.clone(),
        expected: SUPPORTED_PROTOCOL_VERSION,
      });
    }
    if !matches!(self.plugin.runtime, PluginRuntime::Subprocess) {
      return Err(ManifestError::UnsupportedRuntime {
        runtime: self.plugin.runtime.clone(),
      });
    }
    Ok(())
  }

  /// Resolve the entrypoint path against `manifest_dir` if it's relative.
  pub fn resolve_entrypoint(&self, manifest_dir: &Path) -> PathBuf {
    if self.plugin.entrypoint.is_absolute() {
      self.plugin.entrypoint.clone()
    } else {
      manifest_dir.join(&self.plugin.entrypoint)
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_minimal_manifest() {
    let raw = r#"
[plugin]
name = "demo"
version = "0.1.0"
entrypoint = "bin/demo"

[[plugin.nodes]]
type = "demo_node"
description = "A demo node."
"#;
    let manifest: PluginManifest = toml::from_str(raw).unwrap();
    assert_eq!(manifest.plugin.name, "demo");
    assert_eq!(manifest.plugin.runtime, PluginRuntime::Subprocess);
    assert_eq!(manifest.plugin.nodes.len(), 1);
    assert_eq!(manifest.plugin.nodes[0].node_type, "demo_node");
  }

  #[test]
  fn rejects_unknown_protocol() {
    let mut manifest = PluginManifest {
      plugin: PluginSection {
        name: "x".into(),
        version: "0.1.0".into(),
        runtime: PluginRuntime::Subprocess,
        entrypoint: PathBuf::from("bin/x"),
        protocol: "agentflow.plugin/999".into(),
        nodes: vec![],
        capabilities: Capabilities::default(),
      },
    };
    assert!(matches!(
      manifest.validate(),
      Err(ManifestError::ProtocolMismatch { .. })
    ));
    manifest.plugin.protocol = SUPPORTED_PROTOCOL_VERSION.into();
    assert!(manifest.validate().is_ok());
  }

  #[test]
  fn rejects_wasm_runtime_in_v0_3() {
    let manifest = PluginManifest {
      plugin: PluginSection {
        name: "x".into(),
        version: "0.1.0".into(),
        runtime: PluginRuntime::Wasm,
        entrypoint: PathBuf::from("bin/x"),
        protocol: SUPPORTED_PROTOCOL_VERSION.into(),
        nodes: vec![],
        capabilities: Capabilities::default(),
      },
    };
    assert!(matches!(
      manifest.validate(),
      Err(ManifestError::UnsupportedRuntime { .. })
    ));
  }

  #[test]
  fn resolves_relative_entrypoint() {
    let manifest = PluginManifest {
      plugin: PluginSection {
        name: "x".into(),
        version: "0.1.0".into(),
        runtime: PluginRuntime::Subprocess,
        entrypoint: PathBuf::from("bin/x"),
        protocol: SUPPORTED_PROTOCOL_VERSION.into(),
        nodes: vec![],
        capabilities: Capabilities::default(),
      },
    };
    assert_eq!(
      manifest.resolve_entrypoint(Path::new("/tmp/foo")),
      PathBuf::from("/tmp/foo/bin/x")
    );
  }
}

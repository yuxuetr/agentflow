#![cfg(feature = "plugin")]

use std::path::{Path, PathBuf};

use agentflow_core::plugin::{
  Capabilities, FsAccess, PluginManifest, PluginRuntime, SUPPORTED_PROTOCOL_VERSION,
};

#[test]
fn plugin_manifest_fixture_ignores_unknown_optional_fields() {
  let manifest: PluginManifest =
    toml::from_str(include_str!("fixtures/plugin_manifests/plugin.toml")).unwrap();

  assert_eq!(manifest.plugin.name, "compat-plugin");
  assert_eq!(manifest.plugin.runtime, PluginRuntime::Subprocess);
  assert_eq!(manifest.plugin.protocol, SUPPORTED_PROTOCOL_VERSION);
  assert_eq!(manifest.plugin.nodes.len(), 1);
  assert_eq!(manifest.plugin.nodes[0].node_type, "compat_node");
  assert_eq!(manifest.plugin.capabilities.network, vec!["https://api.example.com"]);
  manifest.validate().unwrap();
}

#[test]
fn plugin_manifest_load_fills_empty_protocol_default() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("plugin.toml");
  std::fs::write(
    &path,
    r#"
[plugin]
name = "default-protocol"
version = "0.1.0"
runtime = "subprocess"
entrypoint = "bin/default-protocol"
"#,
  )
  .unwrap();

  let (manifest, manifest_dir) = PluginManifest::load_from_path(&path).unwrap();
  assert_eq!(manifest_dir, dir.path());
  assert_eq!(manifest.plugin.protocol, SUPPORTED_PROTOCOL_VERSION);
  assert_eq!(
    manifest.resolve_entrypoint(&manifest_dir),
    dir.path().join("bin/default-protocol")
  );
}

#[test]
fn plugin_capability_fixture_preserves_access_modes() {
  let capabilities = Capabilities {
    filesystem: vec!["read:./inputs".to_string(), "write:./outputs".to_string()],
    network: vec!["https://api.example.com".to_string()],
    processes: vec!["helper".to_string()],
    env_vars: vec!["COMPAT_TOKEN".to_string()],
  };

  let entries = capabilities.filesystem_entries().unwrap();
  assert_eq!(entries[0].access, FsAccess::Read);
  assert_eq!(entries[0].path, PathBuf::from("./inputs"));
  assert_eq!(entries[1].access, FsAccess::Write);
  assert_eq!(entries[1].path, PathBuf::from("./outputs"));
  assert!(capabilities.requires_fs_read());
  assert!(capabilities.requires_fs_write());
  assert!(capabilities.requires_net());
  assert!(capabilities.requires_exec());
  assert!(capabilities.requires_env());
}

#[test]
fn plugin_manifest_resolves_absolute_entrypoint_without_rewriting() {
  let manifest: PluginManifest = toml::from_str(
    r#"
[plugin]
name = "absolute-entrypoint"
version = "0.1.0"
runtime = "subprocess"
entrypoint = "/opt/agentflow/plugin"
protocol = "agentflow.plugin/1"
"#,
  )
  .unwrap();

  assert_eq!(
    manifest.resolve_entrypoint(Path::new("/tmp/ignored")),
    PathBuf::from("/opt/agentflow/plugin")
  );
}

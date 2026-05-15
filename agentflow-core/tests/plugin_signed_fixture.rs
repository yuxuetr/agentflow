//! P5.2 — Companion test for the signed plugin fixture.
//!
//! The full marketplace signature-gating tests live in
//! `agentflow-skills/tests/marketplace_signed.rs` (they need access
//! to `RemoteMarketplaceCache`, which isn't a dependency of
//! `agentflow-core`). This test only confirms that the fixture
//! shipped at `agentflow-core/tests/fixtures/signed/plugin-echo/`
//! still parses + validates as a real plugin manifest, so the
//! sibling crate's archive build keeps producing a meaningful
//! payload.

#![cfg(feature = "plugin")]

use std::path::Path;

use agentflow_core::plugin::{PluginManifest, PluginRuntime, SUPPORTED_PROTOCOL_VERSION};

#[test]
fn signed_plugin_fixture_manifest_parses_and_validates() {
  let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("tests/fixtures/signed/plugin-echo/plugin.toml");
  let (manifest, manifest_dir) = PluginManifest::load_from_path(&manifest_path)
    .expect("signed plugin fixture must parse");
  assert_eq!(manifest.plugin.name, "echo-plugin");
  assert_eq!(manifest.plugin.runtime, PluginRuntime::Subprocess);
  assert_eq!(manifest.plugin.protocol, SUPPORTED_PROTOCOL_VERSION);
  assert_eq!(manifest.plugin.nodes.len(), 1);
  assert_eq!(manifest.plugin.nodes[0].node_type, "echo_node");
  manifest.validate().expect("validate");

  // The fixture's entrypoint stub must exist on disk so plugin install
  // (and any future end-to-end signature → install test) can resolve
  // it from the package root.
  let entrypoint = manifest.resolve_entrypoint(&manifest_dir);
  assert!(
    entrypoint.is_file(),
    "fixture entrypoint missing at {}",
    entrypoint.display()
  );
}

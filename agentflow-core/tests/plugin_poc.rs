//! End-to-end PoC test for the subprocess plugin runtime (P2 #12).
//!
//! Builds the bundled demo plugin (`agentflow-echo-plugin`) on first run,
//! writes a temporary `plugin.toml` pointing at it, and verifies:
//!
//!  - `PluginHost::load` performs the initialize handshake and surfaces the
//!    plugin's declared node types.
//!  - `PluginNode::execute` round-trips `FlowValue` inputs/outputs through
//!    the plugin.
//!  - `PluginRegistry::shutdown_all` terminates the child process cleanly.
//!  - Calling `execute` after shutdown returns a structured error rather than
//!    hanging or panicking.
//!
//! Run with:
//!   cargo test -p agentflow-core --features plugin --test plugin_poc

#![cfg(feature = "plugin")]

use agentflow_core::async_node::AsyncNode;
use agentflow_core::plugin::{PluginHost, PluginRegistry};
use agentflow_core::value::FlowValue;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::OnceLock;

const DEMO_BIN_NAME: &str = "agentflow-echo-plugin";

/// Build the demo plugin once per test process and return its path.
fn ensure_demo_plugin_built() -> PathBuf {
  static CACHED: OnceLock<PathBuf> = OnceLock::new();
  CACHED
    .get_or_init(|| {
      let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
      let status = Command::new(&cargo)
        .args([
          "build",
          "--quiet",
          "--features",
          "plugin",
          "--bin",
          DEMO_BIN_NAME,
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("failed to invoke cargo build for demo plugin");
      assert!(status.success(), "cargo build for demo plugin failed");

      let exe_name = format!("{DEMO_BIN_NAME}{}", std::env::consts::EXE_SUFFIX);
      let mut candidates: Vec<PathBuf> = Vec::new();
      for dir in candidate_target_dirs() {
        for profile in ["debug", "release"] {
          candidates.push(dir.join(profile).join(&exe_name));
        }
      }
      candidates
        .iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_else(|| {
          panic!(
            "could not locate freshly-built demo plugin '{exe_name}' in any of: {candidates:?}"
          )
        })
    })
    .clone()
}

fn candidate_target_dirs() -> Vec<PathBuf> {
  let mut dirs = Vec::new();
  if let Ok(custom) = std::env::var("CARGO_TARGET_DIR") {
    dirs.push(PathBuf::from(custom));
  }
  // CARGO_TARGET_TMPDIR is `<target-dir>/tmp` for integration tests
  // (set by cargo since 1.54). Its parent is the target dir.
  if let Some(target_tmpdir) = option_env!("CARGO_TARGET_TMPDIR") {
    let path = PathBuf::from(target_tmpdir);
    if let Some(target) = path.parent() {
      dirs.push(target.to_path_buf());
    }
  }
  // The currently-running test binary lives in `<target-dir>/debug/deps/`;
  // walking two parents up gets us to the target dir.
  if let Ok(current) = std::env::current_exe()
    && let Some(deps) = current.parent()
    && let Some(profile) = deps.parent()
    && let Some(target) = profile.parent()
  {
    dirs.push(target.to_path_buf());
  }
  // Fallbacks for the standard layouts.
  let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  if let Some(workspace_root) = manifest_dir.parent() {
    dirs.push(workspace_root.join("target"));
  }
  dirs.push(manifest_dir.join("target"));
  dirs
}

fn write_manifest(tmpdir: &Path, entrypoint: &Path) -> PathBuf {
  let manifest = format!(
    r#"
[plugin]
name = "agentflow-echo-plugin"
version = "0.1.0"
runtime = "subprocess"
entrypoint = "{}"

[[plugin.nodes]]
type = "echo_uppercase"
description = "Uppercase a JSON string."
"#,
    entrypoint.display()
  );
  let path = tmpdir.join("plugin.toml");
  std::fs::write(&path, manifest).unwrap();
  path
}

#[tokio::test]
async fn plugin_loads_and_executes_node() {
  let bin = ensure_demo_plugin_built();
  let tmp = tempfile::tempdir().unwrap();
  let manifest_path = write_manifest(tmp.path(), &bin);

  let host = PluginHost::load(&manifest_path).await.expect("load plugin");
  assert_eq!(host.initialize_result().plugin_name, DEMO_BIN_NAME);
  assert_eq!(
    host.declared_node_types(),
    vec!["echo_uppercase".to_string()]
  );

  let registry = PluginRegistry::new();
  registry.register(Arc::new(host)).await.expect("register");

  assert_eq!(
    registry.node_types().await,
    vec!["echo_uppercase".to_string()]
  );

  let node = registry
    .create_node("echo_uppercase", "echo1")
    .await
    .expect("create node");

  let mut inputs: HashMap<String, FlowValue> = HashMap::new();
  inputs.insert(
    "text".to_string(),
    FlowValue::Json(serde_json::Value::String("hello plugin".into())),
  );
  let outputs = node.execute(&inputs).await.expect("execute node");
  let text = outputs.get("text").expect("text output present");
  match text {
    FlowValue::Json(serde_json::Value::String(s)) => {
      assert_eq!(s, "HELLO PLUGIN");
    }
    other => panic!("unexpected output variant: {other:?}"),
  }

  let shutdown_results = registry.shutdown_all().await;
  assert_eq!(shutdown_results.len(), 1);
  assert!(shutdown_results[0].1.is_ok(), "graceful shutdown");
}

#[tokio::test]
async fn execute_after_shutdown_fails_cleanly() {
  let bin = ensure_demo_plugin_built();
  let tmp = tempfile::tempdir().unwrap();
  let manifest_path = write_manifest(tmp.path(), &bin);

  let host = Arc::new(PluginHost::load(&manifest_path).await.expect("load"));
  let registry = PluginRegistry::new();
  registry.register(host.clone()).await.expect("register");

  // Shut down via the host directly, simulating the plugin going away while
  // a node handle is still around.
  host.shutdown().await.expect("shutdown");

  let node = registry
    .create_node("echo_uppercase", "echo2")
    .await
    .expect("create node");
  let mut inputs: HashMap<String, FlowValue> = HashMap::new();
  inputs.insert(
    "text".to_string(),
    FlowValue::Json(serde_json::Value::String("anything".into())),
  );

  let result = node.execute(&inputs).await;
  assert!(
    result.is_err(),
    "execute after shutdown must surface as an error, got {result:?}"
  );
}

#[tokio::test]
async fn unknown_node_type_returns_protocol_error() {
  let bin = ensure_demo_plugin_built();
  let tmp = tempfile::tempdir().unwrap();
  let manifest_path = write_manifest(tmp.path(), &bin);

  let host = PluginHost::load(&manifest_path).await.expect("load");
  let result = host.execute_node("does_not_exist", HashMap::new()).await;
  assert!(matches!(
    result,
    Err(agentflow_core::plugin::PluginError::RemoteError { .. })
  ));
  host.shutdown().await.expect("shutdown");
}

#[tokio::test]
async fn rejects_manifest_with_unknown_protocol() {
  let bin = ensure_demo_plugin_built();
  let tmp = tempfile::tempdir().unwrap();
  let manifest_str = format!(
    r#"
[plugin]
name = "x"
version = "0.0.1"
runtime = "subprocess"
entrypoint = "{}"
protocol = "agentflow.plugin/999"
"#,
    bin.display()
  );
  let manifest_path = tmp.path().join("plugin.toml");
  std::fs::write(&manifest_path, manifest_str).unwrap();
  let result = PluginHost::load(&manifest_path).await;
  assert!(result.is_err(), "load with bad protocol must fail");
}

//! End-to-end demo of the AgentFlow plugin runtime.
//!
//! Builds the bundled `agentflow-echo-plugin` binary first via cargo
//! (the test harness does the same with `escargot`-style tricks; here we use
//! the well-known `target/debug/agentflow-echo-plugin` path produced by
//! `cargo build --bin agentflow-echo-plugin --features plugin`).
//!
//! Run with:
//!   cargo run -p agentflow-core --features plugin --example plugin_host_demo

use agentflow_core::async_node::AsyncNode;
use agentflow_core::plugin::{PluginHost, PluginRegistry};
use agentflow_core::value::FlowValue;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let entrypoint = locate_demo_plugin_binary()?;

  // Write a temporary plugin manifest that points to the freshly-built
  // demo binary.
  let tmpdir = tempfile::tempdir()?;
  let manifest_path = tmpdir.path().join("plugin.toml");
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
  std::fs::write(&manifest_path, manifest)?;

  println!("loading plugin manifest: {}", manifest_path.display());
  let host = PluginHost::load(&manifest_path).await?;
  println!(
    "plugin '{}' v{} declared node types: {:?}",
    host.initialize_result().plugin_name,
    host.initialize_result().plugin_version,
    host.declared_node_types()
  );

  let registry = PluginRegistry::new();
  registry.register(Arc::new(host)).await?;

  let node = registry.create_node("echo_uppercase", "echo1").await?;
  let mut inputs: HashMap<String, FlowValue> = HashMap::new();
  inputs.insert(
    "text".to_string(),
    FlowValue::Json(serde_json::Value::String("hello plugin".into())),
  );

  let outputs = node.execute(&inputs).await?;
  println!("node outputs: {:?}", outputs);

  let shutdown_results = registry.shutdown_all().await;
  for (name, result) in shutdown_results {
    match result {
      Ok(()) => println!("plugin '{name}' shut down cleanly"),
      Err(e) => eprintln!("plugin '{name}' shutdown error: {e}"),
    }
  }
  Ok(())
}

fn locate_demo_plugin_binary() -> Result<PathBuf, Box<dyn std::error::Error>> {
  // Cargo builds binaries into target/<profile>/<bin-name>. We don't know
  // the absolute target dir here (it could be overridden via
  // CARGO_TARGET_DIR), so we walk a few candidate locations.
  let exe_name = format!("agentflow-echo-plugin{}", std::env::consts::EXE_SUFFIX);

  let mut candidates: Vec<PathBuf> = Vec::new();
  if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
    candidates.push(PathBuf::from(&target_dir).join("debug").join(&exe_name));
    candidates.push(PathBuf::from(&target_dir).join("release").join(&exe_name));
  }
  // Walk up from this example's binary location.
  if let Ok(current_exe) = std::env::current_exe() {
    let mut walker: Option<&std::path::Path> = current_exe.parent();
    while let Some(dir) = walker {
      candidates.push(dir.join(&exe_name));
      let name = dir.file_name().and_then(|s| s.to_str());
      if matches!(name, Some("examples") | Some("debug") | Some("release"))
        && let Some(parent) = dir.parent()
      {
        candidates.push(parent.join(&exe_name));
      }
      walker = dir.parent();
    }
  }

  for candidate in &candidates {
    if candidate.exists() {
      return Ok(candidate.clone());
    }
  }

  Err(
    format!(
      "could not locate '{exe_name}' on disk. Build it first with:\n  \
     cargo build --features plugin --bin agentflow-echo-plugin\n\n\
     Searched: {candidates:?}"
    )
    .into(),
  )
}

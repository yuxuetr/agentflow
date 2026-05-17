//! `agentflow plugin generate-workflow-stub` — emit a YAML stub for a
//! plugin-declared workflow node.
//!
//! Reads a plugin's `plugin.toml`, iterates its `[[plugin.nodes]]` entries
//! (or a single one when `--node` is passed), and prints a `type: plugin`
//! YAML block per node with the canonical `manifest` + `node_type`
//! parameters filled in. Comments mark where the operator should add
//! workflow-specific input parameters.
//!
//! The stub intentionally embeds the absolute manifest path so the
//! emitted YAML works without further editing. Run with `--output` to
//! write to a file instead of stdout.

use std::fs;
use std::path::{Path, PathBuf};

use agentflow_core::plugin::PluginManifest;
use anyhow::{Context, Result, anyhow, bail};

pub async fn execute(plugin: String, node: Option<String>, output: Option<String>) -> Result<()> {
  let manifest_path = resolve_manifest_path(&plugin)?;
  let (manifest, _manifest_dir) =
    PluginManifest::load_from_path(&manifest_path).with_context(|| {
      format!(
        "failed to parse plugin manifest at '{}'",
        manifest_path.display()
      )
    })?;
  manifest.validate().with_context(|| {
    format!(
      "plugin manifest at '{}' failed validation",
      manifest_path.display()
    )
  })?;

  if manifest.plugin.nodes.is_empty() {
    bail!(
      "plugin '{}' at '{}' declares no `[[plugin.nodes]]` entries — nothing to stub",
      manifest.plugin.name,
      manifest_path.display()
    );
  }

  let selected: Vec<&_> = match node.as_deref() {
    Some(name) => {
      let hit = manifest
        .plugin
        .nodes
        .iter()
        .find(|spec| spec.node_type == name);
      match hit {
        Some(spec) => vec![spec],
        None => {
          let known: Vec<&str> = manifest
            .plugin
            .nodes
            .iter()
            .map(|spec| spec.node_type.as_str())
            .collect();
          return Err(anyhow!(
            "plugin '{}' has no node type '{}'. Known node types: {}",
            manifest.plugin.name,
            name,
            known.join(", ")
          ));
        }
      }
    }
    None => manifest.plugin.nodes.iter().collect(),
  };

  let stub = render_stub(&manifest.plugin.name, &manifest_path, &selected);

  if let Some(out_path) = output {
    fs::write(&out_path, &stub)
      .with_context(|| format!("failed to write stub to '{}'", out_path))?;
    println!("📄 Wrote workflow stub to {}", out_path);
  } else {
    print!("{}", stub);
  }
  Ok(())
}

/// Accept either a path to a plugin directory (containing `plugin.toml`)
/// or a direct path to `plugin.toml`. Mirrors `plugin inspect`.
fn resolve_manifest_path(plugin: &str) -> Result<PathBuf> {
  let path = PathBuf::from(plugin);
  if path.is_dir() {
    let candidate = path.join("plugin.toml");
    if !candidate.is_file() {
      bail!(
        "directory '{}' does not contain a plugin.toml manifest",
        path.display()
      );
    }
    return Ok(candidate);
  }
  if !path.is_file() {
    bail!(
      "plugin path '{}' is neither a directory nor a file",
      path.display()
    );
  }
  Ok(path)
}

fn render_stub(
  plugin_name: &str,
  manifest_path: &Path,
  nodes: &[&agentflow_core::plugin::manifest::NodeSpec],
) -> String {
  let mut out = String::new();
  out.push_str(&format!(
    "# Workflow stub generated for plugin '{plugin_name}'.\n"
  ));
  out.push_str("# Edit the `id` and add input parameters under each node as needed.\n");
  out.push_str(&format!("name: {plugin_name}-workflow\n"));
  out.push_str("nodes:\n");
  for spec in nodes {
    let suggested_id = sanitize_id(&spec.node_type);
    out.push_str(&format!("  - id: {suggested_id}\n"));
    out.push_str("    type: plugin\n");
    out.push_str("    parameters:\n");
    out.push_str(&format!(
      "      manifest: \"{}\"\n",
      manifest_path.display()
    ));
    out.push_str(&format!("      node_type: \"{}\"\n", spec.node_type));
    if !spec.description.trim().is_empty() {
      out.push_str(&format!("      # {}\n", spec.description.trim()));
    }
    out.push_str("      # Add workflow-specific input parameters below.\n");
    out.push_str("      # For example: text: \"...\"\n");
  }
  out
}

/// Strip characters that aren't safe in YAML node ids. The plugin's
/// declared `node_type` is usually already a snake_case identifier, but
/// punctuation is tolerated in the TOML — sanitize defensively.
fn sanitize_id(node_type: &str) -> String {
  let mut id = String::with_capacity(node_type.len());
  for ch in node_type.chars() {
    if ch.is_ascii_alphanumeric() || ch == '_' {
      id.push(ch);
    } else if ch == '-' || ch == '.' {
      id.push('_');
    }
  }
  if id.is_empty() {
    return "plugin_node".to_string();
  }
  format!("{id}_node")
}

#[cfg(test)]
mod tests {
  use super::*;
  use agentflow_core::plugin::manifest::NodeSpec;

  fn sample_node(node_type: &str, description: &str) -> NodeSpec {
    NodeSpec {
      node_type: node_type.to_string(),
      description: description.to_string(),
    }
  }

  #[test]
  fn render_stub_emits_one_block_per_node() {
    let nodes = [
      sample_node("upper", "uppercase"),
      sample_node("lower", "lowercase"),
    ];
    let refs: Vec<&NodeSpec> = nodes.iter().collect();
    let out = render_stub("demo", Path::new("/abs/plugin.toml"), &refs);
    assert_eq!(
      out.matches("- id:").count(),
      2,
      "two nodes ⇒ two stub blocks"
    );
    assert!(out.contains("node_type: \"upper\""));
    assert!(out.contains("node_type: \"lower\""));
    assert!(out.contains("manifest: \"/abs/plugin.toml\""));
  }

  #[test]
  fn render_stub_includes_description_comment() {
    let node = sample_node("hello", "Friendly greeting node");
    let refs = vec![&node];
    let out = render_stub("greeter", Path::new("/p.toml"), &refs);
    assert!(out.contains("# Friendly greeting node"));
  }

  #[test]
  fn render_stub_omits_description_when_empty() {
    let node = sample_node("bare", "");
    let refs = vec![&node];
    let out = render_stub("p", Path::new("/p.toml"), &refs);
    assert!(
      !out.contains("# \n"),
      "empty description must not emit a bare `# ` line"
    );
  }

  #[test]
  fn sanitize_id_replaces_punctuation_with_underscore() {
    assert_eq!(sanitize_id("a.b-c"), "a_b_c_node");
    assert_eq!(sanitize_id("simple"), "simple_node");
  }

  #[test]
  fn sanitize_id_falls_back_on_empty_input() {
    assert_eq!(sanitize_id("!!!"), "plugin_node");
  }
}

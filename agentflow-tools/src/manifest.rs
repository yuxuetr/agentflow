//! W4.1d — a serializable tool manifest so a distributed `agent` DAG node
//! can declare which built-in tools its worker should build, instead of
//! `agentflow-worker::execute_agent_payload` always constructing an
//! empty `ToolRegistry`. First cut scoped to `File` (read-only) + `Http`
//! only — see `docs/RFC_TOOL_DISTRIBUTION.md` for the full design and why
//! Shell/Script/`code_exec` are deferred (they need a worker-side
//! approval RPC that doesn't exist yet).
//!
//! The manifest rides inside a DAG node's existing `parameters` JSON bag
//! (`agentflow-worker/src/lib.rs::execute_file_payload`'s `allowed_paths`
//! field is the precedent for shaping tool behavior this way) — no
//! `worker.proto` change is needed for this scope.

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::builtin::{FileTool, HttpTool};
use crate::sandbox::SandboxPolicy;
use crate::{Capability, ToolDefinition, ToolError, ToolRegistry};

/// Which built-in tool a [`ToolManifestEntry`] resolves to. Closed on
/// purpose this pass — extending it (Shell/Script/`code_exec`) is an
/// additive, non-breaking change once a worker-side approval RPC exists
/// to gate those effect-producing tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinToolKind {
  File,
  Http,
}

/// One tool a manifest grants. `definition` and `required` are
/// descriptive metadata — audit/introspection surfaces (e.g. a future
/// `agentflow doctor`/worker admission report) can render "what this
/// node was granted and why" — `build_registry_from_manifest` does not
/// read them to construct the tool; `kind` + `sandbox` alone determine
/// that. `File` is always built read-only this pass, matching
/// `default_governed_registry`'s posture: a distributed worker has no
/// approval RPC yet (deferred, see the module doc), so a write-capable
/// file tool would run ungated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolManifestEntry {
  pub kind: BuiltinToolKind,
  pub definition: ToolDefinition,
  #[serde(default)]
  pub required: Vec<Capability>,
  #[serde(default)]
  pub sandbox: Option<SandboxPolicy>,
}

/// A worker's full tool grant for one `agent` DAG node.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolManifest {
  #[serde(default)]
  pub tools: Vec<ToolManifestEntry>,
}

/// Build a [`ToolRegistry`] from a manifest. Each entry's `sandbox`
/// policy governs that entry's tool; an absent `sandbox` falls back to
/// [`SandboxPolicy::default`] scoped to `workspace_root` (mirrors
/// `default_governed_registry`'s own default, not a wide-open policy).
pub fn build_registry_from_manifest(
  manifest: &ToolManifest,
  workspace_root: &Path,
) -> Result<ToolRegistry, ToolError> {
  let mut registry = ToolRegistry::new();
  for entry in &manifest.tools {
    let policy = Arc::new(entry.sandbox.clone().unwrap_or_else(|| SandboxPolicy {
      allowed_paths: vec![workspace_root.to_path_buf()],
      ..SandboxPolicy::default()
    }));
    match entry.kind {
      BuiltinToolKind::File => {
        registry.register(Arc::new(FileTool::read_only(policy)));
      }
      BuiltinToolKind::Http => {
        registry.register(Arc::new(HttpTool::new(policy)?));
      }
    }
  }
  Ok(registry)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn sample_definition(name: &str) -> ToolDefinition {
    ToolDefinition {
      name: name.to_string(),
      description: format!("{name} tool"),
      parameters: serde_json::json!({}),
      metadata: Default::default(),
    }
  }

  #[test]
  fn build_registry_from_manifest_registers_file_and_http() {
    let manifest = ToolManifest {
      tools: vec![
        ToolManifestEntry {
          kind: BuiltinToolKind::File,
          definition: sample_definition("file"),
          required: vec![],
          sandbox: None,
        },
        ToolManifestEntry {
          kind: BuiltinToolKind::Http,
          definition: sample_definition("http"),
          required: vec![],
          sandbox: None,
        },
      ],
    };

    let registry = build_registry_from_manifest(&manifest, Path::new("/tmp/workspace")).unwrap();
    let names: Vec<String> = registry
      .list()
      .iter()
      .map(|tool| tool.name().to_string())
      .collect();
    assert!(names.contains(&"file".to_string()));
    assert!(names.contains(&"http".to_string()));
    assert_eq!(names.len(), 2);
  }

  #[test]
  fn build_registry_from_manifest_with_no_entries_yields_empty_registry() {
    let manifest = ToolManifest { tools: vec![] };
    let registry = build_registry_from_manifest(&manifest, Path::new("/tmp/workspace")).unwrap();
    assert!(registry.list().is_empty());
  }

  #[tokio::test]
  async fn build_registry_from_manifest_honors_entry_sandbox_over_default() {
    let temp = tempfile::TempDir::new().unwrap();
    let allowed = temp.path().to_path_buf();
    let manifest = ToolManifest {
      tools: vec![ToolManifestEntry {
        kind: BuiltinToolKind::File,
        definition: sample_definition("file"),
        required: vec![],
        sandbox: Some(SandboxPolicy {
          allowed_paths: vec![allowed.clone()],
          ..SandboxPolicy::default()
        }),
      }],
    };

    // A different workspace_root must not override the entry's own
    // explicit sandbox policy.
    let registry =
      build_registry_from_manifest(&manifest, Path::new("/some/other/workspace")).unwrap();
    let tool = registry.get("file").expect("file tool registered");
    let outside = Path::new("/definitely/not/allowed");
    let result = tool
      .execute(serde_json::json!({
        "operation": "read",
        "path": outside,
      }))
      .await;
    assert!(
      result.is_err(),
      "path outside the entry's own sandbox policy must be denied"
    );
  }

  /// W4.1d: manifest DTOs must round-trip through JSON, since they're
  /// meant to ride inside `NodeExecutionPayload.parameters`.
  #[test]
  fn tool_manifest_round_trips_through_json() {
    let manifest = ToolManifest {
      tools: vec![ToolManifestEntry {
        kind: BuiltinToolKind::Http,
        definition: sample_definition("http"),
        required: vec![Capability::Net],
        sandbox: Some(SandboxPolicy::default()),
      }],
    };
    let json = serde_json::to_string(&manifest).unwrap();
    let round_tripped: ToolManifest = serde_json::from_str(&json).unwrap();
    assert_eq!(round_tripped.tools.len(), 1);
    assert_eq!(round_tripped.tools[0].kind, BuiltinToolKind::Http);
    assert_eq!(round_tripped.tools[0].required, vec![Capability::Net]);
  }
}

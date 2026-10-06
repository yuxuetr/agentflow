use std::path::Path;
use std::sync::Arc;

use crate::builtin::{FileTool, HttpTool};
use crate::sandbox::SandboxPolicy;
use crate::{ToolError, ToolRegistry};

/// A conservative, safe-by-default tool registry: read-only filesystem
/// access scoped to `workspace_root`, plus outbound HTTP.
///
/// Callers that need *some* governed tools without granting write access
/// or requiring a skill manifest (server harness sessions, CLI
/// `harness run`/`chat` invoked with `--model` instead of `--skill`) use
/// this instead of an always-empty `ToolRegistry::new()`, which leaves the
/// approval/hook pipeline with nothing to ever govern.
pub fn default_governed_registry(workspace_root: &Path) -> Result<ToolRegistry, ToolError> {
  let policy = Arc::new(SandboxPolicy {
    allowed_paths: vec![workspace_root.to_path_buf()],
    ..SandboxPolicy::default()
  });
  let mut registry = ToolRegistry::new();
  registry.register(Arc::new(FileTool::read_only(policy.clone())));
  registry.register(Arc::new(HttpTool::new(policy)?));
  Ok(registry)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_governed_registry_has_read_only_file_and_http() {
    let registry = default_governed_registry(Path::new("/tmp/workspace")).unwrap();
    let names: Vec<String> = registry
      .list()
      .iter()
      .map(|tool| tool.name().to_string())
      .collect();
    assert!(names.contains(&"file".to_string()));
    assert!(names.contains(&"http".to_string()));
    assert_eq!(names.len(), 2);
  }
}

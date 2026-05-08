//! `PluginWorkflowNode` — adapter that exposes a subprocess plugin to a YAML
//! workflow as `type: plugin`.
//!
//! The wrapper holds the manifest path + plugin-declared node type. On first
//! `execute` it lazily spawns the plugin via [`PluginHost::load`] and caches
//! the resulting [`Arc<PluginHost>`] in a process-wide table keyed by the
//! canonicalized manifest path. Subsequent workflow nodes pointing at the
//! same manifest reuse the same subprocess, which keeps initialize cost paid
//! once per `agentflow workflow run` invocation.
//!
//! When the `AGENTFLOW_PLUGIN_SANDBOX=1` environment variable is set, the
//! cached host is constructed via [`PluginHost::builder`] with a
//! [`OsSandboxPluginPreparer`]. The preparer translates the plugin's
//! `[plugin.capabilities]` block into the existing
//! `agentflow_tools::sandbox` primitives (capability set + scope) and lets
//! the platform backend wrap the spawn command (macOS `sandbox-exec`,
//! Linux seccomp). This is the bridge that fulfils the `[plugin.capabilities]
//! → SandboxPolicy` wiring noted in `docs/PLUGIN_DESIGN.md` §6.5.
//!
//! See `docs/PLUGIN_DESIGN.md` §6 for the wire protocol and §6.4 for the
//! workflow integration covered here.

use agentflow_core::plugin::{
  Capabilities, CommandPreparer, FsAccess, NoopCommandPreparer, PluginError, PluginHost,
  PluginManifest,
};
use agentflow_core::{
  async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
  error::AgentFlowError,
};
use agentflow_tools::capability::Capability;
use agentflow_tools::sandbox::{SandboxBackend, SandboxScope, default_backend};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tokio::process::Command;
use tokio::sync::Mutex;

/// Process-wide cache of loaded plugin hosts, keyed by canonicalized manifest
/// path. Multiple workflow nodes pointing at the same `plugin.toml` share a
/// single subprocess so we pay the spawn + handshake cost exactly once.
fn host_cache() -> &'static Mutex<HashMap<PathBuf, Arc<PluginHost>>> {
  static CELL: OnceLock<Mutex<HashMap<PathBuf, Arc<PluginHost>>>> = OnceLock::new();
  CELL.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Environment variable that opts the plugin host into OS-level sandbox
/// enforcement. Any value other than the empty string or `0` enables it.
const PLUGIN_SANDBOX_ENV: &str = "AGENTFLOW_PLUGIN_SANDBOX";

#[derive(Debug, Clone)]
pub struct PluginWorkflowNode {
  pub workflow_node_id: String,
  pub manifest_path: PathBuf,
  pub plugin_node_type: String,
}

impl PluginWorkflowNode {
  pub fn new(
    workflow_node_id: impl Into<String>,
    manifest_path: PathBuf,
    plugin_node_type: impl Into<String>,
  ) -> Self {
    Self {
      workflow_node_id: workflow_node_id.into(),
      manifest_path,
      plugin_node_type: plugin_node_type.into(),
    }
  }

  async fn ensure_loaded(&self) -> Result<Arc<PluginHost>, AgentFlowError> {
    let canonical =
      self
        .manifest_path
        .canonicalize()
        .map_err(|err| AgentFlowError::NodeInputError {
          message: format!(
            "plugin '{}': manifest path '{}' not accessible: {}",
            self.workflow_node_id,
            self.manifest_path.display(),
            err
          ),
        })?;
    let mut cache = host_cache().lock().await;
    if let Some(existing) = cache.get(&canonical) {
      return Ok(existing.clone());
    }
    let preparer = preparer_from_env();
    let host = PluginHost::builder()
      .with_command_preparer(preparer)
      .load(&canonical)
      .await
      .map_err(|err| AgentFlowError::AsyncExecutionError {
        message: format!(
          "plugin '{}': failed to load manifest '{}': {}",
          self.workflow_node_id,
          canonical.display(),
          err
        ),
      })?;
    let arc = Arc::new(host);
    cache.insert(canonical, arc.clone());
    Ok(arc)
  }
}

#[async_trait]
impl AsyncNode for PluginWorkflowNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let host = self.ensure_loaded().await?;
    let result = host
      .execute_node(&self.plugin_node_type, inputs.clone())
      .await
      .map_err(AgentFlowError::from)?;
    Ok(result.outputs)
  }
}

/// Pick a [`CommandPreparer`] based on the `AGENTFLOW_PLUGIN_SANDBOX`
/// environment variable. Defaults to a no-op preparer to preserve backward
/// compatibility with v0.3 behaviour.
fn preparer_from_env() -> Arc<dyn CommandPreparer> {
  match std::env::var(PLUGIN_SANDBOX_ENV) {
    Ok(value) if !matches!(value.as_str(), "" | "0") => {
      Arc::new(OsSandboxPluginPreparer::new(default_backend()))
    }
    _ => Arc::new(NoopCommandPreparer),
  }
}

/// Bridge from `agentflow-core::plugin::CommandPreparer` to the platform
/// sandbox backend in `agentflow-tools`.
///
/// Translates the plugin manifest's `[plugin.capabilities]` block into:
///
/// * a capability set ([`agentflow_tools::Capability`]) — `FsRead` /
///   `FsWrite` / `Net` / `Exec` / `Env` are granted only when the
///   manifest declares the corresponding section;
/// * a [`SandboxScope`] — paths from `filesystem` entries become
///   `read_paths` / `write_paths`. The plugin's manifest directory is
///   pre-allowed for reading so the binary itself, plus any local
///   resources next to `plugin.toml`, are reachable. Relative paths in
///   the manifest are resolved against the manifest directory.
///
/// The translation is intentionally permissive in two places to match
/// `agentflow_tools::builtin::shell::build_scope_from_policy`:
///
/// * a manifest with no `filesystem` entries gets `/tmp` + the plugin's
///   manifest directory as its read scope (the binary needs to be readable
///   to be executable);
/// * `env_vars` is recorded as a granted capability (`Capability::Env`) but
///   not enforced at the OS sandbox layer — neither macOS sandbox-exec nor
///   Linux seccomp can scrub the env passed to `execve`. The CLI inherits
///   the caller's env unchanged.
pub struct OsSandboxPluginPreparer {
  backend: Arc<dyn SandboxBackend>,
}

impl std::fmt::Debug for OsSandboxPluginPreparer {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("OsSandboxPluginPreparer")
      .field("backend", &self.backend.name())
      .finish()
  }
}

impl OsSandboxPluginPreparer {
  pub fn new(backend: Arc<dyn SandboxBackend>) -> Self {
    Self { backend }
  }
}

impl CommandPreparer for OsSandboxPluginPreparer {
  fn name(&self) -> &str {
    self.backend.name()
  }

  fn prepare(
    &self,
    command: &mut Command,
    manifest: &PluginManifest,
    manifest_dir: &Path,
  ) -> Result<(), PluginError> {
    let caps = capabilities_for_manifest(&manifest.plugin.capabilities);
    let scope = scope_for_manifest(&manifest.plugin.capabilities, manifest_dir).map_err(|err| {
      PluginError::PreparerRejected {
        plugin: manifest.plugin.name.clone(),
        reason: err,
      }
    })?;
    self
      .backend
      .wrap_command(command, &caps, &scope)
      .map_err(|err| PluginError::PreparerRejected {
        plugin: manifest.plugin.name.clone(),
        reason: format!("OS sandbox backend '{}' failed: {err}", self.backend.name()),
      })
  }
}

/// Decompose `[plugin.capabilities]` into the [`Capability`] set the
/// platform backend should enforce.
pub(crate) fn capabilities_for_manifest(caps: &Capabilities) -> Vec<Capability> {
  let mut out = Vec::new();
  if caps.requires_fs_read() {
    out.push(Capability::FsRead);
  }
  if caps.requires_fs_write() {
    out.push(Capability::FsWrite);
  }
  if caps.requires_net() {
    out.push(Capability::Net);
  }
  if caps.requires_exec() {
    out.push(Capability::Exec);
  }
  if caps.requires_env() {
    out.push(Capability::Env);
  }
  out
}

/// Project `[plugin.capabilities].filesystem` into the read/write scope
/// the platform backend will materialise. Relative paths in the manifest
/// are resolved against the manifest directory; the manifest directory
/// itself is always readable so the plugin binary can be exec'd.
pub(crate) fn scope_for_manifest(
  caps: &Capabilities,
  manifest_dir: &Path,
) -> Result<SandboxScope, String> {
  let entries = caps
    .filesystem_entries()
    .map_err(|err| format!("invalid filesystem capability: {err}"))?;

  let mut scope = SandboxScope::new();
  // The plugin executable must be readable. Pre-allow the manifest dir.
  scope.read_paths.push(manifest_dir.to_path_buf());
  // Match shell::build_scope_from_policy's permissive default so plugins
  // that don't declare anything still have a working /tmp.
  if entries.is_empty() {
    scope.read_paths.push(PathBuf::from("/tmp"));
  }

  for entry in entries {
    let resolved = if entry.path.is_absolute() {
      entry.path.clone()
    } else {
      manifest_dir.join(&entry.path)
    };
    match entry.access {
      FsAccess::Read => scope.read_paths.push(resolved),
      FsAccess::Write => {
        // A writable path must also be readable.
        scope.read_paths.push(resolved.clone());
        scope.write_paths.push(resolved);
      }
    }
  }

  scope.working_directory = Some(manifest_dir.to_path_buf());
  Ok(scope)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn full_capabilities() -> Capabilities {
    Capabilities {
      filesystem: vec!["read:./inputs".into(), "write:/tmp/out".into()],
      network: vec!["api.example.com".into()],
      processes: vec!["sh".into()],
      env_vars: vec!["DEMO".into()],
    }
  }

  #[test]
  fn capability_set_reflects_manifest_grants() {
    let resolved = capabilities_for_manifest(&full_capabilities());
    assert!(resolved.contains(&Capability::FsRead));
    assert!(resolved.contains(&Capability::FsWrite));
    assert!(resolved.contains(&Capability::Net));
    assert!(resolved.contains(&Capability::Exec));
    assert!(resolved.contains(&Capability::Env));
  }

  #[test]
  fn empty_capabilities_yield_no_grants() {
    let caps = Capabilities::default();
    assert!(capabilities_for_manifest(&caps).is_empty());
  }

  #[test]
  fn scope_resolves_relative_paths_against_manifest_dir() {
    let caps = Capabilities {
      filesystem: vec!["read:./data".into(), "write:/tmp/out".into()],
      ..Capabilities::default()
    };
    let manifest_dir = Path::new("/var/agentflow/plugins/demo");
    let scope = scope_for_manifest(&caps, manifest_dir).unwrap();

    // Manifest dir is always read-allowed (so the binary is reachable).
    assert!(scope.read_paths.iter().any(|p| p == manifest_dir));
    // Relative read path resolves against manifest dir.
    assert!(
      scope
        .read_paths
        .iter()
        .any(|p| p == &manifest_dir.join("data"))
    );
    // Absolute write path stays absolute and is also read-allowed.
    let tmp_out = PathBuf::from("/tmp/out");
    assert!(scope.write_paths.contains(&tmp_out));
    assert!(scope.read_paths.contains(&tmp_out));
    assert_eq!(scope.working_directory.as_deref(), Some(manifest_dir));
  }

  #[test]
  fn empty_filesystem_block_falls_back_to_tmp() {
    let caps = Capabilities::default();
    let manifest_dir = Path::new("/opt/plugins/demo");
    let scope = scope_for_manifest(&caps, manifest_dir).unwrap();

    assert!(scope.read_paths.iter().any(|p| p == Path::new("/tmp")));
    assert!(scope.write_paths.is_empty());
    assert!(scope.read_paths.iter().any(|p| p == manifest_dir));
  }

  #[test]
  fn invalid_filesystem_entry_surfaces_as_string_reason() {
    let caps = Capabilities {
      filesystem: vec!["read:".into()],
      ..Capabilities::default()
    };
    let err = scope_for_manifest(&caps, Path::new("/tmp")).unwrap_err();
    assert!(err.contains("invalid filesystem capability"));
  }

  #[test]
  fn preparer_from_env_picks_noop_when_unset() {
    // Save and clear so the test is deterministic.
    let prev = std::env::var(PLUGIN_SANDBOX_ENV).ok();
    // SAFETY: tests in this crate are not run in parallel against
    // the same env var; the surrounding restore covers cleanup.
    unsafe {
      std::env::remove_var(PLUGIN_SANDBOX_ENV);
    }
    let preparer = preparer_from_env();
    assert_eq!(preparer.name(), "noop-plugin");
    if let Some(value) = prev {
      unsafe {
        std::env::set_var(PLUGIN_SANDBOX_ENV, value);
      }
    }
  }
}

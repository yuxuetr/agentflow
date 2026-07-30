//! Runtime capability model.
//!
//! [`Capability`] is more granular than [`crate::ToolPermission`]. While
//! `ToolPermission` is a declarative classification used for inspection and
//! soft policy, `Capability` is designed to map directly onto OS-level
//! sandbox primitives (`sandbox-exec` on macOS, seccomp on Linux). Tools
//! report what they need via [`crate::Tool::requires_capabilities`]; the
//! three-way merge in [`EffectiveCapabilities::resolve`] computes which of
//! those requirements survive the skill / policy / CLI layers.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::ToolPermission;
use crate::sandbox::SandboxStatus;

/// A single OS-mappable capability requested by a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
  /// Read from the local filesystem.
  FsRead,
  /// Write or modify the local filesystem.
  FsWrite,
  /// Make outbound network connections.
  Net,
  /// Execute child processes.
  Exec,
  /// Read environment variables (anything beyond the constant inherited set).
  Env,
}

impl Capability {
  /// Stable string token used in trace events and CLI output.
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::FsRead => "fs.read",
      Self::FsWrite => "fs.write",
      Self::Net => "net",
      Self::Exec => "exec",
      Self::Env => "env",
    }
  }

  /// Decompose a [`ToolPermission`] into the OS-level capabilities it implies.
  ///
  /// `ToolPermission::Workflow` returns an empty vec because workflow tools
  /// recursively inherit capabilities from their inner tools — they have no
  /// direct OS capability of their own.
  pub fn from_permission(permission: &ToolPermission) -> Vec<Capability> {
    match permission {
      ToolPermission::FilesystemRead => vec![Self::FsRead],
      ToolPermission::FilesystemWrite => vec![Self::FsWrite],
      ToolPermission::ProcessExec => vec![Self::Exec],
      ToolPermission::Network => vec![Self::Net],
      // Stdio MCP servers run a subprocess and talk JSON-RPC over its pipes;
      // some transports also open a network socket.
      ToolPermission::Mcp => vec![Self::Net, Self::Exec],
      ToolPermission::Workflow => vec![],
    }
  }

  /// Decompose a slice of [`ToolPermission`] into a deduplicated capability list.
  pub fn from_permissions(permissions: &[ToolPermission]) -> Vec<Capability> {
    let mut set: BTreeSet<Capability> = BTreeSet::new();
    for permission in permissions {
      set.extend(Capability::from_permission(permission));
    }
    set.into_iter().collect()
  }
}

/// Source of one layer in the capability merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantSource {
  /// Capabilities the tool itself declares as required.
  ToolRequired,
  /// Skill-level allow-list (typically derived from `SecurityConfig`).
  SkillSecurity,
  /// In-process [`crate::ToolPolicy`] permission filter.
  ToolPolicy,
  /// CLI-level override.
  CliFlag,
}

impl GrantSource {
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::ToolRequired => "tool_required",
      Self::SkillSecurity => "skill_security",
      Self::ToolPolicy => "tool_policy",
      Self::CliFlag => "cli_flag",
    }
  }
}

/// One step in the merge. Records which layer was applied, what it allowed,
/// what it dropped, and the running effective set after the layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityDecisionEntry {
  pub source: GrantSource,
  /// `None` means the layer was permissive (no constraint).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub allowed: Option<Vec<Capability>>,
  /// Capabilities required-but-not-allowed by this specific layer.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub dropped: Vec<Capability>,
  /// Effective set after applying this layer.
  pub running: Vec<Capability>,
}

/// Outcome of resolving the capability merge for a single tool invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveCapabilities {
  pub tool: String,
  pub required: Vec<Capability>,
  pub effective: Vec<Capability>,
  pub denied: Vec<Capability>,
  pub allowed: bool,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub deny_reason: Option<String>,
  pub trace: Vec<CapabilityDecisionEntry>,
  /// Active OS sandbox status for tools that wrap a child process.
  ///
  /// `None` for tools that run entirely in-process (HTTP, file, MCP).
  /// Tools that spawn subprocesses (shell, script, plugin) populate this
  /// so operators can see the backend name and enforcement state in trace
  /// events and doctor diagnostics.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub sandbox: Option<SandboxStatus>,
}

impl EffectiveCapabilities {
  /// Resolve the three-way merge for a tool invocation.
  ///
  /// The merge is an intersection: each layer (when set) restricts the
  /// running set. A `None` layer is permissive (passes everything through).
  /// Layers are applied in this order so that the trace records ownership:
  ///
  /// 1. Tool requires
  /// 2. Skill security
  /// 3. Tool policy
  /// 4. CLI flag
  ///
  /// The tool is allowed iff every required capability survives all layers.
  pub fn resolve(
    tool_name: impl Into<String>,
    required: &[Capability],
    skill_grant: Option<&[Capability]>,
    policy_grant: Option<&[Capability]>,
    cli_grant: Option<&[Capability]>,
  ) -> Self {
    let tool = tool_name.into();
    let required_set: BTreeSet<Capability> = required.iter().copied().collect();
    let mut running: BTreeSet<Capability> = required_set.clone();

    let mut trace = Vec::with_capacity(4);
    trace.push(CapabilityDecisionEntry {
      source: GrantSource::ToolRequired,
      allowed: Some(required_set.iter().copied().collect()),
      dropped: Vec::new(),
      running: running.iter().copied().collect(),
    });

    apply_layer(
      &mut trace,
      &mut running,
      GrantSource::SkillSecurity,
      skill_grant,
    );
    apply_layer(
      &mut trace,
      &mut running,
      GrantSource::ToolPolicy,
      policy_grant,
    );
    apply_layer(&mut trace, &mut running, GrantSource::CliFlag, cli_grant);

    let effective: Vec<Capability> = running.iter().copied().collect();
    let denied: Vec<Capability> = required_set.difference(&running).copied().collect();
    let allowed = denied.is_empty();
    let deny_reason = if allowed {
      None
    } else {
      Some(format!(
        "tool '{}' was denied capabilities: {}",
        tool,
        denied
          .iter()
          .map(Capability::as_str)
          .collect::<Vec<_>>()
          .join(", "),
      ))
    };

    Self {
      tool,
      required: required_set.into_iter().collect(),
      effective,
      denied,
      allowed,
      deny_reason,
      trace,
      sandbox: None,
    }
  }

  /// Attach a sandbox status snapshot to the decision. Tools that wrap a
  /// child process (shell, script, plugin) call this so the active backend
  /// is observable in trace events and doctor output.
  pub fn with_sandbox(mut self, status: SandboxStatus) -> Self {
    self.sandbox = Some(status);
    self
  }
}

fn apply_layer(
  trace: &mut Vec<CapabilityDecisionEntry>,
  running: &mut BTreeSet<Capability>,
  source: GrantSource,
  layer: Option<&[Capability]>,
) {
  match layer {
    None => {
      trace.push(CapabilityDecisionEntry {
        source,
        allowed: None,
        dropped: Vec::new(),
        running: running.iter().copied().collect(),
      });
    }
    Some(allowed_slice) => {
      let allowed_set: BTreeSet<Capability> = allowed_slice.iter().copied().collect();
      let dropped: Vec<Capability> = running.difference(&allowed_set).copied().collect();
      running.retain(|cap| allowed_set.contains(cap));
      trace.push(CapabilityDecisionEntry {
        source,
        allowed: Some(allowed_set.into_iter().collect()),
        dropped,
        running: running.iter().copied().collect(),
      });
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn permission_decomposes_into_capabilities() {
    assert_eq!(
      Capability::from_permission(&ToolPermission::FilesystemRead),
      vec![Capability::FsRead]
    );
    assert_eq!(
      Capability::from_permission(&ToolPermission::FilesystemWrite),
      vec![Capability::FsWrite]
    );
    assert_eq!(
      Capability::from_permission(&ToolPermission::ProcessExec),
      vec![Capability::Exec]
    );
    assert_eq!(
      Capability::from_permission(&ToolPermission::Network),
      vec![Capability::Net]
    );
    assert_eq!(
      Capability::from_permission(&ToolPermission::Mcp),
      vec![Capability::Net, Capability::Exec]
    );
    assert!(Capability::from_permission(&ToolPermission::Workflow).is_empty());
  }

  #[test]
  fn from_permissions_dedupes_and_sorts() {
    let caps = Capability::from_permissions(&[
      ToolPermission::Mcp,
      ToolPermission::Network,
      ToolPermission::ProcessExec,
    ]);
    assert_eq!(caps, vec![Capability::Net, Capability::Exec]);
  }

  #[test]
  fn permissive_layers_grant_full_required_set() {
    let result = EffectiveCapabilities::resolve("shell", &[Capability::Exec], None, None, None);
    assert!(result.allowed);
    assert_eq!(result.effective, vec![Capability::Exec]);
    assert!(result.denied.is_empty());
    assert_eq!(result.trace.len(), 4);
    assert_eq!(result.trace[0].source, GrantSource::ToolRequired);
    assert!(result.trace[1].allowed.is_none());
    assert!(result.trace[2].allowed.is_none());
    assert!(result.trace[3].allowed.is_none());
  }

  #[test]
  fn skill_layer_drops_unallowed_capability() {
    let result = EffectiveCapabilities::resolve(
      "http",
      &[Capability::Net],
      Some(&[Capability::FsRead]),
      None,
      None,
    );
    assert!(!result.allowed);
    assert_eq!(result.denied, vec![Capability::Net]);
    assert!(result.effective.is_empty());
    let skill_entry = &result.trace[1];
    assert_eq!(skill_entry.source, GrantSource::SkillSecurity);
    assert_eq!(skill_entry.dropped, vec![Capability::Net]);
  }

  #[test]
  fn policy_layer_further_restricts_after_skill() {
    let result = EffectiveCapabilities::resolve(
      "shell",
      &[Capability::Exec, Capability::Net],
      Some(&[Capability::Exec, Capability::Net]),
      Some(&[Capability::Exec]),
      None,
    );
    assert!(!result.allowed);
    assert_eq!(result.effective, vec![Capability::Exec]);
    assert_eq!(result.denied, vec![Capability::Net]);
    let policy_entry = &result.trace[2];
    assert_eq!(policy_entry.source, GrantSource::ToolPolicy);
    assert_eq!(policy_entry.dropped, vec![Capability::Net]);
  }

  #[test]
  fn cli_layer_can_force_deny() {
    let result = EffectiveCapabilities::resolve(
      "shell",
      &[Capability::Exec],
      None,
      None,
      Some(&[Capability::Net]),
    );
    assert!(!result.allowed);
    assert_eq!(result.denied, vec![Capability::Exec]);
    let cli_entry = &result.trace[3];
    assert_eq!(cli_entry.source, GrantSource::CliFlag);
    assert_eq!(cli_entry.dropped, vec![Capability::Exec]);
  }

  #[test]
  fn cli_cannot_grant_capability_blocked_by_earlier_layer() {
    // Skill blocks Net; CLI claims to allow Net. Intersection model means
    // CLI cannot resurrect a capability dropped earlier.
    let result = EffectiveCapabilities::resolve(
      "http",
      &[Capability::Net],
      Some(&[Capability::FsRead]),
      None,
      Some(&[Capability::Net, Capability::FsRead]),
    );
    assert!(!result.allowed);
    assert_eq!(result.denied, vec![Capability::Net]);
    assert_eq!(result.trace[3].running, Vec::<Capability>::new());
  }

  #[test]
  fn tool_with_no_required_caps_is_always_allowed() {
    let result = EffectiveCapabilities::resolve(
      "noop",
      &[],
      Some(&[]), // even an empty skill grant
      Some(&[]),
      Some(&[]),
    );
    assert!(result.allowed);
    assert!(result.effective.is_empty());
    assert!(result.denied.is_empty());
  }

  #[test]
  fn deny_reason_lists_each_missing_capability() {
    let result = EffectiveCapabilities::resolve(
      "shell",
      &[Capability::Exec, Capability::Net],
      Some(&[]), // deny everything at the skill layer
      None,
      None,
    );
    assert!(!result.allowed);
    let reason = result.deny_reason.expect("deny_reason");
    assert!(reason.contains("exec"));
    assert!(reason.contains("net"));
  }

  #[test]
  fn effective_capabilities_round_trips_through_serde() {
    let original = EffectiveCapabilities::resolve(
      "shell",
      &[Capability::Exec],
      Some(&[Capability::Exec]),
      None,
      None,
    );
    let value = serde_json::to_value(&original).unwrap();
    assert_eq!(value["allowed"], serde_json::json!(true));
    assert_eq!(
      value["trace"][0]["source"],
      serde_json::json!("tool_required")
    );
    let decoded: EffectiveCapabilities = serde_json::from_value(value).unwrap();
    assert_eq!(decoded, original);
  }
}

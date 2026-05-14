use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::capability::{Capability, EffectiveCapabilities};
use crate::{
  Tool, ToolError, ToolIdempotency, ToolMetadata, ToolOutput, ToolPermission, ToolPolicy,
  ToolPolicyDecision,
};

/// Central registry for all available tools.
///
/// Register tools once at startup; an agent runtime (e.g. the ReAct agent
/// in `agentflow-agents`) uses the registry to look up and dispatch tool
/// calls from LLM responses.
pub struct ToolRegistry {
  tools: HashMap<String, Arc<dyn Tool>>,
  policy: ToolPolicy,
  policy_audit: Arc<Mutex<Vec<ToolPolicyDecision>>>,
  capability_audit: Arc<Mutex<Vec<EffectiveCapabilities>>>,
  skill_capabilities: Option<Vec<Capability>>,
  cli_capabilities: Option<Vec<Capability>>,
}

impl ToolRegistry {
  pub fn new() -> Self {
    Self {
      tools: HashMap::new(),
      policy: ToolPolicy::allow_all(),
      policy_audit: Arc::new(Mutex::new(Vec::new())),
      capability_audit: Arc::new(Mutex::new(Vec::new())),
      skill_capabilities: None,
      cli_capabilities: None,
    }
  }

  pub fn with_policy(mut self, policy: ToolPolicy) -> Self {
    self.policy = policy;
    self
  }

  /// Restrict the registry to the capabilities granted by the owning skill.
  ///
  /// Pass `None` to clear (the default), making the skill layer permissive.
  pub fn with_skill_capabilities(
    mut self,
    capabilities: impl IntoIterator<Item = Capability>,
  ) -> Self {
    let mut caps: Vec<Capability> = capabilities.into_iter().collect();
    caps.sort();
    caps.dedup();
    self.skill_capabilities = Some(caps);
    self
  }

  /// Apply a CLI-level capability override on top of skill + policy layers.
  ///
  /// CLI overrides cannot grant capabilities that earlier layers denied;
  /// the merge is an intersection.
  pub fn with_cli_capabilities(
    mut self,
    capabilities: impl IntoIterator<Item = Capability>,
  ) -> Self {
    let mut caps: Vec<Capability> = capabilities.into_iter().collect();
    caps.sort();
    caps.dedup();
    self.cli_capabilities = Some(caps);
    self
  }

  pub fn skill_capabilities(&self) -> Option<&[Capability]> {
    self.skill_capabilities.as_deref()
  }

  pub fn cli_capabilities(&self) -> Option<&[Capability]> {
    self.cli_capabilities.as_deref()
  }

  /// Register a tool.  A previously registered tool with the same name
  /// is silently replaced.
  pub fn register(&mut self, tool: Arc<dyn Tool>) {
    self.tools.insert(tool.name().to_string(), tool);
  }

  /// Look up a tool by name.
  pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
    self.tools.get(name).cloned()
  }

  /// List all registered tools.
  pub fn list(&self) -> Vec<Arc<dyn Tool>> {
    self.tools.values().cloned().collect()
  }

  /// List tools whose metadata includes a specific permission.
  pub fn list_by_permission(&self, permission: ToolPermission) -> Vec<Arc<dyn Tool>> {
    self
      .tools
      .values()
      .filter(|tool| tool.metadata().permissions.allows(&permission))
      .cloned()
      .collect()
  }

  /// Check whether a registered tool declares a permission.
  pub fn tool_has_permission(&self, name: &str, permission: &ToolPermission) -> bool {
    self
      .tools
      .get(name)
      .map(|tool| tool.metadata().permissions.allows(permission))
      .unwrap_or(false)
  }

  /// Return metadata for a registered tool.
  pub fn tool_metadata(&self, name: &str) -> Option<ToolMetadata> {
    self.tools.get(name).map(|tool| tool.metadata())
  }

  /// Return replay safety for a concrete tool invocation.
  pub fn tool_idempotency(&self, name: &str, params: &Value) -> Option<ToolIdempotency> {
    self.tools.get(name).map(|tool| tool.idempotency(params))
  }

  pub fn evaluate_policy(
    &self,
    name: &str,
    params: &Value,
  ) -> Result<ToolPolicyDecision, ToolError> {
    let tool = self.tools.get(name).ok_or_else(|| ToolError::NotFound {
      name: name.to_string(),
    })?;
    Ok(self.policy.evaluate(name, &tool.metadata(), params))
  }

  /// Resolve the three-way capability merge for a registered tool.
  ///
  /// Layers, in order: tool requires → skill security → tool policy → CLI flag.
  /// Each layer's contribution is recorded in [`EffectiveCapabilities::trace`].
  ///
  /// The returned [`EffectiveCapabilities`] also carries the tool's sandbox
  /// status (when the tool wraps a subprocess), so trace consumers can see
  /// the active backend name and enforcement level without a second lookup.
  pub fn evaluate_capabilities(&self, name: &str) -> Result<EffectiveCapabilities, ToolError> {
    let tool = self.tools.get(name).ok_or_else(|| ToolError::NotFound {
      name: name.to_string(),
    })?;
    let required = tool.requires_capabilities();
    let policy_caps = self.policy.allowed_capabilities();
    let mut effective = EffectiveCapabilities::resolve(
      name,
      &required,
      self.skill_capabilities.as_deref(),
      policy_caps.as_deref(),
      self.cli_capabilities.as_deref(),
    );
    if let Some(status) = tool.sandbox_status() {
      effective = effective.with_sandbox(status);
    }
    Ok(effective)
  }

  pub fn policy_audit_log(&self) -> Vec<ToolPolicyDecision> {
    self
      .policy_audit
      .lock()
      .map(|audit| audit.clone())
      .unwrap_or_default()
  }

  pub fn capability_audit_log(&self) -> Vec<EffectiveCapabilities> {
    self
      .capability_audit
      .lock()
      .map(|audit| audit.clone())
      .unwrap_or_default()
  }

  /// Build an OpenAI-style `tools` array for use in API calls.
  pub fn openai_tools_array(&self) -> Vec<Value> {
    self
      .tools
      .values()
      .map(|t| {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": t.name(),
                "description": t.description(),
                "parameters": t.parameters_schema()
            }
        })
      })
      .collect()
  }

  /// Build a compact plain-text description of all tools for use in
  /// system prompts (prompt-based tool calling).
  pub fn prompt_tools_description(&self) -> String {
    let mut lines: Vec<String> = self
      .tools
      .values()
      .map(|t| t.prompt_description())
      .collect();
    lines.sort(); // deterministic ordering
    lines.join("\n")
  }

  /// Execute a named tool with the given JSON parameters.
  ///
  /// Enforcement order:
  /// 1. [`ToolPolicy`] permission/tool allow-list (records [`ToolPolicyDecision`]).
  /// 2. Three-way capability merge (records [`EffectiveCapabilities`]).
  ///
  /// A denial at either layer short-circuits with [`ToolError::PolicyDenied`].
  pub async fn execute(&self, name: &str, params: Value) -> Result<ToolOutput, ToolError> {
    let tool = self.tools.get(name).ok_or_else(|| ToolError::NotFound {
      name: name.to_string(),
    })?;
    let decision = self.policy.evaluate(name, &tool.metadata(), &params);
    if let Ok(mut audit) = self.policy_audit.lock() {
      audit.push(decision.clone());
    }
    if !decision.allowed {
      return Err(ToolError::PolicyDenied {
        message: decision.deny_reason.unwrap_or_else(|| {
          format!(
            "tool '{}' was denied by policy '{}'",
            name, decision.matched_rule
          )
        }),
      });
    }

    let required = tool.requires_capabilities();
    let policy_caps = self.policy.allowed_capabilities();
    let effective = EffectiveCapabilities::resolve(
      name,
      &required,
      self.skill_capabilities.as_deref(),
      policy_caps.as_deref(),
      self.cli_capabilities.as_deref(),
    );
    if let Ok(mut audit) = self.capability_audit.lock() {
      audit.push(effective.clone());
    }
    if !effective.allowed {
      return Err(ToolError::PolicyDenied {
        message: effective
          .deny_reason
          .clone()
          .unwrap_or_else(|| format!("tool '{}' was denied by capability check", name)),
      });
    }

    tool.execute(params).await
  }
}

impl Default for ToolRegistry {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    ToolDefinition, ToolIdempotency, ToolMetadata, ToolOutput, ToolPermissionSet, ToolSource,
  };
  use async_trait::async_trait;
  use serde_json::json;

  struct StaticTool {
    name: &'static str,
    metadata: ToolMetadata,
  }

  #[async_trait]
  impl Tool for StaticTool {
    fn name(&self) -> &str {
      self.name
    }

    fn description(&self) -> &str {
      "static test tool"
    }

    fn parameters_schema(&self) -> Value {
      json!({"type": "object"})
    }

    fn metadata(&self) -> ToolMetadata {
      self.metadata.clone()
    }

    async fn execute(&self, _params: Value) -> Result<ToolOutput, ToolError> {
      Ok(ToolOutput::success("ok"))
    }
  }

  #[test]
  fn registry_can_filter_tools_by_permission() {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(StaticTool {
      name: "http",
      metadata: ToolMetadata::builtin_named("http"),
    }));
    registry.register(Arc::new(StaticTool {
      name: "workflow",
      metadata: ToolMetadata::workflow(),
    }));

    let network_tools = registry.list_by_permission(ToolPermission::Network);
    assert_eq!(network_tools.len(), 1);
    assert_eq!(network_tools[0].name(), "http");
    assert!(registry.tool_has_permission("workflow", &ToolPermission::Workflow));
    assert!(!registry.tool_has_permission("workflow", &ToolPermission::Network));
  }

  #[test]
  fn tool_definition_serializes_permissions() {
    let definition = ToolDefinition {
      name: "mcp_demo_echo".to_string(),
      description: "demo".to_string(),
      parameters: json!({"type": "object"}),
      metadata: ToolMetadata::mcp("demo", "echo"),
    };
    let value = serde_json::to_value(definition).unwrap();

    assert_eq!(value["metadata"]["source"], json!("mcp"));
    assert_eq!(value["metadata"]["idempotency"], json!("unknown"));
    assert_eq!(
      value["metadata"]["permissions"]["permissions"],
      json!(["mcp", "network"])
    );
  }

  #[test]
  fn registry_reports_invocation_idempotency() {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(StaticTool {
      name: "lookup",
      metadata: ToolMetadata::builtin_named("lookup").with_idempotency(ToolIdempotency::Idempotent),
    }));

    assert_eq!(
      registry.tool_idempotency("lookup", &json!({})),
      Some(ToolIdempotency::Idempotent)
    );
  }

  #[test]
  fn permission_sets_are_stable_and_deduplicated() {
    let set = ToolPermissionSet::new([
      ToolPermission::Network,
      ToolPermission::Network,
      ToolPermission::Mcp,
    ]);

    assert_eq!(
      set.permissions,
      vec![ToolPermission::Mcp, ToolPermission::Network]
    );
  }

  #[test]
  fn default_metadata_is_builtin_without_permissions() {
    let metadata = ToolMetadata::default();

    assert_eq!(metadata.source, ToolSource::Builtin);
    assert!(metadata.permissions.permissions.is_empty());
  }

  #[tokio::test]
  async fn registry_denies_tool_when_policy_rejects_permission() {
    let mut registry = ToolRegistry::new().with_policy(crate::ToolPolicy::allow_permissions([
      ToolPermission::Network,
    ]));
    registry.register(Arc::new(StaticTool {
      name: "shell",
      metadata: ToolMetadata::builtin_named("shell"),
    }));

    let error = registry
      .execute("shell", json!({"command": "echo ok"}))
      .await
      .unwrap_err();

    assert!(matches!(error, ToolError::PolicyDenied { .. }));
    let audit = registry.policy_audit_log();
    assert_eq!(audit.len(), 1);
    assert!(!audit[0].allowed);
    assert_eq!(audit[0].matched_rule, "permission_allowlist");
  }

  #[tokio::test]
  async fn evaluate_capabilities_traces_three_layer_merge() {
    let mut registry = ToolRegistry::new()
      .with_skill_capabilities([Capability::Exec, Capability::Net])
      .with_cli_capabilities([Capability::Exec]);
    registry.register(Arc::new(StaticTool {
      name: "shell",
      metadata: ToolMetadata::builtin_named("shell"),
    }));

    let effective = registry.evaluate_capabilities("shell").unwrap();

    assert!(effective.allowed);
    assert_eq!(effective.required, vec![Capability::Exec]);
    assert_eq!(effective.effective, vec![Capability::Exec]);
    assert_eq!(effective.trace.len(), 4);
    assert_eq!(
      effective.trace[1].source,
      crate::capability::GrantSource::SkillSecurity
    );
    assert!(effective.trace[2].allowed.is_none()); // ToolPolicy is permissive
  }

  #[tokio::test]
  async fn execute_denies_when_skill_blocks_capability() {
    let mut registry = ToolRegistry::new().with_skill_capabilities([Capability::Net]);
    registry.register(Arc::new(StaticTool {
      name: "shell",
      metadata: ToolMetadata::builtin_named("shell"),
    }));

    let error = registry
      .execute("shell", json!({"command": "ls"}))
      .await
      .unwrap_err();

    assert!(matches!(error, ToolError::PolicyDenied { .. }));
    let audit = registry.capability_audit_log();
    assert_eq!(audit.len(), 1);
    assert!(!audit[0].allowed);
    assert_eq!(audit[0].denied, vec![Capability::Exec]);
  }

  #[tokio::test]
  async fn execute_denies_when_cli_overrides_to_empty() {
    let mut registry = ToolRegistry::new().with_cli_capabilities([Capability::FsRead]);
    registry.register(Arc::new(StaticTool {
      name: "http",
      metadata: ToolMetadata::builtin_named("http"),
    }));

    let error = registry
      .execute("http", json!({"url": "https://example.test"}))
      .await
      .unwrap_err();

    assert!(matches!(error, ToolError::PolicyDenied { .. }));
    let audit = registry.capability_audit_log();
    assert_eq!(audit[0].denied, vec![Capability::Net]);
    assert_eq!(
      audit[0].trace.last().map(|entry| entry.source),
      Some(crate::capability::GrantSource::CliFlag)
    );
  }

  #[tokio::test]
  async fn evaluate_capabilities_carries_sandbox_status_for_subprocess_tools() {
    use crate::builtin::ShellTool;
    use crate::sandbox::{SandboxEnforcement, SandboxPolicy};

    let policy = Arc::new(SandboxPolicy::permissive());
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(ShellTool::new(policy)));

    let effective = registry.evaluate_capabilities("shell").unwrap();
    let status = effective
      .sandbox
      .expect("shell tool must surface a sandbox status snapshot");
    // Default ShellTool uses the no-op backend; this is the silent-fall-through
    // case the visibility task is meant to surface. It must be Disabled, not
    // missing.
    assert_eq!(status.backend, "noop");
    assert_eq!(status.enforcement, SandboxEnforcement::Disabled);
  }

  #[tokio::test]
  async fn evaluate_capabilities_omits_sandbox_for_pure_in_process_tools() {
    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(StaticTool {
      name: "http",
      metadata: ToolMetadata::builtin_named("http"),
    }));

    let effective = registry.evaluate_capabilities("http").unwrap();
    assert!(
      effective.sandbox.is_none(),
      "in-process tools that don't spawn subprocesses must report no sandbox status"
    );
  }

  #[tokio::test]
  async fn registry_records_allowed_policy_decisions() {
    let mut registry = ToolRegistry::new().with_policy(crate::ToolPolicy::allow_permissions([
      ToolPermission::Network,
    ]));
    registry.register(Arc::new(StaticTool {
      name: "http",
      metadata: ToolMetadata::builtin_named("http"),
    }));

    let output = registry
      .execute("http", json!({"url": "https://example.test"}))
      .await
      .unwrap();

    assert_eq!(output.content, "ok");
    let audit = registry.policy_audit_log();
    assert_eq!(audit.len(), 1);
    assert!(audit[0].allowed);
    assert_eq!(audit[0].params_summary["url"], json!("string"));
  }
}

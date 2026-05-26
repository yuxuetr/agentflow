use std::collections::BTreeMap;
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
///
/// Q5.4: `tools` is a `BTreeMap` keyed by tool name (not `HashMap`)
/// so iteration yields tools in lexicographic name order. That order
/// flows directly into [`Self::openai_tools_array`] / [`Self::list`]
/// / [`Self::prompt_tools_description`] — i.e. into the LLM request
/// body and trace output. With `HashMap`, two processes registering
/// the same tools could produce different wire bytes; with
/// `BTreeMap`, identical input ⇒ identical output, which the
/// determinism regression test below pins.
pub struct ToolRegistry {
  tools: BTreeMap<String, Arc<dyn Tool>>,
  policy: ToolPolicy,
  policy_audit: Arc<Mutex<Vec<ToolPolicyDecision>>>,
  capability_audit: Arc<Mutex<Vec<EffectiveCapabilities>>>,
  skill_capabilities: Option<Vec<Capability>>,
  cli_capabilities: Option<Vec<Capability>>,
}

impl ToolRegistry {
  pub fn new() -> Self {
    Self {
      tools: BTreeMap::new(),
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

  /// Q2.9.3: validate `params` against the tool's declared
  /// `parameters_schema` (a JSON-Schema fragment) without executing.
  ///
  /// Agent dispatchers call this before forwarding LLM-produced
  /// arguments to `execute`, so a malformed call surfaces as
  /// [`ToolError::SchemaViolation`] and can be replayed back to the
  /// LLM for self-correction instead of crashing the tool or
  /// running with garbage input. Returns `Ok(())` if no tool with
  /// `name` is registered — that path is handled by `execute`'s
  /// `NotFound` branch.
  pub fn validate_params(&self, name: &str, params: &Value) -> Result<(), ToolError> {
    let Some(tool) = self.tools.get(name) else {
      // Defer NotFound classification to `execute` so callers get a
      // single source of truth for unknown tools.
      return Ok(());
    };
    let schema = tool.parameters_schema();
    let compiled = jsonschema::JSONSchema::options()
      .compile(&schema)
      .map_err(|err| ToolError::SchemaViolation {
        tool: name.to_string(),
        message: format!("declared parameters_schema is not valid JSON Schema: {err}"),
      })?;
    if let Err(errors) = compiled.validate(params) {
      let detail = errors.map(|e| e.to_string()).collect::<Vec<_>>().join(", ");
      return Err(ToolError::SchemaViolation {
        tool: name.to_string(),
        message: detail,
      });
    }
    Ok(())
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

    // Q2.9.3: validate `params` against the tool's declared schema
    // before forwarding to `tool.execute(params)`. Any caller (agent
    // dispatcher, CLI, workflow node) that hands a tool malformed
    // arguments now sees `ToolError::SchemaViolation` instead of
    // either crashing the tool or running with garbage input. LLM
    // agents can replay this back to the model for self-correction.
    self.validate_params(name, &params)?;

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

  /// Q5.4 regression: same set of tools registered in any order must
  /// produce byte-identical `openai_tools_array()` / `list()` /
  /// `prompt_tools_description()` output. Pre-fix the underlying
  /// `HashMap` iteration order was a function of process-randomised
  /// hash seeds, so two runs of the same workflow could send
  /// different `tools: [...]` payloads to the LLM — silently
  /// breaking caching and trace replay invariants.
  #[test]
  fn registry_iteration_is_deterministic_across_registration_orders() {
    let names = ["zebra", "alpha", "mango", "delta", "echo"];
    let make_tool = |name: &'static str| -> Arc<dyn Tool> {
      Arc::new(StaticTool {
        name,
        metadata: ToolMetadata::builtin_named(name),
      })
    };

    let build_in_order = |order: &[&'static str]| {
      let mut reg = ToolRegistry::new();
      for &n in order {
        reg.register(make_tool(n));
      }
      reg
    };

    let mut forward_order: Vec<&'static str> = names.to_vec();
    let mut reverse_order: Vec<&'static str> = forward_order.clone();
    reverse_order.reverse();
    let shuffled_order: Vec<&'static str> = vec!["mango", "echo", "zebra", "alpha", "delta"];

    let reg_forward = build_in_order(&forward_order);
    let reg_reverse = build_in_order(&reverse_order);
    let reg_shuffled = build_in_order(&shuffled_order);

    // openai_tools_array → JSON. Must be byte-identical across
    // registration orders so two processes that registered the
    // same tools produce identical LLM request bodies.
    let json_forward = serde_json::to_string(&reg_forward.openai_tools_array()).unwrap();
    let json_reverse = serde_json::to_string(&reg_reverse.openai_tools_array()).unwrap();
    let json_shuffled = serde_json::to_string(&reg_shuffled.openai_tools_array()).unwrap();
    assert_eq!(json_forward, json_reverse, "Q5.4: openai_tools_array");
    assert_eq!(json_forward, json_shuffled, "Q5.4: openai_tools_array");

    // list() must yield the same name sequence regardless of
    // registration order. (BTreeMap → lexicographic.)
    let names_forward: Vec<String> = reg_forward.list().iter().map(|t| t.name().into()).collect();
    let names_reverse: Vec<String> = reg_reverse.list().iter().map(|t| t.name().into()).collect();
    let names_shuffled: Vec<String> = reg_shuffled
      .list()
      .iter()
      .map(|t| t.name().into())
      .collect();
    assert_eq!(names_forward, names_reverse, "Q5.4: list() order");
    assert_eq!(names_forward, names_shuffled, "Q5.4: list() order");
    // The pinned order itself must be lexicographic — anchoring
    // the contract so downstream consumers can rely on it.
    forward_order.sort();
    let expected: Vec<String> = forward_order.iter().map(|s| s.to_string()).collect();
    assert_eq!(names_forward, expected);

    // prompt_tools_description() pre-Q5.4 already called
    // `lines.sort()` so it was deterministic, but we pin it
    // alongside the others so a future refactor that drops the
    // sort still has coverage.
    assert_eq!(
      reg_forward.prompt_tools_description(),
      reg_reverse.prompt_tools_description(),
      "Q5.4: prompt_tools_description"
    );

    // shuffled_order is used to seed reg_shuffled above; reference
    // it again to keep clippy from warning about the intermediate
    // binding even though the test reads from reg_shuffled, not it.
    assert_eq!(shuffled_order.len(), names.len());
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

  /// Q2.9.3 regression: validate_params returns SchemaViolation
  /// when params disagree with the tool's declared schema.
  #[test]
  fn validate_params_rejects_schema_violations() {
    struct StrictTool;
    #[async_trait]
    impl Tool for StrictTool {
      fn name(&self) -> &str {
        "strict"
      }
      fn description(&self) -> &str {
        "tool that requires a url string"
      }
      fn parameters_schema(&self) -> Value {
        json!({
          "type": "object",
          "properties": {
            "url": { "type": "string", "format": "uri" }
          },
          "required": ["url"]
        })
      }
      fn metadata(&self) -> ToolMetadata {
        ToolMetadata::builtin_named("strict")
      }
      async fn execute(&self, _params: Value) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::success("ok"))
      }
    }

    let mut registry = ToolRegistry::new();
    registry.register(Arc::new(StrictTool));

    // Missing required `url` field.
    let err = registry
      .validate_params("strict", &json!({}))
      .expect_err("missing required field must violate schema");
    match err {
      ToolError::SchemaViolation { tool, message } => {
        assert_eq!(tool, "strict");
        assert!(
          message.contains("url") || message.contains("required"),
          "expected mention of missing 'url', got: {message}"
        );
      }
      other => panic!("expected SchemaViolation, got {other:?}"),
    }

    // Wrong type for `url`.
    let err = registry
      .validate_params("strict", &json!({"url": 42}))
      .expect_err("wrong type must violate schema");
    assert!(matches!(err, ToolError::SchemaViolation { .. }));

    // Valid params pass.
    registry
      .validate_params("strict", &json!({"url": "https://example.test"}))
      .expect("valid params must pass schema");

    // Unknown tool is NOT a schema violation — let `execute` raise
    // `NotFound` so callers have a single source of truth.
    registry
      .validate_params("not-a-tool", &json!({}))
      .expect("unknown tool returns Ok from validate_params");
  }
}

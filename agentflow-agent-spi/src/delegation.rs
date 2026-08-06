//! Subagent delegation contract (L5.1).
//!
//! Before this, a supervisor delegating work to a subagent (see
//! `agentflow-agents::supervisor`'s Handoff/Blackboard/Debate patterns) was
//! a prompt-level convention: the caller hand-builds each subagent's
//! `ToolRegistry` and hands it a free-text task string, with no per-
//! subagent capability isolation — every subagent could reach whatever
//! tools its registry happened to carry.
//!
//! [`DelegationSpec`] makes the contract structured and machine-checkable:
//! a goal + context, an optional tool/capability narrowing (enforced via
//! [`agentflow_tool::ToolRegistry::narrowed`], which reuses the *existing*
//! `EffectiveCapabilities::resolve` intersection a Skill's own
//! `security.tool_permission_allowlist` already goes through — no new
//! merge algorithm), an optional expected output JSON Schema
//! ([`validate_output`]), and timeout/budget/evaluation-criteria fields a
//! caller or aggregator (L5.2) can act on.
//!
//! This type deliberately carries no `ToolRegistry` or `ReActAgent`
//! reference itself — it's a plain, serializable contract. Applying it
//! (narrowing a parent registry, constructing the subagent, running it,
//! validating its answer) is the caller's job; see
//! `agentflow-agents::supervisor` for the reference application.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A structured contract for delegating one unit of work to a subagent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationSpec {
  /// What the subagent is being asked to accomplish.
  pub goal: String,
  /// Context handed to the subagent alongside `goal` — e.g. the parent's
  /// relevant findings so far. Free text; the subagent's persona/prompt
  /// decides how to fold it into its own task framing.
  #[serde(default)]
  pub input_context: String,
  /// Tool names the subagent may call. `None` inherits the parent
  /// registry's full tool set unrestricted — the pre-L5.1 default.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub allowed_tools: Option<Vec<String>>,
  /// OS-level capabilities (`FsRead`/`FsWrite`/`Net`/`Exec`/`Env`) the
  /// subagent's tools may exercise. `None` inherits the parent's grant
  /// unrestricted. Enforced through
  /// [`agentflow_tool::capability::EffectiveCapabilities::resolve`]'s
  /// existing `SkillSecurity` layer via
  /// [`agentflow_tool::ToolRegistry::narrowed`].
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub allowed_capabilities: Option<Vec<agentflow_tool::Capability>>,
  /// JSON Schema the subagent's final answer must validate against once
  /// parsed as JSON. `None` (the default) skips validation entirely —
  /// see [`validate_output`].
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub expected_output_schema: Option<Value>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub timeout_ms: Option<u64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub budget_tokens: Option<u32>,
  /// Free-text rubric an aggregator (L5.2) can reference when arbitrating
  /// between multiple subagents' outputs for the same goal.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub evaluation_criteria: Option<String>,
}

impl DelegationSpec {
  pub fn new(goal: impl Into<String>) -> Self {
    Self {
      goal: goal.into(),
      input_context: String::new(),
      allowed_tools: None,
      allowed_capabilities: None,
      expected_output_schema: None,
      timeout_ms: None,
      budget_tokens: None,
      evaluation_criteria: None,
    }
  }

  pub fn with_input_context(mut self, context: impl Into<String>) -> Self {
    self.input_context = context.into();
    self
  }

  pub fn with_allowed_tools(mut self, tools: impl IntoIterator<Item = impl Into<String>>) -> Self {
    self.allowed_tools = Some(tools.into_iter().map(Into::into).collect());
    self
  }

  pub fn with_allowed_capabilities(
    mut self,
    capabilities: impl IntoIterator<Item = agentflow_tool::Capability>,
  ) -> Self {
    self.allowed_capabilities = Some(capabilities.into_iter().collect());
    self
  }

  pub fn with_expected_output_schema(mut self, schema: Value) -> Self {
    self.expected_output_schema = Some(schema);
    self
  }

  pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
    self.timeout_ms = Some(timeout_ms);
    self
  }

  pub fn with_budget_tokens(mut self, budget_tokens: u32) -> Self {
    self.budget_tokens = Some(budget_tokens);
    self
  }

  pub fn with_evaluation_criteria(mut self, criteria: impl Into<String>) -> Self {
    self.evaluation_criteria = Some(criteria.into());
    self
  }

  /// Narrow `parent` per this spec's `allowed_tools` / `allowed_capabilities`
  /// — a thin, self-documenting wrapper over
  /// [`agentflow_tool::ToolRegistry::narrowed`].
  pub fn narrow_registry(
    &self,
    parent: &agentflow_tool::ToolRegistry,
  ) -> agentflow_tool::ToolRegistry {
    parent.narrowed(
      self.allowed_tools.as_deref(),
      self.allowed_capabilities.clone(),
    )
  }
}

/// Outcome of validating a subagent's free-text answer against
/// [`DelegationSpec::expected_output_schema`].
#[derive(Debug, Clone, PartialEq)]
pub enum SchemaValidation {
  /// No schema was configured — nothing to check.
  NotRequired,
  /// The answer parsed as JSON and validated against the schema.
  Valid,
  /// The answer failed to parse as JSON, the schema itself is invalid, or
  /// the parsed answer didn't validate against it.
  Invalid { errors: Vec<String> },
}

impl SchemaValidation {
  pub fn is_valid(&self) -> bool {
    matches!(
      self,
      SchemaValidation::Valid | SchemaValidation::NotRequired
    )
  }
}

/// Validate `answer` (a subagent's free-text final answer) against
/// `spec.expected_output_schema`, if any is configured.
pub fn validate_output(spec: &DelegationSpec, answer: &str) -> SchemaValidation {
  let Some(schema) = &spec.expected_output_schema else {
    return SchemaValidation::NotRequired;
  };
  match crate::schema_validation::validate_json_str_against_schema(schema, answer) {
    Ok(()) => SchemaValidation::Valid,
    Err(errors) => SchemaValidation::Invalid { errors },
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use agentflow_tool::Capability;
  use serde_json::json;

  #[test]
  fn builder_sets_every_field() {
    let spec = DelegationSpec::new("research topic X")
      .with_input_context("prior findings: ...")
      .with_allowed_tools(["http", "file"])
      .with_allowed_capabilities([Capability::Net])
      .with_expected_output_schema(json!({"type": "object"}))
      .with_timeout_ms(30_000)
      .with_budget_tokens(4_000)
      .with_evaluation_criteria("must cite at least one source");

    assert_eq!(spec.goal, "research topic X");
    assert_eq!(spec.input_context, "prior findings: ...");
    assert_eq!(
      spec.allowed_tools,
      Some(vec!["http".to_string(), "file".to_string()])
    );
    assert_eq!(spec.allowed_capabilities, Some(vec![Capability::Net]));
    assert!(spec.expected_output_schema.is_some());
    assert_eq!(spec.timeout_ms, Some(30_000));
    assert_eq!(spec.budget_tokens, Some(4_000));
    assert_eq!(
      spec.evaluation_criteria.as_deref(),
      Some("must cite at least one source")
    );
  }

  #[test]
  fn new_leaves_narrowing_and_schema_unset() {
    let spec = DelegationSpec::new("goal");
    assert_eq!(spec.allowed_tools, None);
    assert_eq!(spec.allowed_capabilities, None);
    assert_eq!(spec.expected_output_schema, None);
  }

  #[test]
  fn narrow_registry_delegates_to_tool_registry_narrowed() {
    let parent = agentflow_tool::ToolRegistry::new();
    let spec = DelegationSpec::new("goal").with_allowed_tools(["http"]);
    let narrowed = spec.narrow_registry(&parent);
    assert!(narrowed.list().is_empty(), "parent had no tools to keep");
  }

  #[test]
  fn validate_output_is_not_required_without_a_schema() {
    let spec = DelegationSpec::new("goal");
    assert_eq!(
      validate_output(&spec, "not json at all"),
      SchemaValidation::NotRequired
    );
  }

  #[test]
  fn validate_output_accepts_a_conforming_answer() {
    let spec = DelegationSpec::new("goal").with_expected_output_schema(json!({
      "type": "object",
      "properties": { "summary": { "type": "string" } },
      "required": ["summary"]
    }));
    let result = validate_output(&spec, r#"{"summary": "done"}"#);
    assert_eq!(result, SchemaValidation::Valid);
    assert!(result.is_valid());
  }

  #[test]
  fn validate_output_rejects_non_json_answer() {
    let spec = DelegationSpec::new("goal").with_expected_output_schema(json!({"type": "object"}));
    let result = validate_output(&spec, "plain text, not JSON");
    assert!(!result.is_valid());
    assert!(matches!(result, SchemaValidation::Invalid { .. }));
  }

  #[test]
  fn validate_output_rejects_json_that_violates_the_schema() {
    let spec = DelegationSpec::new("goal").with_expected_output_schema(json!({
      "type": "object",
      "properties": { "summary": { "type": "string" } },
      "required": ["summary"]
    }));
    let result = validate_output(&spec, r#"{"wrong_field": 1}"#);
    match result {
      SchemaValidation::Invalid { errors } => assert!(!errors.is_empty()),
      other => panic!("expected Invalid, got {other:?}"),
    }
  }
}

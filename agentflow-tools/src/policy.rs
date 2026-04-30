use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{ToolMetadata, ToolPermission, ToolPermissionSet};

#[derive(Debug, Clone, Default)]
pub struct ToolPolicy {
  allowed_tools: Option<BTreeSet<String>>,
  allowed_permissions: Option<ToolPermissionSet>,
}

impl ToolPolicy {
  pub fn allow_all() -> Self {
    Self::default()
  }

  pub fn allow_tools(tools: impl IntoIterator<Item = impl Into<String>>) -> Self {
    Self {
      allowed_tools: Some(tools.into_iter().map(Into::into).collect()),
      allowed_permissions: None,
    }
  }

  pub fn allow_permissions(permissions: impl IntoIterator<Item = ToolPermission>) -> Self {
    Self {
      allowed_tools: None,
      allowed_permissions: Some(ToolPermissionSet::new(permissions)),
    }
  }

  pub fn evaluate(
    &self,
    tool_name: &str,
    metadata: &ToolMetadata,
    params: &Value,
  ) -> ToolPolicyDecision {
    let permissions = metadata
      .permissions
      .permissions
      .iter()
      .map(|permission| permission.as_str().to_string())
      .collect::<Vec<_>>();
    let mut decision = ToolPolicyDecision {
      tool: tool_name.to_string(),
      allowed: true,
      matched_rule: "allow_all".to_string(),
      deny_reason: None,
      source: Some(metadata.source.as_str().to_string()),
      permissions,
      params_summary: summarize_params(params),
    };

    if let Some(allowed_tools) = &self.allowed_tools {
      decision.matched_rule = "tool_allowlist".to_string();
      if !allowed_tools.contains(tool_name) {
        decision.allowed = false;
        decision.deny_reason = Some(format!("tool '{}' is not in the allowlist", tool_name));
        return decision;
      }
    }

    if let Some(allowed_permissions) = &self.allowed_permissions {
      decision.matched_rule = "permission_allowlist".to_string();
      if let Some(permission) = metadata
        .permissions
        .permissions
        .iter()
        .find(|permission| !allowed_permissions.allows(permission))
      {
        decision.allowed = false;
        decision.deny_reason = Some(format!(
          "permission '{}' is not allowed for tool '{}'",
          permission.as_str(),
          tool_name
        ));
      }
    }

    decision
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolPolicyDecision {
  pub tool: String,
  pub allowed: bool,
  pub matched_rule: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub deny_reason: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub source: Option<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub permissions: Vec<String>,
  pub params_summary: Value,
}

fn summarize_params(params: &Value) -> Value {
  match params {
    Value::Object(map) => Value::Object(
      map
        .iter()
        .map(|(key, value)| {
          (
            key.clone(),
            Value::String(
              match value {
                Value::Null => "null",
                Value::Bool(_) => "boolean",
                Value::Number(_) => "number",
                Value::String(_) => "string",
                Value::Array(_) => "array",
                Value::Object(_) => "object",
              }
              .to_string(),
            ),
          )
        })
        .collect(),
    ),
    other => Value::String(
      match other {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => unreachable!(),
      }
      .to_string(),
    ),
  }
}

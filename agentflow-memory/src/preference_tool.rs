//! `remember_preference` — the write-side product surface for the
//! Preference memory layer (U2.2, `docs/MEMORY_LAYERING.md` §3).
//!
//! `SqlitePreferenceStore` was previously only usable "standalone" (a
//! direct Rust API call, or the `agentflow memory prune` CLI for
//! retention) — nothing in a real conversation could ever write to it, so
//! configuring `[memory.preference]` on a Skill had no product-visible
//! effect. [`RememberPreferenceTool`] closes that gap by wrapping a store
//! as a registry-installable [`Tool`] the agent can call mid-conversation,
//! mirroring how `agentflow-rag::RagSearchTool` wraps a `KnowledgeBackend`.

use std::sync::Arc;

use agentflow_tools::{Tool, ToolError, ToolIdempotency, ToolMetadata, ToolOutput};
use async_trait::async_trait;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::layer::{PreferenceScope, PreferenceStore};
use crate::preference::SqlitePreferenceStore;

/// Registry-installable tool that lets the agent persist a durable
/// preference for the current user.
///
/// `PreferenceStore::put_preference` takes `&mut self` (a single-owner
/// design, matching its original "standalone" / CLI-only usage) — this
/// tool needs to share one store between itself (writes) and the agent's
/// prompt-assembly code (reads), so it wraps the store in a
/// [`tokio::sync::Mutex`] rather than requiring `PreferenceStore`'s
/// signature to change.
pub struct RememberPreferenceTool {
  store: Arc<Mutex<SqlitePreferenceStore>>,
  scope: PreferenceScope,
}

impl RememberPreferenceTool {
  /// Matches [`Tool::name`] — exposed as a const so callers that need
  /// to reference the tool by name (e.g. an admission filter) don't
  /// have to duplicate the string literal.
  pub const TOOL_NAME: &'static str = "remember_preference";

  pub fn new(store: Arc<Mutex<SqlitePreferenceStore>>, scope: PreferenceScope) -> Self {
    Self { store, scope }
  }
}

#[async_trait]
impl Tool for RememberPreferenceTool {
  fn name(&self) -> &str {
    Self::TOOL_NAME
  }

  fn description(&self) -> &str {
    "Persist a durable preference about the current user (e.g. preferred language, tone, \
     formatting) so future sessions remember it automatically. Only use this for stable facts \
     the user explicitly states or clearly implies — not one-off requests specific to this \
     conversation."
  }

  fn parameters_schema(&self) -> Value {
    json!({
      "type": "object",
      "properties": {
        "key": {
          "type": "string",
          "description": "Short, stable identifier for this preference (e.g. \"language\", \"tone\")."
        },
        "value": {
          "description": "The preference value. Any JSON value (string, number, bool, object)."
        }
      },
      "required": ["key", "value"]
    })
  }

  fn metadata(&self) -> ToolMetadata {
    // Overwrites the prior value for `key` — replaying the same call
    // with the same arguments reaches the same end state (last write
    // wins), so this is safe to auto-replay on partial resume.
    ToolMetadata::builtin().with_idempotency(ToolIdempotency::Idempotent)
  }

  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
    let key =
      params
        .get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::InvalidParams {
          message: "missing required string field `key`".to_string(),
        })?;
    let value = params
      .get("value")
      .cloned()
      .ok_or_else(|| ToolError::InvalidParams {
        message: "missing required field `value`".to_string(),
      })?;

    let mut store = self.store.lock().await;
    store
      .put_preference(&self.scope, key, value)
      .await
      .map_err(|e| ToolError::ExecutionFailed {
        message: format!("failed to store preference: {e}"),
      })?;

    Ok(ToolOutput::success(format!("remembered {key}")))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  async fn tool() -> (RememberPreferenceTool, Arc<Mutex<SqlitePreferenceStore>>) {
    let store = Arc::new(Mutex::new(
      SqlitePreferenceStore::in_memory().await.unwrap(),
    ));
    let tool = RememberPreferenceTool::new(store.clone(), PreferenceScope::local("default"));
    (tool, store)
  }

  #[tokio::test]
  async fn declares_idempotent_metadata() {
    let (tool, _store) = tool().await;
    assert_eq!(tool.metadata().idempotency, ToolIdempotency::Idempotent);
    assert_eq!(tool.name(), "remember_preference");
  }

  #[tokio::test]
  async fn execute_persists_the_value_into_the_shared_store() {
    let (tool, store) = tool().await;
    let out = tool
      .execute(json!({"key": "language", "value": "en-GB"}))
      .await
      .expect("execute ok");
    assert!(!out.is_error);

    let scope = PreferenceScope::local("default");
    let stored = store
      .lock()
      .await
      .get_preference(&scope, "language")
      .await
      .unwrap()
      .expect("preference was written");
    assert_eq!(stored.value, json!("en-GB"));
  }

  #[tokio::test]
  async fn execute_missing_key_is_invalid_params() {
    let (tool, _store) = tool().await;
    let err = tool
      .execute(json!({"value": "en-GB"}))
      .await
      .expect_err("missing key rejected");
    assert!(matches!(err, ToolError::InvalidParams { .. }));
  }

  #[tokio::test]
  async fn execute_missing_value_is_invalid_params() {
    let (tool, _store) = tool().await;
    let err = tool
      .execute(json!({"key": "language"}))
      .await
      .expect_err("missing value rejected");
    assert!(matches!(err, ToolError::InvalidParams { .. }));
  }
}

//! Preference layer contract (L3, U2.6): durable per-user key/value
//! storage. See `docs/MEMORY_LAYERING.md` §3.
//!
//! Extracted from `agentflow-memory` mirroring the `ProjectMemoryStore`
//! split (U2.5) — `agentflow-agents`'s `ReActAgent` needs the trait +
//! data types without depending on the concrete `SqlitePreferenceStore`
//! implementation.
//!
//! Originally `PreferenceStore`'s write methods (`put_preference` /
//! `delete_preference` / `prune_older_than`) took `&mut self`, which was
//! the reason U2.2/U2.5 left this trait out of `store-spi`: it didn't fit
//! the bare `Arc<dyn Trait>` shape the rest of this crate's contracts use.
//! Re-auditing for U2.6 found that constraint wasn't actually load-bearing
//! — [`SqlitePreferenceStore`](../../agentflow_memory/struct.SqlitePreferenceStore.html)'s
//! writes only ever touch `&self.pool` (`sqlx::SqlitePool` is internally
//! `Arc`-backed and already safe to call concurrently through a shared
//! reference), and the `AgeEncryptedPreferenceStore` wrapper only
//! encrypts/decrypts and forwards to its inner store. So the methods
//! below are `&self`, matching [`crate::ProjectMemoryStore`] /
//! [`crate::TaskSummaryStore`] exactly, and callers no longer need to
//! wrap a store in a `Mutex` to share it.

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::MemoryError;

/// `(tenant_id, user_id)` identity that scopes every preference write.
///
/// Both fields are required. For single-tenant local-dev use, pass
/// `PreferenceScope::local(user_id)` which hard-codes `tenant_id = "default"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PreferenceScope {
  pub tenant_id: String,
  pub user_id: String,
}

impl PreferenceScope {
  pub fn new(tenant_id: impl Into<String>, user_id: impl Into<String>) -> Self {
    Self {
      tenant_id: tenant_id.into(),
      user_id: user_id.into(),
    }
  }

  /// Zero-config scope for single-tenant local-dev: tenant = `"default"`.
  pub fn local(user_id: impl Into<String>) -> Self {
    Self::new("default", user_id)
  }
}

/// A stored preference value with provenance.
///
/// `version` increments on every `put_preference`; consumers can use it to
/// detect concurrent writes from a different agent process.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreferenceValue {
  pub value: Value,
  pub updated_at: DateTime<Utc>,
  pub version: i64,
}

/// Durable per-user key/value store. See `docs/MEMORY_LAYERING.md` §3.
#[async_trait]
pub trait PreferenceStore: Send + Sync {
  /// Fetch the value for `key` under `scope`. Returns `Ok(None)` if absent.
  async fn get_preference(
    &self,
    scope: &PreferenceScope,
    key: &str,
  ) -> Result<Option<PreferenceValue>, MemoryError>;

  /// Insert or update the value for `key` under `scope`. Increments
  /// `version` and stamps `updated_at` server-side.
  async fn put_preference(
    &self,
    scope: &PreferenceScope,
    key: &str,
    value: Value,
  ) -> Result<(), MemoryError>;

  /// Remove the value for `key`. Idempotent — succeeds if the row was
  /// already absent.
  async fn delete_preference(&self, scope: &PreferenceScope, key: &str) -> Result<(), MemoryError>;

  /// Enumerate every `(key, value)` pair under `scope`. Used by the
  /// agent runtime to surface "what does the agent know about me?" UX.
  async fn list_preferences(
    &self,
    scope: &PreferenceScope,
  ) -> Result<Vec<(String, PreferenceValue)>, MemoryError>;

  /// Drop rows whose `updated_at` is older than `older_than`. Returns the
  /// number of rows removed.
  async fn prune_older_than(&self, older_than: Duration) -> Result<u64, MemoryError>;
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn preference_scope_local_uses_default_tenant() {
    let scope = PreferenceScope::local("alice");
    assert_eq!(scope.tenant_id, "default");
    assert_eq!(scope.user_id, "alice");
  }
}

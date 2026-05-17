//! Memory layer traits and shared types for P4.7.
//!
//! The four canonical memory layers are described in
//! `docs/MEMORY_LAYERING.md`. This module is the trait surface those
//! layers share.
//!
//! Why a dedicated module instead of bolting more methods onto
//! [`crate::MemoryStore`]: preferences and entity facts are not
//! `Message`-shaped, so every backend would have to stub the methods it
//! doesn't support. The separate traits here keep each backend honest:
//! a `SqlitePreferenceStore` implements `PreferenceStore` and nothing
//! else, an agent runtime that wants preferences asks for that trait
//! object explicitly, and a `MemoryStore` backend keeps doing what it's
//! always done.
//!
//! Stability: experimental at first land. See `docs/STABILITY.md`.

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{MemoryError, MemoryStore, Message};

/// Identifier for one of the four memory layers documented in
/// `docs/MEMORY_LAYERING.md`. Used by retention policies and by the
/// agent runtime when it decides which store to dispatch to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryLayer {
  Session,
  Semantic,
  Preference,
  EntityFacts,
}

impl MemoryLayer {
  /// Stable string identifier used in CLI flags and skill manifests.
  pub const fn as_str(&self) -> &'static str {
    match self {
      Self::Session => "session",
      Self::Semantic => "semantic",
      Self::Preference => "preference",
      Self::EntityFacts => "entity_facts",
    }
  }
}

impl std::fmt::Display for MemoryLayer {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(self.as_str())
  }
}

/// Per-layer retention configuration. The CLI subcommand wiring is tracked
/// alongside the design doc; the type itself is what backends consult when
/// pruning.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RetentionPolicy {
  /// Hard cap on row age (by `updated_at` / `extracted_at`). `None` means
  /// keep indefinitely — the preference / entity-facts default.
  pub max_age: Option<Duration>,
  /// For Entity facts: how long to keep rows after `invalidated_at` is set
  /// before a `prune_invalidated` call drops them. Defaults to 2 years per
  /// the design doc.
  pub keep_invalidated_for: Option<Duration>,
}

impl RetentionPolicy {
  /// Default retention for the named layer. Matches the table in
  /// `docs/MEMORY_LAYERING.md`.
  pub fn default_for(layer: MemoryLayer) -> Self {
    match layer {
      MemoryLayer::Session => Self {
        max_age: None,
        keep_invalidated_for: None,
      },
      MemoryLayer::Semantic => Self {
        max_age: None,
        keep_invalidated_for: None,
      },
      MemoryLayer::Preference => Self {
        max_age: None,
        keep_invalidated_for: None,
      },
      MemoryLayer::EntityFacts => Self {
        max_age: None,
        // 2 years in seconds (365.25 days × 2)
        keep_invalidated_for: Some(Duration::from_secs(63_115_200)),
      },
    }
  }
}

// ── Preference layer ────────────────────────────────────────────────────────

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
    &mut self,
    scope: &PreferenceScope,
    key: &str,
    value: Value,
  ) -> Result<(), MemoryError>;

  /// Remove the value for `key`. Idempotent — succeeds if the row was
  /// already absent.
  async fn delete_preference(
    &mut self,
    scope: &PreferenceScope,
    key: &str,
  ) -> Result<(), MemoryError>;

  /// Enumerate every `(key, value)` pair under `scope`. Used by the
  /// agent runtime to surface "what does the agent know about me?" UX.
  async fn list_preferences(
    &self,
    scope: &PreferenceScope,
  ) -> Result<Vec<(String, PreferenceValue)>, MemoryError>;

  /// Drop rows whose `updated_at` is older than `older_than`. Returns the
  /// number of rows removed.
  async fn prune_older_than(&mut self, older_than: Duration) -> Result<u64, MemoryError>;
}

// ── Entity facts layer ──────────────────────────────────────────────────────

/// A structured fact about an entity (person, project, codebase, …).
///
/// Two facts about the same `(entity_id, attribute)` stay as separate rows
/// (different `fact_id`s). The runtime renders each fact with its own
/// citation so the agent can show its work when challenged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityFact {
  /// Stable id of the entity (UUID, slug, or external system id).
  pub entity_id: String,
  /// Stable id of *this fact*. Multiple facts can share an entity_id.
  pub fact_id: String,
  /// Name of the attribute (e.g. `"birthday"`, `"primary_language"`).
  pub attribute: String,
  /// Value associated with the attribute. JSON so structured facts can
  /// nest (`{"start": "2026-01-01", "end": "2026-12-31"}`).
  pub value: Value,
  /// Where this fact came from — usually a `message_id` from the
  /// conversation that extracted it. Optional so external pipelines can
  /// seed facts without a conversation.
  pub source_message_id: Option<String>,
  /// Extractor confidence in `[0.0, 1.0]`. Higher is more confident.
  pub confidence: f32,
  /// When the fact was extracted.
  pub extracted_at: DateTime<Utc>,
  /// When the fact was invalidated. Invalidated facts are preserved for
  /// audit until `prune_invalidated` removes them.
  pub invalidated_at: Option<DateTime<Utc>>,
  /// Operator-supplied reason captured on invalidation.
  pub invalidation_reason: Option<String>,
}

impl EntityFact {
  /// Convenience constructor for newly-extracted facts (not yet invalidated).
  pub fn new(
    entity_id: impl Into<String>,
    fact_id: impl Into<String>,
    attribute: impl Into<String>,
    value: Value,
    confidence: f32,
  ) -> Self {
    Self {
      entity_id: entity_id.into(),
      fact_id: fact_id.into(),
      attribute: attribute.into(),
      value,
      source_message_id: None,
      confidence,
      extracted_at: Utc::now(),
      invalidated_at: None,
      invalidation_reason: None,
    }
  }

  pub fn with_source(mut self, message_id: impl Into<String>) -> Self {
    self.source_message_id = Some(message_id.into());
    self
  }

  pub fn is_invalidated(&self) -> bool {
    self.invalidated_at.is_some()
  }
}

/// Durable, provenance-tracked structured-fact store. See
/// `docs/MEMORY_LAYERING.md` §4.
#[async_trait]
pub trait EntityFactStore: Send + Sync {
  /// Insert a new fact. Replaces an existing `(entity_id, fact_id)` row
  /// if one exists — callers that want a strict "no-clobber" insert
  /// should check `get_facts` first.
  async fn record_fact(&mut self, fact: EntityFact) -> Result<(), MemoryError>;

  /// Fetch every fact for an entity. `include_invalidated` controls
  /// whether previously-invalidated rows are returned (default: hide
  /// them — the agent should not surface stale facts to the user).
  async fn get_facts(
    &self,
    entity_id: &str,
    include_invalidated: bool,
  ) -> Result<Vec<EntityFact>, MemoryError>;

  /// Mark a fact invalidated. Preserves the row for audit; `prune_invalidated`
  /// hard-deletes it later.
  async fn invalidate_fact(
    &mut self,
    entity_id: &str,
    fact_id: &str,
    reason: &str,
  ) -> Result<(), MemoryError>;

  /// Hard-delete invalidated rows whose `invalidated_at` is older than
  /// `older_than`. Returns the number of rows removed.
  async fn prune_invalidated(&mut self, older_than: Duration) -> Result<u64, MemoryError>;
}

// ── Semantic layer ──────────────────────────────────────────────────────────

/// Typed semantic search API on top of [`MemoryStore`].
///
/// `MemoryStore::search` is preserved for callers that already depend on
/// it; new code should reach for `search_semantic` since it returns the
/// per-row similarity scores. `session_id = None` is reserved for stores
/// that index across sessions — today's `SemanticMemory` requires a
/// session id, but the trait shape leaves the door open.
#[async_trait]
pub trait SemanticMemoryStore: MemoryStore {
  async fn search_semantic(
    &self,
    session_id: Option<&str>,
    query: &str,
    k: usize,
  ) -> Result<Vec<(Message, f32)>, MemoryError>;
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn memory_layer_as_str_matches_serde() {
    for layer in [
      MemoryLayer::Session,
      MemoryLayer::Semantic,
      MemoryLayer::Preference,
      MemoryLayer::EntityFacts,
    ] {
      let serialized = serde_json::to_string(&layer).unwrap();
      let trimmed = serialized.trim_matches('"');
      assert_eq!(trimmed, layer.as_str(), "serde / as_str must agree");
    }
  }

  #[test]
  fn entity_facts_default_keeps_invalidated_for_two_years() {
    let policy = RetentionPolicy::default_for(MemoryLayer::EntityFacts);
    let two_years = Duration::from_secs(63_115_200);
    assert_eq!(policy.keep_invalidated_for, Some(two_years));
    assert!(policy.max_age.is_none());
  }

  #[test]
  fn preference_scope_local_uses_default_tenant() {
    let scope = PreferenceScope::local("alice");
    assert_eq!(scope.tenant_id, "default");
    assert_eq!(scope.user_id, "alice");
  }

  #[test]
  fn entity_fact_new_is_not_invalidated() {
    let fact = EntityFact::new("e1", "f1", "color", serde_json::json!("blue"), 0.9);
    assert!(!fact.is_invalidated());
    assert_eq!(fact.confidence, 0.9);
    assert!(fact.source_message_id.is_none());
  }
}

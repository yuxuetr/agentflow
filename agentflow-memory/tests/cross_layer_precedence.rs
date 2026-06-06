//! Cross-layer integration test for P4.7.
//!
//! The four memory layers (Session / Semantic / Preference / Entity facts)
//! must coexist in a single agent runtime without aliasing the same data.
//! This test exercises each layer for one slice of an end-to-end scenario:
//!
//!   * Session — fold an active conversation into the prompt window.
//!   * Semantic — search past sessions by similarity (scored).
//!   * Preference — exact-match per-user key/value (tone setting).
//!   * Entity facts — structured fact with provenance + invalidation.
//!
//! The test treats each layer through its own trait object (`MemoryStore`,
//! `SemanticMemoryStore`, `PreferenceStore`, `EntityFactStore`) — exactly
//! how the agent runtime will dispatch when it ships in
//! `agentflow-agents`.

use std::sync::Arc;

use agentflow_memory::{
  EntityFact, EntityFactStore, MemoryStore, Message, PreferenceScope, PreferenceStore,
  SemanticMemory, SemanticMemoryStore, SessionMemory, SqliteEntityFactStore, SqlitePreferenceStore,
};
use agentflow_rag::embeddings::EmbeddingProvider;
use agentflow_rag::error::{RAGError, Result as RagResult};
use async_trait::async_trait;
use serde_json::json;

const SESSION_ID: &str = "cross-layer-session";

/// Tiny deterministic embedder seeded by the first character of the input.
/// Lifted from `agentflow_memory::semantic` tests; reproduced here so the
/// integration test stays self-contained.
struct FixedEmbedding;

#[async_trait]
impl EmbeddingProvider for FixedEmbedding {
  async fn embed_text(&self, text: &str) -> RagResult<Vec<f32>> {
    if text.is_empty() {
      return Err(RAGError::embedding("empty text"));
    }
    let seed = text.chars().next().map(|c| c as u32).unwrap_or(1) as f32;
    let mut v = vec![0.0f32; 4];
    v[0] = seed.sin();
    v[1] = seed.cos();
    v[2] = (seed * 0.5).sin();
    v[3] = (seed * 0.5).cos();
    let mag: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if mag > 0.0 {
      for x in &mut v {
        *x /= mag;
      }
    }
    Ok(v)
  }
  async fn embed_batch(&self, texts: Vec<&str>) -> RagResult<Vec<Vec<f32>>> {
    let mut out = Vec::with_capacity(texts.len());
    for t in texts {
      out.push(self.embed_text(t).await?);
    }
    Ok(out)
  }
  fn dimension(&self) -> usize {
    4
  }
  fn model_name(&self) -> &str {
    "fixed-cross-layer"
  }
}

/// Each layer is attached as its own typed handle. The runtime would do
/// the same: one trait object per layer, no shared underlying type.
#[tokio::test]
async fn four_layers_coexist_without_aliasing() {
  // ── Session: in-process token window ─────────────────────────────────
  let session: Box<dyn MemoryStore> = Box::new(SessionMemory::default_window());
  session
    .add_message(Message::user(SESSION_ID, "the project deadline is March"))
    .await
    .unwrap();
  session
    .add_message(Message::assistant(SESSION_ID, "noted"))
    .await
    .unwrap();
  let history = session.get_all(SESSION_ID).await.unwrap();
  assert_eq!(history.len(), 2, "session captures both turns");

  // ── Semantic: similarity search with scores ──────────────────────────
  let semantic = SemanticMemory::in_memory(Arc::new(FixedEmbedding), 8_000)
    .await
    .unwrap();
  // Seed with a few user messages so the search has candidates.
  for content in ["alpha context", "beta context", "alpha follow-up"] {
    semantic
      .add_message(Message::user(SESSION_ID, content))
      .await
      .unwrap();
  }
  let results = semantic
    .search_semantic(Some(SESSION_ID), "alpha", 2)
    .await
    .unwrap();
  assert!(
    !results.is_empty(),
    "semantic search must return at least one result"
  );
  // Top result must be one of the 'a'-seeded entries (deterministic vec).
  assert!(
    results
      .iter()
      .any(|(msg, _score)| msg.content.starts_with("alpha")),
    "alpha-seeded entry must rank in the top-k for an 'alpha' query"
  );
  // Scores are real cosine values (or 0.0 fallback when embedding fails);
  // verify they are finite numbers.
  for (_, score) in &results {
    assert!(score.is_finite(), "scores must be finite");
  }

  // ── Preference: exact-match per-user key/value ───────────────────────
  let mut preferences = SqlitePreferenceStore::in_memory().await.unwrap();
  let scope = PreferenceScope::local("alice");
  preferences
    .put_preference(&scope, "tone", json!("formal"))
    .await
    .unwrap();
  let pref = preferences
    .get_preference(&scope, "tone")
    .await
    .unwrap()
    .expect("preference present");
  assert_eq!(pref.value, json!("formal"));
  assert_eq!(pref.version, 1);

  // Another user must not see Alice's tone.
  let bob = PreferenceScope::local("bob");
  assert!(
    preferences
      .get_preference(&bob, "tone")
      .await
      .unwrap()
      .is_none(),
    "preference layer is per-user; Bob must not see Alice's tone"
  );

  // ── Entity facts: structured fact + invalidation lifecycle ───────────
  let mut facts = SqliteEntityFactStore::in_memory().await.unwrap();
  let fact = EntityFact::new(
    "project:atlas",
    "fact_deadline_1",
    "deadline",
    json!("2026-03-31"),
    0.9,
  )
  .with_source("msg_42");
  facts.record_fact(fact).await.unwrap();
  let active = facts.get_facts("project:atlas", false).await.unwrap();
  assert_eq!(active.len(), 1);
  assert_eq!(active[0].confidence, 0.9);

  // Invalidate, then verify default get hides it but include_invalidated
  // surfaces it for audit.
  facts
    .invalidate_fact("project:atlas", "fact_deadline_1", "deadline rescheduled")
    .await
    .unwrap();
  assert!(
    facts
      .get_facts("project:atlas", false)
      .await
      .unwrap()
      .is_empty(),
    "invalidated facts hidden from default reads"
  );
  let audit = facts.get_facts("project:atlas", true).await.unwrap();
  assert_eq!(audit.len(), 1);
  assert!(audit[0].is_invalidated());
  assert_eq!(
    audit[0].invalidation_reason.as_deref(),
    Some("deadline rescheduled")
  );

  // ── Independence guarantee ───────────────────────────────────────────
  // Setting a preference must NOT show up under semantic search or
  // session history — the four layers own distinct data.
  preferences
    .put_preference(&scope, "language", json!("en"))
    .await
    .unwrap();

  let session_after = session.get_all(SESSION_ID).await.unwrap();
  assert_eq!(
    session_after.len(),
    2,
    "preference write must not leak into session"
  );

  let semantic_after = semantic
    .search_semantic(Some(SESSION_ID), "language", 5)
    .await
    .unwrap();
  // The semantic layer never saw the "language" preference (it was
  // written to a different store), so the search can only hit the
  // a/b-seeded user messages. None of them should contain the literal
  // "language" word — but content match isn't the contract here, the
  // contract is "no leakage". So assert that the preference key isn't
  // present in any returned message content.
  for (msg, _) in &semantic_after {
    assert!(
      !msg.content.contains("language"),
      "preference data must not surface through semantic search; got: {}",
      msg.content
    );
  }
}

//! Thin wrapper around `agentflow_memory::SqliteEntityFactStore` that
//! tracks "papers I've already seen / briefed on" so periodic runs only
//! summarize genuinely new arrivals.
//!
//! Data shape (one EntityFact per seen paper):
//!
//! - `entity_id` = `"arxiv:<category>"` (e.g. `"arxiv:cs.AI"`) — keep
//!   per-category so different research interests don't collide.
//! - `fact_id` = arxiv paper id (e.g. `"2501.12345"`) — the unique key
//!   that makes "have I seen this?" an O(1) lookup against the in-mem
//!   filter built from `get_facts`.
//! - `attribute` = `"first_seen"`.
//! - `value` = `{"title": "...", "published": "...", "abs_url": "..."}`
//!   — denormalized snapshot so the briefing tool can describe a paper
//!   without re-fetching arxiv if it ever needs to.

use agentflow_memory::SqliteEntityFactStore;
use agentflow_memory::layer::{EntityFact, EntityFactStore};
use anyhow::{Context, Result};
use serde_json::json;
use std::path::PathBuf;

use crate::arxiv_fetch::Paper;

pub struct SeenStore {
  inner: SqliteEntityFactStore,
}

impl SeenStore {
  /// Open (or create) a SQLite file at `path`. Parent directory is
  /// created if missing.
  pub async fn open(path: PathBuf) -> Result<Self> {
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent)
        .with_context(|| format!("create parent dir for state file: {}", parent.display()))?;
    }
    let inner = SqliteEntityFactStore::open(&path)
      .await
      .with_context(|| format!("open SqliteEntityFactStore at {}", path.display()))?;
    Ok(Self { inner })
  }

  /// Return the subset of `candidates` whose paper_ids are NOT yet
  /// recorded under `arxiv:<category>`. The caller decides whether to
  /// mark the unseen ones as seen — typically yes, after the briefing
  /// successfully consumes them (call `mark_seen_batch`).
  pub async fn filter_unseen(&self, category: &str, candidates: &[Paper]) -> Result<Vec<Paper>> {
    let entity_id = entity_id_for_category(category);
    let existing = self
      .inner
      .get_facts(&entity_id, false)
      .await
      .with_context(|| format!("get_facts({entity_id})"))?;
    let seen: std::collections::HashSet<String> = existing.into_iter().map(|f| f.fact_id).collect();
    Ok(
      candidates
        .iter()
        .filter(|p| !seen.contains(&p.paper_id))
        .cloned()
        .collect(),
    )
  }

  /// Record each paper as "seen" under `arxiv:<category>`. Idempotent —
  /// re-recording the same paper just replaces the row (SqliteEntityFactStore's
  /// `record_fact` semantics).
  pub async fn mark_seen_batch(&mut self, category: &str, papers: &[Paper]) -> Result<()> {
    let entity_id = entity_id_for_category(category);
    for paper in papers {
      let fact = EntityFact::new(
        entity_id.clone(),
        paper.paper_id.clone(),
        "first_seen".to_string(),
        json!({
          "title": paper.title,
          "published": paper.published.to_rfc3339(),
          "abs_url": paper.abs_url,
        }),
        1.0, // We're certain we saw the paper — we just fetched it.
      );
      self
        .inner
        .record_fact(fact)
        .await
        .with_context(|| format!("record_fact({entity_id}, {})", paper.paper_id))?;
    }
    Ok(())
  }
}

/// Pure helper, in its own function so we can unit-test the format
/// without touching SQLite.
pub fn entity_id_for_category(category: &str) -> String {
  format!("arxiv:{category}")
}

#[cfg(test)]
mod tests {
  use super::*;

  fn paper(id: &str) -> Paper {
    Paper {
      paper_id: id.to_string(),
      abs_url: format!("http://arxiv.org/abs/{id}"),
      title: format!("Title of {id}"),
      summary: "Summary".into(),
      authors: vec!["A".into()],
      published: chrono::Utc::now(),
    }
  }

  #[test]
  fn entity_id_includes_category() {
    assert_eq!(entity_id_for_category("cs.AI"), "arxiv:cs.AI");
    assert_eq!(entity_id_for_category("math.ST"), "arxiv:math.ST");
  }

  #[tokio::test]
  async fn first_run_treats_all_papers_as_unseen() {
    let store = SqliteEntityFactStore::in_memory()
      .await
      .expect("in_memory store");
    let seen = SeenStore { inner: store };
    let candidates = vec![paper("2501.00001"), paper("2501.00002")];
    let unseen = seen
      .filter_unseen("cs.AI", &candidates)
      .await
      .expect("filter");
    assert_eq!(unseen.len(), 2);
  }

  #[tokio::test]
  async fn second_run_filters_out_already_recorded() {
    let store = SqliteEntityFactStore::in_memory()
      .await
      .expect("in_memory store");
    let mut seen = SeenStore { inner: store };
    let first_batch = vec![paper("2501.00001"), paper("2501.00002")];
    seen
      .mark_seen_batch("cs.AI", &first_batch)
      .await
      .expect("mark");
    let second_batch = vec![paper("2501.00001"), paper("2501.00003")];
    let unseen = seen
      .filter_unseen("cs.AI", &second_batch)
      .await
      .expect("filter");
    assert_eq!(unseen.len(), 1);
    assert_eq!(unseen[0].paper_id, "2501.00003");
  }

  #[tokio::test]
  async fn categories_are_isolated() {
    let store = SqliteEntityFactStore::in_memory()
      .await
      .expect("in_memory store");
    let mut seen = SeenStore { inner: store };
    seen
      .mark_seen_batch("cs.AI", &[paper("2501.00001")])
      .await
      .expect("mark");
    let unseen_in_other = seen
      .filter_unseen("cs.CL", &[paper("2501.00001")])
      .await
      .expect("filter");
    assert_eq!(
      unseen_in_other.len(),
      1,
      "papers in cs.AI shouldn't dedupe cs.CL"
    );
  }

  #[tokio::test]
  async fn mark_seen_is_idempotent() {
    let store = SqliteEntityFactStore::in_memory()
      .await
      .expect("in_memory store");
    let mut seen = SeenStore { inner: store };
    let p = paper("2501.99999");
    seen
      .mark_seen_batch("cs.AI", std::slice::from_ref(&p))
      .await
      .expect("mark1");
    // Re-marking shouldn't error.
    seen
      .mark_seen_batch("cs.AI", std::slice::from_ref(&p))
      .await
      .expect("mark2");
    let unseen = seen
      .filter_unseen("cs.AI", std::slice::from_ref(&p))
      .await
      .expect("filter");
    assert_eq!(unseen.len(), 0);
  }
}

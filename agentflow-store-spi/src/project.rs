//! Project-scoped memory contract (L3.1): durable facts about a *project*,
//! shared across every session that runs against it.
//!
//! Extracted from `agentflow-memory` (U2.5) mirroring the `TaskSummaryStore`
//! split in this same crate: `agentflow-agents` needs the trait + data type
//! without depending on the concrete store implementations. Unlike the
//! `agentflow-memory` preference layer (`PreferenceStore`, deliberately left
//! out of `store-spi` per U2.2's scope decision), `ProjectMemoryStore`'s
//! methods are all `&self`, so it composes cleanly with the rest of this
//! crate's `Arc<dyn Trait>` contracts.

use std::path::Path;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::MemoryError;

/// A single durable fact about a project: a command observed to have been
/// run (via the `shell` or `script` tool) during some prior session.
///
/// Deliberately narrow: a deterministic extractor can safely say "this
/// exact command ran" without guessing at its *purpose* (build vs. test vs.
/// deploy) — that classification would need real judgment, so this type
/// doesn't attempt it. A richer, LLM-backed fact generator could produce
/// that classification later without changing this shape (it would just
/// populate more instances, or a future variant).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectFact {
  /// Tool that ran the command (`"shell"`, `"script"`, or `"code_exec"`).
  pub tool: String,
  /// The exact command text observed.
  pub command: String,
  /// When this command was first observed for this project.
  pub first_seen: DateTime<Utc>,
  /// When this command was most recently observed.
  pub last_seen: DateTime<Utc>,
  /// How many separate runs have observed this exact command.
  pub observation_count: u32,
}

/// Persistence contract for [`ProjectFact`], keyed by a caller-computed
/// `project_key` — a stable identifier for a project (see
/// [`project_key_for_path`] for the recommended derivation). Recording the
/// same `(tool, command)` pair again is an upsert: it bumps
/// `observation_count`/`last_seen` rather than duplicating, so a command
/// that shows up in every run doesn't grow the fact list unboundedly.
#[async_trait]
pub trait ProjectMemoryStore: Send + Sync {
  /// All facts recorded for `project_key`, most-recently-seen first.
  async fn get_project_facts(&self, project_key: &str) -> Result<Vec<ProjectFact>, MemoryError>;

  /// Record an observation of `command` (run via `tool`) for `project_key`.
  async fn record_project_fact(
    &self,
    project_key: &str,
    tool: &str,
    command: &str,
  ) -> Result<(), MemoryError>;

  /// Delete every fact recorded for `project_key`.
  async fn clear_project_facts(&self, project_key: &str) -> Result<(), MemoryError>;
}

/// Derive a stable `project_key` from a project root path: sha256 of the
/// canonicalized (symlink-resolved) absolute path, lowercase hex — same
/// "sha256 of a canonical, stable input" pattern `agentflow-cli`'s MCP
/// discovery cache already uses for content-addressed keys. Falls back to
/// hashing the path as-given if it doesn't exist yet (canonicalize requires
/// the path to exist) or can't be canonicalized.
pub fn project_key_for_path(root: &Path) -> String {
  let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
  let mut hasher = Sha256::new();
  hasher.update(canonical.to_string_lossy().as_bytes());
  format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn project_key_is_stable_for_the_same_path() {
    let dir = tempfile::TempDir::new().unwrap();
    let key1 = project_key_for_path(dir.path());
    let key2 = project_key_for_path(dir.path());
    assert_eq!(key1, key2);
    assert_eq!(key1.len(), 64, "sha256 hex digest should be 64 chars");
  }

  #[test]
  fn project_key_differs_for_different_paths() {
    let dir_a = tempfile::TempDir::new().unwrap();
    let dir_b = tempfile::TempDir::new().unwrap();
    assert_ne!(
      project_key_for_path(dir_a.path()),
      project_key_for_path(dir_b.path())
    );
  }
}

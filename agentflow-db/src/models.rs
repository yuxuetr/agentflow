//! Strongly-typed row models for the gateway's six-table schema.
//!
//! These structs round-trip through `sqlx::FromRow` for queries and `Serialize`
//! for HTTP responses, so they double as the wire format for `agentflow-server`
//! routes. Stick to plain owned types — no borrowed slices — to keep
//! `Repository` impls and async handlers simple.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Lifecycle states a `runs` row can hold.
///
/// Stored as TEXT so future variants are additive without a migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
  Queued,
  Running,
  Succeeded,
  Failed,
  Cancelled,
}

impl RunStatus {
  pub fn as_str(self) -> &'static str {
    match self {
      Self::Queued => "queued",
      Self::Running => "running",
      Self::Succeeded => "succeeded",
      Self::Failed => "failed",
      Self::Cancelled => "cancelled",
    }
  }

  /// Parse the canonical `runs.status` column into the typed enum.
  ///
  /// Returns `None` for unknown values; callers usually want to bubble that
  /// up as a 500 since it indicates DB / app drift. Named `parse` (not
  /// `from_str`) so it doesn't shadow the standard `std::str::FromStr`
  /// signature and clippy's `should_implement_trait` lint stays quiet.
  pub fn parse(value: &str) -> Option<Self> {
    Some(match value {
      "queued" => Self::Queued,
      "running" => Self::Running,
      "succeeded" => Self::Succeeded,
      "failed" => Self::Failed,
      "cancelled" => Self::Cancelled,
      _ => return None,
    })
  }
}

/// One row in the `runs` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Run {
  pub id: Uuid,
  pub workflow: String,
  pub status: String,
  pub started_at: DateTime<Utc>,
  pub finished_at: Option<DateTime<Utc>>,
  pub run_dir: Option<String>,
  pub tenant_id: String,
  pub error: Option<String>,
}

/// Input for creating a new run via [`crate::repo::RunRepo::create`].
#[derive(Debug, Clone)]
pub struct NewRun {
  pub id: Uuid,
  pub workflow: String,
  pub status: RunStatus,
  pub run_dir: Option<String>,
  pub tenant_id: String,
}

/// One row in the `steps` table. `payload` is provider-specific JSON
/// captured at the source — typically the serialised step trace.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Step {
  pub run_id: Uuid,
  pub seq: i32,
  pub node_id: String,
  pub kind: String,
  pub status: String,
  pub started_at: DateTime<Utc>,
  pub duration_ms: Option<i64>,
  pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct NewStep {
  pub run_id: Uuid,
  pub seq: i32,
  pub node_id: String,
  pub kind: String,
  pub status: String,
  pub duration_ms: Option<i64>,
  pub payload: Option<serde_json::Value>,
}

/// One row in the `events` table. `seq` is monotonic per `run_id`; SSE
/// subscribers use it as a resume cursor.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Event {
  pub run_id: Uuid,
  pub seq: i64,
  pub kind: String,
  pub payload: serde_json::Value,
  pub ts: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewEvent {
  pub run_id: Uuid,
  pub seq: i64,
  pub kind: String,
  pub payload: serde_json::Value,
}

/// One row in the `artifacts` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Artifact {
  pub id: Uuid,
  pub run_id: Uuid,
  pub node_id: String,
  pub name: String,
  pub path_or_url: String,
  pub mime_type: Option<String>,
  pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewArtifact {
  pub id: Uuid,
  pub run_id: Uuid,
  pub node_id: String,
  pub name: String,
  pub path_or_url: String,
  pub mime_type: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct SkillInstall {
  pub name: String,
  pub version: String,
  pub source: String,
  pub installed_at: DateTime<Utc>,
  pub checksum: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct McpSession {
  pub id: Uuid,
  pub server: String,
  pub started_at: DateTime<Utc>,
  pub ended_at: Option<DateTime<Utc>>,
  pub tool_calls: i32,
  pub metadata: Option<serde_json::Value>,
}

/// Lifecycle states a `harness_sessions` row can hold.
///
/// Mirrors the closed enum of [`RunStatus`] but uses a Harness-specific
/// terminal vocabulary (`completed` / `failed` / `cancelled`). Stored as
/// TEXT so adding new variants is additive without a migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessSessionStatus {
  Running,
  Completed,
  Failed,
  Cancelled,
}

impl HarnessSessionStatus {
  pub fn as_str(self) -> &'static str {
    match self {
      Self::Running => "running",
      Self::Completed => "completed",
      Self::Failed => "failed",
      Self::Cancelled => "cancelled",
    }
  }

  /// Parse the canonical `harness_sessions.status` column into the typed
  /// enum. Returns `None` for unknown values; callers usually want to
  /// bubble that up as a 500 since it indicates DB / app drift. Named
  /// `parse` (not `from_str`) so it doesn't shadow `std::str::FromStr`.
  pub fn parse(value: &str) -> Option<Self> {
    Some(match value {
      "running" => Self::Running,
      "completed" => Self::Completed,
      "failed" => Self::Failed,
      "cancelled" => Self::Cancelled,
      _ => return None,
    })
  }

  /// Terminal statuses are not transitioned through again — used by the
  /// cancel route to short-circuit when a session has already finished.
  pub fn is_terminal(self) -> bool {
    matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
  }
}

/// One row in the `harness_sessions` table.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct HarnessSession {
  pub id: Uuid,
  pub tenant_id: String,
  pub status: String,
  pub user_input: String,
  pub workspace_root: String,
  pub profile: String,
  pub runtime_kind: String,
  pub model: String,
  pub skill_name: Option<String>,
  pub started_at: DateTime<Utc>,
  pub finished_at: Option<DateTime<Utc>>,
  pub final_answer: Option<String>,
  pub error: Option<String>,
}

/// Input for creating a new harness session via
/// [`crate::repo::HarnessSessionRepo::create`].
#[derive(Debug, Clone)]
pub struct NewHarnessSession {
  pub id: Uuid,
  pub tenant_id: String,
  pub user_input: String,
  pub workspace_root: String,
  pub profile: String,
  pub runtime_kind: String,
  pub model: String,
  pub skill_name: Option<String>,
}

/// One row in the `harness_session_events` table. `seq` is monotonic per
/// `session_id`; SSE subscribers use it as a resume cursor — same shape
/// and contract as the workflow `events` table, just scoped to a session.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct HarnessSessionEvent {
  pub session_id: Uuid,
  pub seq: i64,
  pub kind: String,
  pub payload: serde_json::Value,
  pub ts: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewHarnessSessionEvent {
  pub session_id: Uuid,
  pub seq: i64,
  pub kind: String,
  pub payload: serde_json::Value,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn run_status_round_trips() {
    for status in [
      RunStatus::Queued,
      RunStatus::Running,
      RunStatus::Succeeded,
      RunStatus::Failed,
      RunStatus::Cancelled,
    ] {
      assert_eq!(RunStatus::parse(status.as_str()), Some(status));
    }
    assert_eq!(RunStatus::parse("unknown"), None);
  }

  #[test]
  fn harness_session_status_round_trips() {
    for status in [
      HarnessSessionStatus::Running,
      HarnessSessionStatus::Completed,
      HarnessSessionStatus::Failed,
      HarnessSessionStatus::Cancelled,
    ] {
      assert_eq!(HarnessSessionStatus::parse(status.as_str()), Some(status));
    }
    assert_eq!(HarnessSessionStatus::parse("queued"), None);
    assert!(HarnessSessionStatus::Completed.is_terminal());
    assert!(HarnessSessionStatus::Cancelled.is_terminal());
    assert!(!HarnessSessionStatus::Running.is_terminal());
  }
}

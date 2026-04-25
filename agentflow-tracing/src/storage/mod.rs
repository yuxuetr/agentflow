//! Storage backends for execution traces

pub mod file;

use crate::types::*;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// Trace storage trait
///
/// Implementations provide different storage backends (file, database, etc.)
#[async_trait]
pub trait TraceStorage: Send + Sync {
  /// Save a trace to storage
  async fn save_trace(&self, trace: &ExecutionTrace) -> Result<(), anyhow::Error>;

  /// Get a trace by workflow ID
  async fn get_trace(&self, workflow_id: &str) -> Result<Option<ExecutionTrace>, anyhow::Error>;

  /// Query traces with filters
  async fn query_traces(&self, query: TraceQuery) -> Result<Vec<ExecutionTrace>, anyhow::Error>;

  /// Delete old traces (cleanup)
  async fn delete_old_traces(&self, older_than: DateTime<Utc>) -> Result<usize, anyhow::Error>;
}

/// Query parameters for trace search
#[derive(Debug, Clone, Default)]
pub struct TraceQuery {
  /// Filter by specific workflow IDs
  pub workflow_ids: Option<Vec<String>>,

  /// Filter by status
  pub status: Option<TraceStatus>,

  /// Filter by user ID
  pub user_id: Option<String>,

  /// Filter by tags (any match)
  pub tags: Option<Vec<String>>,

  /// Filter by time range
  pub time_range: Option<TimeRange>,

  /// Limit number of results
  pub limit: Option<usize>,

  /// Offset for pagination
  pub offset: Option<usize>,
}

/// Time range for queries
#[derive(Debug, Clone)]
pub struct TimeRange {
  /// Start time (inclusive)
  pub start: DateTime<Utc>,

  /// End time (inclusive)
  pub end: DateTime<Utc>,
}

impl TimeRange {
  /// Create a new time range
  pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
    Self { start, end }
  }

  /// Check if a time is within this range
  pub fn contains(&self, time: DateTime<Utc>) -> bool {
    time >= self.start && time <= self.end
  }
}

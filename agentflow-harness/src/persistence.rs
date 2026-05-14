//! Event persistence sinks for Harness sessions.
//!
//! Phase H1 ships the in-memory [`InMemoryEventSink`] (used by tests
//! and for transient runs) and the file-backed [`JsonlEventSink`]
//! (default for `agentflow harness run`). SQLite / Postgres sinks are
//! intentionally deferred to Phase H5 alongside the server integration.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::error::HarnessError;
use crate::event::HarnessEvent;

/// Async trait implemented by every Harness event sink.
///
/// Implementations MUST be safe to share across tasks. The runtime
/// fans the event stream out to all registered sinks in registration
/// order; a single failing sink does not stop the others, but the
/// runtime records the first error and surfaces it on the run result.
#[async_trait]
pub trait HarnessEventSink: Send + Sync {
  /// Stable identifier (`jsonl`, `memory`, `sqlite`, ...).
  fn name(&self) -> &str;

  /// Persist a single envelope. The implementation owns its own
  /// synchronization; the runtime calls `write` serially per session
  /// but several sessions may share a sink concurrently.
  async fn write(&self, event: &HarnessEvent) -> Result<(), HarnessError>;

  /// Flush any buffered writes. Called when a session terminates so
  /// no events are lost on shutdown.
  async fn flush(&self) -> Result<(), HarnessError> {
    Ok(())
  }
}

/// Append-only JSONL sink keyed by `<dir>/<session_id>.jsonl`.
///
/// One file per session keeps replay / inspection simple and avoids
/// cross-session locking. The sink creates `dir` lazily and uses an
/// internal mutex so concurrent writers serialize without ordering
/// across distinct sinks.
pub struct JsonlEventSink {
  dir: PathBuf,
  state: Mutex<JsonlState>,
}

struct JsonlState {
  current_session: Option<String>,
  file: Option<tokio::fs::File>,
}

impl JsonlEventSink {
  /// Build a sink that writes session files under `dir`. The directory
  /// is created on the first write.
  pub fn new(dir: impl Into<PathBuf>) -> Self {
    Self {
      dir: dir.into(),
      state: Mutex::new(JsonlState {
        current_session: None,
        file: None,
      }),
    }
  }

  /// Returns the on-disk path for the given session id without
  /// requiring the file to exist yet.
  pub fn session_path(&self, session_id: &str) -> PathBuf {
    self.dir.join(format!("{session_id}.jsonl"))
  }

  /// Read back a previously persisted session as a vector of events.
  /// Useful in tests and for trace replay tooling.
  pub async fn read_session(&self, session_id: &str) -> Result<Vec<HarnessEvent>, HarnessError> {
    let path = self.session_path(session_id);
    let raw = match tokio::fs::read_to_string(&path).await {
      Ok(text) => text,
      Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
      Err(err) => return Err(HarnessError::Envelope(err.to_string())),
    };
    let mut events = Vec::new();
    for line in raw.lines() {
      if line.trim().is_empty() {
        continue;
      }
      let event: HarnessEvent = serde_json::from_str(line)
        .map_err(|err| HarnessError::Envelope(format!("decode {path:?}: {err}")))?;
      events.push(event);
    }
    Ok(events)
  }

  async fn ensure_open(
    &self,
    state: &mut JsonlState,
    session_id: &str,
  ) -> Result<(), HarnessError> {
    let needs_reopen = match &state.current_session {
      Some(active) if active == session_id => state.file.is_none(),
      _ => true,
    };
    if !needs_reopen {
      return Ok(());
    }
    if !self.dir.exists() {
      tokio::fs::create_dir_all(&self.dir)
        .await
        .map_err(|err| HarnessError::Envelope(err.to_string()))?;
    }
    let path = self.session_path(session_id);
    let file = OpenOptions::new()
      .create(true)
      .append(true)
      .open(&path)
      .await
      .map_err(|err| HarnessError::Envelope(format!("open {path:?}: {err}")))?;
    state.current_session = Some(session_id.to_owned());
    state.file = Some(file);
    Ok(())
  }
}

#[async_trait]
impl HarnessEventSink for JsonlEventSink {
  fn name(&self) -> &str {
    "jsonl"
  }

  async fn write(&self, event: &HarnessEvent) -> Result<(), HarnessError> {
    let line = serde_json::to_string(event)
      .map_err(|err| HarnessError::Envelope(format!("encode harness event: {err}")))?;
    let mut state = self.state.lock().await;
    self.ensure_open(&mut state, &event.session_id).await?;
    let file = state
      .file
      .as_mut()
      .expect("file opened by ensure_open above");
    file
      .write_all(line.as_bytes())
      .await
      .map_err(|err| HarnessError::Envelope(err.to_string()))?;
    file
      .write_all(b"\n")
      .await
      .map_err(|err| HarnessError::Envelope(err.to_string()))?;
    Ok(())
  }

  async fn flush(&self) -> Result<(), HarnessError> {
    let mut state = self.state.lock().await;
    if let Some(file) = state.file.as_mut() {
      file
        .flush()
        .await
        .map_err(|err| HarnessError::Envelope(err.to_string()))?;
    }
    Ok(())
  }
}

/// Process-local sink that collects events into a `Vec` for tests and
/// stream-json fan-out from a sibling sink.
#[derive(Default)]
pub struct InMemoryEventSink {
  events: Mutex<Vec<HarnessEvent>>,
}

impl InMemoryEventSink {
  pub fn new() -> Self {
    Self::default()
  }

  pub async fn snapshot(&self) -> Vec<HarnessEvent> {
    self.events.lock().await.clone()
  }
}

#[async_trait]
impl HarnessEventSink for InMemoryEventSink {
  fn name(&self) -> &str {
    "memory"
  }

  async fn write(&self, event: &HarnessEvent) -> Result<(), HarnessError> {
    self.events.lock().await.push(event.clone());
    Ok(())
  }
}

/// Sink chain: a thin shared owner the runtime hands to the agent
/// hop. Reused publicly so tests can construct one without poking at
/// runtime internals.
#[derive(Clone, Default)]
pub struct SinkChain {
  sinks: Vec<Arc<dyn HarnessEventSink>>,
}

impl SinkChain {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn push(mut self, sink: Arc<dyn HarnessEventSink>) -> Self {
    self.sinks.push(sink);
    self
  }

  pub fn is_empty(&self) -> bool {
    self.sinks.is_empty()
  }

  pub fn len(&self) -> usize {
    self.sinks.len()
  }

  /// Fan out to every registered sink. Returns the first error
  /// observed but keeps the remaining writes going.
  pub async fn dispatch(&self, event: &HarnessEvent) -> Result<(), HarnessError> {
    let mut first_err: Option<HarnessError> = None;
    for sink in &self.sinks {
      if let Err(err) = sink.write(event).await {
        tracing::warn!(target: "harness", sink = %sink.name(), error = %err, "event sink write failed");
        if first_err.is_none() {
          first_err = Some(err);
        }
      }
    }
    match first_err {
      Some(err) => Err(err),
      None => Ok(()),
    }
  }

  pub async fn flush_all(&self) -> Result<(), HarnessError> {
    let mut first_err: Option<HarnessError> = None;
    for sink in &self.sinks {
      if let Err(err) = sink.flush().await {
        tracing::warn!(target: "harness", sink = %sink.name(), error = %err, "event sink flush failed");
        if first_err.is_none() {
          first_err = Some(err);
        }
      }
    }
    match first_err {
      Some(err) => Err(err),
      None => Ok(()),
    }
  }

  pub fn iter(&self) -> impl Iterator<Item = &Arc<dyn HarnessEventSink>> {
    self.sinks.iter()
  }
}

impl std::fmt::Debug for SinkChain {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let names: Vec<&str> = self.sinks.iter().map(|s| s.name()).collect();
    f.debug_struct("SinkChain").field("sinks", &names).finish()
  }
}

fn _ensure_object_safe(_sink: Arc<dyn HarnessEventSink>) {}

/// Helper exported so the runtime can build a sink path from a
/// `--trace-dir` style argument without re-resolving conventions.
pub fn default_session_dir(base: &Path) -> PathBuf {
  base.join("harness").join("sessions")
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::event::{HarnessEventBody, StepStartedPayload};
  use chrono::Utc;
  use tempfile::TempDir;

  fn sample_event(seq: u64, session: &str) -> HarnessEvent {
    HarnessEvent {
      seq,
      session_id: session.into(),
      ts: Utc::now(),
      body: HarnessEventBody::StepStarted(StepStartedPayload {
        step_index: seq as usize,
        step_type: "plan".into(),
      }),
    }
  }

  #[tokio::test]
  async fn jsonl_sink_writes_and_reads_back() {
    let dir = TempDir::new().unwrap();
    let sink = JsonlEventSink::new(dir.path().join("sessions"));
    for seq in 0..3 {
      sink.write(&sample_event(seq, "sess-1")).await.unwrap();
    }
    sink.flush().await.unwrap();

    let read_back = sink.read_session("sess-1").await.unwrap();
    assert_eq!(read_back.len(), 3);
    for (i, event) in read_back.iter().enumerate() {
      assert_eq!(event.seq, i as u64);
      assert_eq!(event.session_id, "sess-1");
    }
  }

  #[tokio::test]
  async fn jsonl_sink_returns_empty_for_unknown_session() {
    let dir = TempDir::new().unwrap();
    let sink = JsonlEventSink::new(dir.path());
    let events = sink.read_session("missing").await.unwrap();
    assert!(events.is_empty());
  }

  #[tokio::test]
  async fn in_memory_sink_collects_events() {
    let sink = InMemoryEventSink::new();
    sink.write(&sample_event(0, "s")).await.unwrap();
    sink.write(&sample_event(1, "s")).await.unwrap();
    let snap = sink.snapshot().await;
    assert_eq!(snap.len(), 2);
    assert_eq!(snap[1].seq, 1);
  }

  #[tokio::test]
  async fn sink_chain_dispatches_to_all_sinks() {
    let mem_a = Arc::new(InMemoryEventSink::new());
    let mem_b = Arc::new(InMemoryEventSink::new());
    let chain = SinkChain::new()
      .push(mem_a.clone() as Arc<dyn HarnessEventSink>)
      .push(mem_b.clone() as Arc<dyn HarnessEventSink>);
    assert_eq!(chain.len(), 2);
    chain.dispatch(&sample_event(0, "s")).await.unwrap();
    chain.dispatch(&sample_event(1, "s")).await.unwrap();
    chain.flush_all().await.unwrap();
    assert_eq!(mem_a.snapshot().await.len(), 2);
    assert_eq!(mem_b.snapshot().await.len(), 2);
  }
}

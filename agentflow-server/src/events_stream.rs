//! In-process broker that forwards persisted run events to SSE subscribers.
//!
//! `GET /v1/runs/{id}/events` first replays everything already in the
//! `events` table for that run, then keeps the connection open and pushes
//! anything the broker publishes for that `run_id`. New events come from
//! the executor: it writes through the broker, which both persists to the
//! DB and forwards to live subscribers.
//!
//! The broker is intentionally process-local. A multi-replica deployment
//! would replace it with Redis Pub/Sub or NATS — that's an N9 / N10
//! follow-up and lives behind the same trait surface.

use async_trait::async_trait;
use axum::{
  Json,
  extract::{Path, Query, State},
  response::sse::{Event, KeepAlive, Sse},
};
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_stream::wrappers::{BroadcastStream, errors::BroadcastStreamRecvError};
use uuid::Uuid;

use agentflow_db::{DbError, EventRepo, NewEvent, Repositories, RunRepo};

use crate::AppState;
use crate::error::ApiError;

/// Channel capacity per run. Slow subscribers drop oldest events when
/// they fall this far behind; the SSE handler logs a warning and lets the
/// client reconnect with `?after_seq=` to fill the gap from the DB.
const RUN_CHANNEL_CAPACITY: usize = 256;

/// Wire shape published over SSE. Mirrors `agentflow_db::Event` but stays
/// minimal so we don't tie SSE consumers to internal DB columns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamedEvent {
  pub run_id: Uuid,
  pub seq: i64,
  pub kind: String,
  pub payload: serde_json::Value,
  pub ts: chrono::DateTime<chrono::Utc>,
}

impl From<agentflow_db::Event> for StreamedEvent {
  fn from(e: agentflow_db::Event) -> Self {
    Self {
      run_id: e.run_id,
      seq: e.seq,
      kind: e.kind,
      payload: e.payload,
      ts: e.ts,
    }
  }
}

/// Process-local broker over a sharded broadcast channel keyed by `run_id`.
///
/// Cloning is cheap — `Arc<Mutex<...>>` inside.
#[derive(Clone, Default)]
pub struct EventBroker {
  inner: Arc<Mutex<HashMap<Uuid, broadcast::Sender<StreamedEvent>>>>,
}

impl std::fmt::Debug for EventBroker {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let len = self.inner.lock().map(|g| g.len()).unwrap_or(0);
    f.debug_struct("EventBroker")
      .field("active_runs", &len)
      .finish()
  }
}

impl EventBroker {
  pub fn new() -> Self {
    Self::default()
  }

  /// Subscribe to live events for `run_id`. Creates the channel if no
  /// subscriber has registered for this run yet — that's deliberate so
  /// publishers don't need to coordinate with subscribers.
  pub fn subscribe(&self, run_id: Uuid) -> broadcast::Receiver<StreamedEvent> {
    let mut map = self.inner.lock().expect("event broker mutex poisoned");
    map
      .entry(run_id)
      .or_insert_with(|| broadcast::channel(RUN_CHANNEL_CAPACITY).0)
      .subscribe()
  }

  /// Publish without persisting. Use [`Self::publish_through`] instead when
  /// you also want a DB row — keeps the persisted log and live stream in
  /// sync.
  pub fn publish(&self, event: StreamedEvent) {
    let mut map = self.inner.lock().expect("event broker mutex poisoned");
    let sender = map
      .entry(event.run_id)
      .or_insert_with(|| broadcast::channel(RUN_CHANNEL_CAPACITY).0);
    let _ = sender.send(event);
  }

  /// Drop the channel for a finished run so it doesn't leak.
  ///
  /// Safe to call multiple times. If a subscriber is mid-flight when this
  /// is called, their `recv()` will return `Closed` after the existing
  /// queue drains — the SSE handler treats that as end-of-stream.
  pub fn finalise(&self, run_id: Uuid) {
    let mut map = self.inner.lock().expect("event broker mutex poisoned");
    map.remove(&run_id);
  }
}

/// Persist + publish an event in one shot. The DB row is the source of
/// truth; the broker is best-effort (slow subscribers may miss events,
/// they reconnect with `?after_seq=`).
pub async fn publish_through(
  repos: &Repositories,
  broker: &EventBroker,
  event: NewEvent,
) -> Result<(), DbError> {
  let stored = repos.events.append(event).await?;
  broker.publish(StreamedEvent::from(stored));
  Ok(())
}

#[derive(Debug, Deserialize)]
pub struct EventsQuery {
  /// Resume after this `seq`. SSE clients reconnecting after a network blip
  /// pass the last seq they saw to avoid duplicates and gaps.
  #[serde(default)]
  pub after_seq: Option<i64>,
}

/// `GET /v1/runs/{id}/events` — server-sent events stream.
///
/// 1. Verifies the run exists; 404s if not (clients shouldn't burn a
///    long-lived connection on a typo).
/// 2. Subscribes to the broker first so events emitted while we're still
///    setting up don't fall on the floor.
/// 3. Replays any events with `seq > after_seq` (default `-1`) from the DB.
/// 4. Forwards live broker events for as long as the channel stays open.
pub async fn stream_events(
  State(state): State<AppState>,
  Path(run_id): Path<Uuid>,
  Query(params): Query<EventsQuery>,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, ApiError> {
  let _run = state
    .repos
    .runs
    .get(run_id)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("run {} not found", run_id)))?;

  let mut after_seq = params.after_seq.unwrap_or(-1);

  let receiver = state.event_broker.subscribe(run_id);

  // Backfill from DB so resuming clients can see history. Page through to
  // avoid loading huge runs in one query — 200 per page is plenty for
  // human-paced SSE streams.
  let mut backfill: Vec<StreamedEvent> = Vec::new();
  loop {
    let page = state
      .repos
      .events
      .list_after(run_id, after_seq, 200)
      .await?;
    if page.is_empty() {
      break;
    }
    after_seq = page.last().map(|e| e.seq).unwrap_or(after_seq);
    backfill.extend(page.into_iter().map(StreamedEvent::from));
    if backfill.len() >= 1_000 {
      // Defensive cap: if we somehow have more than 1k pending events, let
      // the live stream catch the tail. The client can reconnect.
      break;
    }
  }

  let backfill_stream = futures::stream::iter(backfill).map(BrokerItem::Event);
  let live_stream = BroadcastStream::new(receiver).map(|res| match res {
    Ok(event) => BrokerItem::Event(event),
    Err(BroadcastStreamRecvError::Lagged(_)) => BrokerItem::Lagged,
  });
  let stream = backfill_stream
    .chain(live_stream)
    .map(serialise_item)
    .map(Ok::<_, Infallible>);

  Ok(
    Sse::new(stream).keep_alive(
      KeepAlive::new()
        .interval(Duration::from_secs(15))
        .text("keep-alive"),
    ),
  )
}

/// Internal stream item: a real event or a "you fell behind, reconnect" hint.
enum BrokerItem {
  Event(StreamedEvent),
  Lagged,
}

fn serialise_item(item: BrokerItem) -> Event {
  match item {
    BrokerItem::Event(event) => serialise_event(&event),
    BrokerItem::Lagged => {
      // Surface as a comment so the connection survives — clients should
      // reconnect with their last seen seq to refill from the DB.
      Event::default().comment("lagged: reconnect with ?after_seq=<last_seen>")
    }
  }
}

fn serialise_event(event: &StreamedEvent) -> Event {
  let json = serde_json::to_string(event).unwrap_or_else(|_| "{}".to_string());
  Event::default()
    .id(event.seq.to_string())
    .event(event.kind.clone())
    .data(json)
}

/// Convenience handler for the future "events as JSON list" route. Not
/// wired into the router yet but kept here so the broker can be exercised
/// from tests without an SSE client.
pub async fn list_events(
  State(state): State<AppState>,
  Path(run_id): Path<Uuid>,
  Query(params): Query<EventsQuery>,
) -> Result<Json<Vec<StreamedEvent>>, ApiError> {
  let _run = state
    .repos
    .runs
    .get(run_id)
    .await?
    .ok_or_else(|| ApiError::NotFound(format!("run {} not found", run_id)))?;

  let after_seq = params.after_seq.unwrap_or(-1);
  let events = state
    .repos
    .events
    .list_after(run_id, after_seq, 1_000)
    .await?
    .into_iter()
    .map(StreamedEvent::from)
    .collect();
  Ok(Json(events))
}

/// Trait used by the executor to publish events. Lets the route layer
/// inject a writer that goes through both the DB and the broker without
/// the executor knowing about either type directly.
#[async_trait]
pub trait EventSink: Send + Sync {
  async fn publish(&self, event: NewEvent) -> Result<(), DbError>;
}

/// Sink that writes through the DB and forwards to the broker. The
/// executor uses this; tests can substitute a fake.
#[derive(Clone)]
pub struct PersistingEventSink {
  pub repos: Repositories,
  pub broker: EventBroker,
}

#[async_trait]
impl EventSink for PersistingEventSink {
  async fn publish(&self, event: NewEvent) -> Result<(), DbError> {
    publish_through(&self.repos, &self.broker, event).await
  }
}

/// Bridge from `agentflow_core::events::WorkflowEvent` (synchronous, fired
/// inside Flow execution) to the gateway's persisted + broadcast event
/// pipeline (async).
///
/// `EventListener::on_event` is synchronous, but `EventSink::publish` is
/// async — we bridge the two with an unbounded mpsc channel and a single
/// drain task that keeps writes ordered. The listener buffers events
/// while writes catch up; writes that fail (DB hiccup) are logged and
/// dropped, since dropping a synthetic event is safer than blocking the
/// Flow scheduler. SSE subscribers can reconnect with `?after_seq=` to
/// refill from the DB if anything was dropped.
pub struct WorkflowEventListener {
  run_id: Uuid,
  tx: tokio::sync::mpsc::UnboundedSender<NewEvent>,
  seq: std::sync::atomic::AtomicI64,
  start_seq: i64,
}

impl WorkflowEventListener {
  /// Create a listener for `run_id`. The drain task owns `sink` for its
  /// lifetime; closing the channel (drop the listener) ends the task.
  ///
  /// `start_seq` is the first event sequence number to assign — pass the
  /// last seq already persisted so the listener picks up after any
  /// pre-existing rows (avoids duplicate-key violations on the
  /// `(run_id, seq)` PK).
  pub fn new(run_id: Uuid, sink: Arc<dyn EventSink>, start_seq: i64) -> Self {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<NewEvent>();
    tokio::spawn(async move {
      while let Some(event) = rx.recv().await {
        if let Err(e) = sink.publish(event).await {
          tracing::warn!(error = %e, "WorkflowEventListener: persist failed");
        }
      }
    });
    Self {
      run_id,
      tx,
      seq: std::sync::atomic::AtomicI64::new(start_seq),
      start_seq,
    }
  }

  /// Construct from the standard `Repositories` + `EventBroker` pair.
  pub fn from_state(
    run_id: Uuid,
    repos: Repositories,
    broker: EventBroker,
    start_seq: i64,
  ) -> Self {
    let sink: Arc<dyn EventSink> = Arc::new(PersistingEventSink { repos, broker });
    Self::new(run_id, sink, start_seq)
  }

  /// First sequence number this listener will assign. Useful for tests
  /// that need to predict the persisted event seq range.
  pub fn start_seq(&self) -> i64 {
    self.start_seq
  }
}

impl agentflow_core::events::EventListener for WorkflowEventListener {
  fn on_event(&self, event: &agentflow_core::events::WorkflowEvent) {
    let seq = self.seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let payload = workflow_event_payload(event);
    let kind = event.event_type().to_string();
    if let Err(e) = self.tx.send(NewEvent {
      run_id: self.run_id,
      seq,
      kind,
      payload,
    }) {
      // Drain task gone (listener was dropped). Synchronous on_event has
      // no good way to surface this — log and move on so the Flow
      // scheduler keeps making progress.
      tracing::debug!(error = %e, "WorkflowEventListener: drain task closed");
    }
  }
}

/// Convert a `WorkflowEvent` to a JSON payload suitable for the `events`
/// table. Drops `std::time::Instant` (not serialisable) and surfaces
/// duration as milliseconds; keeps `node_id`, `error`, `model`, etc. so
/// SSE subscribers can render meaningful UIs.
fn workflow_event_payload(event: &agentflow_core::events::WorkflowEvent) -> serde_json::Value {
  use agentflow_core::events::WorkflowEvent as W;
  match event {
    W::WorkflowStarted { workflow_id, .. } => serde_json::json!({"workflow_id": workflow_id}),
    W::WorkflowCompleted {
      workflow_id,
      duration,
      ..
    } => serde_json::json!({
      "workflow_id": workflow_id,
      "duration_ms": duration.as_millis() as u64,
    }),
    W::WorkflowFailed {
      workflow_id,
      error,
      duration,
      ..
    } => serde_json::json!({
      "workflow_id": workflow_id,
      "error": error,
      "duration_ms": duration.as_millis() as u64,
    }),
    W::NodeStarted {
      workflow_id,
      node_id,
      ..
    } => serde_json::json!({"workflow_id": workflow_id, "node_id": node_id}),
    W::NodeCompleted {
      workflow_id,
      node_id,
      duration,
      ..
    } => serde_json::json!({
      "workflow_id": workflow_id,
      "node_id": node_id,
      "duration_ms": duration.as_millis() as u64,
    }),
    W::NodeOutputCaptured {
      workflow_id,
      node_id,
      output,
      ..
    } => serde_json::json!({
      "workflow_id": workflow_id,
      "node_id": node_id,
      "output": output,
    }),
    W::NodeFailed {
      workflow_id,
      node_id,
      error,
      duration,
      ..
    } => serde_json::json!({
      "workflow_id": workflow_id,
      "node_id": node_id,
      "error": error,
      "duration_ms": duration.as_millis() as u64,
    }),
    W::NodeSkipped {
      workflow_id,
      node_id,
      reason,
      ..
    } => serde_json::json!({
      "workflow_id": workflow_id,
      "node_id": node_id,
      "reason": reason,
    }),
    W::CheckpointSaved {
      workflow_id,
      checkpoint_id,
      ..
    } => serde_json::json!({
      "workflow_id": workflow_id,
      "checkpoint_id": checkpoint_id,
    }),
    W::CheckpointRestored {
      workflow_id,
      checkpoint_id,
      ..
    } => serde_json::json!({
      "workflow_id": workflow_id,
      "checkpoint_id": checkpoint_id,
    }),
    W::RetryAttempt {
      workflow_id,
      node_id,
      attempt,
      max_attempts,
      ..
    } => serde_json::json!({
      "workflow_id": workflow_id,
      "node_id": node_id,
      "attempt": attempt,
      "max_attempts": max_attempts,
    }),
    W::ResourceWarning {
      workflow_id,
      resource_type,
      usage,
      limit,
      ..
    } => serde_json::json!({
      "workflow_id": workflow_id,
      "resource_type": resource_type,
      "usage": usage,
      "limit": limit,
    }),
    W::LLMPromptSent {
      workflow_id,
      node_id,
      model,
      provider,
      temperature,
      max_tokens,
      ..
    } => serde_json::json!({
      "workflow_id": workflow_id,
      "node_id": node_id,
      "model": model,
      "provider": provider,
      "temperature": temperature,
      "max_tokens": max_tokens,
    }),
    W::LLMResponseReceived {
      workflow_id,
      node_id,
      model,
      duration,
      usage,
      ..
    } => serde_json::json!({
      "workflow_id": workflow_id,
      "node_id": node_id,
      "model": model,
      "duration_ms": duration.as_millis() as u64,
      "usage": usage.as_ref().map(|u| serde_json::json!({
        "prompt_tokens": u.prompt_tokens,
        "completion_tokens": u.completion_tokens,
        "total_tokens": u.total_tokens,
      })),
    }),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::Utc;

  fn sample_event(run_id: Uuid, seq: i64) -> StreamedEvent {
    StreamedEvent {
      run_id,
      seq,
      kind: "test".into(),
      payload: serde_json::json!({"seq": seq}),
      ts: Utc::now(),
    }
  }

  #[tokio::test]
  async fn broker_subscribe_then_publish_delivers_event() {
    let broker = EventBroker::new();
    let run_id = Uuid::new_v4();
    let mut rx = broker.subscribe(run_id);
    broker.publish(sample_event(run_id, 0));
    let received = rx.recv().await.expect("event delivered");
    assert_eq!(received.seq, 0);
  }

  #[tokio::test]
  async fn broker_isolates_events_per_run_id() {
    let broker = EventBroker::new();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let mut rx_a = broker.subscribe(a);
    let _rx_b = broker.subscribe(b);
    broker.publish(sample_event(b, 0));
    // rx_a should see nothing — `try_recv` returns Empty.
    assert!(rx_a.try_recv().is_err());
  }

  #[tokio::test]
  async fn broker_finalise_closes_subscribers() {
    let broker = EventBroker::new();
    let run_id = Uuid::new_v4();
    let mut rx = broker.subscribe(run_id);
    broker.finalise(run_id);
    // After finalise the sender is dropped; recv eventually yields Closed.
    let result = rx.recv().await;
    assert!(matches!(result, Err(broadcast::error::RecvError::Closed)));
  }

  #[tokio::test]
  async fn workflow_event_listener_bridges_to_sink() {
    use agentflow_core::events::{EventListener, WorkflowEvent};
    use std::time::{Duration, Instant};

    /// In-memory sink that records every published event for assertions.
    struct CapturingSink {
      tx: tokio::sync::mpsc::UnboundedSender<NewEvent>,
    }

    #[async_trait]
    impl EventSink for CapturingSink {
      async fn publish(&self, event: NewEvent) -> Result<(), DbError> {
        self.tx.send(event).expect("test channel closed");
        Ok(())
      }
    }

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<NewEvent>();
    let sink: Arc<dyn EventSink> = Arc::new(CapturingSink { tx });
    let run_id = Uuid::new_v4();
    let listener = WorkflowEventListener::new(run_id, sink, 0);

    listener.on_event(&WorkflowEvent::WorkflowStarted {
      workflow_id: "demo".into(),
      timestamp: Instant::now(),
    });
    listener.on_event(&WorkflowEvent::NodeStarted {
      workflow_id: "demo".into(),
      node_id: "n1".into(),
      timestamp: Instant::now(),
    });
    listener.on_event(&WorkflowEvent::NodeCompleted {
      workflow_id: "demo".into(),
      node_id: "n1".into(),
      duration: Duration::from_millis(7),
      timestamp: Instant::now(),
    });

    // Drain task is async; give it a moment to flush all three events.
    let mut events = Vec::new();
    for _ in 0..3 {
      let event = tokio::time::timeout(Duration::from_millis(500), rx.recv())
        .await
        .expect("listener delivers events promptly")
        .expect("listener channel still open");
      events.push(event);
    }
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].seq, 0);
    assert_eq!(events[0].kind, "workflow.started");
    assert_eq!(events[1].kind, "node.started");
    assert_eq!(events[2].kind, "node.completed");
    assert_eq!(events[2].payload["duration_ms"], 7);
    assert_eq!(events[2].payload["node_id"], "n1");
  }

  #[test]
  fn workflow_event_payload_preserves_error_text() {
    use agentflow_core::events::WorkflowEvent;
    use std::time::{Duration, Instant};

    let event = WorkflowEvent::NodeFailed {
      workflow_id: "demo".into(),
      node_id: "n1".into(),
      error: "boom".into(),
      duration: Duration::from_millis(20),
      timestamp: Instant::now(),
    };
    let payload = workflow_event_payload(&event);
    assert_eq!(payload["error"], "boom");
    assert_eq!(payload["node_id"], "n1");
    assert_eq!(payload["duration_ms"], 20);
  }
}

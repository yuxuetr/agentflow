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
}

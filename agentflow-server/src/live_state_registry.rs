//! Process-local registry of live state-pool sizes per active run
//! (P10.14.2-FU6).
//!
//! Bridges `agentflow_core::state_size::StateSizeObserver` to the
//! Prometheus `/metrics` scrape path:
//!
//! * The DAG executor builds a [`Flow`](agentflow_core::Flow) with an
//!   observer obtained from [`LiveStateRegistry::observer_for`]. Every
//!   time a node completes, the observer writes the current estimated
//!   state-pool bytes into the registry, keyed by `run_id`.
//! * The `/metrics` scrape-time handler iterates the registry via
//!   [`LiveStateRegistry::snapshot`] and emits one
//!   `agentflow_state_size_bytes{run_id="..."}` gauge per entry.
//! * On run completion (success, failure, or cancel) the executor calls
//!   [`LiveStateRegistry::deregister`] so the gauge stops being emitted
//!   and label cardinality stays bounded.
//!
//! The registry is intentionally in-process — restart-survival isn't
//! useful for a "live" gauge: the new process has no in-flight runs to
//! report. Shared between the server's `AppState` and any spawned
//! executor task via `Clone` (cheap — the inner map is wrapped in
//! `Arc<Mutex<...>>`).

use agentflow_core::state_size::StateSizeObserver;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// In-memory map of `run_id -> last-observed state pool bytes`.
///
/// Clones share the underlying map. Read paths
/// ([`snapshot`](Self::snapshot)) are cheap — they copy the small
/// `(Uuid, u64)` pairs and drop the lock immediately. Write paths
/// ([`observer_for`](Self::observer_for)-returned observers and
/// [`deregister`](Self::deregister)) are single hash operations under
/// a short-held mutex.
#[derive(Clone, Default)]
pub struct LiveStateRegistry {
  inner: Arc<Mutex<HashMap<Uuid, u64>>>,
}

impl LiveStateRegistry {
  /// Create an empty registry.
  pub fn new() -> Self {
    Self::default()
  }

  /// Return an observer bound to `run_id`. Every
  /// [`StateSizeObserver::observe`] call updates the entry for
  /// `run_id` in-place; concurrent observers for different runs
  /// don't contend beyond the mutex hold.
  pub fn observer_for(&self, run_id: Uuid) -> Arc<dyn StateSizeObserver> {
    Arc::new(RegistryObserver {
      run_id,
      inner: self.inner.clone(),
    })
  }

  /// Drop the entry for `run_id`. Safe to call multiple times — the
  /// second call is a no-op. Called by the executor on terminal
  /// transitions (success / failure / cancel) so the gauge label
  /// cardinality only tracks currently-running runs.
  pub fn deregister(&self, run_id: &Uuid) {
    if let Ok(mut guard) = self.inner.lock() {
      guard.remove(run_id);
    }
  }

  /// Take a snapshot of `(run_id, bytes)` for every active entry.
  /// Holds the mutex only long enough to copy out the entries; the
  /// returned `Vec` is detached so the caller can iterate without
  /// blocking observer writes.
  pub fn snapshot(&self) -> Vec<(Uuid, u64)> {
    self
      .inner
      .lock()
      .map(|guard| guard.iter().map(|(k, v)| (*k, *v)).collect())
      .unwrap_or_default()
  }

  /// Test-only: read the current entry count.
  #[cfg(test)]
  #[allow(clippy::len_without_is_empty)]
  pub fn len(&self) -> usize {
    self.inner.lock().map(|g| g.len()).unwrap_or(0)
  }
}

struct RegistryObserver {
  run_id: Uuid,
  inner: Arc<Mutex<HashMap<Uuid, u64>>>,
}

impl StateSizeObserver for RegistryObserver {
  fn observe(&self, bytes: u64) {
    // Silently drop the sample on a poisoned mutex — losing one
    // gauge update is preferable to crashing the executor task.
    if let Ok(mut guard) = self.inner.lock() {
      guard.insert(self.run_id, bytes);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn snapshot_starts_empty() {
    let reg = LiveStateRegistry::new();
    assert!(reg.snapshot().is_empty());
    assert_eq!(reg.len(), 0);
  }

  #[test]
  fn observer_records_under_its_run_id() {
    let reg = LiveStateRegistry::new();
    let run = Uuid::new_v4();
    reg.observer_for(run).observe(1024);
    let snap = reg.snapshot();
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0], (run, 1024));
  }

  #[test]
  fn multiple_observers_isolate_per_run_id() {
    let reg = LiveStateRegistry::new();
    let r1 = Uuid::new_v4();
    let r2 = Uuid::new_v4();
    reg.observer_for(r1).observe(10);
    reg.observer_for(r2).observe(20);
    let snap: HashMap<Uuid, u64> = reg.snapshot().into_iter().collect();
    assert_eq!(snap.get(&r1).copied(), Some(10));
    assert_eq!(snap.get(&r2).copied(), Some(20));
  }

  #[test]
  fn observe_overwrites_prior_sample_same_run_id() {
    let reg = LiveStateRegistry::new();
    let run = Uuid::new_v4();
    let obs = reg.observer_for(run);
    obs.observe(100);
    obs.observe(250);
    obs.observe(50); // gauge tracks absolute value, can shrink too
    let snap = reg.snapshot();
    assert_eq!(snap, vec![(run, 50)]);
  }

  #[test]
  fn deregister_removes_entry_and_is_idempotent() {
    let reg = LiveStateRegistry::new();
    let run = Uuid::new_v4();
    reg.observer_for(run).observe(42);
    assert_eq!(reg.len(), 1);
    reg.deregister(&run);
    assert_eq!(reg.len(), 0);
    // Second deregister is a no-op, must not panic.
    reg.deregister(&run);
    assert_eq!(reg.len(), 0);
  }

  #[test]
  fn cloned_registries_share_state() {
    let reg = LiveStateRegistry::new();
    let clone = reg.clone();
    let run = Uuid::new_v4();
    reg.observer_for(run).observe(7);
    // The clone sees the same entry — the inner Arc<Mutex<...>> is
    // shared, which is the invariant AppState relies on.
    assert_eq!(clone.snapshot(), vec![(run, 7)]);
  }
}

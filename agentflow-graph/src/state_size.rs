//! Live state-pool size observation (P10.14.2-FU6).
//!
//! The DAG executor owns a `HashMap<String, AsyncNodeResult>` state pool as
//! a local variable for the duration of a single run. No persistent registry
//! holds the in-flight contents, so external observers (e.g. the server's
//! Prometheus `/metrics` handler) cannot ask `Flow` for the current size
//! after the fact. This module provides the indirection: the executor calls
//! [`StateSizeObserver::observe`] with the estimated state-pool byte count
//! after every node completes, and embedders attach an observer when they
//! need scrape-time visibility.
//!
//! Observers are best-effort. A misbehaving observer must not crash the
//! executor — implementations should swallow their own errors. The executor
//! itself does not interpret the byte count; it is purely operator-facing
//! telemetry.

use crate::async_node::AsyncNodeResult;
use std::collections::HashMap;

/// Sink for live state-pool size samples.
///
/// Called by the DAG executor after each node completes (both serial and
/// concurrent execution paths). The `bytes` argument is the
/// [`estimated_state_pool_bytes`] of the post-step state pool — see that
/// helper for the exact accounting rules.
pub trait StateSizeObserver: Send + Sync {
  fn observe(&self, bytes: u64);
}

/// Sum the [`crate::value::FlowValue::estimated_size_bytes`] of every entry
/// in a state pool, plus each key string's length, plus each result's outer
/// entry key.
///
/// Failed `AsyncNodeResult` entries (the `Err` arm) contribute only their
/// outer key length — the executor never persists the error message into
/// the pool's serialized state, so counting the `AgentFlowError` would
/// overstate the on-the-wire footprint a future serializer would see.
///
/// `saturating_add` is used throughout so a pathologically large pool
/// doesn't wrap a `u64`; the observer just sees `u64::MAX`.
pub fn estimated_state_pool_bytes(pool: &HashMap<String, AsyncNodeResult>) -> u64 {
  let mut total: u64 = 0;
  for (key, result) in pool {
    total = total.saturating_add(key.len() as u64);
    if let Ok(map) = result {
      for (inner_key, value) in map {
        total = total.saturating_add(inner_key.len() as u64);
        total = total.saturating_add(value.estimated_size_bytes() as u64);
      }
    }
  }
  total
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::error::AgentFlowError;
  use crate::value::FlowValue;
  use serde_json::json;
  use std::sync::Arc;
  use std::sync::Mutex;

  #[test]
  fn estimated_state_pool_bytes_empty_pool_is_zero() {
    let pool: HashMap<String, AsyncNodeResult> = HashMap::new();
    assert_eq!(estimated_state_pool_bytes(&pool), 0);
  }

  #[test]
  fn estimated_state_pool_bytes_sums_keys_and_values() {
    let mut pool: HashMap<String, AsyncNodeResult> = HashMap::new();
    let mut outputs = HashMap::new();
    outputs.insert("out".to_string(), FlowValue::Json(json!("hi")));
    pool.insert("node_a".to_string(), Ok(outputs));

    // node_a (6) + out (3) + json "hi" (4 bytes serialised: "\"hi\"")
    let expected = 6 + 3 + serde_json::to_vec(&json!("hi")).unwrap().len() as u64;
    assert_eq!(estimated_state_pool_bytes(&pool), expected);
  }

  #[test]
  fn estimated_state_pool_bytes_skips_error_results_payloads() {
    let mut pool: HashMap<String, AsyncNodeResult> = HashMap::new();
    pool.insert("errored_node".to_string(), Err(AgentFlowError::NodeSkipped));
    // Only the outer key length contributes.
    assert_eq!(
      estimated_state_pool_bytes(&pool),
      "errored_node".len() as u64
    );
  }

  #[derive(Default)]
  struct RecordingObserver {
    samples: Mutex<Vec<u64>>,
  }

  impl StateSizeObserver for RecordingObserver {
    fn observe(&self, bytes: u64) {
      self.samples.lock().unwrap().push(bytes);
    }
  }

  #[test]
  fn observer_trait_is_object_safe_and_receives_samples() {
    let recording = Arc::new(RecordingObserver::default());
    let obs: Arc<dyn StateSizeObserver> = recording.clone();
    obs.observe(42);
    obs.observe(100);
    assert_eq!(*recording.samples.lock().unwrap(), vec![42, 100]);
  }
}

//! The Harness event-sink contract.
//!
//! The trait the Harness runtime fans its event stream out to; concrete
//! sinks (`JsonlEventSink`, `StdoutEventSink`, `InMemoryEventSink`,
//! `SinkChain`) live in `agentflow-harness::persistence`.

use async_trait::async_trait;

use crate::HarnessError;
use crate::HarnessEvent;

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

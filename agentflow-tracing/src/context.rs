//! Cross-hop W3C trace context propagation helpers (P3.8).
//!
//! Several AgentFlow boundaries can carry a `traceparent` to keep an
//! OpenTelemetry trace stitched across process and protocol hops:
//!
//! - LLM HTTP calls (already wired in `agentflow_llm::trace_context`).
//! - Subprocess plugin spawns (this slice; injected as the `TRACEPARENT`
//!   env var per the W3C convention).
//! - MCP transport JSON-RPC envelope (planned follow-up; the protocol's
//!   `meta` field is the carrier).
//! - Worker gRPC metadata (planned follow-up).
//!
//! Producers install a context via [`scope`] (typically per-run) and
//! consumers call [`current_traceparent`] right before they emit the
//! outbound carrier. When no context is active, [`current_traceparent`]
//! returns `None` and consumers MUST NOT emit a `TRACEPARENT` /
//! `traceparent` field — propagating an empty header would mask the
//! "no upstream context" case.
//!
//! See [`docs/TRACE_PERSISTENCE_SCHEMA.md`](../../docs/TRACE_PERSISTENCE_SCHEMA.md)
//! for the wire-level "Hop continuity" rules.

use std::future::Future;

tokio::task_local! {
  /// Active W3C `traceparent` header value for the current task.
  /// Stored as the raw string (`00-<trace>-<span>-<flags>`) so every
  /// consumer can drop it straight into an env var, gRPC metadata
  /// entry, or HTTP header without re-formatting.
  static CURRENT_TRACEPARENT: String;
}

/// Install `traceparent` for the duration of `fut`. The header value is
/// stored verbatim; callers that need it formatted should do so before
/// calling `scope` (e.g. via
/// `agentflow_llm::trace_context::LlmTraceContext::to_traceparent()`).
pub async fn scope<F, T>(traceparent: String, fut: F) -> T
where
  F: Future<Output = T>,
{
  CURRENT_TRACEPARENT.scope(traceparent, fut).await
}

/// Return a clone of the active `traceparent` header, or `None` if
/// nothing is in scope.
pub fn current_traceparent() -> Option<String> {
  CURRENT_TRACEPARENT.try_with(|c| c.clone()).ok()
}

/// Canonical env var consumers (notably subprocess plugins) read to
/// pick up the parent traceparent. Matches the W3C-suggested name so
/// runtimes that already speak it (OpenTelemetry SDKs, jaegertracing
/// reference impls) work without translation.
pub const TRACEPARENT_ENV: &str = "TRACEPARENT";

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn scope_installs_value_for_inner_future() {
    let observed = scope(
      "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01".to_string(),
      async { current_traceparent() },
    )
    .await;
    assert_eq!(
      observed.as_deref(),
      Some("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01")
    );
  }

  #[tokio::test]
  async fn outside_scope_returns_none() {
    assert!(current_traceparent().is_none());
  }

  #[tokio::test]
  async fn nested_scopes_shadow_outer_value() {
    let (outer_seen, inner_seen, after_inner) = scope("outer".to_string(), async {
      let outer_seen = current_traceparent();
      let inner_seen = scope("inner".to_string(), async { current_traceparent() }).await;
      let after_inner = current_traceparent();
      (outer_seen, inner_seen, after_inner)
    })
    .await;
    assert_eq!(outer_seen.as_deref(), Some("outer"));
    assert_eq!(inner_seen.as_deref(), Some("inner"));
    assert_eq!(after_inner.as_deref(), Some("outer"));
  }

  #[test]
  fn env_constant_is_w3c_canonical() {
    // Pinned so a downstream consumer can rely on the exact spelling.
    // Bumping this is a breaking change that requires updating every
    // sink that reads the env var.
    assert_eq!(TRACEPARENT_ENV, "TRACEPARENT");
  }
}

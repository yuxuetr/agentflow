//! W3C Trace Context propagation for outbound LLM HTTP calls.
//!
//! When an agent or workflow runs, the surrounding execution forms an
//! OpenTelemetry trace tree. Each LLM HTTP call is one outbound hop in that
//! tree; without a `traceparent` header, OTel-aware servers and proxies
//! cannot link their own spans back to the AgentFlow run, so the trace
//! breaks at the LLM boundary.
//!
//! This module gives callers two things:
//!
//! 1. [`LlmTraceContext`] — re-exported from `agentflow-value` (R1.1,
//!    2026-07-28: moved there so L0 contract crates like
//!    `agentflow-agent-spi` can carry a trace context without depending on
//!    this crate) — round-trips through the W3C
//!    [`traceparent`](https://www.w3.org/TR/trace-context/#traceparent-header)
//!    format.
//! 2. A tokio task-local (`tokio::task_local!`) so the active context flows through
//!    `await` points without explicit plumbing — set it once around an
//!    [`crate::LLMClient::execute`] call and every provider's
//!    `build_headers` will pick it up automatically.
//!
//! The module is opt-in: when no context is in scope, providers add no
//! tracing header and behaviour is identical to v0.2.0.

use std::future::Future;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

pub use agentflow_value::LlmTraceContext;

const TRACEPARENT_HEADER: &str = "traceparent";
const TRACESTATE_HEADER: &str = "tracestate";

tokio::task_local! {
  static CURRENT: LlmTraceContext;
}

/// Run `fut` with `ctx` installed as the active [`LlmTraceContext`].
///
/// Anything inside `fut` that calls [`current`] will observe `ctx`. Nesting
/// is supported — the inner scope shadows the outer for its duration.
pub async fn scope<F, T>(ctx: LlmTraceContext, fut: F) -> T
where
  F: Future<Output = T>,
{
  CURRENT.scope(ctx, fut).await
}

/// Return a clone of the active context, or `None` if there is none.
pub fn current() -> Option<LlmTraceContext> {
  CURRENT.try_with(|c| c.clone()).ok()
}

/// Inject `traceparent` (and `tracestate` if non-empty) into `headers` if
/// there is an active context. Existing entries with the same key are
/// replaced; this matches W3C semantics (forward, don't accumulate).
pub fn inject_into_headers(headers: &mut HeaderMap) {
  if let Some(ctx) = current() {
    inject_context_into_headers(&ctx, headers);
  }
}

/// Same as [`inject_into_headers`] but uses an explicit context. Used by
/// tests that set the header without a task-local installed.
pub fn inject_context_into_headers(ctx: &LlmTraceContext, headers: &mut HeaderMap) {
  if let Ok(value) = HeaderValue::from_str(&ctx.to_traceparent()) {
    headers.insert(HeaderName::from_static(TRACEPARENT_HEADER), value);
  }
  if !ctx.tracestate.is_empty()
    && let Ok(value) = HeaderValue::from_str(&ctx.tracestate)
  {
    headers.insert(HeaderName::from_static(TRACESTATE_HEADER), value);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  // R1.1: pure-data round-trip tests for LlmTraceContext (new/random/
  // traceparent parsing) moved to agentflow-value/src/trace_context.rs
  // alongside the type itself. What's left here exercises this crate's
  // own surface: the tokio task-local scope + HTTP header injection.

  #[tokio::test]
  async fn scope_installs_task_local() {
    let outer = LlmTraceContext::random();
    let observed = scope(outer.clone(), async { current() }).await;
    assert_eq!(observed.as_ref(), Some(&outer));

    // Outside the scope, current() is None again.
    assert!(current().is_none());
  }

  #[tokio::test]
  async fn nested_scopes_shadow_outer_context() {
    let outer = LlmTraceContext::random();
    let inner = LlmTraceContext::random();
    let (in_outer, in_inner, after_inner) = scope(outer.clone(), async {
      let in_outer = current();
      let in_inner = scope(inner.clone(), async { current() }).await;
      let after_inner = current();
      (in_outer, in_inner, after_inner)
    })
    .await;

    assert_eq!(in_outer, Some(outer.clone()));
    assert_eq!(in_inner, Some(inner));
    assert_eq!(after_inner, Some(outer));
  }

  #[tokio::test]
  async fn inject_into_headers_writes_traceparent_when_active() {
    let ctx = LlmTraceContext::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331").unwrap();

    let header_value = scope(ctx.clone(), async {
      let mut headers = HeaderMap::new();
      inject_into_headers(&mut headers);
      headers
        .get(TRACEPARENT_HEADER)
        .and_then(|v| v.to_str().ok().map(str::to_string))
    })
    .await;

    assert_eq!(header_value.as_deref(), Some(ctx.to_traceparent().as_str()));
  }

  #[tokio::test]
  async fn inject_is_noop_when_no_context_active() {
    let mut headers = HeaderMap::new();
    inject_into_headers(&mut headers);
    assert!(headers.get(TRACEPARENT_HEADER).is_none());
  }

  #[test]
  fn tracestate_round_trips_when_present() {
    let ctx = LlmTraceContext::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331")
      .unwrap()
      .with_tracestate("rojo=00f067aa0ba902b7,congo=t61rcWkgMzE");
    let mut headers = HeaderMap::new();
    inject_context_into_headers(&ctx, &mut headers);

    assert_eq!(
      headers.get(TRACESTATE_HEADER).and_then(|v| v.to_str().ok()),
      Some("rojo=00f067aa0ba902b7,congo=t61rcWkgMzE")
    );
  }
}

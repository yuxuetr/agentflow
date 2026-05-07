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
//! 1. A typed [`LlmTraceContext`] that round-trips through the W3C
//!    [`traceparent`](https://www.w3.org/TR/trace-context/#traceparent-header)
//!    format.
//! 2. A tokio [task-local](task_local) so the active context flows through
//!    `await` points without explicit plumbing — set it once around an
//!    [`crate::LLMClient::execute`] call and every provider's
//!    `build_headers` will pick it up automatically.
//!
//! The module is opt-in: when no context is in scope, providers add no
//! tracing header and behaviour is identical to v0.2.0.

use std::future::Future;

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const TRACEPARENT_HEADER: &str = "traceparent";
const TRACESTATE_HEADER: &str = "tracestate";

/// W3C Trace Context for one outbound LLM HTTP call.
///
/// `trace_id` is 16 bytes (32 hex chars), `span_id` is 8 bytes (16 hex chars).
/// `flags` is a single byte (typically `0x01` for "sampled"). Optional
/// `tracestate` is opaque to AgentFlow; we propagate it verbatim if set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmTraceContext {
  /// 32 lowercase hex characters.
  pub trace_id: String,
  /// 16 lowercase hex characters. Identifies the span that *issued* this
  /// outbound call; receivers will create child spans of this id.
  pub span_id: String,
  /// W3C trace flags, default `0x01` (sampled).
  #[serde(default = "default_flags")]
  pub flags: u8,
  /// Opaque vendor-defined state. Empty string means absent.
  #[serde(default, skip_serializing_if = "String::is_empty")]
  pub tracestate: String,
}

fn default_flags() -> u8 {
  0x01
}

impl LlmTraceContext {
  /// Construct a context from raw hex-encoded ids. Returns `None` if either
  /// id is malformed.
  pub fn new(trace_id: impl Into<String>, span_id: impl Into<String>) -> Option<Self> {
    let trace_id = trace_id.into();
    let span_id = span_id.into();
    if !is_lower_hex(&trace_id, 32) || !is_lower_hex(&span_id, 16) {
      return None;
    }
    if trace_id.bytes().all(|b| b == b'0') || span_id.bytes().all(|b| b == b'0') {
      // W3C: an all-zero id is invalid.
      return None;
    }
    Some(Self {
      trace_id,
      span_id,
      flags: default_flags(),
      tracestate: String::new(),
    })
  }

  /// Generate a fresh context with random ids and `flags = 0x01`.
  ///
  /// Uses two UUIDv4s as entropy sources. UUIDv4 has 122 random bits which
  /// is more than enough for 128-bit trace ids and 64-bit span ids.
  pub fn random() -> Self {
    let mut trace_bytes = *Uuid::new_v4().as_bytes();
    let span_bytes_full = *Uuid::new_v4().as_bytes();
    let mut span_bytes = [0u8; 8];
    span_bytes.copy_from_slice(&span_bytes_full[..8]);
    // W3C requires non-zero ids. The probability of all-zero from UUIDv4 is
    // negligible, but defend against it deterministically.
    if trace_bytes.iter().all(|b| *b == 0) {
      trace_bytes[0] = 1;
    }
    if span_bytes.iter().all(|b| *b == 0) {
      span_bytes[0] = 1;
    }
    Self {
      trace_id: hex_lower(&trace_bytes),
      span_id: hex_lower(&span_bytes),
      flags: default_flags(),
      tracestate: String::new(),
    }
  }

  /// Replace the `tracestate` propagation key.
  pub fn with_tracestate(mut self, state: impl Into<String>) -> Self {
    self.tracestate = state.into();
    self
  }

  /// Override the trace flags.
  pub fn with_flags(mut self, flags: u8) -> Self {
    self.flags = flags;
    self
  }

  /// Format as a `traceparent` header value (`00-<trace>-<span>-<flags>`).
  pub fn to_traceparent(&self) -> String {
    format!("00-{}-{}-{:02x}", self.trace_id, self.span_id, self.flags)
  }

  /// Parse a `traceparent` header value. Accepts only version `00`.
  pub fn from_traceparent(value: &str) -> Option<Self> {
    let parts: Vec<&str> = value.split('-').collect();
    if parts.len() != 4 || parts[0] != "00" {
      return None;
    }
    let trace_id = parts[1].to_string();
    let span_id = parts[2].to_string();
    let flags = u8::from_str_radix(parts[3], 16).ok()?;
    let mut ctx = Self::new(trace_id, span_id)?;
    ctx.flags = flags;
    Some(ctx)
  }
}

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

fn is_lower_hex(s: &str, expected_len: usize) -> bool {
  s.len() == expected_len && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn hex_lower(bytes: &[u8]) -> String {
  let mut out = String::with_capacity(bytes.len() * 2);
  for byte in bytes {
    use std::fmt::Write as _;
    let _ = write!(out, "{:02x}", byte);
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn new_rejects_malformed_ids() {
    assert!(LlmTraceContext::new("not-hex", "0123456789abcdef").is_none());
    assert!(LlmTraceContext::new("0".repeat(32), "0123456789abcdef").is_none());
    assert!(LlmTraceContext::new("ABCDEFabcdef0123456789abcdef0123", "0123456789abcdef").is_none());
  }

  #[test]
  fn random_yields_well_formed_lowercase_ids() {
    let ctx = LlmTraceContext::random();
    assert_eq!(ctx.trace_id.len(), 32);
    assert_eq!(ctx.span_id.len(), 16);
    assert!(ctx.trace_id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    assert!(ctx.span_id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
  }

  #[test]
  fn traceparent_round_trips() {
    let ctx = LlmTraceContext::new(
      "0af7651916cd43dd8448eb211c80319c",
      "b7ad6b7169203331",
    )
    .unwrap();
    let header = ctx.to_traceparent();
    assert_eq!(header, "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01");

    let parsed = LlmTraceContext::from_traceparent(&header).unwrap();
    assert_eq!(parsed, ctx);
  }

  #[test]
  fn from_traceparent_rejects_unsupported_version() {
    assert!(
      LlmTraceContext::from_traceparent(
        "ff-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
      )
      .is_none()
    );
  }

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
    let ctx = LlmTraceContext::new(
      "0af7651916cd43dd8448eb211c80319c",
      "b7ad6b7169203331",
    )
    .unwrap();

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
    let ctx = LlmTraceContext::new(
      "0af7651916cd43dd8448eb211c80319c",
      "b7ad6b7169203331",
    )
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

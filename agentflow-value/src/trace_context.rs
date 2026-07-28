//! W3C Trace Context data type.
//!
//! `LlmTraceContext` is a plain [W3C `traceparent`](https://www.w3.org/TR/trace-context/#traceparent-header)
//! value: `trace_id` / `span_id` / `flags` / `tracestate`. It lives here (an
//! L0 leaf with no runtime/transport dependencies) rather than in
//! `agentflow-llm` because contract types like `AgentContext`
//! (`agentflow-agent-spi`) need to carry a trace context without depending
//! on the LLM crate — R1.1 (2026-07-28): `agentflow-agent-spi` previously
//! depended on `agentflow_llm::LlmTraceContext` directly, which is an L0→L2
//! edge that violates the contract-kernel dependency rules in
//! `docs/RFC_CRATE_ARCHITECTURE.md` §7 ("contract crates depend only
//! downward to `value`"). `agentflow-llm` re-exports this type from
//! `agentflow_llm::LlmTraceContext` (same name, same shape) so every
//! existing call site keeps compiling unchanged; it also keeps the
//! task-local `scope`/`current`/`inject_into_headers` helpers that touch
//! `tokio`/`reqwest`, which stay LLM-transport concerns and have no reason
//! to live in a dependency-free contract crate.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// W3C Trace Context for one outbound hop (originally: an LLM HTTP call).
///
/// `trace_id` is 16 bytes (32 hex chars), `span_id` is 8 bytes (16 hex chars).
/// `flags` is a single byte (typically `0x01` for "sampled"). Optional
/// `tracestate` is opaque to AgentFlow; callers propagate it verbatim if set.
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

fn is_lower_hex(s: &str, expected_len: usize) -> bool {
  s.len() == expected_len
    && s
      .bytes()
      .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
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
    assert!(
      ctx
        .trace_id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    );
    assert!(
      ctx
        .span_id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
    );
  }

  #[test]
  fn traceparent_round_trips() {
    let ctx = LlmTraceContext::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331").unwrap();
    let header = ctx.to_traceparent();
    assert_eq!(
      header,
      "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
    );

    let parsed = LlmTraceContext::from_traceparent(&header).unwrap();
    assert_eq!(parsed, ctx);
  }

  #[test]
  fn from_traceparent_rejects_unsupported_version() {
    assert!(
      LlmTraceContext::from_traceparent("ff-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01")
        .is_none()
    );
  }

  #[test]
  fn tracestate_round_trips_when_present() {
    let ctx = LlmTraceContext::new("0af7651916cd43dd8448eb211c80319c", "b7ad6b7169203331")
      .unwrap()
      .with_tracestate("rojo=00f067aa0ba902b7,congo=t61rcWkgMzE");
    assert_eq!(ctx.tracestate, "rojo=00f067aa0ba902b7,congo=t61rcWkgMzE");
  }
}

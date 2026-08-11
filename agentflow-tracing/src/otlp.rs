//! First-party OTLP/HTTP span exporter (W4.4).
//!
//! Implements [`crate::otel::OtelSpanSink`] on top of the official
//! `opentelemetry-otlp` crate's HTTP+JSON transport, so the wire encoding
//! is spec-correct by construction rather than hand-matched against the
//! OTLP JSON schema (base64-encoded trace/span IDs, `AnyValue` tagged
//! unions, etc. — easy to get subtly wrong by hand). This is deliberately
//! feature-gated (`otlp-http`) and additive: every existing `OtelSpanSink`
//! caller (including a hand-rolled one) keeps working unchanged; this is
//! just the first in-tree implementation of that trait.
//!
//! Only HTTP transport is implemented, matching the "HTTP first" scope —
//! gRPC transport is a separate, not-yet-decided follow-up.

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use opentelemetry::trace::{
  Event, SpanContext, SpanId, SpanKind, Status, TraceFlags, TraceId, TraceState,
};
use opentelemetry::{InstrumentationScope, KeyValue, Value};
use opentelemetry_otlp::{Protocol, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::trace::{SpanData, SpanEvents, SpanExporter as SdkSpanExporter};

use crate::otel::{OtelSpan, OtelSpanKind, OtelSpanSink, OtelStatusCode, OtelValue};

/// Endpoint/headers/timeout for the OTLP/HTTP exporter. Mirrors the
/// OpenTelemetry SDK's own env-var convention
/// (`OTEL_EXPORTER_OTLP_ENDPOINT` / `OTEL_EXPORTER_OTLP_HEADERS`) via
/// [`OtlpHttpConfig::from_env`] so operators can configure this the same
/// way they'd configure any other OTel exporter, without AgentFlow-specific
/// env vars.
#[derive(Debug, Clone)]
pub struct OtlpHttpConfig {
  /// Collector base URL, e.g. `http://localhost:4318`. The `/v1/traces`
  /// signal path is appended by `opentelemetry-otlp` itself.
  pub endpoint: String,
  /// Extra headers sent with every export request (e.g. an API key).
  pub headers: HashMap<String, String>,
  /// Per-export HTTP timeout. Independent of, and typically shorter than,
  /// `TraceConfig::exporter_timeout` (the collector-side deadline around
  /// the whole `TraceExporter::export_trace` call).
  pub timeout: Duration,
}

impl OtlpHttpConfig {
  pub fn new(endpoint: impl Into<String>) -> Self {
    Self {
      endpoint: endpoint.into(),
      headers: HashMap::new(),
      timeout: Duration::from_secs(10),
    }
  }

  pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
    self.headers.insert(key.into(), value.into());
    self
  }

  pub fn with_timeout(mut self, timeout: Duration) -> Self {
    self.timeout = timeout;
    self
  }

  /// Read `OTEL_EXPORTER_OTLP_ENDPOINT` (required — `None` if unset) and
  /// `OTEL_EXPORTER_OTLP_HEADERS` (optional, `key1=value1,key2=value2`
  /// comma-separated, matching the OTel SDK spec's header env-var format).
  pub fn from_env() -> Option<Self> {
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
      .ok()
      .filter(|v| !v.trim().is_empty())?;
    let mut config = Self::new(endpoint);
    if let Ok(raw_headers) = std::env::var("OTEL_EXPORTER_OTLP_HEADERS") {
      for pair in raw_headers.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
          continue;
        }
        if let Some((key, value)) = pair.split_once('=') {
          config = config.with_header(key.trim(), value.trim());
        }
      }
    }
    Some(config)
  }
}

/// [`OtelSpanSink`] backed by `opentelemetry_otlp::SpanExporter`'s HTTP+JSON
/// transport. AgentFlow's own `TraceCollector` already owns batching (one
/// export per completed `ExecutionTrace`, via [`crate::otel::OtelTraceExporter`])
/// and the per-export timeout (`TraceConfig::exporter_timeout`), so this
/// sink calls the SDK exporter's `export` directly rather than routing
/// through a full `SdkTracerProvider` — no OTel SDK batch processor,
/// sampler, or global tracer registration involved.
pub struct OtlpHttpSpanSink {
  inner: opentelemetry_otlp::SpanExporter,
}

impl OtlpHttpSpanSink {
  pub fn new(config: OtlpHttpConfig) -> Result<Self, anyhow::Error> {
    let mut builder = opentelemetry_otlp::SpanExporter::builder()
      .with_http()
      .with_protocol(Protocol::HttpJson)
      .with_endpoint(&config.endpoint)
      .with_timeout(config.timeout);
    if !config.headers.is_empty() {
      builder = builder.with_headers(config.headers.clone());
    }
    let inner = builder
      .build()
      .map_err(|err| anyhow::anyhow!("failed to build OTLP/HTTP span exporter: {err}"))?;
    Ok(Self { inner })
  }
}

#[async_trait]
impl OtelSpanSink for OtlpHttpSpanSink {
  async fn export_spans(&self, spans: Vec<OtelSpan>) -> Result<(), anyhow::Error> {
    if spans.is_empty() {
      return Ok(());
    }
    let batch: Vec<SpanData> = spans.into_iter().map(to_span_data).collect();
    self
      .inner
      .export(batch)
      .await
      .map_err(|err| anyhow::anyhow!("OTLP/HTTP export failed: {err}"))
  }
}

/// Convert AgentFlow's transport-agnostic [`OtelSpan`] into the OTel SDK's
/// [`SpanData`], which `opentelemetry_otlp::SpanExporter::export` consumes.
fn to_span_data(span: OtelSpan) -> SpanData {
  let trace_id = parse_trace_id(&span.trace_id);
  let span_id = parse_span_id(&span.span_id);
  let parent_span_id = span
    .parent_span_id
    .as_deref()
    .map(parse_span_id)
    .unwrap_or(SpanId::INVALID);

  let span_context = SpanContext::new(
    trace_id,
    span_id,
    TraceFlags::SAMPLED,
    // `is_remote` describes whether *this* span's context was propagated
    // in from a remote process — always `false` here since every span
    // AgentFlow emits originates in-process; only the *trace*'s root may
    // have an external parent (`parent_span_id` above), which is a
    // separate concept from span-context remoteness.
    false,
    TraceState::default(),
  );

  let mut events = SpanEvents::default();
  events.events = span
    .events
    .into_iter()
    .map(|event| {
      Event::new(
        event.name,
        std::time::UNIX_EPOCH + Duration::from_nanos(event.time_unix_nano),
        event.attributes.into_iter().map(to_key_value).collect(),
        0,
      )
    })
    .collect();

  SpanData {
    span_context,
    parent_span_id,
    parent_span_is_remote: false,
    span_kind: to_span_kind(&span.kind),
    name: span.name.into(),
    start_time: std::time::UNIX_EPOCH + Duration::from_nanos(span.start_time_unix_nano),
    end_time: std::time::UNIX_EPOCH + Duration::from_nanos(span.end_time_unix_nano),
    attributes: span.attributes.into_iter().map(to_key_value).collect(),
    dropped_attributes_count: 0,
    events,
    links: Default::default(),
    status: to_status(span.status.code, span.status.message),
    instrumentation_scope: InstrumentationScope::builder("agentflow-tracing")
      .with_version(env!("CARGO_PKG_VERSION"))
      .build(),
  }
}

fn to_span_kind(kind: &OtelSpanKind) -> SpanKind {
  match kind {
    OtelSpanKind::Internal => SpanKind::Internal,
    OtelSpanKind::Client => SpanKind::Client,
  }
}

fn to_key_value(attr: crate::otel::OtelAttribute) -> KeyValue {
  let value = match attr.value {
    OtelValue::String(s) => Value::String(s.into()),
    OtelValue::Bool(b) => Value::Bool(b),
    OtelValue::I64(i) => Value::I64(i),
  };
  KeyValue::new(attr.key, value)
}

fn to_status(code: OtelStatusCode, message: Option<String>) -> Status {
  match code {
    OtelStatusCode::Unset => Status::Unset,
    OtelStatusCode::Ok => Status::Ok,
    OtelStatusCode::Error => Status::error(message.unwrap_or_default()),
  }
}

/// Parse a lowercase-hex trace_id (32 chars = 16 bytes) into a
/// [`TraceId`]. Falls back to [`TraceId::INVALID`] on malformed input
/// (defensive — every producer inside this crate emits W3C-compliant hex
/// via `otel::random_hex_id`, but an exporter must not panic on
/// unexpected input).
fn parse_trace_id(hex: &str) -> TraceId {
  let mut bytes = [0u8; 16];
  if hex_to_bytes(hex, &mut bytes) {
    TraceId::from_bytes(bytes)
  } else {
    TraceId::INVALID
  }
}

fn parse_span_id(hex: &str) -> SpanId {
  let mut bytes = [0u8; 8];
  if hex_to_bytes(hex, &mut bytes) {
    SpanId::from_bytes(bytes)
  } else {
    SpanId::INVALID
  }
}

fn hex_to_bytes(hex: &str, out: &mut [u8]) -> bool {
  if hex.len() != out.len() * 2 {
    return false;
  }
  for (i, byte) in out.iter_mut().enumerate() {
    let Ok(parsed) = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16) else {
      return false;
    };
    *byte = parsed;
  }
  true
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::otel::{OtelAttribute, OtelSpanEvent, OtelStatus};

  fn sample_span() -> OtelSpan {
    OtelSpan {
      trace_id: "0102030405060708090a0b0c0d0e0f10".to_string(),
      span_id: "0102030405060708".to_string(),
      parent_span_id: Some("1112131415161718".to_string()),
      name: "test.span".to_string(),
      kind: OtelSpanKind::Client,
      start_time_unix_nano: 1_000_000_000,
      end_time_unix_nano: 2_000_000_000,
      attributes: vec![
        OtelAttribute::string("k1", "v1"),
        OtelAttribute::bool("k2", true),
        OtelAttribute::i64("k3", 42),
      ],
      events: vec![OtelSpanEvent {
        name: "ev".to_string(),
        time_unix_nano: 1_500_000_000,
        attributes: vec![],
      }],
      status: OtelStatus::error("boom"),
    }
  }

  #[test]
  fn to_span_data_round_trips_ids_and_timing() {
    let span = sample_span();
    let data = to_span_data(span.clone());
    assert_eq!(
      data.span_context.trace_id().to_string(),
      "0102030405060708090a0b0c0d0e0f10"
    );
    assert_eq!(data.span_context.span_id().to_string(), "0102030405060708");
    assert_eq!(data.parent_span_id.to_string(), "1112131415161718");
    assert_eq!(data.name, "test.span");
    assert_eq!(data.attributes.len(), 3);
    assert_eq!(data.events.events.len(), 1);
    assert!(matches!(data.status, Status::Error { .. }));
  }

  #[test]
  fn malformed_ids_fall_back_to_invalid_rather_than_panicking() {
    let mut span = sample_span();
    span.trace_id = "not-hex".to_string();
    span.span_id = "also-not-hex".to_string();
    let data = to_span_data(span);
    assert_eq!(data.span_context.trace_id(), TraceId::INVALID);
    assert_eq!(data.span_context.span_id(), SpanId::INVALID);
  }

  /// Single test function (not several `#[test]`s) so the
  /// `OTEL_EXPORTER_OTLP_ENDPOINT`/`_HEADERS` mutations can't race a
  /// parallel-running sibling test touching the same process-wide env
  /// vars — cargo runs `#[test]`s in parallel by default.
  #[test]
  fn config_from_env_reads_endpoint_and_headers_and_is_none_when_unset() {
    // SAFETY: these env vars are not read by any other test in this crate.
    unsafe {
      std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
    }
    assert!(OtlpHttpConfig::from_env().is_none());

    unsafe {
      std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "http://collector:4318");
      std::env::set_var(
        "OTEL_EXPORTER_OTLP_HEADERS",
        "x-api-key=secret, x-tenant=acme",
      );
    }
    let config = OtlpHttpConfig::from_env().expect("endpoint set");
    assert_eq!(config.endpoint, "http://collector:4318");
    assert_eq!(config.headers.get("x-api-key"), Some(&"secret".to_string()));
    assert_eq!(config.headers.get("x-tenant"), Some(&"acme".to_string()));

    unsafe {
      std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
      std::env::remove_var("OTEL_EXPORTER_OTLP_HEADERS");
    }
  }

  #[test]
  fn sink_constructs_successfully_for_a_well_formed_endpoint() {
    let result = OtlpHttpSpanSink::new(OtlpHttpConfig::new("http://localhost:4318"));
    assert!(result.is_ok());
  }
}

//! Cross-hop W3C traceparent injection / extraction for the MCP
//! JSON-RPC envelope (P3.8).
//!
//! AgentFlow stitches an OpenTelemetry trace across process and
//! protocol hops using the W3C `traceparent` header. For HTTP-shaped
//! hops (LLM calls, gRPC, plugin subprocess env) this is direct; for
//! MCP, JSON-RPC has no native metadata channel, so the de facto
//! convention is to nest a `_meta` object inside `params`. This
//! module is the canonical reader / writer for that field so every
//! MCP integration speaks the same wire shape.
//!
//! ## Wire shape
//!
//! When an active context exists, requests gain:
//!
//! ```json
//! {
//!   "jsonrpc": "2.0",
//!   "id": "...",
//!   "method": "tools/call",
//!   "params": {
//!     "name": "search",
//!     "arguments": { "q": "foo" },
//!     "_meta": {
//!       "traceparent": "00-<trace-id>-<span-id>-<flags>"
//!     }
//!   }
//! }
//! ```
//!
//! When there is no active context, the carrier is **omitted entirely**
//! (no empty `_meta`, no `null` traceparent) so consumers can tell
//! "no upstream trace" apart from "upstream trace exists but isn't a
//! valid W3C value".
//!
//! ## Why `_meta` and not a sibling of `params`
//!
//! Inspecting the official MCP spec at the time of writing, protocol-
//! level metadata is conventionally stowed inside `params._meta` rather
//! than as a sibling top-level field. This keeps the canonical
//! JSON-RPC schema (`jsonrpc`/`id`/`method`/`params` only) intact and
//! lets the server side ignore the field without schema breakage.

use crate::protocol::types::JsonRpcRequest;
use serde_json::{Map, Value};

/// Field name nested under `params` that carries protocol-level
/// metadata. Pinned here so consumers can't accidentally rename it.
pub const META_FIELD: &str = "_meta";

/// Field name within `_meta` that carries the W3C traceparent value.
/// Matches the lowercase `traceparent` convention used in HTTP headers
/// and gRPC metadata so cross-hop tooling can grep one string.
pub const TRACEPARENT_FIELD: &str = "traceparent";

/// Inject the currently-active W3C traceparent (when set) into a
/// JSON-RPC request's `params._meta.traceparent` field.
///
/// Behaviour matrix:
/// - No active context (`current_traceparent() == None`) ⇒ no-op.
/// - `params` is `None` ⇒ becomes `{"_meta":{"traceparent":"..."}}`.
/// - `params` is a JSON object ⇒ `_meta.traceparent` added /
///   overwritten in place.
/// - `params` is a non-object JSON value (array / primitive) ⇒ no-op;
///   we'd otherwise have to wrap the value, changing the protocol
///   wire shape downstream consumers see. JSON-RPC methods that pass
///   non-object params are rare; if one shows up, the producer can
///   construct an object form explicitly.
///
/// Returns `true` when the value was actually injected, `false`
/// otherwise. Tests use the return value to assert reachability of
/// each branch; production callers can ignore it.
pub fn inject_traceparent_into_request(request: &mut JsonRpcRequest) -> bool {
  let Some(traceparent) = agentflow_tracing::context::current_traceparent() else {
    return false;
  };
  inject_traceparent_into_request_with(request, &traceparent)
}

/// Version of [`inject_traceparent_into_request`] that takes an
/// explicit value instead of reading the task-local. Exposed so unit
/// tests can exercise the wire-shape logic without spinning up a
/// `tokio::task_local!` scope, and so callers that already have the
/// traceparent string (e.g. extracted upstream from gRPC metadata)
/// don't have to round-trip it through the scope.
pub fn inject_traceparent_into_request_with(
  request: &mut JsonRpcRequest,
  traceparent: &str,
) -> bool {
  match request.params.as_mut() {
    Some(Value::Object(map)) => {
      set_meta_traceparent(map, traceparent);
      true
    }
    Some(_non_object) => {
      // Don't mutate a non-object params — wrapping would change the
      // wire shape downstream consumers see.
      false
    }
    None => {
      let mut params = Map::new();
      set_meta_traceparent(&mut params, traceparent);
      request.params = Some(Value::Object(params));
      true
    }
  }
}

/// Extract `params._meta.traceparent` from a JSON-RPC request, if
/// present. Servers / receivers use this to install the parent
/// context before dispatching the method handler.
///
/// Returns `None` when:
/// - `params` is absent or non-object,
/// - `_meta` is absent or non-object,
/// - `traceparent` is absent or not a string.
///
/// Empty-string traceparent is intentionally returned as `Some("")`
/// rather than `None` — the caller decides whether to treat that as
/// "no context" or as a malformed wire value. The default convention
/// in `agentflow_tracing::context::scope` is to treat any non-empty
/// string as the parent.
pub fn extract_traceparent_from_request(request: &JsonRpcRequest) -> Option<String> {
  let Some(Value::Object(params)) = request.params.as_ref() else {
    return None;
  };
  let Some(Value::Object(meta)) = params.get(META_FIELD) else {
    return None;
  };
  meta
    .get(TRACEPARENT_FIELD)
    .and_then(|v| v.as_str())
    .map(str::to_owned)
}

fn set_meta_traceparent(params: &mut Map<String, Value>, traceparent: &str) {
  let meta = params
    .entry(META_FIELD)
    .or_insert_with(|| Value::Object(Map::new()));
  if let Value::Object(meta_map) = meta {
    meta_map.insert(
      TRACEPARENT_FIELD.to_owned(),
      Value::String(traceparent.to_owned()),
    );
  } else {
    // `_meta` exists but isn't an object (caller put something
    // weird there). Overwrite — the protocol contract owns this
    // field, so a non-object value is a bug in the producer, not
    // something to preserve.
    *meta = Value::Object(Map::from_iter([(
      TRACEPARENT_FIELD.to_owned(),
      Value::String(traceparent.to_owned()),
    )]));
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::protocol::types::{JsonRpcRequest, RequestId};
  use serde_json::json;

  fn sample_request_with_params(params: Option<Value>) -> JsonRpcRequest {
    JsonRpcRequest::new(RequestId::Number(1), "tools/call", params)
  }

  #[test]
  fn inject_with_none_params_populates_full_meta_path() {
    let mut req = sample_request_with_params(None);
    let did_inject = inject_traceparent_into_request_with(&mut req, "trace-001");
    assert!(did_inject);
    let params = req.params.expect("params populated");
    assert_eq!(params, json!({ "_meta": { "traceparent": "trace-001" } }));
  }

  #[test]
  fn inject_with_object_params_preserves_existing_fields() {
    let mut req = sample_request_with_params(Some(json!({
      "name": "search",
      "arguments": { "q": "foo" }
    })));
    assert!(inject_traceparent_into_request_with(&mut req, "trace-002"));
    let params = req.params.expect("params populated");
    assert_eq!(
      params,
      json!({
        "name": "search",
        "arguments": { "q": "foo" },
        "_meta": { "traceparent": "trace-002" }
      })
    );
  }

  #[test]
  fn inject_preserves_existing_meta_fields_other_than_traceparent() {
    let mut req = sample_request_with_params(Some(json!({
      "_meta": { "request_origin": "ci", "traceparent": "old" }
    })));
    assert!(inject_traceparent_into_request_with(&mut req, "new"));
    let params = req.params.expect("params");
    let meta = &params.as_object().unwrap()["_meta"];
    // request_origin preserved; traceparent overwritten.
    assert_eq!(meta["request_origin"], json!("ci"));
    assert_eq!(meta["traceparent"], json!("new"));
  }

  #[test]
  fn inject_with_array_params_is_noop() {
    let mut req = sample_request_with_params(Some(json!([1, 2, 3])));
    assert!(!inject_traceparent_into_request_with(&mut req, "trace"));
    assert_eq!(req.params, Some(json!([1, 2, 3])));
  }

  #[test]
  fn inject_with_primitive_params_is_noop() {
    let mut req = sample_request_with_params(Some(json!("plain string")));
    assert!(!inject_traceparent_into_request_with(&mut req, "trace"));
    assert_eq!(req.params, Some(json!("plain string")));
  }

  #[test]
  fn inject_replaces_non_object_meta_value() {
    // Defensive: if a producer accidentally stored a string under
    // `_meta`, we still want to land traceparent there. The
    // protocol owns the `_meta` field semantics.
    let mut req = sample_request_with_params(Some(json!({
      "name": "search",
      "_meta": "oops-not-an-object"
    })));
    assert!(inject_traceparent_into_request_with(&mut req, "tp"));
    let params = req.params.expect("params populated");
    assert_eq!(
      params,
      json!({
        "name": "search",
        "_meta": { "traceparent": "tp" }
      })
    );
  }

  // ── extract path ──────────────────────────────────────────────────

  #[test]
  fn extract_returns_traceparent_when_meta_is_populated() {
    let req = sample_request_with_params(Some(
      json!({ "_meta": { "traceparent": "tp-extract" } }),
    ));
    assert_eq!(
      extract_traceparent_from_request(&req).as_deref(),
      Some("tp-extract")
    );
  }

  #[test]
  fn extract_returns_none_when_params_absent() {
    let req = sample_request_with_params(None);
    assert!(extract_traceparent_from_request(&req).is_none());
  }

  #[test]
  fn extract_returns_none_when_meta_absent() {
    let req = sample_request_with_params(Some(json!({ "name": "search" })));
    assert!(extract_traceparent_from_request(&req).is_none());
  }

  #[test]
  fn extract_returns_none_when_meta_is_not_object() {
    let req = sample_request_with_params(Some(json!({ "_meta": "string" })));
    assert!(extract_traceparent_from_request(&req).is_none());
  }

  #[test]
  fn extract_returns_none_when_traceparent_is_not_string() {
    let req = sample_request_with_params(Some(json!({ "_meta": { "traceparent": 42 } })));
    assert!(extract_traceparent_from_request(&req).is_none());
  }

  #[test]
  fn extract_returns_empty_string_verbatim() {
    // Per the module docs, we surface empty strings as Some("") so
    // callers decide how to interpret them. Mismatch detection
    // (well-formed traceparent vs malformed) belongs upstream.
    let req = sample_request_with_params(Some(json!({ "_meta": { "traceparent": "" } })));
    assert_eq!(extract_traceparent_from_request(&req).as_deref(), Some(""));
  }

  // ── env-driven injection (task-local round trip) ──────────────────

  #[tokio::test]
  async fn inject_picks_up_scope_traceparent() {
    let mut req = sample_request_with_params(Some(json!({ "name": "search" })));
    // No scope ⇒ no injection.
    assert!(!inject_traceparent_into_request(&mut req));
    assert!(extract_traceparent_from_request(&req).is_none());

    // Inside scope ⇒ injection lands the active value.
    let mut req2 = sample_request_with_params(Some(json!({ "name": "search" })));
    agentflow_tracing::context::scope("00-trace-id-span-01".to_string(), async {
      assert!(inject_traceparent_into_request(&mut req2));
    })
    .await;
    assert_eq!(
      extract_traceparent_from_request(&req2).as_deref(),
      Some("00-trace-id-span-01")
    );
  }

  #[tokio::test]
  async fn inject_outside_any_scope_is_a_noop() {
    let mut req = sample_request_with_params(Some(json!({})));
    let snapshot_before = req.clone();
    assert!(!inject_traceparent_into_request(&mut req));
    assert_eq!(req, snapshot_before);
  }
}

//! Modern-era (`2026-07-28`) MCP protocol types and helpers.
//!
//! Additive scaffolding for W5.8-2, the first implementation sub-item of
//! `docs/RFC_MCP_PROTOCOL_MODERNIZATION.md`'s Phase 2. **Nothing in this
//! module is wired into any request/response path yet** — `MCPClient`
//! still only speaks the Legacy `initialize()` handshake
//! ([`crate::protocol::types::MCP_PROTOCOL_VERSION`], `"2024-11-05"`).
//! W5.8-3 (Streamable HTTP transport) and W5.8-4 (client-side era-probe)
//! build the code that actually constructs and interprets these types on
//! the wire.
//!
//! Wire shapes below are transcribed from the RFC's own live spec
//! research (`docs/RFC_MCP_PROTOCOL_MODERNIZATION.md`, "The spec,
//! precisely" section) rather than re-derived here. Where the RFC's
//! prose is abbreviated — it summarizes per-request metadata as
//! "`_meta.io.modelcontextprotocol/protocolVersion` + `clientInfo` +
//! `clientCapabilities` inline" without spelling out the exact JSON
//! nesting for the latter two, and doesn't reproduce `server/discover`'s
//! or MRTR's full JSON schema — this module documents the interpretation
//! taken and flags it for verification once W5.8-3/4 can exercise a real
//! external Modern MCP server or an authoritative fixture.

use crate::protocol::traceparent::META_FIELD;
use crate::protocol::types::{
  ClientCapabilities, Implementation, JsonRpcError, JsonRpcRequest, MCP_PROTOCOL_VERSION, RequestId,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

// ============================================================================
// Version timeline + era classification
// ============================================================================

/// `2024-11-05` — the first released MCP protocol version. Re-exported
/// under this module's naming convention; identical value to
/// [`MCP_PROTOCOL_VERSION`], which remains the canonical constant this
/// crate's Legacy `initialize()` path sends.
pub const MCP_PROTOCOL_VERSION_2024_11_05: &str = MCP_PROTOCOL_VERSION;
/// `2025-03-26` — introduced session-based Streamable HTTP v1; still
/// Legacy era (uses `initialize`).
pub const MCP_PROTOCOL_VERSION_2025_03_26: &str = "2025-03-26";
/// `2025-11-25` — last Legacy-era revision.
pub const MCP_PROTOCOL_VERSION_2025_11_25: &str = "2025-11-25";
/// `2026-07-28` — current stable version; the sole Modern-era revision to
/// date. Removes `initialize` entirely for a stateless, per-request model.
pub const MCP_PROTOCOL_VERSION_2026_07_28: &str = "2026-07-28";

/// All released protocol versions, oldest first. Useful for validating a
/// `server/discover` response's supported-version list and for picking a
/// mutually supported version during `UnsupportedProtocolVersionError`
/// retry.
pub const KNOWN_PROTOCOL_VERSIONS: &[&str] = &[
  MCP_PROTOCOL_VERSION_2024_11_05,
  MCP_PROTOCOL_VERSION_2025_03_26,
  MCP_PROTOCOL_VERSION_2025_11_25,
  MCP_PROTOCOL_VERSION_2026_07_28,
];

/// Which architectural era a protocol version belongs to. Per the RFC,
/// `2026-07-28` is the sole Modern-era version to date; every other
/// version string (including ones this crate doesn't otherwise
/// recognize) is treated as Legacy, since only Modern replaced the
/// `initialize`-handshake model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpEra {
  /// `initialize`/`notifications/initialized` handshake, persistent
  /// session. Everything this crate implemented before W5.8.
  Legacy,
  /// Stateless, per-request `_meta`-carried version + capabilities. No
  /// handshake, no session.
  Modern,
}

impl McpEra {
  /// Classify a protocol version string by era.
  pub fn for_version(version: &str) -> Self {
    if version == MCP_PROTOCOL_VERSION_2026_07_28 {
      McpEra::Modern
    } else {
      McpEra::Legacy
    }
  }
}

// ============================================================================
// Per-request `_meta` construction (Modern-era request path)
// ============================================================================

/// `_meta` key carrying the Modern-era per-request protocol version.
/// Namespaced per the MCP spec's `io.modelcontextprotocol/...` `_meta`
/// key convention.
pub const MODERN_PROTOCOL_VERSION_META_FIELD: &str = "io.modelcontextprotocol/protocolVersion";
/// `_meta` key carrying the Modern-era client implementation info.
pub const MODERN_CLIENT_INFO_META_FIELD: &str = "clientInfo";
/// `_meta` key carrying the Modern-era client capabilities.
pub const MODERN_CLIENT_CAPABILITIES_META_FIELD: &str = "clientCapabilities";

/// Inject the Modern-era per-request `_meta` block into a JSON-RPC
/// request: protocol version + client info + client capabilities, all
/// nested under `params._meta` alongside whatever else already lives
/// there (e.g. `traceparent` — see [`crate::protocol::traceparent`]).
/// Mirrors `traceparent::inject_traceparent_into_request_with`'s
/// preserve-existing-`_meta`-keys behavior; only touches the three keys
/// above.
///
/// No-op (returns `false`) when `params` is a non-object JSON value, for
/// the same reason `inject_traceparent_into_request_with` declines:
/// wrapping would change the wire shape a caller expects.
pub fn inject_modern_meta_into_request(
  request: &mut JsonRpcRequest,
  protocol_version: &str,
  client_info: &Implementation,
  client_capabilities: &ClientCapabilities,
) -> bool {
  let client_info_value = serde_json::to_value(client_info).unwrap_or(Value::Null);
  let client_capabilities_value = serde_json::to_value(client_capabilities).unwrap_or(Value::Null);

  match request.params.as_mut() {
    Some(Value::Object(map)) => {
      set_modern_meta(
        map,
        protocol_version,
        client_info_value,
        client_capabilities_value,
      );
      true
    }
    Some(_non_object) => false,
    None => {
      let mut params = Map::new();
      set_modern_meta(
        &mut params,
        protocol_version,
        client_info_value,
        client_capabilities_value,
      );
      request.params = Some(Value::Object(params));
      true
    }
  }
}

fn set_modern_meta(
  params: &mut Map<String, Value>,
  protocol_version: &str,
  client_info: Value,
  client_capabilities: Value,
) {
  let meta = params
    .entry(META_FIELD.to_owned())
    .or_insert_with(|| Value::Object(Map::new()));
  if !meta.is_object() {
    // `_meta` exists but isn't an object (caller put something weird
    // there). Overwrite — same rationale as `traceparent::set_meta_traceparent`.
    *meta = Value::Object(Map::new());
  }
  if let Value::Object(meta_map) = meta {
    meta_map.insert(
      MODERN_PROTOCOL_VERSION_META_FIELD.to_owned(),
      Value::String(protocol_version.to_owned()),
    );
    meta_map.insert(MODERN_CLIENT_INFO_META_FIELD.to_owned(), client_info);
    meta_map.insert(
      MODERN_CLIENT_CAPABILITIES_META_FIELD.to_owned(),
      client_capabilities,
    );
  }
}

// ============================================================================
// Recognized Modern-era JSON-RPC errors
// ============================================================================

/// JSON-RPC error code for `UnsupportedProtocolVersionError`.
pub const UNSUPPORTED_PROTOCOL_VERSION_ERROR_CODE: i32 = -32022;
/// JSON-RPC error code for `HeaderMismatch` (Streamable HTTP header vs.
/// body disagreement).
pub const HEADER_MISMATCH_ERROR_CODE: i32 = -32020;

/// `data` payload of an `UnsupportedProtocolVersionError`: the server's
/// supported version list, used to pick a mutually supported version for
/// the retry (RFC: "the client retries with a mutually supported
/// version").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct UnsupportedProtocolVersionErrorData {
  /// Protocol versions the server supports.
  #[serde(default)]
  pub supported: Vec<String>,
}

/// Interpret a [`JsonRpcError`] as an `UnsupportedProtocolVersionError`.
/// Returns `None` if the error code doesn't match. A code match with
/// unparseable/absent `data` still returns `Some(_)` with an empty
/// `supported` list — the code itself is what identifies a Modern
/// server during era-probing; the version list is only needed for the
/// retry-with-supported-version step.
pub fn as_unsupported_protocol_version_error(
  error: &JsonRpcError,
) -> Option<UnsupportedProtocolVersionErrorData> {
  if error.code != UNSUPPORTED_PROTOCOL_VERSION_ERROR_CODE {
    return None;
  }
  Some(
    error
      .data
      .as_ref()
      .and_then(|d| serde_json::from_value(d.clone()).ok())
      .unwrap_or_default(),
  )
}

/// `true` when a [`JsonRpcError`]'s code is one this crate recognizes as
/// "the server is Modern-era" per the RFC's era-detection rules — used
/// by the era-probe (W5.8-4) to distinguish a real Modern rejection
/// (stay Modern, retry with a supported version) from an
/// unrelated/Legacy error (fall back to Legacy).
pub fn is_recognized_modern_error(error: &JsonRpcError) -> bool {
  matches!(
    error.code,
    UNSUPPORTED_PROTOCOL_VERSION_ERROR_CODE | HEADER_MISMATCH_ERROR_CODE
  )
}

// ============================================================================
// `server/discover`
// ============================================================================

/// Method name for the mandatory-for-servers `server/discover` RPC (RFC:
/// "lets a client learn supported versions/capabilities/identity up
/// front in one call").
pub const SERVER_DISCOVER_METHOD: &str = "server/discover";

/// Build a `server/discover` request. The RFC's summary of the RPC
/// doesn't specify any request params.
pub fn server_discover_request(id: RequestId) -> JsonRpcRequest {
  JsonRpcRequest::new(id, SERVER_DISCOVER_METHOD, None)
}

/// Result shape of a successful `server/discover` response: the
/// server's supported protocol versions, capabilities, and identity.
/// Field shape is this crate's best-effort reading of the RFC's summary
/// ("supported versions/capabilities/identity") — the RFC doesn't
/// reproduce the RPC's full JSON schema, so `capabilities` is left as a
/// raw [`Value`] rather than the Legacy [`crate::protocol::types::ServerCapabilities`]
/// type (Modern capabilities may not be shaped identically), pending
/// verification against a real server in W5.8-4.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverResult {
  /// Protocol versions the server supports.
  #[serde(default)]
  pub supported_versions: Vec<String>,
  /// Server capabilities, left untyped pending verification (see struct docs).
  #[serde(default)]
  pub capabilities: Value,
  /// Server implementation info, when present.
  #[serde(default)]
  pub server_info: Option<Implementation>,
}

// ============================================================================
// MRTR (Multi Round-Trip Requests, SEP-2322) / `InputRequiredResult`
// ============================================================================

/// `_meta`-adjacent JSON-RPC result field that marks a response as
/// requiring client input rather than being final (RFC: "Server-to-client
/// interactions... are now embedded as `InputRequiredResult` inside the
/// *response* to the original request").
pub const INPUT_REQUIRED_RESULT_FIELD: &str = "inputRequired";
/// Request params field the client attaches when re-issuing a request
/// after answering an `InputRequiredResult` (RFC: "the client answers by
/// retrying the same request with `inputResponses` attached").
pub const INPUT_RESPONSES_FIELD: &str = "inputResponses";

/// `true` when a JSON-RPC result [`Value`] looks like an
/// `InputRequiredResult` (carries an `inputRequired` field) rather than
/// a normal method result. The Modern request path (W5.8-4) uses this to
/// decide whether to treat a response as final or to re-issue it with
/// `inputResponses` (MRTR).
pub fn is_input_required_result(result: &Value) -> bool {
  result
    .as_object()
    .is_some_and(|obj| obj.contains_key(INPUT_REQUIRED_RESULT_FIELD))
}

/// Attach an MRTR `inputResponses` field to a request's params, for
/// re-issuing the same request after answering an `InputRequiredResult`.
/// No-op when `params` is a non-object JSON value, for the same reason
/// [`inject_modern_meta_into_request`] declines.
pub fn attach_input_responses(request: &mut JsonRpcRequest, input_responses: Value) -> bool {
  match request.params.as_mut() {
    Some(Value::Object(map)) => {
      map.insert(INPUT_RESPONSES_FIELD.to_owned(), input_responses);
      true
    }
    Some(_non_object) => false,
    None => {
      let mut params = Map::new();
      params.insert(INPUT_RESPONSES_FIELD.to_owned(), input_responses);
      request.params = Some(Value::Object(params));
      true
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::protocol::types::RequestId;
  use serde_json::json;

  // ── era classification ────────────────────────────────────────────

  #[test]
  fn era_2026_07_28_is_modern() {
    assert_eq!(
      McpEra::for_version(MCP_PROTOCOL_VERSION_2026_07_28),
      McpEra::Modern
    );
  }

  #[test]
  fn every_other_known_version_is_legacy() {
    assert_eq!(
      McpEra::for_version(MCP_PROTOCOL_VERSION_2024_11_05),
      McpEra::Legacy
    );
    assert_eq!(
      McpEra::for_version(MCP_PROTOCOL_VERSION_2025_03_26),
      McpEra::Legacy
    );
    assert_eq!(
      McpEra::for_version(MCP_PROTOCOL_VERSION_2025_11_25),
      McpEra::Legacy
    );
  }

  #[test]
  fn unknown_version_defaults_to_legacy() {
    assert_eq!(McpEra::for_version("2099-01-01"), McpEra::Legacy);
  }

  // ── modern `_meta` injection ──────────────────────────────────────

  fn sample_client_info() -> Implementation {
    Implementation::new("agentflow-mcp", "0.2.0")
  }

  #[test]
  fn inject_with_none_params_populates_full_meta_path() {
    let mut req = JsonRpcRequest::new(RequestId::Number(1), "tools/list", None);
    let injected = inject_modern_meta_into_request(
      &mut req,
      MCP_PROTOCOL_VERSION_2026_07_28,
      &sample_client_info(),
      &ClientCapabilities::default(),
    );
    assert!(injected);
    let meta = &req.params.unwrap()["_meta"];
    assert_eq!(
      meta[MODERN_PROTOCOL_VERSION_META_FIELD],
      json!(MCP_PROTOCOL_VERSION_2026_07_28)
    );
    assert_eq!(
      meta[MODERN_CLIENT_INFO_META_FIELD],
      serde_json::to_value(sample_client_info()).unwrap()
    );
    assert_eq!(
      meta[MODERN_CLIENT_CAPABILITIES_META_FIELD],
      serde_json::to_value(ClientCapabilities::default()).unwrap()
    );
  }

  #[test]
  fn inject_preserves_existing_meta_fields_like_traceparent() {
    let mut req = JsonRpcRequest::new(
      RequestId::Number(1),
      "tools/call",
      Some(json!({
        "name": "search",
        "_meta": { "traceparent": "00-abc" }
      })),
    );
    assert!(inject_modern_meta_into_request(
      &mut req,
      MCP_PROTOCOL_VERSION_2026_07_28,
      &sample_client_info(),
      &ClientCapabilities::default(),
    ));
    let params = req.params.unwrap();
    assert_eq!(params["name"], json!("search"));
    assert_eq!(params["_meta"]["traceparent"], json!("00-abc"));
    assert_eq!(
      params["_meta"][MODERN_PROTOCOL_VERSION_META_FIELD],
      json!(MCP_PROTOCOL_VERSION_2026_07_28)
    );
  }

  #[test]
  fn inject_with_array_params_is_noop() {
    let mut req = JsonRpcRequest::new(RequestId::Number(1), "tools/list", Some(json!([1, 2])));
    assert!(!inject_modern_meta_into_request(
      &mut req,
      MCP_PROTOCOL_VERSION_2026_07_28,
      &sample_client_info(),
      &ClientCapabilities::default(),
    ));
    assert_eq!(req.params, Some(json!([1, 2])));
  }

  // ── recognized Modern errors ──────────────────────────────────────

  #[test]
  fn unsupported_protocol_version_error_is_recognized_with_supported_list() {
    let error = JsonRpcError::with_data(
      UNSUPPORTED_PROTOCOL_VERSION_ERROR_CODE,
      "unsupported version".to_string(),
      json!({ "supported": ["2025-11-25", "2026-07-28"] }),
    );
    let data = as_unsupported_protocol_version_error(&error).expect("recognized");
    assert_eq!(
      data.supported,
      vec!["2025-11-25".to_string(), "2026-07-28".to_string()]
    );
    assert!(is_recognized_modern_error(&error));
  }

  #[test]
  fn unsupported_protocol_version_error_with_bad_data_still_recognized() {
    let error = JsonRpcError::with_data(
      UNSUPPORTED_PROTOCOL_VERSION_ERROR_CODE,
      "unsupported version".to_string(),
      json!("not-an-object"),
    );
    let data = as_unsupported_protocol_version_error(&error).expect("recognized by code alone");
    assert!(data.supported.is_empty());
  }

  #[test]
  fn header_mismatch_is_recognized_as_modern_but_not_as_unsupported_version() {
    let error = JsonRpcError::new(HEADER_MISMATCH_ERROR_CODE, "header mismatch".to_string());
    assert!(is_recognized_modern_error(&error));
    assert!(as_unsupported_protocol_version_error(&error).is_none());
  }

  #[test]
  fn unrelated_error_code_is_not_recognized_as_modern() {
    let error = JsonRpcError::new(-32601, "method not found".to_string());
    assert!(!is_recognized_modern_error(&error));
    assert!(as_unsupported_protocol_version_error(&error).is_none());
  }

  // ── server/discover ───────────────────────────────────────────────

  #[test]
  fn server_discover_request_has_no_params() {
    let req = server_discover_request(RequestId::Number(1));
    assert_eq!(req.method, SERVER_DISCOVER_METHOD);
    assert_eq!(req.params, None);
  }

  #[test]
  fn discover_result_round_trips() {
    let result = DiscoverResult {
      supported_versions: vec![
        MCP_PROTOCOL_VERSION_2025_11_25.to_string(),
        MCP_PROTOCOL_VERSION_2026_07_28.to_string(),
      ],
      capabilities: json!({ "tools": {} }),
      server_info: Some(Implementation::new("test-server", "1.0.0")),
    };
    let value = serde_json::to_value(&result).unwrap();
    let round_tripped: DiscoverResult = serde_json::from_value(value).unwrap();
    assert_eq!(round_tripped, result);
  }

  #[test]
  fn discover_result_defaults_missing_fields() {
    let result: DiscoverResult = serde_json::from_value(json!({})).unwrap();
    assert!(result.supported_versions.is_empty());
    assert!(result.server_info.is_none());
  }

  // ── MRTR ───────────────────────────────────────────────────────────

  #[test]
  fn input_required_result_is_detected() {
    assert!(is_input_required_result(
      &json!({ "inputRequired": { "kind": "sampling" } })
    ));
    assert!(!is_input_required_result(&json!({ "tools": [] })));
    assert!(!is_input_required_result(&json!([1, 2, 3])));
  }

  #[test]
  fn attach_input_responses_adds_field_to_object_params() {
    let mut req = JsonRpcRequest::new(
      RequestId::Number(1),
      "sampling/createMessage",
      Some(json!({ "prompt": "hi" })),
    );
    assert!(attach_input_responses(
      &mut req,
      json!([{ "kind": "sampling", "value": "ok" }])
    ));
    let params = req.params.unwrap();
    assert_eq!(params["prompt"], json!("hi"));
    assert_eq!(
      params[INPUT_RESPONSES_FIELD],
      json!([{ "kind": "sampling", "value": "ok" }])
    );
  }

  #[test]
  fn attach_input_responses_with_none_params_creates_object() {
    let mut req = JsonRpcRequest::new(RequestId::Number(1), "sampling/createMessage", None);
    assert!(attach_input_responses(&mut req, json!([])));
    assert_eq!(req.params.unwrap()[INPUT_RESPONSES_FIELD], json!([]));
  }

  #[test]
  fn attach_input_responses_with_array_params_is_noop() {
    let mut req = JsonRpcRequest::new(RequestId::Number(1), "tools/list", Some(json!([1, 2])));
    assert!(!attach_input_responses(&mut req, json!([])));
    assert_eq!(req.params, Some(json!([1, 2])));
  }
}

//! End-to-end Modern-era (`2026-07-28`) client behavior (W5.8-4).
//!
//! Uses `MockTransport::with_transport_type(TransportType::StreamableHttp)`
//! to exercise `MCPClient`'s Modern request path without a real HTTP
//! server — the mock's queue-based `send_message` is transport-agnostic,
//! and per `client::era::era_for_transport`, era is determined purely by
//! the transport type the client is connected over.
//!
//! Companion coverage: `protocol/modern.rs`'s own unit tests cover the
//! wire-shape helpers in isolation; `client/session.rs`'s existing tests
//! (unchanged by W5.8-4) cover the untouched Legacy path.

use agentflow_mcp::client::ClientBuilder;
use agentflow_mcp::protocol::McpEra;
use agentflow_mcp::transport::{MockTransport, TransportType};
use serde_json::{Value, json};

fn modern_mock() -> MockTransport {
  MockTransport::new().with_transport_type(TransportType::StreamableHttp)
}

#[tokio::test]
async fn connect_over_streamable_http_skips_initialize_and_sets_modern_era() {
  // No responses queued at all — if `connect()` tried to run the
  // Legacy `initialize()` handshake here, it would fail immediately
  // ("No response configured for this request"). Succeeding proves
  // the Modern path really is handshake-free.
  let transport = modern_mock();
  let sent = transport.sent_messages_handle();

  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();
  client
    .connect()
    .await
    .expect("modern connect needs no handshake");

  assert_eq!(client.era().await, McpEra::Modern);
  assert!(
    sent.lock().unwrap().is_empty(),
    "connect() must not send any wire request for Modern era"
  );
}

#[tokio::test]
async fn legacy_connect_over_default_mock_transport_is_unchanged() {
  let mut transport = MockTransport::new(); // default: Stdio ⇒ Legacy
  transport.add_response(MockTransport::standard_initialize_response());
  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();
  client
    .connect()
    .await
    .expect("legacy connect via initialize");
  assert_eq!(client.era().await, McpEra::Legacy);
}

#[tokio::test]
async fn modern_list_tools_bypasses_capability_gate_and_injects_meta() {
  let mut transport = modern_mock();
  // Only a `tools/list` response is queued — no `initialize` response
  // exists, and `server_capabilities` is never populated for a Modern
  // client, so this also proves `require_server_capability`'s Modern
  // bypass (W5.8-4) actually fires: without it, `list_tools()` would
  // fail with "client is not initialized" before ever reaching the
  // transport.
  transport.add_response(MockTransport::tools_list_response(vec![]));
  let sent = transport.sent_messages_handle();

  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();
  client.connect().await.unwrap();
  client.list_tools().await.expect("capability gate bypassed");

  let messages = sent.lock().unwrap().clone();
  assert_eq!(messages.len(), 1, "no initialize request should be sent");
  let meta = &messages[0]["params"]["_meta"];
  assert_eq!(
    meta["io.modelcontextprotocol/protocolVersion"],
    json!("2026-07-28")
  );
  assert!(meta["clientInfo"]["name"].is_string());
  assert!(meta["clientCapabilities"].is_object());
}

#[tokio::test]
async fn unsupported_protocol_version_error_with_nothing_to_retry_to_is_surfaced() {
  // This crate implements exactly one Modern version (`2026-07-28`), so
  // a server claiming to support only that same version leaves nothing
  // to retry to — `send_request` must surface the original error rather
  // than loop or panic.
  let mut transport = modern_mock();
  let error_response: Value = json!({
    "jsonrpc": "2.0",
    "id": 1,
    "error": {
      "code": -32022,
      "message": "unsupported protocol version",
      "data": { "supported": ["2026-07-28"] }
    }
  });
  transport.add_response(error_response);
  let sent = transport.sent_messages_handle();

  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();
  client.connect().await.unwrap();
  let err = client
    .list_tools()
    .await
    .expect_err("no mutually supported version to retry with");
  assert!(err.to_string().contains("tools/list failed"));

  // Exactly one request went out — no blind retry storm.
  assert_eq!(sent.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn input_required_result_is_surfaced_as_a_distinct_error_not_a_normal_result() {
  let mut transport = modern_mock();
  transport.add_response(json!({
    "jsonrpc": "2.0",
    "id": 1,
    "result": { "inputRequired": { "kind": "sampling" } }
  }));

  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();
  client.connect().await.unwrap();
  let err = client
    .list_tools()
    .await
    .expect_err("MRTR result must not be silently parsed as a normal tools/list result");
  let message = err.to_string();
  assert!(message.contains("InputRequiredResult"));
  assert!(message.contains("sampling/elicitation/roots"));
}

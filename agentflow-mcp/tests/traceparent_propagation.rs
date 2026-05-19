//! End-to-end MCP traceparent propagation (P3.8).
//!
//! Verifies the contract `client/session.rs::send_request` /
//! `send_notification` carry the active
//! `agentflow_tracing::context::current_traceparent` onto the wire
//! as `params._meta.traceparent`, and that server-side extraction
//! recovers the same value.
//!
//! Uses `MockTransport` to capture the serialised JSON-RPC bytes
//! the client would have sent. The captured bytes are then re-parsed
//! into `JsonRpcRequest` so the extract helper runs against the
//! exact shape a real server would see.

use agentflow_mcp::client::ClientBuilder;
use agentflow_mcp::protocol::{JsonRpcRequest, extract_traceparent_from_request};
use agentflow_mcp::transport_new::MockTransport;
use serde_json::Value;

fn captured_request(messages: &[Value], method: &str) -> JsonRpcRequest {
  // Find the message whose `method` field matches what the test sent.
  // `initialize` always lands first; subsequent methods come after.
  let value = messages
    .iter()
    .find(|m| m["method"].as_str() == Some(method))
    .unwrap_or_else(|| panic!("no captured request with method={method}; got: {messages:?}"));
  serde_json::from_value(value.clone()).expect("captured value parses as JsonRpcRequest")
}

#[tokio::test]
async fn list_tools_emits_meta_traceparent_inside_active_scope() {
  let mut transport = MockTransport::new();
  transport.add_response(MockTransport::standard_initialize_response());
  transport.add_response(MockTransport::tools_list_response(vec![]));
  let sent = transport.sent_messages_handle();

  agentflow_tracing::context::scope("00-traceparent-abc-01".to_string(), async {
    let mut client = ClientBuilder::new()
      .with_transport(transport)
      .build()
      .await
      .unwrap();
    client.connect().await.unwrap();
    let _ = client.list_tools().await.unwrap();
  })
  .await;

  let messages = sent.lock().unwrap().clone();
  let init_request = captured_request(&messages, "initialize");
  let list_request = captured_request(&messages, "tools/list");
  // Both outbound requests must carry the traceparent — the OTel
  // trace stitches across every hop, not just the first one.
  assert_eq!(
    extract_traceparent_from_request(&init_request).as_deref(),
    Some("00-traceparent-abc-01")
  );
  assert_eq!(
    extract_traceparent_from_request(&list_request).as_deref(),
    Some("00-traceparent-abc-01")
  );
}

#[tokio::test]
async fn requests_outside_scope_omit_meta_entirely() {
  // No scope ⇒ no carrier. The contract is strict: we never emit
  // `_meta` with an empty / null traceparent, because consumers use
  // the field's presence as the "upstream context exists" signal.
  let mut transport = MockTransport::new();
  transport.add_response(MockTransport::standard_initialize_response());
  transport.add_response(MockTransport::tools_list_response(vec![]));
  let sent = transport.sent_messages_handle();

  let mut client = ClientBuilder::new()
    .with_transport(transport)
    .build()
    .await
    .unwrap();
  client.connect().await.unwrap();
  let _ = client.list_tools().await.unwrap();

  let messages = sent.lock().unwrap().clone();
  // Every captured request must lack `_meta` — both `initialize`
  // (params is an object) and `tools/list` (params absent originally,
  // so the inject-with-none path must also stay a no-op outside a
  // scope).
  for message in &messages {
    if let Some(params) = message.get("params") {
      assert!(
        !params
          .as_object()
          .map(|m| m.contains_key("_meta"))
          .unwrap_or(false),
        "expected no _meta field outside any scope; got: {message:?}"
      );
    }
  }
}

#[tokio::test]
async fn nested_scope_overrides_outer_traceparent_on_wire() {
  // Inner scope must shadow outer for outbound requests; same
  // contract `agentflow_tracing::context::scope` enforces locally.
  let mut transport = MockTransport::new();
  transport.add_response(MockTransport::standard_initialize_response());
  transport.add_response(MockTransport::tools_list_response(vec![]));
  let sent = transport.sent_messages_handle();

  agentflow_tracing::context::scope("00-outer-01".to_string(), async {
    let mut client = ClientBuilder::new()
      .with_transport(transport)
      .build()
      .await
      .unwrap();
    // `connect` (which sends `initialize`) runs in the outer scope.
    client.connect().await.unwrap();
    // `list_tools` runs in the inner scope — the wire request must
    // carry the inner value, not the outer.
    agentflow_tracing::context::scope("00-inner-01".to_string(), async {
      let _ = client.list_tools().await.unwrap();
    })
    .await;
  })
  .await;

  let messages = sent.lock().unwrap().clone();
  let init_request = captured_request(&messages, "initialize");
  let list_request = captured_request(&messages, "tools/list");
  assert_eq!(
    extract_traceparent_from_request(&init_request).as_deref(),
    Some("00-outer-01"),
    "initialize sent while outer scope was active"
  );
  assert_eq!(
    extract_traceparent_from_request(&list_request).as_deref(),
    Some("00-inner-01"),
    "list_tools sent while inner scope was active"
  );
}
